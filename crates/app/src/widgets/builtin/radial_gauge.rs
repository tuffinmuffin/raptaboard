//! Radial Gauge (also saved as Simple Dial): a number on a round dial,
//! with a filled arc, a needle, labels around the edge and the value in
//! the middle.
//!
//! Settings are Elastic's: `min_value` and `max_value` (0 to 100),
//! `start_angle` and `end_angle` (where the dial starts and ends, in
//! degrees clockwise from straight up: -140 to 140), `number_of_labels`
//! (8), `wrap_value` (wrap values outside the range around, as for an
//! angle), `show_pointer` (the needle) and `show_ticks`; and raptaboard's
//! [`scale`] settings: the arc's color (solid or by value), a unit, and
//! labels on or off. A dot past an end of the dial shows the value is
//! beyond it.

use std::f32::consts::PI;

use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use nt4::Value;
use serde_json::{Map, Value as Json};

use crate::widgets::shared::format::{number_of, short_number};
use crate::widgets::shared::{card, props, scale};
use crate::widgets::{ValueInfo, WidgetType};

/// The value at the dial's start (Elastic).
pub const MIN_VALUE: &str = "min_value";
/// The value at the dial's end (Elastic).
pub const MAX_VALUE: &str = "max_value";
/// Where the dial starts, in degrees clockwise from straight up (Elastic).
pub const START_ANGLE: &str = "start_angle";
/// Where the dial ends, in degrees clockwise from straight up (Elastic).
pub const END_ANGLE: &str = "end_angle";
/// How many steps between labels around the dial (Elastic).
pub const NUMBER_OF_LABELS: &str = "number_of_labels";
/// Wrap values outside the range back into it (Elastic).
pub const WRAP_VALUE: &str = "wrap_value";
/// Draw the needle (Elastic).
pub const SHOW_POINTER: &str = "show_pointer";
/// Draw tick marks at the labels (Elastic).
pub const SHOW_TICKS: &str = "show_ticks";

const KEYS: [&str; 17] = [
    scale::SHOW_VALUE,
    MIN_VALUE,
    MAX_VALUE,
    START_ANGLE,
    END_ANGLE,
    NUMBER_OF_LABELS,
    WRAP_VALUE,
    SHOW_POINTER,
    SHOW_TICKS,
    scale::FILL_COLOR,
    scale::GRADIENT,
    scale::LOW_COLOR,
    scale::MID_COLOR,
    scale::MID_VALUE,
    scale::HIGH_COLOR,
    scale::UNIT,
    scale::SHOW_LABELS,
];

/// The value arc's color when none is set (Material blue 500).
const DEFAULT_FILL: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);

/// Elastic's defaults.
const DEFAULT_MIN: f64 = 0.0;
const DEFAULT_MAX: f64 = 100.0;
const DEFAULT_START: f64 = -140.0;
const DEFAULT_END: f64 = 140.0;
const DEFAULT_LABELS: i64 = 8;

/// Thickness of the dial's track, in points.
const TRACK_WIDTH: f32 = 6.0;
/// Room left outside the track for labels, in points.
const LABEL_ROOM: f32 = 14.0;
/// Size of the labels, in points.
const LABEL_SIZE: f32 = 10.0;
/// Size of the value text, in points.
const VALUE_SIZE: f32 = 16.0;
/// How far past the dial's end the "past the end" dot sits, in degrees.
const PAST_DOT_DEGREES: f64 = 8.0;

/// The Radial Gauge widget (Elastic's `"Radial Gauge"`, also `"Simple Dial"`).
pub struct RadialGauge;

/// `value` wrapped into `min` to `max`, as for an angle: 370 between 0 and
/// 360 is 10.
///
/// ```
/// use raptaboard::widgets::builtin::radial_gauge::wrapped;
/// assert_eq!(wrapped(370.0, 0.0, 360.0), 10.0);
/// assert_eq!(wrapped(-90.0, -180.0, 180.0), -90.0);
/// assert_eq!(wrapped(190.0, -180.0, 180.0), -170.0);
/// ```
pub fn wrapped(value: f64, min: f64, max: f64) -> f64 {
    let span = max - min;
    if span <= 0.0 {
        return value;
    }
    min + (value - min).rem_euclid(span)
}

/// The dial angle (degrees clockwise from straight up) for `value`, which
/// is first kept between `min` and `max`.
///
/// ```
/// use raptaboard::widgets::builtin::radial_gauge::angle_for;
/// assert_eq!(angle_for(50.0, 0.0, 100.0, -140.0, 140.0), 0.0);
/// assert_eq!(angle_for(500.0, 0.0, 100.0, -140.0, 140.0), 140.0);
/// ```
pub fn angle_for(value: f64, min: f64, max: f64, start: f64, end: f64) -> f64 {
    if max <= min {
        return start;
    }
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    start + (end - start) * t
}

/// The values labelled around the dial: `steps` equal steps from `min` to
/// `max`, both ends included, except the end when the dial is a full
/// circle (it would sit on the start).
///
/// ```
/// use raptaboard::widgets::builtin::radial_gauge::label_values;
/// assert_eq!(label_values(0.0, 100.0, 4, false), [0.0, 25.0, 50.0, 75.0, 100.0]);
/// assert_eq!(label_values(0.0, 360.0, 4, true), [0.0, 90.0, 180.0, 270.0]);
/// ```
pub fn label_values(min: f64, max: f64, steps: usize, full_circle: bool) -> Vec<f64> {
    let steps = steps.max(1);
    let last = if full_circle { steps - 1 } else { steps };
    (0..=last)
        .map(|i| min + (max - min) * i as f64 / steps as f64)
        .collect()
}

/// The point at `angle` (degrees clockwise from straight up) and `radius`
/// from `center`, on screen (y down).
fn point(center: Pos2, angle: f64, radius: f32) -> Pos2 {
    let a = angle as f32 * PI / 180.0;
    center + egui::vec2(a.sin() * radius, -a.cos() * radius)
}

/// Points along the arc from angle `from` to `to`, about every 3 degrees.
fn arc(center: Pos2, radius: f32, from: f64, to: f64) -> Vec<Pos2> {
    let n = (((to - from).abs() / 3.0).ceil() as usize).max(1);
    (0..=n)
        .map(|i| point(center, from + (to - from) * i as f64 / n as f64, radius))
        .collect()
}

impl WidgetType for RadialGauge {
    fn names(&self) -> &'static [&'static str] {
        &["Radial Gauge", "Simple Dial"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        matches!(nt_type, "double" | "float" | "int")
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        p: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        let Some(mut v) = value.and_then(number_of) else {
            let why = if value.is_some() {
                "not a number"
            } else {
                "no value"
            };
            return card::draw_message(painter, visuals, rect, why);
        };
        let min = props::number(p, MIN_VALUE, DEFAULT_MIN);
        let max = props::number(p, MAX_VALUE, DEFAULT_MAX);
        let start = props::number(p, START_ANGLE, DEFAULT_START);
        let end = props::number(p, END_ANGLE, DEFAULT_END);
        let steps =
            props::number(p, NUMBER_OF_LABELS, DEFAULT_LABELS as f64).clamp(1.0, 50.0) as usize;
        if props::flag(p, WRAP_VALUE, false) {
            v = wrapped(v, min, max);
        }
        let shown = v.clamp(min.min(max), max.max(min));
        let center = rect.center();
        let radius = (rect.width().min(rect.height()) / 2.0 - LABEL_ROOM).max(4.0);
        let weak = visuals.weak_text_color();
        let accent = scale::fill_color(p, v, min, max, DEFAULT_FILL);

        // The track, then the filled part: from the minimum, or from zero
        // when the range crosses it, as Elastic draws it.
        painter.line(
            arc(center, radius, start, end),
            Stroke::new(TRACK_WIDTH, visuals.widgets.inactive.bg_fill),
        );
        let origin = if min < 0.0 && max > 0.0 { 0.0 } else { min };
        let from = angle_for(origin, min, max, start, end);
        let to = angle_for(shown, min, max, start, end);
        painter.line(
            arc(center, radius, from, to),
            Stroke::new(TRACK_WIDTH, accent),
        );
        // Past an end: a dot just beyond that end of the track.
        if let Some(past) = scale::past_end(v, min, max) {
            let (end_angle, away) = match past {
                scale::Past::Min => (start, -(end - start).signum()),
                scale::Past::Max => (end, (end - start).signum()),
            };
            scale::draw_past_dot(
                painter,
                point(center, end_angle + away * PAST_DOT_DEGREES, radius),
            );
        }

        // Labels and ticks around the outside.
        let full = ((end - start).abs() - 360.0).abs() < 1e-6;
        let font = egui::FontId::proportional(LABEL_SIZE);
        let ticks = props::flag(p, SHOW_TICKS, true);
        let labels = scale::show_labels(p);
        for n in label_values(min, max, steps, full) {
            let a = angle_for(n, min, max, start, end);
            if ticks {
                painter.line_segment(
                    [
                        point(center, a, radius - TRACK_WIDTH),
                        point(center, a, radius - TRACK_WIDTH - 5.0),
                    ],
                    Stroke::new(1.0, weak),
                );
            }
            if labels {
                painter.text(
                    point(center, a, radius + LABEL_ROOM / 2.0 + 2.0),
                    egui::Align2::CENTER_CENTER,
                    short_number(n),
                    font.clone(),
                    weak,
                );
            }
        }

        // The needle and its hub.
        if props::flag(p, SHOW_POINTER, true) {
            let tip = point(center, to, radius * 0.75);
            painter.line_segment([center, tip], Stroke::new(3.0, visuals.text_color()));
            painter.circle_filled(center, radius * 0.08, weak);
        }

        // The value below the middle.
        if scale::show_value(p) {
            let whole = matches!(value, Some(Value::Int(_)));
            let text = scale::value_with_unit(v, whole, p, "");
            painter.text(
                center + egui::vec2(0.0, radius * 0.45),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::monospace(VALUE_SIZE),
                visuals.text_color(),
            );
        }
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::number_row(ui, p, "Minimum", MIN_VALUE, DEFAULT_MIN, -1e9..=1e9);
        props::number_row(ui, p, "Maximum", MAX_VALUE, DEFAULT_MAX, -1e9..=1e9);
        props::number_row(
            ui,
            p,
            "Start angle (°)",
            START_ANGLE,
            DEFAULT_START,
            -360.0..=360.0,
        );
        props::number_row(
            ui,
            p,
            "End angle (°)",
            END_ANGLE,
            DEFAULT_END,
            -360.0..=360.0,
        );
        props::whole_row(
            ui,
            p,
            "Label steps",
            NUMBER_OF_LABELS,
            DEFAULT_LABELS,
            1..=50,
        );
        props::flag_row(
            ui,
            p,
            "Wrap",
            "Wrap values outside the range (for angles)",
            WRAP_VALUE,
            false,
        );
        props::flag_row(ui, p, "Needle", "Show the needle", SHOW_POINTER, true);
        props::flag_row(ui, p, "Ticks", "Show tick marks", SHOW_TICKS, true);
        scale::rows(ui, p, "", DEFAULT_FILL);
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
    fn values_map_onto_the_dial() {
        assert_eq!(angle_for(0.0, 0.0, 100.0, -140.0, 140.0), -140.0);
        assert_eq!(angle_for(100.0, 0.0, 100.0, -140.0, 140.0), 140.0);
        assert_eq!(angle_for(-5.0, 0.0, 100.0, -140.0, 140.0), -140.0);
        // A dial that goes the other way round.
        assert_eq!(angle_for(25.0, 0.0, 100.0, 90.0, -90.0), 45.0);
        // A backwards range sits at the start.
        assert_eq!(angle_for(5.0, 10.0, 0.0, -140.0, 140.0), -140.0);
    }

    #[test]
    fn straight_up_is_angle_zero() {
        let c = egui::pos2(0.0, 0.0);
        let up = point(c, 0.0, 10.0);
        assert!((up.x).abs() < 1e-4 && (up.y + 10.0).abs() < 1e-4);
        let right = point(c, 90.0, 10.0);
        assert!((right.x - 10.0).abs() < 1e-4 && right.y.abs() < 1e-4);
        // An arc reaches both ends.
        let a = arc(c, 10.0, -90.0, 90.0);
        assert!((a[0].x + 10.0).abs() < 1e-4);
        assert!((a.last().unwrap().x - 10.0).abs() < 1e-4);
    }

    #[test]
    fn wrapping_and_labels() {
        assert_eq!(wrapped(-10.0, 0.0, 360.0), 350.0);
        assert_eq!(wrapped(5.0, 1.0, 1.0), 5.0);
        assert_eq!(label_values(0.0, 100.0, 8, false).len(), 9);
        assert_eq!(label_values(0.0, 10.0, 0, false), [0.0, 10.0]);
    }
}
