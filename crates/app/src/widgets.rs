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
use raptaboard_layout::{Rect as LayoutRect, StructMeta, WidgetContainer};
use serde_json::Value as Json;

use crate::struct_view::EnumDisplay;
use crate::tree::format_number;

/// Elastic's default Boolean Box colors (Material green 500 and red 500).
pub const DEFAULT_TRUE_COLOR: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
/// See [`DEFAULT_TRUE_COLOR`].
pub const DEFAULT_FALSE_COLOR: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);

/// Height of a card's title strip, in points.
pub const TITLE_HEIGHT: f32 = 24.0;

/// Size of a Text Display's value text when `font_size` isn't set, in points.
pub const DEFAULT_FONT_SIZE: f32 = 16.0;
/// The smallest `font_size` used, in points.
pub const MIN_FONT_SIZE: f32 = 8.0;
/// The largest `font_size` used, in points.
pub const MAX_FONT_SIZE: f32 = 96.0;
/// The most `decimal_places` used.
pub const MAX_DECIMAL_PLACES: usize = 10;

/// Widget setting keys. `true_color` and `false_color` are Elastic's; the
/// rest are raptaboard's own, which Elastic ignores.
pub mod keys {
    /// Update period in seconds (Elastic).
    pub const PERIOD: &str = "period";
    /// Boolean Box color when true, as an ARGB number (Elastic).
    pub const TRUE_COLOR: &str = "true_color";
    /// Boolean Box color when false, as an ARGB number (Elastic).
    pub const FALSE_COLOR: &str = "false_color";
    /// Text Display: show numbers with exactly this many decimal places.
    pub const DECIMAL_PLACES: &str = "decimal_places";
    /// Text Display: value text size in points.
    pub const FONT_SIZE: &str = "font_size";
    /// Enum struct fields: `"name"`, `"number"` or `"both"`.
    pub const ENUM_DISPLAY: &str = "enum_display";
}

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

/// Elastic's default widget update period, in seconds.
pub const DEFAULT_PERIOD_S: f64 = 0.06;
/// The shortest update period used, in seconds. A saved period of 0 would
/// ask for every change, so shorter ones are raised to this.
pub const MIN_PERIOD_S: f64 = 0.01;

/// How often `widget` updates, in seconds: its `period`, or
/// [`DEFAULT_PERIOD_S`] if not set, and at least [`MIN_PERIOD_S`].
///
/// ```
/// use raptaboard::widgets::{for_topic, period_s};
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let mut w = for_topic("/Sine", "double", None, at).unwrap();
/// assert_eq!(period_s(&w), 0.06);
/// w.properties.insert("period".into(), 0.0.into());
/// assert_eq!(period_s(&w), 0.01);
/// ```
pub fn period_s(widget: &WidgetContainer) -> f64 {
    let period = widget.period().unwrap_or(DEFAULT_PERIOD_S);
    if period.is_finite() {
        period.max(MIN_PERIOD_S)
    } else {
        DEFAULT_PERIOD_S
    }
}

/// The widget type to make for a topic dropped on the grid, by its NT type
/// string: a Boolean Box for booleans, a Text Display for numbers, strings
/// and arrays of them. `None` for types with no widget yet, such as structs
/// (a Pose2d gets the Field widget in milestone 3); those can't be dragged.
///
/// ```
/// use raptaboard::widgets::type_for_topic;
/// assert_eq!(type_for_topic("boolean"), Some("Boolean Box"));
/// assert_eq!(type_for_topic("double"), Some("Text Display"));
/// assert_eq!(type_for_topic("struct:Pose2d"), None);
/// ```
pub fn type_for_topic(nt_type: &str) -> Option<&'static str> {
    match nt_type {
        "boolean" => Some("Boolean Box"),
        "double" | "float" | "int" | "string" | "boolean[]" | "double[]" | "float[]" | "int[]"
        | "string[]" => Some("Text Display"),
        _ => None,
    }
}

/// A new widget for the topic `name` of NT type `nt_type`, placed at `rect`.
/// It is titled with the last part of the name and saved the way Elastic
/// saves widgets: `topic`, `period` and `data_type` properties. `None` if
/// [`type_for_topic`] has no widget for the type.
///
/// With `field`, the widget shows one field of the struct topic `name`, and
/// `nt_type` is the field's type. It's saved with Elastic's `struct_meta`
/// and titled like `pose.translation.x`.
///
/// ```
/// use raptaboard::widgets::for_topic;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 128.0, y: 0.0, width: 128.0, height: 128.0 };
/// let w = for_topic("/Drive/Health/allModulesHealthy", "boolean", None, at).unwrap();
/// assert_eq!(w.title, "allModulesHealthy");
/// assert_eq!(w.widget_type, "Boolean Box");
/// assert_eq!(w.topic(), Some("/Drive/Health/allModulesHealthy"));
/// assert_eq!(w.period(), Some(0.06));
/// assert!(for_topic("/Drive/pose", "struct:Pose2d", None, at).is_none());
/// ```
pub fn for_topic(
    name: &str,
    nt_type: &str,
    field: Option<&StructMeta>,
    rect: LayoutRect,
) -> Option<WidgetContainer> {
    let widget_type = type_for_topic(nt_type)?;
    let title = source_title(name, field);
    let mut properties = serde_json::Map::new();
    properties.insert("topic".into(), name.into());
    properties.insert("period".into(), DEFAULT_PERIOD_S.into());
    properties.insert("data_type".into(), nt_type.into());
    if let Some(field) = field {
        properties.insert("struct_meta".into(), field.to_json(nt_type));
    }
    Some(WidgetContainer {
        title,
        rect,
        widget_type: widget_type.to_owned(),
        properties,
        extra: serde_json::Map::new(),
    })
}

/// The title a widget gets from what it shows: the last part of the topic
/// name, then the struct field's path if it shows one field of a struct.
///
/// ```
/// use raptaboard::widgets::source_title;
/// use raptaboard_layout::StructMeta;
/// assert_eq!(source_title("/Drive/Health/ok", None), "ok");
/// let x = StructMeta { path: vec!["translation".into(), "x".into()], schema_name: "Pose2d".into() };
/// assert_eq!(source_title("/Drive/pose", Some(&x)), "pose.translation.x");
/// ```
pub fn source_title(name: &str, field: Option<&StructMeta>) -> String {
    let last = name
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(name);
    match field {
        Some(field) => format!("{last}.{}", field.path.join(".")),
        None => last.to_owned(),
    }
}

/// The `decimal_places` setting: how many decimal places a Text Display
/// shows numbers with, at most [`MAX_DECIMAL_PLACES`]. `None` if not set,
/// for up to 4 places without trailing zeros.
///
/// ```
/// use raptaboard::widgets::decimal_places;
/// let props = serde_json::json!({"decimal_places": 2});
/// assert_eq!(decimal_places(props.as_object().unwrap()), Some(2));
/// assert_eq!(decimal_places(&serde_json::Map::new()), None);
/// ```
pub fn decimal_places(properties: &serde_json::Map<String, Json>) -> Option<usize> {
    let n = properties.get(keys::DECIMAL_PLACES)?.as_u64()?;
    Some((n as usize).min(MAX_DECIMAL_PLACES))
}

/// The `font_size` setting in points, kept between [`MIN_FONT_SIZE`] and
/// [`MAX_FONT_SIZE`]; [`DEFAULT_FONT_SIZE`] if not set.
///
/// ```
/// use raptaboard::widgets::{font_size, DEFAULT_FONT_SIZE};
/// let props = serde_json::json!({"font_size": 30.0});
/// assert_eq!(font_size(props.as_object().unwrap()), 30.0);
/// assert_eq!(font_size(&serde_json::Map::new()), DEFAULT_FONT_SIZE);
/// ```
pub fn font_size(properties: &serde_json::Map<String, Json>) -> f32 {
    properties
        .get(keys::FONT_SIZE)
        .and_then(Json::as_f64)
        .filter(|f| f.is_finite())
        .map_or(DEFAULT_FONT_SIZE, |f| {
            (f as f32).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
        })
}

/// The `enum_display` setting; [`EnumDisplay::Name`] if not set.
///
/// ```
/// use raptaboard::struct_view::EnumDisplay;
/// use raptaboard::widgets::enum_display;
/// let props = serde_json::json!({"enum_display": "both"});
/// assert_eq!(enum_display(props.as_object().unwrap()), EnumDisplay::Both);
/// ```
pub fn enum_display(properties: &serde_json::Map<String, Json>) -> EnumDisplay {
    properties
        .get(keys::ENUM_DISPLAY)
        .and_then(Json::as_str)
        .and_then(EnumDisplay::from_key)
        .unwrap_or_default()
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
        WidgetKind::TextDisplay => match text_of(value, decimal_places(properties)) {
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

/// A value as a Text Display shows it: strings without quotes, arrays as
/// `[a, b, c]`, and numbers with exactly `decimals` places, or up to 4
/// without trailing zeros if `None`. Raw bytes (such as struct values) have
/// no text form.
fn text_of(value: &Value, decimals: Option<usize>) -> Option<String> {
    fn list<T>(items: &[T], one: impl Fn(&T) -> String) -> String {
        format!("[{}]", items.iter().map(one).collect::<Vec<_>>().join(", "))
    }
    let number = |n: f64| match decimals {
        Some(places) if n.is_finite() => format!("{n:.places$}"),
        _ => format_number(n),
    };
    Some(match value {
        Value::Boolean(b) => b.to_string(),
        Value::Double(d) => number(*d),
        Value::Float(f) => number(f64::from(*f)),
        Value::Int(i) => i.to_string(),
        Value::String(s) => s.clone(),
        Value::BooleanArray(a) => list(a, |b| b.to_string()),
        Value::DoubleArray(a) => list(a, |d| number(*d)),
        Value::FloatArray(a) => list(a, |f| number(f64::from(*f))),
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

/// A color as Elastic saves it: a 32-bit ARGB number. The opposite of
/// [`color_property`].
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::color_json;
/// assert_eq!(color_json(Color32::from_rgb(0x4C, 0xAF, 0x50)), serde_json::json!(4283215696u32));
/// ```
pub fn color_json(color: Color32) -> Json {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    u32::from_be_bytes([a, r, g, b]).into()
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
                egui::FontId::monospace(font_size(&widget.properties)),
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
    fn dropped_topics_become_widgets() {
        let at = LayoutRect {
            x: 0.0,
            y: 256.0,
            width: 128.0,
            height: 128.0,
        };
        let w = for_topic("/DashTest/ModeName", "string", None, at).unwrap();
        assert_eq!(
            (w.title.as_str(), w.widget_type.as_str()),
            ("ModeName", "Text Display")
        );
        assert_eq!(w.rect, at);
        assert_eq!(w.properties["data_type"], "string");
        // It can be drawn: the kind is one raptaboard knows.
        assert_eq!(
            WidgetKind::from_type(&w.widget_type),
            Some(WidgetKind::TextDisplay)
        );
        // A name ending in a slash still gets a title.
        assert_eq!(for_topic("/odd/", "double", None, at).unwrap().title, "odd");
        // A struct field keeps the struct topic and says which field.
        let field = StructMeta {
            path: vec!["translation".into(), "x".into()],
            schema_name: "Pose2d".into(),
        };
        let x = for_topic("/Drive/pose", "double", Some(&field), at).unwrap();
        assert_eq!(x.title, "pose.translation.x");
        assert_eq!(x.topic(), Some("/Drive/pose"));
        assert_eq!(x.struct_meta(), Some(field));
        assert_eq!(
            x.properties["struct_meta"],
            serde_json::json!({"path": ["translation", "x"], "schema_name": "Pose2d", "type": "double"})
        );
        // Arrays go in a Text Display; raw bytes and schemas have no widget.
        assert_eq!(type_for_topic("int[]"), Some("Text Display"));
        assert_eq!(type_for_topic("raw"), None);
        assert_eq!(type_for_topic("structschema"), None);
        // The JSON matches what Elastic writes for a dropped topic.
        let json = serde_json::to_value(&w).unwrap();
        assert_eq!(
            json["properties"],
            serde_json::json!({"topic": "/DashTest/ModeName", "period": 0.06, "data_type": "string"})
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

    #[test]
    fn text_display_settings() {
        let two = props(serde_json::json!({"decimal_places": 2, "font_size": 500}));
        let text = |v: Value| body(WidgetKind::TextDisplay, &two, Some(&v));
        // Fixed places keep trailing zeros, so the number doesn't jump around.
        assert_eq!(text(Value::Double(1.0)), Body::Text("1.00".into()));
        assert_eq!(text(Value::Float(-0.125)), Body::Text("-0.12".into()));
        assert_eq!(
            text(Value::DoubleArray(vec![1.0, 2.555])),
            Body::Text("[1.00, 2.56]".into())
        );
        // Whole numbers and non-numbers aren't changed.
        assert_eq!(text(Value::Int(7)), Body::Text("7".into()));
        assert_eq!(text(Value::Double(f64::NAN)), Body::Text("NaN".into()));
        let zero = props(serde_json::json!({"decimal_places": 0}));
        assert_eq!(
            body(WidgetKind::TextDisplay, &zero, Some(&Value::Double(2.6))),
            Body::Text("3".into())
        );
        // Out of range settings are kept in range; bad ones are ignored.
        assert_eq!(font_size(&two), MAX_FONT_SIZE);
        let bad = props(serde_json::json!({"decimal_places": -1, "font_size": "big",
            "enum_display": "loud"}));
        assert_eq!(decimal_places(&bad), None);
        assert_eq!(font_size(&bad), DEFAULT_FONT_SIZE);
        assert_eq!(enum_display(&bad), EnumDisplay::Name);
        let many = props(serde_json::json!({"decimal_places": 99, "font_size": 1}));
        assert_eq!(decimal_places(&many), Some(MAX_DECIMAL_PLACES));
        assert_eq!(font_size(&many), MIN_FONT_SIZE);
    }

    #[test]
    fn colors_round_trip_through_json() {
        for color in [
            DEFAULT_TRUE_COLOR,
            DEFAULT_FALSE_COLOR,
            Color32::from_rgb(1, 2, 3),
        ] {
            let mut p = serde_json::Map::new();
            p.insert("c".into(), color_json(color));
            assert_eq!(color_property(&p, "c"), Some(color));
        }
    }
}
