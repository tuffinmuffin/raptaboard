//! Large Text Display: a value as text, as large as fits the widget.
//!
//! The text is the same as a Text Display's, so the number and enum
//! settings in [`format`](mod@crate::widgets::shared::format) apply; the
//! size comes from the widget's size instead of a font size setting.

use eframe::egui::{self, Rect};
use nt4::Value;
use serde_json::{Map, Value as Json};

use crate::widgets::shared::{card, format, value_colors};
use crate::widgets::{ValueInfo, WidgetType};

/// The Large Text Display widget (Elastic's `"Large Text Display"`).
pub struct LargeTextDisplay;

impl WidgetType for LargeTextDisplay {
    fn names(&self) -> &'static [&'static str] {
        &["Large Text Display"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        matches!(nt_type, "double" | "float" | "int" | "string" | "boolean")
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        match value.map(|v| format::value_text(v, props)) {
            Some(Some(text)) => {
                let ink = value_colors::paint_background(painter, rect, props, value)
                    .unwrap_or(visuals.text_color());
                card::draw_fitted(painter, rect, &text, ink);
            }
            Some(None) => card::draw_message(painter, visuals, rect, "not text"),
            None => card::draw_message(painter, visuals, rect, "no value"),
        }
    }

    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, value: ValueInfo<'_>) {
        format::rows(ui, props, value);
        if value
            .nt_type
            .is_none_or(|t| matches!(t, "double" | "float" | "int"))
        {
            value_colors::rows(ui, props);
        }
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        const KEYS: [&str; format::KEYS.len() + value_colors::KEYS.len()] = {
            let mut all = [""; format::KEYS.len() + value_colors::KEYS.len()];
            let mut i = 0;
            while i < format::KEYS.len() {
                all[i] = format::KEYS[i];
                i += 1;
            }
            let mut j = 0;
            while j < value_colors::KEYS.len() {
                all[i + j] = value_colors::KEYS[j];
                j += 1;
            }
            all
        };
        &KEYS
    }
}
