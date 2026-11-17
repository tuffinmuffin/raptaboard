//! Widgets drawn on the grid: Text Display and Boolean Box so far.
//!
//! Each widget is a card with its title on top and a body that shows the
//! bound topic's newest value. Widget types use Elastic's exact names
//! (`"Text Display"`, `"Boolean Box"`) so Elastic layouts open unchanged.
//! Types that aren't built yet are drawn as a placeholder card.
//!
//! Widgets are read-only for now: typing into a Text Display to publish a
//! value comes later.

use eframe::egui::{self, Color32, CornerRadius, Rect, Stroke, StrokeKind};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::Value as Json;

use crate::tree::format_number;

/// Elastic's default Boolean Box colors (Material green 500 and red 500).
pub const DEFAULT_TRUE_COLOR: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
/// See [`DEFAULT_TRUE_COLOR`].
pub const DEFAULT_FALSE_COLOR: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);

/// Height of a card's title strip, in points.
pub const TITLE_HEIGHT: f32 = 24.0;

/// The widget types raptaboard can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    /// Shows any number, string or array as text.
    TextDisplay,
    /// Fills with one color when the boolean is true and another when false.
    BooleanBox,
}

impl WidgetKind {
    /// The kind for an Elastic widget `type` string, including Elastic's
    /// aliases. `None` for types not built yet.
    ///
    /// ```
    /// use raptaboard::widgets::WidgetKind;
    /// assert_eq!(WidgetKind::from_type("Text View"), Some(WidgetKind::TextDisplay));
    /// assert_eq!(WidgetKind::from_type("Boolean Box"), Some(WidgetKind::BooleanBox));
    /// assert_eq!(WidgetKind::from_type("Graph"), None);
    /// ```
    pub fn from_type(ty: &str) -> Option<Self> {
        match ty {
            "Text Display" | "Text View" => Some(WidgetKind::TextDisplay),
            "Boolean Box" => Some(WidgetKind::BooleanBox),
            _ => None,
        }
    }
}

/// What a widget body shows for a value. Kept apart from drawing so it can
/// be tested without a window.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// Text, as in a Text Display.
    Text(String),
    /// A solid color, as in a Boolean Box.
    Fill(Color32),
    /// No value yet, or a value of the wrong type: says why in grey.
    NoData(&'static str),
}

/// Decide what `widget` shows for the newest value of its topic.
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::{body, Body, WidgetKind, DEFAULT_TRUE_COLOR};
/// let props = serde_json::Map::new();
/// assert_eq!(
///     body(WidgetKind::TextDisplay, &props, Some(&Value::Double(2.5))),
///     Body::Text("2.5".into())
/// );
/// assert_eq!(
///     body(WidgetKind::BooleanBox, &props, Some(&Value::Boolean(true))),
///     Body::Fill(DEFAULT_TRUE_COLOR)
/// );
/// assert_eq!(body(WidgetKind::BooleanBox, &props, None), Body::NoData("no value"));
/// ```
pub fn body(
    kind: WidgetKind,
    properties: &serde_json::Map<String, Json>,
    value: Option<&Value>,
) -> Body {
    let Some(value) = value else {
        return Body::NoData("no value");
    };
    match kind {
        WidgetKind::TextDisplay => match text_of(value) {
            Some(text) => Body::Text(text),
            None => Body::NoData("not text"),
        },
        WidgetKind::BooleanBox => match value {
            Value::Boolean(true) => {
                Body::Fill(color_property(properties, "true_color").unwrap_or(DEFAULT_TRUE_COLOR))
            }
            Value::Boolean(false) => {
                Body::Fill(color_property(properties, "false_color").unwrap_or(DEFAULT_FALSE_COLOR))
            }
            _ => Body::NoData("not a boolean"),
        },
    }
}

/// A value as a Text Display shows it: strings without quotes, numbers with
/// up to 4 decimal places, arrays as `[a, b, c]`. Raw bytes (such as struct
/// values) have no text form.
fn text_of(value: &Value) -> Option<String> {
    fn list<T>(items: &[T], one: impl Fn(&T) -> String) -> String {
        format!("[{}]", items.iter().map(one).collect::<Vec<_>>().join(", "))
    }
    Some(match value {
        Value::Boolean(b) => b.to_string(),
        Value::Double(d) => format_number(*d),
        Value::Float(f) => format_number(f64::from(*f)),
        Value::Int(i) => i.to_string(),
        Value::String(s) => s.clone(),
        Value::BooleanArray(a) => list(a, |b| b.to_string()),
        Value::DoubleArray(a) => list(a, |d| format_number(*d)),
        Value::FloatArray(a) => list(a, |f| format_number(f64::from(*f))),
        Value::IntArray(a) => list(a, |i| i.to_string()),
        Value::StringArray(a) => list(a, |s| s.clone()),
        Value::Raw(_) => return None,
    })
}

/// Read a color saved by Elastic: a 32-bit ARGB number, such as
/// `4283215696` (`0xFF4CAF50`, green).
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::color_property;
/// let props = serde_json::json!({"true_color": 4283215696u32});
/// let props = props.as_object().unwrap();
/// assert_eq!(color_property(props, "true_color"), Some(Color32::from_rgb(0x4C, 0xAF, 0x50)));
/// assert_eq!(color_property(props, "false_color"), None);
/// ```
pub fn color_property(properties: &serde_json::Map<String, Json>, key: &str) -> Option<Color32> {
    let argb = u32::try_from(properties.get(key)?.as_u64()?).ok()?;
    let [a, r, g, b] = argb.to_be_bytes();
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// Draw an empty card with `title` in a strip along the top, and return
/// the space left below the title for the body. Also used for List Layouts.
pub fn draw_card(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    title: &str,
) -> Rect {
    painter.rect(
        rect,
        CornerRadius::same(10),
        visuals.extreme_bg_color,
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );
    let (title_rect, body_rect) = rect.split_top_bottom_at_y(rect.min.y + TITLE_HEIGHT);
    painter.rect_filled(
        title_rect.shrink(1.0),
        CornerRadius {
            nw: 9,
            ne: 9,
            sw: 0,
            se: 0,
        },
        visuals.faint_bg_color,
    );
    painter
        .with_clip_rect(title_rect.intersect(painter.clip_rect()))
        .text(
            title_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(14.0),
            visuals.text_color(),
        );
    body_rect
}

/// Draw a widget card: a rounded box, its title on top, and its body.
/// `value` is the newest value of the widget's topic, if any.
pub fn draw(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    widget: &WidgetContainer,
    value: Option<&Value>,
) {
    let body_rect = draw_card(painter, visuals, rect, &widget.title);
    // Keep long text inside the card.
    let painter = painter.with_clip_rect(body_rect.intersect(painter.clip_rect()));
    let body_rect = body_rect.shrink(6.0);
    let Some(kind) = WidgetKind::from_type(&widget.widget_type) else {
        // Not built yet: say what it is and what it's bound to.
        let font = egui::FontId::proportional(12.0);
        let center = body_rect.center();
        for (line, dy) in [
            (widget.widget_type.as_str(), -8.0),
            (widget.topic().unwrap_or(""), 8.0),
        ] {
            painter.text(
                center + egui::vec2(0.0, dy),
                egui::Align2::CENTER_CENTER,
                line,
                font.clone(),
                visuals.weak_text_color(),
            );
        }
        return;
    };
    match body(kind, &widget.properties, value) {
        Body::Text(text) => {
            painter.text(
                body_rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::monospace(16.0),
                visuals.text_color(),
            );
        }
        Body::Fill(color) => {
            painter.rect_filled(body_rect, CornerRadius::same(6), color);
        }
        Body::NoData(why) => {
            painter.text(
                body_rect.center(),
                egui::Align2::CENTER_CENTER,
                why,
                egui::FontId::proportional(12.0),
                visuals.weak_text_color(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn props(json: serde_json::Value) -> serde_json::Map<String, Json> {
        json.as_object().unwrap().clone()
    }

    #[test]
    fn text_display_formats_each_type() {
        let none = props(serde_json::json!({}));
        let text = |v: Value| body(WidgetKind::TextDisplay, &none, Some(&v));
        assert_eq!(
            text(Value::String("Blue 1".into())),
            Body::Text("Blue 1".into())
        );
        assert_eq!(text(Value::Double(1.0 / 3.0)), Body::Text("0.3333".into()));
        assert_eq!(text(Value::Int(-4)), Body::Text("-4".into()));
        assert_eq!(text(Value::Boolean(false)), Body::Text("false".into()));
        assert_eq!(
            text(Value::DoubleArray(vec![1.0, 2.5])),
            Body::Text("[1, 2.5]".into())
        );
        assert_eq!(text(Value::Raw(vec![0; 8])), Body::NoData("not text"));
        assert_eq!(
            body(WidgetKind::TextDisplay, &none, None),
            Body::NoData("no value")
        );
    }

    #[test]
    fn boolean_box_uses_saved_colors() {
        // The colors from crates/layout/tests/data/elastic_sample.json.
        let saved =
            props(serde_json::json!({"true_color": 4283215696u32, "false_color": 4294198070u32}));
        assert_eq!(
            body(WidgetKind::BooleanBox, &saved, Some(&Value::Boolean(true))),
            Body::Fill(DEFAULT_TRUE_COLOR)
        );
        assert_eq!(
            body(WidgetKind::BooleanBox, &saved, Some(&Value::Boolean(false))),
            Body::Fill(DEFAULT_FALSE_COLOR)
        );
        let blue = props(serde_json::json!({"true_color": 0xFF2196F3u32}));
        assert_eq!(
            body(WidgetKind::BooleanBox, &blue, Some(&Value::Boolean(true))),
            Body::Fill(Color32::from_rgb(0x21, 0x96, 0xF3))
        );
        assert_eq!(
            body(WidgetKind::BooleanBox, &blue, Some(&Value::Double(1.0))),
            Body::NoData("not a boolean")
        );
    }

    #[test]
    fn bad_colors_fall_back_to_defaults() {
        let bad = props(serde_json::json!({"true_color": "green", "false_color": -1}));
        assert_eq!(color_property(&bad, "true_color"), None);
        assert_eq!(color_property(&bad, "false_color"), None);
        assert_eq!(
            body(WidgetKind::BooleanBox, &bad, Some(&Value::Boolean(false))),
            Body::Fill(DEFAULT_FALSE_COLOR)
        );
    }
}
