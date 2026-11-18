//! Boolean Box: fills with one color when the boolean is true and another
//! when false, optionally with text over the color.
//!
//! Settings: `true_color` and `false_color` (Elastic's), and raptaboard's
//! `show_text`, `true_text`, `false_text` and `font_size`.

use eframe::egui::{self, Color32, CornerRadius, Rect};
use nt4::Value;
use serde_json::{Map, Value as Json};

use crate::widgets::shared::card;
use crate::widgets::shared::props::{self, set_property};
use crate::widgets::{ValueInfo, WidgetType};

/// Color when true, as an ARGB number (Elastic).
pub const TRUE_COLOR: &str = "true_color";
/// Color when false, as an ARGB number (Elastic).
pub const FALSE_COLOR: &str = "false_color";
/// Whether to write `true_text` or `false_text` over the color (raptaboard).
pub const SHOW_TEXT: &str = "show_text";
/// Text over the color when true, such as `"Good"` (raptaboard).
pub const TRUE_TEXT: &str = "true_text";
/// Text over the color when false; empty for none (raptaboard).
pub const FALSE_TEXT: &str = "false_text";

/// Elastic's default true color (Material green 500).
pub const DEFAULT_TRUE_COLOR: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
/// Elastic's default false color (Material red 500).
pub const DEFAULT_FALSE_COLOR: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);

/// The Boolean Box widget (Elastic's `"Boolean Box"`).
pub struct BooleanBox;

/// The `show_text` setting: whether the true or false text is written over
/// the color. Off if not set.
pub fn show_text(props: &Map<String, Json>) -> bool {
    props
        .get(SHOW_TEXT)
        .and_then(Json::as_bool)
        .unwrap_or(false)
}

/// What a Boolean Box shows for `value`: its fill color and the text over
/// it (if `show_text` is on and the text isn't empty), or why there's
/// nothing (no value yet, or not a boolean).
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::builtin::boolean_box::{fill, DEFAULT_TRUE_COLOR};
/// let props = serde_json::Map::new();
/// assert_eq!(fill(&props, Some(&Value::Boolean(true))), Ok((DEFAULT_TRUE_COLOR, None)));
/// assert_eq!(fill(&props, Some(&Value::Double(1.0))), Err("not a boolean"));
/// assert_eq!(fill(&props, None), Err("no value"));
/// ```
pub fn fill(
    props: &Map<String, Json>,
    value: Option<&Value>,
) -> Result<(Color32, Option<String>), &'static str> {
    let Value::Boolean(on) = value.ok_or("no value")? else {
        return Err("not a boolean");
    };
    let (color, text, default) = if *on {
        (TRUE_COLOR, TRUE_TEXT, DEFAULT_TRUE_COLOR)
    } else {
        (FALSE_COLOR, FALSE_TEXT, DEFAULT_FALSE_COLOR)
    };
    let text = show_text(props)
        .then(|| props.get(text).and_then(Json::as_str))
        .flatten()
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    Ok((props::color_property(props, color).unwrap_or(default), text))
}

impl WidgetType for BooleanBox {
    fn names(&self) -> &'static [&'static str] {
        &["Boolean Box"]
    }

    fn accepts(&self, nt_type: &str) -> bool {
        nt_type == "boolean"
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        match fill(props, value) {
            Ok((color, text)) => {
                painter.rect_filled(rect, CornerRadius::same(6), color);
                if let Some(text) = text {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional(props::font_size(props)),
                        props::text_color_on(color),
                    );
                }
            }
            Err(why) => card::draw_message(painter, visuals, rect, why),
        }
    }

    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, _value: ValueInfo<'_>) {
        props::color_row(ui, props, "When true", TRUE_COLOR, DEFAULT_TRUE_COLOR);
        props::color_row(ui, props, "When false", FALSE_COLOR, DEFAULT_FALSE_COLOR);

        ui.label("Text");
        let mut shown = show_text(props);
        if ui
            .checkbox(&mut shown, "Write text over the color")
            .changed()
        {
            set_property(props, SHOW_TEXT, shown.then_some(true.into()));
            // Start with something visible the first time.
            for (key, first) in [(TRUE_TEXT, "True"), (FALSE_TEXT, "False")] {
                if shown && !props.contains_key(key) {
                    set_property(props, key, Some(first.into()));
                }
            }
        }
        ui.end_row();
        if shown {
            for (label, key) in [
                ("Text when true", TRUE_TEXT),
                ("Text when false", FALSE_TEXT),
            ] {
                ui.label(label);
                let mut text = props
                    .get(key)
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let edit = egui::TextEdit::singleline(&mut text)
                    .hint_text("none")
                    .desired_width(180.0);
                if ui.add(edit).changed() {
                    set_property(props, key, Some(text.into()));
                }
                ui.end_row();
            }
            props::text_size_row(ui, props);
        }
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &[SHOW_TEXT, TRUE_TEXT, FALSE_TEXT, props::FONT_SIZE]
    }

    /// Custom colors go back to green and red. Elastic saves the default
    /// colors in every Boolean Box; those are left as they are.
    fn reset(&self, props: &mut Map<String, Json>) {
        for (key, default) in [
            (TRUE_COLOR, DEFAULT_TRUE_COLOR),
            (FALSE_COLOR, DEFAULT_FALSE_COLOR),
        ] {
            if props.contains_key(key) && props::color_property(props, key) != Some(default) {
                props.remove(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn props(json: Json) -> Map<String, Json> {
        json.as_object().unwrap().clone()
    }

    fn on(p: &Map<String, Json>, value: bool) -> Result<(Color32, Option<String>), &'static str> {
        fill(p, Some(&Value::Boolean(value)))
    }

    #[test]
    fn uses_saved_colors() {
        // The colors from crates/layout/tests/data/elastic_sample.json.
        let saved =
            props(serde_json::json!({"true_color": 4283215696u32, "false_color": 4294198070u32}));
        assert_eq!(on(&saved, true), Ok((DEFAULT_TRUE_COLOR, None)));
        assert_eq!(on(&saved, false), Ok((DEFAULT_FALSE_COLOR, None)));
        let blue = props(serde_json::json!({"true_color": 0xFF2196F3u32}));
        assert_eq!(
            on(&blue, true),
            Ok((Color32::from_rgb(0x21, 0x96, 0xF3), None))
        );
        // Bad colors fall back to the defaults.
        let bad = props(serde_json::json!({"true_color": "green", "false_color": -1}));
        assert_eq!(on(&bad, false), Ok((DEFAULT_FALSE_COLOR, None)));
    }

    #[test]
    fn text_over_the_color() {
        let both = props(serde_json::json!({
            "show_text": true, "true_text": "Good", "false_text": "Bad"}));
        assert_eq!(
            on(&both, true),
            Ok((DEFAULT_TRUE_COLOR, Some("Good".into())))
        );
        assert_eq!(
            on(&both, false),
            Ok((DEFAULT_FALSE_COLOR, Some("Bad".into())))
        );
        // Text only when true, grey when false.
        let one = props(serde_json::json!({
            "show_text": true, "true_text": "Elevator moving", "false_text": "",
            "false_color": 0xFF9E9E9Eu32}));
        assert_eq!(
            on(&one, false),
            Ok((Color32::from_rgb(0x9E, 0x9E, 0x9E), None))
        );
        // Turned off: the text is kept in the file but not shown.
        let off = props(serde_json::json!({"show_text": false, "true_text": "Good"}));
        assert_eq!(on(&off, true), Ok((DEFAULT_TRUE_COLOR, None)));
        let bad = props(serde_json::json!({"show_text": "yes", "true_text": 5}));
        assert_eq!(on(&bad, true), Ok((DEFAULT_TRUE_COLOR, None)));
        // The default colors get white text.
        assert_eq!(props::text_color_on(DEFAULT_TRUE_COLOR), Color32::WHITE);
        assert_eq!(props::text_color_on(DEFAULT_FALSE_COLOR), Color32::WHITE);
    }
}
