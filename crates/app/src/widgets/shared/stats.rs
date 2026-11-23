//! Min, max and average on number widgets (Text Display, Number Bar,
//! Voltage View, Radial Gauge): a line under the value with the lowest,
//! highest and average values, over the last few seconds or since the last
//! reset.
//!
//! They're taken from every value received (the app records them in
//! [`Traces`](crate::traces::Traces), as graphs' values are), not from the
//! value the widget shows once per `period`, so short peaks count. The
//! average counts each value received once. "Since reset" means since the
//! app started, the server reconnected, or "Reset min/max" in the widget's
//! right-click menu. Off unless `show_stats` is set; never saved.

use eframe::egui::{self, Rect};
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::format::short_number;
use super::props::{self, set_property};
use super::scale;
use crate::traces::{SeriesKey, Stats};
use crate::widgets::{Live, Reset};

/// `true` shows the min, max and average under the value.
pub const SHOW_STATS: &str = "show_stats";
/// Over how many seconds back, the last ones; not set (or 0) is since the
/// last reset.
pub const STATS_WINDOW: &str = "stats_window";

/// Every setting key in this file.
pub const KEYS: [&str; 2] = [SHOW_STATS, STATS_WINDOW];

/// The longest window, in seconds: 10 minutes.
pub const MAX_WINDOW_S: f64 = 600.0;
/// Height of the line under the value, in points.
pub const LINE_HEIGHT: f32 = 16.0;
/// The line's text size, in points, unless it must shrink to fit.
const TEXT_SIZE: f32 = 11.0;
/// The smallest the line's text shrinks to, in points.
const MIN_TEXT_SIZE: f32 = 7.0;

/// Whether `widget` shows its min, max and average (`show_stats`).
pub fn shows(widget: &WidgetContainer) -> bool {
    props::flag(&widget.properties, SHOW_STATS, false)
}

/// The window the statistics cover, in seconds, or `None` for since the
/// last reset.
///
/// ```
/// use raptaboard::widgets::shared::stats::window_s;
/// let p = serde_json::json!({"stats_window": 10});
/// assert_eq!(window_s(p.as_object().unwrap()), Some(10.0));
/// assert_eq!(window_s(&serde_json::Map::new()), None);
/// ```
pub fn window_s(p: &Map<String, Json>) -> Option<f64> {
    let s = props::number(p, STATS_WINDOW, 0.0);
    (s > 0.0).then(|| s.min(MAX_WINDOW_S))
}

/// The series `widget` needs recorded, and for how many seconds back, for
/// [`WidgetType::traced`](crate::widgets::WidgetType::traced): its own for
/// the statistics (their window, or no points kept for since reset) and an
/// autoscaled range.
pub fn traced(widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
    let mut out: Vec<(SeriesKey, f64)> = scale::autoscale_series(widget)
        .into_iter()
        .map(|k| (k, 0.0))
        .collect();
    if shows(widget)
        && let Some(key) = SeriesKey::of(widget)
    {
        let seconds = window_s(&widget.properties).unwrap_or(0.0);
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = entry.1.max(seconds),
            None => out.push((key, seconds)),
        }
    }
    out
}

/// What `widget`'s right-click menu resets, for
/// [`WidgetType::resettable`](crate::widgets::WidgetType::resettable):
/// "Reset min/max" for statistics since reset (and any autoscaled range
/// with them), "Reset scale" for an autoscaled range alone.
pub fn reset(widget: &WidgetContainer) -> Option<Reset> {
    let since = shows(widget) && window_s(&widget.properties).is_none();
    if since && let Some(key) = SeriesKey::of(widget) {
        return Some(Reset {
            label: "Reset min/max",
            hover: "Start the lowest, highest and average again from the value now",
            series: vec![key],
        });
    }
    let series = scale::autoscale_series(widget);
    (!series.is_empty()).then_some(Reset {
        label: "Reset scale",
        hover: "Start the autoscaled range again from the value now",
        series,
    })
}

/// The statistics `widget` shows now, from what `live` has recorded; `None`
/// before any value.
pub fn of(widget: &WidgetContainer, live: &Live<'_>) -> Option<Stats> {
    let key = SeriesKey::of(widget)?;
    let traces = live.frame.traces;
    match window_s(&widget.properties) {
        Some(s) => traces.window(&key, live.frame.now_us, s),
        None => traces.since_reset(&key),
    }
}

/// The line of statistics: `min 0.10  avg 0.52  max 0.98`.
///
/// ```
/// use raptaboard::traces::Stats;
/// use raptaboard::widgets::shared::stats::text;
/// let s = Stats::from_values([0.1, 0.5, 0.96]).unwrap();
/// assert_eq!(text(Some(s)), "min 0.10  avg 0.52  max 0.96");
/// assert_eq!(text(None), "no values yet");
/// ```
pub fn text(stats: Option<Stats>) -> String {
    match stats {
        Some(s) => format!(
            "min {}  avg {}  max {}",
            short_number(s.min),
            short_number(s.mean()),
            short_number(s.max)
        ),
        None => "no values yet".into(),
    }
}

/// `rect` split for `widget`: where the value goes, and the line for the
/// statistics along the bottom if it shows them (and there's room).
pub fn split(rect: Rect, widget: &WidgetContainer) -> (Rect, Option<Rect>) {
    if !shows(widget) || rect.height() < LINE_HEIGHT * 2.0 {
        return (rect, None);
    }
    let (top, line) = rect.split_top_bottom_at_y(rect.max.y - LINE_HEIGHT);
    (top, Some(line))
}

/// Draw `widget`'s statistics in `line` (from [`split`]), shrinking the
/// text to fit, with an accessible label: "Arm: min 0.10, avg 0.52, max 0.96".
pub fn draw(ui: &mut egui::Ui, line: Rect, widget: &WidgetContainer, live: &Live<'_>) {
    let stats = of(widget, live);
    let words = text(stats);
    let color = ui.visuals().weak_text_color();
    let painter = ui.painter().with_clip_rect(line.intersect(ui.clip_rect()));
    let mut galley =
        painter.layout_no_wrap(words.clone(), egui::FontId::proportional(TEXT_SIZE), color);
    if galley.size().x > line.width() && galley.size().x > 0.0 {
        let size = (TEXT_SIZE * line.width() / galley.size().x).max(MIN_TEXT_SIZE);
        galley = painter.layout_no_wrap(words.clone(), egui::FontId::proportional(size), color);
    }
    let at = line.center() - galley.size() / 2.0;
    painter.galley(at, galley, color);
    let label = format!("{}: {}", widget.title, words.replace("  ", ", "));
    ui.interact(line, ui.scope_id().with("stats"), egui::Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Label, true, &label));
}

/// The settings dialog rows: on or off, and the window.
pub fn rows(ui: &mut egui::Ui, p: &mut Map<String, Json>) {
    ui.label("Min / max");
    let mut on = props::flag(p, SHOW_STATS, false);
    if ui
        .checkbox(&mut on, "Show the min, max and average")
        .on_hover_text(
            "From every value received, so short peaks count. Not saved: they \
             start again when the app starts.",
        )
        .changed()
    {
        set_property(p, SHOW_STATS, on.then_some(true.into()));
    }
    ui.end_row();
    if !on {
        return;
    }
    ui.label("Over");
    ui.horizontal(|ui| {
        let window = window_s(p);
        let mut since = window.is_none();
        if ui
            .radio_value(&mut since, true, "Since reset")
            .on_hover_text(
                "Since the app started, the robot reconnected, or Reset min/max in \
                 the widget's right-click menu",
            )
            .changed()
        {
            set_property(p, STATS_WINDOW, None);
        }
        if ui.radio_value(&mut since, false, "The last").changed() {
            set_property(p, STATS_WINDOW, Some(10.0.into()));
        }
        if let Some(mut seconds) = window
            && ui
                .add(
                    egui::DragValue::new(&mut seconds)
                        .range(0.5..=MAX_WINDOW_S)
                        .speed(0.5)
                        .suffix(" s"),
                )
                .changed()
        {
            set_property(p, STATS_WINDOW, Some(seconds.into()));
        }
    });
    ui.end_row();
}
