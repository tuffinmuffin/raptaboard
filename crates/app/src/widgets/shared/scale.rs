//! Settings shared by widgets that show a number on a scale (Number Bar,
//! Voltage View, Radial Gauge): the fill color, solid or blended by value;
//! a unit after the value; showing the value and the scale's labels; and
//! whether the value is past either end of the scale.
//!
//! All are raptaboard's own settings; Elastic ignores them.

use eframe::egui::{self, Color32, Pos2};
use serde_json::{Map, Value as Json};

use super::props::{self, set_property};

/// The fill color when it doesn't change with the value, as ARGB.
pub const FILL_COLOR: &str = "fill_color";
/// `true` blends the fill color by value, from `low_color` through
/// `mid_color` (if set) to `high_color`.
pub const GRADIENT: &str = "gradient";
/// The fill color at the scale's minimum, with `gradient`, as ARGB.
pub const LOW_COLOR: &str = "low_color";
/// The fill color at `mid_value`, with `gradient`, as ARGB. Not set: no
/// middle color.
pub const MID_COLOR: &str = "mid_color";
/// Where the middle color sits on the scale; the middle of the range if
/// not set.
pub const MID_VALUE: &str = "mid_value";
/// The fill color at the scale's maximum, with `gradient`, as ARGB.
pub const HIGH_COLOR: &str = "high_color";
/// Text after the value, such as `V` or `m/s`.
pub const UNIT: &str = "unit";
/// `false` hides the scale's labels (the numbers along it).
pub const SHOW_LABELS: &str = "show_labels";
/// `false` hides the current value (and its unit), leaving just the scale.
pub const SHOW_VALUE: &str = "show_value";
/// How thick the bar or the gauge's track is, in points.
pub const THICKNESS: &str = "thickness";

/// The thickest a bar or track may be, in points.
pub const MAX_THICKNESS: f64 = 200.0;

/// Every setting key in this file.
pub const KEYS: [&str; 10] = [
    THICKNESS,
    SHOW_VALUE,
    FILL_COLOR,
    GRADIENT,
    LOW_COLOR,
    MID_COLOR,
    MID_VALUE,
    HIGH_COLOR,
    UNIT,
    SHOW_LABELS,
];

/// The gradient's default low color (Material red 500).
pub const DEFAULT_LOW: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);
/// The middle color offered when one is first added (Material yellow 500).
pub const DEFAULT_MID: Color32 = Color32::from_rgb(0xFF, 0xEB, 0x3B);
/// The gradient's default high color (Material green 500).
pub const DEFAULT_HIGH: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
/// Color of the dot shown when the value is past an end of the scale.
pub const SATURATED: Color32 = Color32::from_rgb(0xFF, 0x98, 0x00);

/// `a` blended toward `b` by `t` (0 is `a`, 1 is `b`).
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(ch(ar, br), ch(ag, bg), ch(ab, bb), ch(aa, ba))
}

/// The fill color for `value` on a scale from `min` to `max`: the solid
/// `fill_color` (or `default`), or with `gradient`, blended from the low
/// color through the middle color (if set) to the high color.
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::shared::scale::{fill_color, DEFAULT_HIGH, DEFAULT_LOW};
/// let solid = serde_json::Map::new();
/// assert_eq!(fill_color(&solid, 5.0, 0.0, 10.0, Color32::BLUE), Color32::BLUE);
/// let blend = serde_json::json!({"gradient": true});
/// let blend = blend.as_object().unwrap();
/// assert_eq!(fill_color(blend, 0.0, 0.0, 10.0, Color32::BLUE), DEFAULT_LOW);
/// assert_eq!(fill_color(blend, 10.0, 0.0, 10.0, Color32::BLUE), DEFAULT_HIGH);
/// ```
pub fn fill_color(
    props: &Map<String, Json>,
    value: f64,
    min: f64,
    max: f64,
    default: Color32,
) -> Color32 {
    if !props::flag(props, GRADIENT, false) {
        return props::color_property(props, FILL_COLOR).unwrap_or(default);
    }
    let low = props::color_property(props, LOW_COLOR).unwrap_or(DEFAULT_LOW);
    let high = props::color_property(props, HIGH_COLOR).unwrap_or(DEFAULT_HIGH);
    let along = |from: f64, to: f64| {
        if to > from {
            ((value - from) / (to - from)) as f32
        } else {
            0.0
        }
    };
    match props::color_property(props, MID_COLOR) {
        None => mix(low, high, along(min, max)),
        Some(mid) => {
            let middle = props::number(props, MID_VALUE, (min + max) / 2.0).clamp(min, max);
            if value <= middle {
                mix(low, mid, along(min, middle))
            } else {
                mix(mid, high, along(middle, max))
            }
        }
    }
}

/// The value as text with the `unit` setting after it: `12.40 V`. Whole
/// (int) values have no decimal places, as Elastic shows them.
///
/// ```
/// use raptaboard::widgets::shared::scale::value_with_unit;
/// let volts = serde_json::json!({"unit": "V"});
/// assert_eq!(value_with_unit(12.4, false, volts.as_object().unwrap(), ""), "12.40 V");
/// assert_eq!(value_with_unit(3.0, true, &serde_json::Map::new(), ""), "3");
/// ```
pub fn value_with_unit(
    value: f64,
    whole: bool,
    props: &Map<String, Json>,
    default: &str,
) -> String {
    let number = if whole {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    };
    match props::text(props, UNIT, default).trim() {
        "" => number,
        unit => format!("{number} {unit}"),
    }
}

/// The `thickness` setting in points, between 1 and [`MAX_THICKNESS`];
/// `default` if not set.
///
/// ```
/// use raptaboard::widgets::shared::scale::thickness;
/// let fat = serde_json::json!({"thickness": 40});
/// assert_eq!(thickness(fat.as_object().unwrap(), 8.0), 40.0);
/// assert_eq!(thickness(&serde_json::Map::new(), 8.0), 8.0);
/// ```
pub fn thickness(props: &Map<String, Json>, default: f32) -> f32 {
    props::number(props, THICKNESS, f64::from(default)).clamp(1.0, MAX_THICKNESS) as f32
}

/// Whether the current value shows (`show_value`, on unless turned off).
pub fn show_value(props: &Map<String, Json>) -> bool {
    props::flag(props, SHOW_VALUE, true)
}

/// Whether the scale's labels show (`show_labels`, on unless turned off).
pub fn show_labels(props: &Map<String, Json>) -> bool {
    props::flag(props, SHOW_LABELS, true)
}

/// Which end of the scale `value` is past, if either: the scale shows it
/// stuck at that end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Past {
    /// Below the minimum.
    Min,
    /// Above the maximum.
    Max,
}

/// Which end of `min` to `max` `value` is past, if either.
///
/// ```
/// use raptaboard::widgets::shared::scale::{past_end, Past};
/// assert_eq!(past_end(1.5, -1.0, 1.0), Some(Past::Max));
/// assert_eq!(past_end(1.0, -1.0, 1.0), None);
/// ```
pub fn past_end(value: f64, min: f64, max: f64) -> Option<Past> {
    if value < min.min(max) {
        Some(Past::Min)
    } else if value > max.max(min) {
        Some(Past::Max)
    } else {
        None
    }
}

/// Draw the "past the end" dot at `at`.
pub fn draw_past_dot(painter: &egui::Painter, at: Pos2) {
    painter.circle_filled(at, 3.5, SATURATED);
}

/// The settings dialog rows for these settings: unit, labels, and the
/// fill color (solid, or blended low → middle → high). `default` is the
/// solid color when none is set.
pub fn rows(
    ui: &mut egui::Ui,
    p: &mut Map<String, Json>,
    default_unit: &str,
    default: Color32,
    default_thickness: f32,
) {
    props::number_row(
        ui,
        p,
        "Thickness (pt)",
        THICKNESS,
        f64::from(default_thickness),
        1.0..=MAX_THICKNESS,
    );
    ui.label("Unit");
    let mut unit = props::text(p, UNIT, default_unit).to_owned();
    if ui
        .add(
            egui::TextEdit::singleline(&mut unit)
                .hint_text("none")
                .desired_width(80.0),
        )
        .changed()
    {
        set_property(p, UNIT, (unit != default_unit).then(|| unit.into()));
    }
    ui.end_row();

    props::flag_row(ui, p, "Value", "Show the value and unit", SHOW_VALUE, true);
    props::flag_row(
        ui,
        p,
        "Labels",
        "Show the numbers along the scale",
        SHOW_LABELS,
        true,
    );

    ui.label("Fill");
    let mut gradient = props::flag(p, GRADIENT, false);
    if ui
        .checkbox(&mut gradient, "Change color with the value")
        .changed()
    {
        set_property(p, GRADIENT, gradient.then_some(true.into()));
    }
    ui.end_row();
    if !gradient {
        props::color_row(ui, p, "Color", FILL_COLOR, default);
        return;
    }
    props::color_row(ui, p, "Low color", LOW_COLOR, DEFAULT_LOW);
    ui.label("Middle color");
    ui.horizontal(|ui| {
        let mut middle = p.contains_key(MID_COLOR);
        if ui.checkbox(&mut middle, "").changed() {
            set_property(p, MID_COLOR, middle.then(|| props::color_json(DEFAULT_MID)));
        }
        if middle {
            let mut color = props::color_property(p, MID_COLOR).unwrap_or(DEFAULT_MID);
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut color,
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
            {
                set_property(p, MID_COLOR, Some(props::color_json(color)));
            }
            ui.label("at");
            let mut at = props::number(p, MID_VALUE, f64::NAN);
            let mut typed = if at.is_nan() { 0.0 } else { at };
            if ui
                .add(egui::DragValue::new(&mut typed).speed(0.1))
                .on_hover_text("Where on the scale the middle color is; the middle if not set")
                .changed()
            {
                at = typed;
                set_property(p, MID_VALUE, Some(at.into()));
            }
        }
    });
    ui.end_row();
    props::color_row(ui, p, "High color", HIGH_COLOR, DEFAULT_HIGH);
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn props(json: Json) -> Map<String, Json> {
        json.as_object().unwrap().clone()
    }

    #[test]
    fn gradients_blend_through_the_middle() {
        let p = props(serde_json::json!({
            "gradient": true,
            "low_color": props::color_json(Color32::from_rgb(0, 0, 0)),
            "mid_color": props::color_json(Color32::from_rgb(100, 100, 100)),
            "mid_value": 2.0,
            "high_color": props::color_json(Color32::from_rgb(200, 200, 200)),
        }));
        let at = |v| fill_color(&p, v, 0.0, 10.0, Color32::BLUE);
        assert_eq!(at(0.0), Color32::from_rgb(0, 0, 0));
        assert_eq!(at(1.0), Color32::from_rgb(50, 50, 50));
        assert_eq!(at(2.0), Color32::from_rgb(100, 100, 100));
        assert_eq!(at(6.0), Color32::from_rgb(150, 150, 150));
        // Past the ends it stays at the end colors.
        assert_eq!(at(-5.0), Color32::from_rgb(0, 0, 0));
        assert_eq!(at(50.0), Color32::from_rgb(200, 200, 200));
        // Without a middle color, low to high straight.
        let two = props(serde_json::json!({"gradient": true}));
        let half = fill_color(&two, 5.0, 0.0, 10.0, Color32::BLUE);
        assert_eq!(half, mix(DEFAULT_LOW, DEFAULT_HIGH, 0.5));
        // A solid color when not blending.
        let solid = props(serde_json::json!({"fill_color": props::color_json(Color32::RED)}));
        assert_eq!(
            fill_color(&solid, 1.0, 0.0, 10.0, Color32::BLUE),
            Color32::RED
        );
    }

    #[test]
    fn units_and_ends() {
        let none = Map::new();
        assert_eq!(value_with_unit(4.0, false, &none, "V"), "4.00 V");
        let blank = props(serde_json::json!({"unit": ""}));
        assert_eq!(value_with_unit(4.0, false, &blank, "V"), "4.00");
        assert_eq!(past_end(-2.0, -1.0, 1.0), Some(Past::Min));
        // A backwards range still knows its ends.
        assert_eq!(past_end(5.0, 1.0, -1.0), Some(Past::Max));
        assert!(show_labels(&none));
    }
}
