//! Text Display: shows any number, string or array as text.
//!
//! Settings: the number and enum settings in [`format`](super::format), and
//! `font_size`.

use eframe::egui::{self, Rect};
use nt4::Value;
use serde_json::{Map, Value as Json};

use super::{ValueInfo, WidgetType, card, format, props};

/// The Text Display widget (Elastic's `"Text Display"`, also saved as
/// `"Text View"`).
pub struct TextDisplay;

/// What a Text Display shows for `value`: its text, or why there's none
/// (no value yet, or raw bytes that have no text form).
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::text_display::text;
/// let props = serde_json::Map::new();
/// assert_eq!(text(&props, Some(&Value::Double(2.5))), Ok("2.5".into()));
/// assert_eq!(text(&props, None), Err("no value"));
/// ```
pub fn text(props: &Map<String, Json>, value: Option<&Value>) -> Result<String, &'static str> {
    format::value_text(value.ok_or("no value")?, props).ok_or("not text")
}

impl WidgetType for TextDisplay {
    fn names(&self) -> &'static [&'static str] {
        &["Text Display", "Text View"]
    }

    fn accepts(&self, nt_type: &str) -> bool {
        matches!(
            nt_type,
            "double"
                | "float"
                | "int"
                | "string"
                | "boolean[]"
                | "double[]"
                | "float[]"
                | "int[]"
                | "string[]"
        )
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        match text(props, value) {
            Ok(text) => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    text,
                    egui::FontId::monospace(props::font_size(props)),
                    visuals.text_color(),
                );
            }
            Err(why) => card::draw_message(painter, visuals, rect, why),
        }
    }

    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, value: ValueInfo<'_>) {
        format::rows(ui, props, value);
        props::text_size_row(ui, props);
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        const KEYS: [&str; 5] = [
            format::DECIMAL_PLACES,
            format::INT_FORMAT,
            format::INT_DIGITS,
            format::ENUM_DISPLAY,
            props::FONT_SIZE,
        ];
        &KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn shows_text_or_says_why_not() {
        let none = Map::new();
        assert_eq!(text(&none, Some(&Value::Int(-4))), Ok("-4".into()));
        assert_eq!(
            text(&none, Some(&Value::String("Blue 1".into()))),
            Ok("Blue 1".into())
        );
        assert_eq!(text(&none, Some(&Value::Raw(vec![0; 8]))), Err("not text"));
        assert_eq!(text(&none, None), Err("no value"));
    }

    #[test]
    fn its_settings_keys_include_the_format_keys() {
        for key in format::KEYS {
            assert!(TextDisplay.setting_keys().contains(&key), "{key}");
        }
    }
}
