//! Color by value (raptaboard): a color picked by a number's value, for a
//! text widget's background or a bar's or gauge's fill. `color_by` says
//! how:
//!
//! - `thresholds`: the low color below `color_below`, the high color above
//!   `color_above`, the middle color between (10 and 20 if not set: below
//!   10 red, 10 to 20 yellow, above green);
//! - `gradient`: blended from the low color at `color_below`, through the
//!   middle color halfway, to the high color at `color_above`.
//!
//! The colors are the scale's `low_color`, `mid_color` and `high_color`
//! (red, yellow and green if not set), shared with the gauges' gradient.

use eframe::egui::{self, Color32};
use serde_json::{Map, Value as Json};

use super::props::{self, set_property};
use super::scale::{DEFAULT_HIGH, DEFAULT_LOW, DEFAULT_MID, HIGH_COLOR, LOW_COLOR, MID_COLOR};

/// How the color is picked: `thresholds` or `gradient` (off if not set).
pub const COLOR_BY: &str = "color_by";
/// Below this the low color (or where the gradient starts).
pub const COLOR_BELOW: &str = "color_below";
/// Above this the high color (or where the gradient ends).
pub const COLOR_ABOVE: &str = "color_above";

/// [`COLOR_BY`]'s values.
pub const THRESHOLDS: &str = "thresholds";
/// [`COLOR_BY`]'s values.
pub const GRADIENT: &str = "gradient";

/// The keys here and the colors they use, for a widget's setting keys.
pub const KEYS: [&str; 6] = [
    COLOR_BY,
    COLOR_BELOW,
    COLOR_ABOVE,
    LOW_COLOR,
    MID_COLOR,
    HIGH_COLOR,
];

const DEFAULT_BELOW: f64 = 10.0;
const DEFAULT_ABOVE: f64 = 20.0;

/// `a` blended toward `b` by `t` (0 is `a`, 1 is `b`).
fn mix(a: Color32, b: Color32, t: f64) -> Color32 {
    let t = t.clamp(0.0, 1.0) as f32;
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    let [ar, ag, ab, _] = a.to_srgba_unmultiplied();
    let [br, bg, bb, _] = b.to_srgba_unmultiplied();
    Color32::from_rgb(ch(ar, br), ch(ag, bg), ch(ab, bb))
}

/// Whether `p` turns color by value on.
pub fn enabled(p: &Map<String, Json>) -> bool {
    matches!(props::text(p, COLOR_BY, ""), THRESHOLDS | GRADIENT)
}

/// The color for `value` with settings `p`, or `None` if color by value is
/// off.
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::shared::value_colors::color_for;
/// use raptaboard::widgets::shared::scale::{DEFAULT_HIGH, DEFAULT_LOW, DEFAULT_MID};
/// let p = serde_json::json!({"color_by": "thresholds"});
/// let p = p.as_object().unwrap();
/// assert_eq!(color_for(p, 5.0), Some(DEFAULT_LOW));
/// assert_eq!(color_for(p, 15.0), Some(DEFAULT_MID));
/// assert_eq!(color_for(p, 25.0), Some(DEFAULT_HIGH));
/// assert_eq!(color_for(&serde_json::Map::new(), 5.0), None);
/// ```
pub fn color_for(p: &Map<String, Json>, value: f64) -> Option<Color32> {
    let below = props::number(p, COLOR_BELOW, DEFAULT_BELOW);
    let above = props::number(p, COLOR_ABOVE, DEFAULT_ABOVE);
    let low = props::color_property(p, LOW_COLOR).unwrap_or(DEFAULT_LOW);
    let mid = props::color_property(p, MID_COLOR).unwrap_or(DEFAULT_MID);
    let high = props::color_property(p, HIGH_COLOR).unwrap_or(DEFAULT_HIGH);
    match props::text(p, COLOR_BY, "") {
        THRESHOLDS => Some(if value < below {
            low
        } else if value > above {
            high
        } else {
            mid
        }),
        GRADIENT => {
            if above <= below {
                return Some(if value < below { low } else { high });
            }
            let t = (value - below) / (above - below);
            Some(if t <= 0.5 {
                mix(low, mid, t * 2.0)
            } else {
                mix(mid, high, (t - 0.5) * 2.0)
            })
        }
        _ => None,
    }
}

/// Fill `rect` with the color for `value` (a number) if color by value is
/// on in `p`, and return the text color that reads on it (black or white).
/// `None`, drawing nothing, if it's off or `value` isn't a number.
pub fn paint_background(
    painter: &egui::Painter,
    rect: egui::Rect,
    p: &Map<String, Json>,
    value: Option<&nt4::Value>,
) -> Option<Color32> {
    let n = value.and_then(super::format::number_of)?;
    let color = color_for(p, n)?;
    painter.rect_filled(
        rect,
        egui::CornerRadius::same(super::card::BODY_CORNER_RADIUS),
        color,
    );
    Some(props::text_color_on(color))
}

/// The settings dialog rows: how (off, thresholds, gradient), the two
/// numbers and the three colors.
pub fn rows(ui: &mut egui::Ui, p: &mut Map<String, Json>) {
    let row_name = ui.label("Color by value").id;
    let current = props::text(p, COLOR_BY, "").to_owned();
    let mut picked = current.clone();
    let name = |v: &str| match v {
        THRESHOLDS => "Thresholds",
        GRADIENT => "Gradient",
        _ => "Off",
    };
    egui::ComboBox::from_id_salt(COLOR_BY)
        .selected_text(name(&picked))
        .show_ui(ui, |ui| {
            for v in ["", THRESHOLDS, GRADIENT] {
                ui.selectable_value(&mut picked, v.to_owned(), name(v));
            }
        })
        .response
        .labelled_by(row_name)
        .on_hover_text(
            "Thresholds: low color below the first number, high above the second, \
             middle between. Gradient: blended from low to high between them.",
        );
    if picked != current {
        set_property(p, COLOR_BY, (!picked.is_empty()).then(|| picked.into()));
    }
    ui.end_row();
    if !enabled(p) {
        return;
    }
    props::number_row(ui, p, "Low below", COLOR_BELOW, DEFAULT_BELOW, -1e9..=1e9);
    props::number_row(ui, p, "High above", COLOR_ABOVE, DEFAULT_ABOVE, -1e9..=1e9);
    props::color_row(ui, p, "Low color", LOW_COLOR, DEFAULT_LOW);
    props::color_row(ui, p, "Middle color", MID_COLOR, DEFAULT_MID);
    props::color_row(ui, p, "High color", HIGH_COLOR, DEFAULT_HIGH);
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn p(json: Json) -> Map<String, Json> {
        json.as_object().unwrap().clone()
    }

    #[test]
    fn thresholds_and_gradients_pick_colors_from_the_settings() {
        // Thresholds set and colors set.
        let t = p(serde_json::json!({
            "color_by": "thresholds", "color_below": 0.0, "color_above": 1.0,
            "low_color": 0xFF0000FFu32, "high_color": 0xFFFFFFFFu32
        }));
        assert_eq!(color_for(&t, -0.5), Some(Color32::from_rgb(0, 0, 0xFF)));
        assert_eq!(color_for(&t, 0.5), Some(DEFAULT_MID));
        assert_eq!(color_for(&t, 1.0), Some(DEFAULT_MID), "at the edge: middle");
        assert_eq!(color_for(&t, 1.5), Some(Color32::WHITE));
        // A gradient: the ends and halfway are the three colors.
        let g = p(
            serde_json::json!({"color_by": "gradient", "color_below": 0.0, "color_above": 100.0}),
        );
        assert_eq!(color_for(&g, -10.0), Some(DEFAULT_LOW));
        assert_eq!(color_for(&g, 50.0), Some(DEFAULT_MID));
        assert_eq!(color_for(&g, 100.0), Some(DEFAULT_HIGH));
        let quarter = color_for(&g, 25.0).unwrap();
        assert!(
            quarter != DEFAULT_LOW && quarter != DEFAULT_MID,
            "{quarter:?}"
        );
        // Backwards numbers don't divide by zero.
        let odd =
            p(serde_json::json!({"color_by": "gradient", "color_below": 5.0, "color_above": 5.0}));
        assert_eq!(color_for(&odd, 4.0), Some(DEFAULT_LOW));
        // Anything else is off.
        assert_eq!(
            color_for(&p(serde_json::json!({"color_by": "rainbow"})), 1.0),
            None
        );
    }
}
