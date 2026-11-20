//! Alerts: the robot's active alerts (WPILib's `Alert` class), as a circle
//! with the three signs in it, and optionally their text.
//!
//! The circle has a red ✕ (errors), a yellow ! (warnings) and a blue i
//! (infos); each sign lights while there's an alert of its level, with a
//! count when there's more than one, and the circle's ring takes the color
//! of the most urgent. With `show_text` (on unless set) the alerts' text is
//! listed beside the circle or below it (`text_position`): errors first,
//! each level newest first. Small, it's a status light; big, a list.
//!
//! Bound to one alert group's table (`topic`, `.type` `"Alerts"`), or with
//! `all_groups` to every group on the server. When a new error arrives the
//! widget pulses for `pulse_seconds` (`pulse`, on unless set), and can play
//! a tone (`sound`, off unless set; `tone` picks which; the bell button
//! mutes it). Warnings and infos don't ask for attention. While the
//! connection is down it keeps showing the last alerts it saw, greyed, so
//! what was wrong just before isn't lost.

use std::collections::HashSet;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::sound::Tone;
use crate::types::alerts::{self, ActiveAlert, Level};
use crate::widgets::shared::{card, props};
use crate::widgets::{Effect, Live, ValueInfo, WidgetType};

/// Every alert group on the server, not just the widget's `topic`.
pub const ALL_GROUPS: &str = "all_groups";
/// List the alerts' text beside or below the circle (on unless set).
pub const SHOW_TEXT: &str = "show_text";
/// Where the text goes: `auto` (beside a wide widget, below a tall one),
/// `right` or `below`.
pub const TEXT_POSITION: &str = "text_position";
/// The least urgent level shown: `info` (all), `warning` or `error`.
pub const MIN_LEVEL: &str = "min_level";
/// Pulse when a new error arrives (on unless set).
pub const PULSE: &str = "pulse";
/// How long the pulse lasts, in seconds.
pub const PULSE_SECONDS: &str = "pulse_seconds";
/// Play a tone when a new error arrives (off unless set).
pub const SOUND: &str = "sound";
/// Which tone: `beep`, `double beep` or `chirp`.
pub const TONE: &str = "tone";

const KEYS: [&str; 8] = [
    ALL_GROUPS,
    SHOW_TEXT,
    TEXT_POSITION,
    MIN_LEVEL,
    PULSE,
    PULSE_SECONDS,
    SOUND,
    TONE,
];

/// The pulse's length when `pulse_seconds` isn't set.
const DEFAULT_PULSE_S: f64 = 5.0;
/// The fewest seconds between two tones, so a flickering alert doesn't
/// sound over and over.
const TONE_GAP_S: f64 = 3.0;
/// Pulses a second.
const PULSE_RATE: f64 = 2.0;
/// Size of the alerts' text, in points.
const TEXT_SIZE: f32 = 13.0;
/// Space between the circle and the text, and around, in points.
const GAP: f32 = 6.0;

/// The least urgent level shown, from `min_level`.
fn min_level(p: &Map<String, Json>) -> Level {
    match props::text(p, MIN_LEVEL, "info") {
        "error" => Level::Error,
        "warning" => Level::Warning,
        _ => Level::Info,
    }
}

/// The alert tables a widget with settings `p` reads.
fn tables(widget: &WidgetContainer, live: &Live<'_>) -> Vec<String> {
    if props::flag(&widget.properties, ALL_GROUPS, false) {
        alerts::alert_tables(live.others)
    } else {
        widget.topic().map(str::to_owned).into_iter().collect()
    }
}

/// The alerts in `tables` now, or `None` if none of them is published.
fn read(tables: &[String], live: &Live<'_>) -> Option<Vec<ActiveAlert>> {
    let mut found = false;
    let mut all = Vec::new();
    for table in tables {
        if let Some(group) = alerts::read_group(table, live.others) {
            found = true;
            all.extend(group);
        }
    }
    // Errors first across groups; each level keeps the robot's order.
    all.sort_by_key(|a| a.level);
    found.then_some(all)
}

/// What a widget remembers between frames.
#[derive(Debug, Clone, Default)]
struct Memory {
    /// The alerts seen last while connected.
    last: Vec<ActiveAlert>,
    /// The errors active last frame, to see new ones.
    errors: HashSet<(String, String)>,
    /// Seen anything yet: the first look isn't "new".
    started: bool,
    /// When the pulse ends (egui time, seconds).
    pulse_until: f64,
    /// When the last tone played, if one has.
    last_tone: Option<f64>,
}

/// The errors in `alerts`, by group and text.
fn error_set(alerts: &[ActiveAlert]) -> HashSet<(String, String)> {
    alerts
        .iter()
        .filter(|a| a.level == Level::Error)
        .map(|a| (a.group.clone(), a.text.clone()))
        .collect()
}

/// Whether `now` has an error that `before` didn't.
///
/// ```
/// use std::collections::HashSet;
/// use raptaboard::widgets::builtin::alerts::has_new;
/// let before: HashSet<_> = [("A".to_string(), "x".to_string())].into();
/// let mut now = before.clone();
/// assert!(!has_new(&before, &now));
/// now.insert(("A".into(), "y".into()));
/// assert!(has_new(&before, &now));
/// ```
pub fn has_new(before: &HashSet<(String, String)>, now: &HashSet<(String, String)>) -> bool {
    now.iter().any(|e| !before.contains(e))
}

/// Draw the ✕ (an error's sign) at `c`, `size` across.
fn draw_cross(painter: &egui::Painter, c: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    let stroke = Stroke::new((size * 0.18).max(1.5), color);
    painter.line_segment([c + egui::vec2(-h, -h), c + egui::vec2(h, h)], stroke);
    painter.line_segment([c + egui::vec2(-h, h), c + egui::vec2(h, -h)], stroke);
}

/// Draw a ! (a warning's sign, `upside_down` false) or an i (info, true)
/// at `c`, `size` tall: a bar and a dot.
fn draw_bar_and_dot(
    painter: &egui::Painter,
    c: Pos2,
    size: f32,
    color: Color32,
    upside_down: bool,
) {
    let h = size / 2.0;
    let width = (size * 0.2).max(1.5);
    let dot = width * 0.75;
    let (bar_top, bar_bottom, dot_y) = if upside_down {
        (c.y - h * 0.35, c.y + h, c.y - h + dot)
    } else {
        (c.y - h, c.y + h * 0.35, c.y + h - dot)
    };
    painter.line_segment(
        [egui::pos2(c.x, bar_top), egui::pos2(c.x, bar_bottom)],
        Stroke::new(width, color),
    );
    painter.circle_filled(egui::pos2(c.x, dot_y), dot, color);
}

/// Draw `level`'s sign at `c`, `size` across.
pub(crate) fn draw_sign(painter: &egui::Painter, level: Level, c: Pos2, size: f32, color: Color32) {
    match level {
        Level::Error => draw_cross(painter, c, size * 0.8, color),
        Level::Warning => draw_bar_and_dot(painter, c, size, color, false),
        Level::Info => draw_bar_and_dot(painter, c, size, color, true),
    }
}

/// Draw the circle in `rect` (centered, as big as fits): the three signs,
/// lit by `counts`, with counts over 1 beside them, and the ring in the
/// most urgent color. `pulse` (0 to 1) fills it red; `dim` greys it all.
fn draw_circle(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    counts: [usize; 3],
    pulse: f32,
    dim: bool,
) {
    let r = rect.width().min(rect.height()) / 2.0 - 2.0;
    if r <= 2.0 {
        return;
    }
    let c = rect.center();
    let off = visuals.weak_text_color().gamma_multiply(0.5);
    let lit = |level: Level| {
        if counts[level as usize] == 0 {
            off
        } else if dim {
            visuals.weak_text_color()
        } else {
            level.color()
        }
    };
    if pulse > 0.0 {
        painter.circle_filled(
            c,
            r,
            Level::Error.color().gamma_multiply(0.15 + 0.35 * pulse),
        );
    }
    let ring = Level::ALL
        .into_iter()
        .find(|l| counts[*l as usize] > 0)
        .map_or(off, lit);
    painter.circle_stroke(c, r, Stroke::new((r * 0.08).clamp(1.5, 4.0), ring));
    // ✕ top left, ! top right, i at the bottom.
    let size = r * 0.42;
    let places = [
        (Level::Error, c + egui::vec2(-r * 0.4, -r * 0.25)),
        (Level::Warning, c + egui::vec2(r * 0.4, -r * 0.25)),
        (Level::Info, c + egui::vec2(0.0, r * 0.45)),
    ];
    for (level, at) in places {
        let color = lit(level);
        draw_sign(painter, level, at, size, color);
        let n = counts[level as usize];
        if n > 1 {
            painter.text(
                at + egui::vec2(size * 0.55, size * 0.55),
                egui::Align2::LEFT_TOP,
                n.to_string(),
                egui::FontId::proportional((size * 0.6).max(8.0)),
                color,
            );
        }
    }
}

/// Where the circle and the text go in `rect`: the circle's area and the
/// text's (if it's shown).
fn layout(rect: Rect, show_text: bool, position: &str) -> (Rect, Option<Rect>) {
    if !show_text {
        return (rect, None);
    }
    let beside = match position {
        "right" => true,
        "below" => false,
        _ => rect.width() >= rect.height() * 1.6,
    };
    if beside {
        let side = rect.height().min(rect.width() * 0.4);
        let (circle, text) = rect.split_left_right_at_x(rect.min.x + side);
        (circle, Some(text.with_min_x(text.min.x + GAP)))
    } else {
        let side = rect.width().min(rect.height() * 0.45);
        let (circle, text) = rect.split_top_bottom_at_y(rect.min.y + side);
        (circle, Some(text.with_min_y(text.min.y + GAP)))
    }
}

/// List `alerts` in `rect`, each with its sign, wrapping long text; the
/// group's name first when there's more than one group. `note`, if any,
/// heads the list (disconnected, nothing published). What doesn't fit is
/// counted at the end.
fn draw_list(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    alerts: &[ActiveAlert],
    note: Option<&str>,
    dim: bool,
) {
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let font = egui::FontId::proportional(TEXT_SIZE);
    let mut y = rect.min.y;
    let line = TEXT_SIZE * 1.3;
    if let Some(note) = note {
        let g = painter.layout(
            note.to_owned(),
            font.clone(),
            visuals.weak_text_color(),
            rect.width(),
        );
        let height = g.size().y;
        painter.galley(egui::pos2(rect.min.x, y), g, visuals.weak_text_color());
        y += height + 4.0;
    }
    if alerts.is_empty() && note.is_none() {
        painter.text(
            rect.left_top(),
            egui::Align2::LEFT_TOP,
            "No active alerts",
            font,
            visuals.weak_text_color(),
        );
        return;
    }
    let groups: HashSet<&str> = alerts.iter().map(|a| a.group.as_str()).collect();
    let text_x = rect.min.x + TEXT_SIZE + 4.0;
    let width = (rect.max.x - text_x).max(10.0);
    for (i, a) in alerts.iter().enumerate() {
        let color = if dim {
            visuals.weak_text_color()
        } else {
            visuals.text_color()
        };
        let text = if groups.len() > 1 {
            format!("{}: {}", a.group, a.text)
        } else {
            a.text.clone()
        };
        let g = painter.layout(text, font.clone(), color, width);
        let height = g.size().y.max(line);
        // Not enough room for this one and a "more" line: say how many.
        if y + height > rect.max.y && i > 0 {
            let more = format!("+{} more", alerts.len() - i);
            painter.text(
                egui::pos2(text_x, y),
                egui::Align2::LEFT_TOP,
                more,
                font,
                visuals.weak_text_color(),
            );
            return;
        }
        let sign = if dim {
            visuals.weak_text_color()
        } else {
            a.level.color()
        };
        draw_sign(
            &painter,
            a.level,
            egui::pos2(rect.min.x + TEXT_SIZE / 2.0, y + line / 2.0),
            TEXT_SIZE * 0.8,
            sign,
        );
        painter.galley(egui::pos2(text_x, y), g, color);
        y += height + 2.0;
    }
}

/// The Alerts widget (Elastic's `"Alerts"`).
pub struct AlertsWidget;

impl AlertsWidget {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let p = &widget.properties;
        let id = ui.scope_id().with("alerts");
        let now = ui.input(|i| i.time);
        let mut memory: Memory = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let current = read(&tables(widget, live), live);
        let (shown, note, dim) = match (&current, live.connected) {
            (Some(now_alerts), true) => {
                // A new error: pulse, and maybe a tone.
                let errors = error_set(now_alerts);
                if memory.started && has_new(&memory.errors, &errors) {
                    if props::flag(p, PULSE, true) {
                        memory.pulse_until = now + props::number(p, PULSE_SECONDS, DEFAULT_PULSE_S);
                    }
                    if props::flag(p, SOUND, false)
                        && memory.last_tone.is_none_or(|last| now - last >= TONE_GAP_S)
                    {
                        memory.last_tone = Some(now);
                        let tone = Tone::named(props::text(p, TONE, "beep"));
                        live.effects.borrow_mut().push(Effect::Tone(tone));
                    }
                }
                memory.errors = errors;
                memory.started = true;
                memory.last = now_alerts.clone();
                (now_alerts.clone(), None, false)
            }
            // Connected, but nothing published (yet, or any more: a robot
            // dropping out can lose its topics before the connection says
            // so). The last alerts seen stay, greyed, and so do the errors
            // remembered, so they aren't "new" when they come back.
            (None, true) if !memory.last.is_empty() => (
                memory.last.clone(),
                Some("Not published now: the last alerts seen"),
                true,
            ),
            (None, true) => {
                let why = if props::flag(p, ALL_GROUPS, false) {
                    "No alert groups published"
                } else {
                    "Alerts not published"
                };
                (Vec::new(), Some(why), false)
            }
            // Down: the last alerts seen, greyed.
            (_, false) => (
                memory.last.clone(),
                Some("Robot disconnected: the last alerts seen"),
                true,
            ),
        };
        let lowest = min_level(p);
        let shown: Vec<ActiveAlert> = shown.into_iter().filter(|a| a.level <= lowest).collect();
        let counts = alerts::counts(&shown);
        let pulse = if now < memory.pulse_until {
            ui.ctx().request_repaint();
            (0.5 - 0.5 * (now * PULSE_RATE * std::f64::consts::TAU).cos()) as f32
        } else {
            0.0
        };
        ui.data_mut(|d| d.insert_temp(id, memory));
        // Named for screen readers with its counts (and read by the tests).
        let summary = format!(
            "{}: {} {}, {} {}, {} {}{}",
            widget.title,
            counts[0],
            Level::Error.name(counts[0]),
            counts[1],
            Level::Warning.name(counts[1]),
            counts[2],
            Level::Info.name(counts[2]),
            match (dim, live.connected) {
                (false, _) => "",
                (true, false) => " (disconnected)",
                (true, true) => " (not published)",
            },
        );
        ui.interact(rect, id, Sense::hover()).widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::Status, true, &summary)
        });
        let visuals = ui.visuals().clone();
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let (circle, text) = layout(
            rect,
            props::flag(p, SHOW_TEXT, true),
            props::text(p, TEXT_POSITION, "auto"),
        );
        draw_circle(&painter, &visuals, circle, counts, pulse, dim);
        if let Some(text) = text {
            draw_list(&painter, &visuals, text, &shown, note, dim);
        }
    }
}

/// The highest level above info active in the alerts a widget shows, from
/// the server's `values`, for a mark on its tab. `None` for other widget
/// types, no alerts, or only infos.
pub fn tab_level(
    widget: &WidgetContainer,
    values: &std::collections::HashMap<&str, &Value>,
) -> Option<Level> {
    if crate::widgets::find(&widget.widget_type).map(|t| t.names()[0]) != Some("Alerts") {
        return None;
    }
    let tables = if props::flag(&widget.properties, ALL_GROUPS, false) {
        alerts::alert_tables(values)
    } else {
        widget.topic().map(str::to_owned).into_iter().collect()
    };
    let shown: Vec<ActiveAlert> = tables
        .iter()
        .filter_map(|t| alerts::read_group(t, values))
        .flatten()
        .filter(|a| a.level <= min_level(&widget.properties))
        .collect();
    alerts::highest(&shown).filter(|l| *l != Level::Info)
}

/// The topics an Alerts widget with settings `p` needs, given every topic
/// on the server: its group's, or every group's with `all_groups` (every
/// table's `.type`, to tell which are alert groups, and their lists).
pub fn needed_topics(p: &Map<String, Json>, server: &[String]) -> Vec<String> {
    if props::flag(p, ALL_GROUPS, false) {
        server
            .iter()
            .filter_map(|t| t.strip_suffix("/.type"))
            .flat_map(alerts::table_topics)
            .collect()
    } else {
        props::text(p, "topic", "")
            .is_empty()
            .then(Vec::new)
            .unwrap_or_else(|| alerts::table_topics(props::text(p, "topic", "")))
    }
}

impl WidgetType for AlertsWidget {
    fn names(&self) -> &'static [&'static str] {
        &["Alerts"]
    }
    fn accepts(&self, _: &str) -> bool {
        false
    }
    fn table_types(&self) -> &'static [&'static str] {
        &[alerts::ALERTS_TYPE]
    }
    fn extra_topics(&self, p: &Map<String, Json>) -> Vec<String> {
        // Every group's are added by the app (it knows the server's topics).
        if props::flag(p, ALL_GROUPS, false) {
            Vec::new()
        } else {
            needed_topics(p, &[])
        }
    }
    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        self.show_in(ui, body.shrink(card::BODY_MARGIN), widget, live);
        None
    }
    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.show_in(ui, live.free.unwrap_or(rect), widget, live);
        None
    }
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        p: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        // Without values (while dragged): the circle, unlit.
        let (circle, _) = layout(
            rect,
            props::flag(p, SHOW_TEXT, true),
            props::text(p, TEXT_POSITION, "auto"),
        );
        draw_circle(painter, visuals, circle, [0; 3], 0.0, false);
    }
    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::flag_row(
            ui,
            p,
            "Groups",
            "Every alert group on the server",
            ALL_GROUPS,
            false,
        );
        props::flag_row(ui, p, "Text", "List the alerts' text", SHOW_TEXT, true);
        props::choice_row(
            ui,
            p,
            "Text goes",
            TEXT_POSITION,
            "auto",
            &["auto", "right", "below"],
        );
        props::choice_row(
            ui,
            p,
            "Show down to",
            MIN_LEVEL,
            "info",
            &["info", "warning", "error"],
        );
        props::flag_row(
            ui,
            p,
            "Pulse",
            "Pulse when a new error arrives",
            PULSE,
            true,
        );
        props::number_row(
            ui,
            p,
            "Pulse for (s)",
            PULSE_SECONDS,
            DEFAULT_PULSE_S,
            1.0..=60.0,
        );
        props::flag_row(
            ui,
            p,
            "Sound",
            "Play a tone when a new error arrives",
            SOUND,
            false,
        );
        let names: Vec<&str> = Tone::CHOICES.iter().map(|(_, n)| *n).collect();
        props::choice_row(ui, p, "Tone", TONE, "beep", &names);
    }
    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn text_goes_beside_a_wide_widget_and_below_a_tall_one() {
        let wide = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 100.0));
        let tall = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 300.0));
        let (c, t) = layout(wide, true, "auto");
        assert!(t.unwrap().min.x > c.max.x);
        let (c, t) = layout(tall, true, "auto");
        assert!(t.unwrap().min.y > c.max.y);
        // Set by hand, and none without text.
        assert!(layout(tall, true, "right").1.unwrap().min.x > 0.0);
        assert_eq!(layout(wide, false, "auto"), (wide, None));
    }

    #[test]
    fn all_groups_needs_every_tables_lists() {
        let mut p = Map::new();
        p.insert("topic".into(), "/SmartDashboard/Alerts".into());
        assert_eq!(needed_topics(&p, &[]).len(), 4);
        p.insert(ALL_GROUPS.into(), true.into());
        let server = vec![
            "/A/.type".to_string(),
            "/B/.type".to_string(),
            "/A/x".to_string(),
        ];
        let needed = needed_topics(&p, &server);
        assert_eq!(needed.len(), 8);
        assert!(needed.contains(&"/B/errors".to_string()));
        assert!(AlertsWidget.extra_topics(&p).is_empty());
    }
}
