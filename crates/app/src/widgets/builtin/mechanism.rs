//! Mechanism2d: WPILib's `Mechanism2d` drawing of an arm, elevator or
//! other mechanism, as robot code publishes it (read by
//! [`crate::types::mechanism`]): its canvas (`dims`, `backgroundColor`)
//! fitted to the card, y up, and each ligament as a line in its color and
//! weight from where the one before it ends.
//!
//! Bound to the Mechanism2d's table (`.type` "Mechanism2d"); dragging the
//! table from the tree makes one. Ligament weights are WPILib's line
//! widths, scaled with the card.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::color_view::parse_color;
use crate::types::mechanism::{self, MECHANISM_TYPE, Mechanism};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// A ligament's color when it has none (WPILib's default, orange).
const DEFAULT_LINE: Color32 = Color32::from_rgb(0xFF, 0xA5, 0x00);
/// The canvas height, points, at which a weight is drawn that many points
/// wide; on a bigger or smaller card lines scale with it.
const WEIGHT_AT: f32 = 300.0;

/// The Mechanism2d widget.
pub struct Mechanism2d;

/// The mechanism `widget` shows now.
fn mechanism(widget: &WidgetContainer, live: &Live<'_>) -> Option<Mechanism> {
    mechanism::mechanism_of(widget.topic()?, live.frame.others)
}

/// Where the canvas goes in `rect`: as big as fits, its shape kept.
fn canvas_rect(rect: Rect, m: &Mechanism) -> Rect {
    let aspect = (m.width / m.height) as f32;
    let size = if rect.width() / rect.height() > aspect {
        egui::vec2(rect.height() * aspect, rect.height())
    } else {
        egui::vec2(rect.width(), rect.width() / aspect)
    };
    Rect::from_center_size(rect.center(), size)
}

/// A canvas point (units from the bottom left, y up) on screen.
fn to_screen(canvas: Rect, m: &Mechanism, p: (f64, f64)) -> Pos2 {
    egui::pos2(
        canvas.min.x + (p.0 / m.width) as f32 * canvas.width(),
        canvas.max.y - (p.1 / m.height) as f32 * canvas.height(),
    )
}

/// Draw `m` in `rect`.
fn draw(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect, m: &Mechanism) {
    let canvas = canvas_rect(rect, m);
    let background = m
        .background
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(visuals.extreme_bg_color);
    painter.rect_filled(canvas, 2.0, background);
    painter.rect_stroke(
        canvas,
        2.0,
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    let scale = (canvas.height() / WEIGHT_AT).clamp(0.3, 2.0);
    for s in m.segments() {
        let color = s
            .color
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(DEFAULT_LINE);
        let width = (s.weight as f32 * scale).clamp(1.0, 40.0);
        let (a, b) = (to_screen(canvas, m, s.start), to_screen(canvas, m, s.end));
        painter.line_segment([a, b], Stroke::new(width, color));
        // Round the joints, as WPILib's sim GUI does.
        painter.circle_filled(a, width / 2.0, color);
        painter.circle_filled(b, width / 2.0, color);
    }
}

/// `n` to `places` decimals, never `-0`.
fn rounded(n: f64, places: usize) -> String {
    let text = format!("{n:.places$}");
    match text.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_owned(),
        _ => text,
    }
}

/// The widget's summary, for screen readers and tests: each ligament's
/// path, length and its own angle: `base/elevator: 0.80 at 90°`.
fn summary(title: &str, m: Option<&Mechanism>) -> String {
    let Some(m) = m else {
        return format!("{title}: no mechanism");
    };
    let parts: Vec<String> = m
        .segments()
        .iter()
        .map(|s| {
            format!(
                "{}: {} at {}°",
                s.path,
                rounded(s.length, 2),
                rounded(s.angle, 0)
            )
        })
        .collect();
    if parts.is_empty() {
        format!("{title}: no ligaments")
    } else {
        format!("{title}: {}", parts.join("; "))
    }
}

impl Mechanism2d {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let m = mechanism(widget, live);
        match &m {
            Some(m) => draw(&painter, &visuals, rect, m),
            None => card::draw_message(&painter, &visuals, rect, "no mechanism"),
        }
        let text = summary(&widget.title, m.as_ref());
        ui.interact(rect, ui.scope_id().with("mechanism"), Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text));
    }
}

impl WidgetType for Mechanism2d {
    fn names(&self) -> &'static [&'static str] {
        &["Mechanism2d"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[MECHANISM_TYPE]
    }

    /// Everything under the table, whatever ligaments robot code adds.
    fn extra_topics(&self, p: &Map<String, Json>, server: Server<'_>) -> Vec<String> {
        let table = props::text(p, "topic", "").trim();
        if table.is_empty() {
            return Vec::new();
        }
        mechanism::table_topics(table, server.topics.iter().map(|t| t.name.as_ref()))
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
        self.show_in(ui, rect, widget, live);
        None
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        card::draw_message(painter, visuals, rect, "Mechanism2d");
    }

    fn settings(&self, _: &mut egui::Ui, _: &mut Map<String, Json>, _: ValueInfo<'_>) {}

    fn setting_keys(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::types::mechanism::{Ligament, Root};

    fn lift() -> Mechanism {
        let arm = Ligament {
            name: "arm".into(),
            angle: -45.0,
            length: 0.5,
            color: Some("#00FF00".into()),
            weight: 4.0,
            children: vec![],
        };
        Mechanism {
            width: 3.0,
            height: 2.0,
            background: None,
            roots: vec![Root {
                name: "base".into(),
                x: 1.5,
                y: 0.0,
                ligaments: vec![Ligament {
                    name: "elevator".into(),
                    angle: 90.0,
                    length: 0.8,
                    color: None,
                    weight: 6.0,
                    children: vec![arm],
                }],
            }],
        }
    }

    #[test]
    fn the_canvas_keeps_its_shape_and_y_goes_up() {
        let m = lift();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(300.0, 300.0));
        let canvas = canvas_rect(rect, &m);
        assert!((canvas.width() / canvas.height() - 1.5).abs() < 1e-4);
        assert_eq!(canvas.width(), 300.0);
        // The bottom left corner is the canvas's (0, 0); y goes up.
        let origin = to_screen(canvas, &m, (0.0, 0.0));
        assert_eq!(origin, egui::pos2(canvas.min.x, canvas.max.y));
        let top = to_screen(canvas, &m, (0.0, 2.0));
        assert!((top.y - canvas.min.y).abs() < 1e-4);
    }

    #[test]
    fn the_summary_lists_each_ligament_as_robot_code_set_it() {
        assert_eq!(
            summary("Lift", Some(&lift())),
            "Lift: base/elevator: 0.80 at 90°; base/elevator/arm: 0.50 at -45°"
        );
        assert_eq!(summary("Lift", None), "Lift: no mechanism");
        assert!(Mechanism2d.table_types().contains(&"Mechanism2d"));
    }
}
