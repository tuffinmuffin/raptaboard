//! Number Bar and Voltage View: a number as a bar filled from the minimum,
//! with the value written above and evenly spaced labels along the bar.
//!
//! The two are the same widget with different defaults: Number Bar runs
//! from -1 to 1 in blue, Voltage View from 4 to 13 in yellow with the unit
//! "V" (it's kept so Elastic layouts open; a Number Bar can do the same).
//! Elastic's settings: `min_value`, `max_value`, `divisions` (how many
//! labels), `inverted` (fill from the other end) and `orientation`
//! (`horizontal` or `vertical`); and raptaboard's [`scale`] settings: fill
//! color (solid or by value), unit, and labels on or off. A dot past an
//! end of the bar shows the value is beyond it.

use eframe::egui::{self, Color32, CornerRadius, Rect, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::traces::SeriesKey;
use crate::widgets::shared::format::{number_of, short_number};
use crate::widgets::shared::{card, props, scale, stats};
use crate::widgets::{Live, Reset, ValueInfo, WidgetType};

/// The lowest value on the bar (Elastic).
pub const MIN_VALUE: &str = scale::MIN_VALUE;
/// The highest value on the bar (Elastic).
pub const MAX_VALUE: &str = scale::MAX_VALUE;
/// How many labels along the bar, ends included (Elastic).
pub const DIVISIONS: &str = "divisions";
/// Fill from the far end (Elastic).
pub const INVERTED: &str = "inverted";
/// `horizontal` or `vertical` (Elastic).
pub const ORIENTATION: &str = "orientation";

const KEYS: [&str; 22] = [
    scale::AUTOSCALE,
    scale::AUTOSCALE_KEEP_RANGE,
    stats::SHOW_STATS,
    stats::STATS_WINDOW,
    crate::widgets::shared::value_colors::COLOR_BY,
    crate::widgets::shared::value_colors::COLOR_BELOW,
    crate::widgets::shared::value_colors::COLOR_ABOVE,
    scale::THICKNESS,
    scale::SHOW_VALUE,
    MIN_VALUE,
    MAX_VALUE,
    DIVISIONS,
    INVERTED,
    ORIENTATION,
    scale::FILL_COLOR,
    scale::GRADIENT,
    scale::LOW_COLOR,
    scale::MID_COLOR,
    scale::MID_VALUE,
    scale::HIGH_COLOR,
    scale::UNIT,
    scale::SHOW_LABELS,
];

/// Thickness of the bar when `thickness` isn't set, in points.
const BAR_THICKNESS: f32 = 8.0;
/// Size of the labels along the bar, in points.
const LABEL_SIZE: f32 = 11.0;
/// Size of the value text, in points.
const VALUE_SIZE: f32 = 16.0;
/// Space around the bar, in points.
const PAD: f32 = 6.0;
/// How far past the bar's end the "past the end" dot sits, in points.
const DOT_GAP: f32 = 7.0;

/// The Number Bar widget (Elastic's `"Number Bar"`).
pub struct NumberBar;
/// The Voltage View widget (Elastic's `"Voltage View"`).
pub struct VoltageView;

/// What differs between Number Bar and Voltage View.
struct Style {
    min: f64,
    max: f64,
    color: Color32,
    unit: &'static str,
}

const NUMBER_BAR: Style = Style {
    min: -1.0,
    max: 1.0,
    color: Color32::from_rgb(0x21, 0x96, 0xF3),
    unit: "",
};
const VOLTAGE_VIEW: Style = Style {
    min: 4.0,
    max: 13.0,
    color: Color32::from_rgb(0xFF, 0xEB, 0x3B),
    unit: "V",
};

/// How full the bar is for `value` between `min` and `max`, from 0 to 1.
///
/// ```
/// use raptaboard::widgets::builtin::number_bar::fraction;
/// assert_eq!(fraction(0.0, -1.0, 1.0), 0.5);
/// assert_eq!(fraction(5.0, -1.0, 1.0), 1.0);
/// assert_eq!(fraction(1.0, 1.0, 1.0), 0.0);
/// ```
pub fn fraction(value: f64, min: f64, max: f64) -> f32 {
    if max <= min {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
}

/// The label values along a bar: `divisions` evenly spaced from `min` to
/// `max`, ends included (at least the two ends).
///
/// ```
/// use raptaboard::widgets::builtin::number_bar::labels;
/// assert_eq!(labels(-1.0, 1.0, 5), [-1.0, -0.5, 0.0, 0.5, 1.0]);
/// ```
pub fn labels(min: f64, max: f64, divisions: usize) -> Vec<f64> {
    let n = divisions.max(2);
    (0..n)
        .map(|i| min + (max - min) * i as f64 / (n - 1) as f64)
        .collect()
}

fn draw_bar(
    style: &Style,
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    p: &Map<String, Json>,
    value: Option<&Value>,
) {
    let Some(v) = value.and_then(number_of) else {
        let why = if value.is_some() {
            "not a number"
        } else {
            "no value"
        };
        return card::draw_message(painter, visuals, rect, why);
    };
    let min = props::number(p, MIN_VALUE, style.min);
    let max = props::number(p, MAX_VALUE, style.max);
    let divisions = props::number(p, DIVISIONS, 5.0).clamp(2.0, 50.0) as usize;
    let inverted = props::flag(p, INVERTED, false);
    let vertical = props::text(p, ORIENTATION, "horizontal") == "vertical";
    let whole = matches!(value, Some(Value::Int(_)));
    let fill_color = scale::fill_color(p, v, min, max, style.color);
    let show_labels = scale::show_labels(p);
    let past = scale::past_end(v, min, max);
    let track_color = visuals.widgets.inactive.bg_fill;
    let label_color = visuals.weak_text_color();
    let label_font = egui::FontId::proportional(LABEL_SIZE);
    let amount = fraction(v, min, max);
    let text = scale::value_with_unit(v, whole, p, style.unit);
    let steps = (labels(min, max, divisions).len() - 1) as f32;

    let show_value = scale::show_value(p);
    let thick = scale::thickness(p, BAR_THICKNESS);
    let round = CornerRadius::same((thick / 2.0).min(4.0) as u8);
    if vertical {
        // The value on the left, the bar beside it, labels to its right.
        // Without the value, the bar sits in the middle.
        let bar_area = if show_value {
            let (text_area, bar_area) = rect.split_left_right_at_fraction(0.45);
            painter.text(
                text_area.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::monospace(VALUE_SIZE),
                visuals.text_color(),
            );
            bar_area
        } else {
            rect.with_min_x(rect.center().x - thick / 2.0 - PAD)
        };
        let track = Rect::from_min_size(
            egui::pos2(bar_area.min.x + PAD, bar_area.min.y + PAD),
            egui::vec2(thick, (bar_area.height() - 2.0 * PAD).max(0.0)),
        );
        painter.rect_filled(track, round, track_color);
        let filled = track.height() * amount;
        let fill = if inverted {
            track.with_max_y(track.min.y + filled)
        } else {
            track.with_min_y(track.max.y - filled)
        };
        painter.rect_filled(fill, round, fill_color);
        // Past an end: a dot just beyond it.
        if let Some(end) = past {
            let low_end_at_bottom = !inverted;
            let at_bottom = (end == scale::Past::Min) == low_end_at_bottom;
            let y = if at_bottom {
                track.max.y + DOT_GAP
            } else {
                track.min.y - DOT_GAP
            };
            scale::draw_past_dot(painter, egui::pos2(track.center().x, y));
        }
        let shown_labels = if show_labels {
            labels(min, max, divisions)
        } else {
            Vec::new()
        };
        for (i, n) in shown_labels.into_iter().enumerate() {
            let t = i as f32 / steps;
            let y = if inverted {
                track.min.y + track.height() * t
            } else {
                track.max.y - track.height() * t
            };
            let x = track.max.x + 4.0;
            painter.hline(track.max.x..=x, y, Stroke::new(1.0, label_color));
            painter.text(
                egui::pos2(x + 2.0, y),
                egui::Align2::LEFT_CENTER,
                short_number(n),
                label_font.clone(),
                label_color,
            );
        }
    } else {
        // The value on top, the bar below it, labels under the bar.
        // Without the value, the bar moves up into its place.
        let top = if show_value {
            painter.text(
                egui::pos2(rect.center().x, rect.min.y + VALUE_SIZE * 0.75),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::monospace(VALUE_SIZE),
                visuals.text_color(),
            );
            rect.min.y + VALUE_SIZE * 1.5 + PAD
        } else {
            rect.min.y + PAD
        };
        let track = Rect::from_min_size(
            egui::pos2(rect.min.x + PAD * 2.0, top),
            egui::vec2((rect.width() - PAD * 4.0).max(0.0), thick),
        );
        painter.rect_filled(track, round, track_color);
        let filled = track.width() * amount;
        let fill = if inverted {
            track.with_min_x(track.max.x - filled)
        } else {
            track.with_max_x(track.min.x + filled)
        };
        painter.rect_filled(fill, round, fill_color);
        if let Some(end) = past {
            let at_left = (end == scale::Past::Min) != inverted;
            let x = if at_left {
                track.min.x - DOT_GAP
            } else {
                track.max.x + DOT_GAP
            };
            scale::draw_past_dot(painter, egui::pos2(x, track.center().y));
        }
        let shown_labels = if show_labels {
            labels(min, max, divisions)
        } else {
            Vec::new()
        };
        for (i, n) in shown_labels.into_iter().enumerate() {
            let t = i as f32 / steps;
            let x = if inverted {
                track.max.x - track.width() * t
            } else {
                track.min.x + track.width() * t
            };
            let y = track.max.y + 4.0;
            painter.vline(x, track.max.y..=y, Stroke::new(1.0, label_color));
            painter.text(
                egui::pos2(x, y + 1.0),
                egui::Align2::CENTER_TOP,
                short_number(n),
                label_font.clone(),
                label_color,
            );
        }
    }
}

fn settings_rows(style: &Style, ui: &mut egui::Ui, p: &mut Map<String, Json>) {
    props::number_row(ui, p, "Minimum", MIN_VALUE, style.min, -1e9..=1e9);
    props::number_row(ui, p, "Maximum", MAX_VALUE, style.max, -1e9..=1e9);
    scale::autoscale_rows(ui, p);
    stats::rows(ui, p);
    props::whole_row(ui, p, "Labels", DIVISIONS, 5, 2..=50);
    props::flag_row(ui, p, "Direction", "Fill from the far end", INVERTED, false);
    props::choice_row(
        ui,
        p,
        "Orientation",
        ORIENTATION,
        "horizontal",
        &["horizontal", "vertical"],
    );
    scale::rows(ui, p, style.unit, style.color, BAR_THICKNESS);
}

/// The NT types bars show.
fn is_number(nt_type: &str) -> bool {
    matches!(nt_type, "double" | "float" | "int")
}

impl WidgetType for NumberBar {
    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        scale::show(
            self,
            ui,
            rect,
            widget,
            live,
            (NUMBER_BAR.min, NUMBER_BAR.max),
        );
        None
    }
    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        scale::show_body(
            self,
            ui,
            rect,
            widget,
            live,
            (NUMBER_BAR.min, NUMBER_BAR.max),
        );
        None
    }
    fn traced(&self, widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
        stats::traced(widget)
    }
    fn resettable(&self, widget: &WidgetContainer) -> Option<Reset> {
        stats::reset(widget)
    }
    fn names(&self) -> &'static [&'static str] {
        &["Number Bar"]
    }
    fn accepts(&self, _: &str) -> bool {
        false
    }
    fn shows(&self, nt_type: &str) -> bool {
        is_number(nt_type)
    }
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        draw_bar(&NUMBER_BAR, painter, visuals, rect, props, value);
    }
    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, _: ValueInfo<'_>) {
        settings_rows(&NUMBER_BAR, ui, props);
    }
    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

impl WidgetType for VoltageView {
    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        scale::show(
            self,
            ui,
            rect,
            widget,
            live,
            (VOLTAGE_VIEW.min, VOLTAGE_VIEW.max),
        );
        None
    }
    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        scale::show_body(
            self,
            ui,
            rect,
            widget,
            live,
            (VOLTAGE_VIEW.min, VOLTAGE_VIEW.max),
        );
        None
    }
    fn traced(&self, widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
        stats::traced(widget)
    }
    fn resettable(&self, widget: &WidgetContainer) -> Option<Reset> {
        stats::reset(widget)
    }
    fn names(&self) -> &'static [&'static str] {
        &["Voltage View"]
    }
    fn accepts(&self, _: &str) -> bool {
        false
    }
    fn shows(&self, nt_type: &str) -> bool {
        is_number(nt_type)
    }
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        draw_bar(&VOLTAGE_VIEW, painter, visuals, rect, props, value);
    }
    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, _: ValueInfo<'_>) {
        settings_rows(&VOLTAGE_VIEW, ui, props);
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
    fn the_bar_fills_between_its_ends() {
        assert_eq!(fraction(-1.0, -1.0, 1.0), 0.0);
        assert_eq!(fraction(12.0, 4.0, 13.0), 8.0 / 9.0);
        assert_eq!(fraction(-50.0, -1.0, 1.0), 0.0);
        // A backwards range draws an empty bar rather than nonsense.
        assert_eq!(fraction(0.5, 1.0, -1.0), 0.0);
    }

    #[test]
    fn labels_are_evenly_spaced() {
        assert_eq!(labels(4.0, 13.0, 4), [4.0, 7.0, 10.0, 13.0]);
        // At least the two ends.
        assert_eq!(labels(0.0, 1.0, 1), [0.0, 1.0]);
        let none = Map::new();
        assert_eq!(scale::value_with_unit(12.345, false, &none, "V"), "12.35 V");
        assert_eq!(scale::value_with_unit(3.0, true, &none, ""), "3");
    }
}
