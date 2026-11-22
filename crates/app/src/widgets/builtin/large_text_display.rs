//! Large Text Display: Elastic's name for a value as text as large as fits
//! the widget. In raptaboard that's a Text Display with "Fit text" on
//! ([`text_display::FIT_TEXT`]), so this is one with it on unless set:
//! layouts that have one (Elastic's, or older raptaboard ones) open and
//! save as they were, with all of Text Display's settings (editing, color
//! by value, number format). It isn't offered as a type of its own
//! ([`WidgetType::listed`]); a Text Display's "Fit text" makes the same.

use eframe::egui::{self, Rect};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::text_display::{self, TextDisplay};
use crate::widgets::shared::card;
use crate::widgets::{Live, ValueInfo, WidgetType};

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
        TextDisplay.shows(nt_type)
    }

    /// A Text Display with "Fit text" is offered instead.
    fn listed(&self) -> bool {
        false
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        let area = if live.place.locked { rect } else { body };
        let title = rect.with_max_y(body.min.y);
        text_display::show_text(
            ui,
            area,
            body.shrink(card::BODY_MARGIN),
            title,
            widget,
            live,
            true,
        )
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let area = live.place.input.unwrap_or(rect);
        text_display::show_text(ui, area, rect, rect, widget, live, true)
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        text_display::draw_text(painter, visuals, rect, props, value, true);
    }

    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, value: ValueInfo<'_>) {
        text_display::settings_rows(ui, props, value, true);
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &text_display::KEYS
    }
}
