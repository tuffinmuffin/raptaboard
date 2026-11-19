//! Toggle Switch and Toggle Button: show a boolean and change it when
//! clicked, sending the opposite value to the topic.
//!
//! Locked, clicking anywhere on the card toggles; unlocked, the body below
//! the title bar toggles and the title bar moves the widget. With no value
//! yet the widget shows "off" and a click sends `true`, as in Elastic. Both
//! have no settings of their own.

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Rect, Sense, Stroke, Vec2};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::widgets::shared::card;
use crate::widgets::{Live, ValueInfo, WidgetType};

/// Track of the switch when on (Material blue, like Elastic's switch).
const ON_COLOR: Color32 = Color32::from_rgb(0x42, 0xA5, 0xF5);
/// The button's fill when off (Elastic's dark grey).
const OFF_BUTTON: Color32 = Color32::from_rgb(50, 50, 50);
/// The switch's size relative to the body's height, and its most width
/// relative to the body's width.
const SWITCH_HEIGHT: f32 = 0.45;
const SWITCH_MAX_WIDTH: f32 = 0.8;

/// Whether `value` is `true`; anything else (no value, not a boolean)
/// counts as off, as in Elastic.
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::builtin::toggle::is_on;
/// assert!(is_on(Some(&Value::Boolean(true))));
/// assert!(!is_on(None));
/// ```
pub fn is_on(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Boolean(true)))
}

/// The value a click sends: the opposite of what's shown now.
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::builtin::toggle::flipped;
/// assert_eq!(flipped(Some(&Value::Boolean(true))), Value::Boolean(false));
/// assert_eq!(flipped(None), Value::Boolean(true));
/// ```
pub fn flipped(value: Option<&Value>) -> Value {
    Value::Boolean(!is_on(value))
}

/// The card, then a click area: the whole card when locked, the body below
/// the title bar when not (the title bar moves the widget then). Returns the
/// body and whether it was clicked this frame.
fn card_and_click(
    ui: &mut egui::Ui,
    rect: Rect,
    widget: &WidgetContainer,
    locked: bool,
) -> (Rect, bool) {
    let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
    let area = if locked { rect } else { body };
    let response = ui.interact(area, ui.scope_id().with("toggle"), Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    (body, response.clicked())
}

/// The Toggle Switch widget (Elastic's `"Toggle Switch"`): a switch that
/// slides on and off.
pub struct ToggleSwitch;

impl WidgetType for ToggleSwitch {
    fn names(&self) -> &'static [&'static str] {
        &["Toggle Switch"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        nt_type == "boolean"
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let (body, clicked) = card_and_click(ui, rect, widget, live.locked);
        let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
        self.draw_body(
            &painter,
            ui.visuals(),
            body.shrink(card::BODY_MARGIN),
            &widget.properties,
            live.value,
        );
        clicked.then(|| flipped(live.value))
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        let on = is_on(value);
        let height = (rect.height() * SWITCH_HEIGHT).min(rect.width() * SWITCH_MAX_WIDTH / 2.0);
        let size = Vec2::new(height * 2.0, height);
        let track = Rect::from_center_size(rect.center(), size);
        let radius = CornerRadius::same((height / 2.0).min(255.0) as u8);
        let (fill, knob_x) = if on {
            (ON_COLOR, track.right() - height / 2.0)
        } else {
            (
                visuals.widgets.inactive.bg_fill,
                track.left() + height / 2.0,
            )
        };
        painter.rect_filled(track, radius, fill);
        painter.rect_stroke(
            track,
            radius,
            Stroke::new(1.0, visuals.widgets.inactive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
        painter.circle_filled(
            egui::pos2(knob_x, track.center().y),
            height * 0.4,
            Color32::WHITE,
        );
        if value.is_none() {
            card::draw_message(
                painter,
                visuals,
                Rect::from_min_max(egui::pos2(rect.left(), track.bottom()), rect.max),
                "no value",
            );
        }
    }

    fn settings(&self, _: &mut egui::Ui, _: &mut Map<String, Json>, _: ValueInfo<'_>) {}

    fn setting_keys(&self) -> &'static [&'static str] {
        &[]
    }
}

/// The Toggle Button widget (Elastic's `"Toggle Button"`): a button that
/// lights up while on, labelled with the topic's last name part.
pub struct ToggleButton;

/// The button's label: the last part of the widget's topic name, as in
/// Elastic (`/Arm/brake` is `brake`), or its title with no topic.
///
/// ```
/// use raptaboard::widgets::builtin::toggle::button_label;
/// assert_eq!(button_label(Some("/Arm/brake"), "Brake"), "brake");
/// assert_eq!(button_label(None, "Brake"), "Brake");
/// ```
pub fn button_label<'a>(topic: Option<&'a str>, title: &'a str) -> &'a str {
    topic
        .and_then(|t| t.rsplit('/').next())
        .filter(|t| !t.is_empty())
        .unwrap_or(title)
}

impl WidgetType for ToggleButton {
    fn names(&self) -> &'static [&'static str] {
        &["Toggle Button"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        nt_type == "boolean"
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let (body, clicked) = card_and_click(ui, rect, widget, live.locked);
        let inside = body.shrink(card::BODY_MARGIN);
        let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
        let fill = if is_on(live.value) {
            ui.visuals().selection.bg_fill
        } else {
            OFF_BUTTON
        };
        painter.rect_filled(inside, CornerRadius::same(card::BODY_CORNER_RADIUS), fill);
        painter.text(
            inside.center(),
            egui::Align2::CENTER_CENTER,
            button_label(widget.topic(), &widget.title),
            egui::FontId::proportional(16.0),
            crate::widgets::shared::props::text_color_on(fill),
        );
        clicked.then(|| flipped(live.value))
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        // Drawn without a Ui (in a list, or while dragged): the same look.
        let fill = if is_on(value) {
            visuals.selection.bg_fill
        } else {
            OFF_BUTTON
        };
        painter.rect_filled(rect, CornerRadius::same(card::BODY_CORNER_RADIUS), fill);
    }

    fn settings(&self, _: &mut egui::Ui, _: &mut Map<String, Json>, _: ValueInfo<'_>) {}

    fn setting_keys(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn a_click_sends_the_opposite() {
        assert_eq!(flipped(Some(&Value::Boolean(false))), Value::Boolean(true));
        assert_eq!(flipped(Some(&Value::Boolean(true))), Value::Boolean(false));
        // Not a boolean counts as off, as in Elastic.
        assert_eq!(flipped(Some(&Value::Double(1.0))), Value::Boolean(true));
    }

    #[test]
    fn the_label_is_the_topics_last_part() {
        assert_eq!(button_label(Some("/a/b/c"), "x"), "c");
        assert_eq!(button_label(Some("/a/"), "x"), "x");
    }
}
