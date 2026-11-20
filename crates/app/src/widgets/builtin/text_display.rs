//! Text Display: shows any number, string or array as text, and with
//! `editable` on, takes a typed value too.
//!
//! Settings: the number and enum settings in [`format`](mod@crate::widgets::shared::format),
//! `font_size`, and `editable` (raptaboard's, off unless set). An editable
//! Text Display has a blue dot in its corner; a click on it (locked, the
//! whole card; unlocked, the body, since the title bar moves it) opens a
//! box with the value selected, and Enter sends what's typed, in the
//! topic's type ([`entry`] has the rest). Text that isn't a number for a
//! number topic, or `true` or `false` for a boolean, turns red and isn't
//! sent. Arrays and struct fields can't be typed in.

use eframe::egui::{self, CursorIcon, Rect, Sense};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::widgets::shared::{card, entry, format, props};
use crate::widgets::{Live, ValueInfo, WidgetType};

/// Values can be typed in (raptaboard's; off unless set).
pub const EDITABLE: &str = "editable";

/// The Text Display widget (Elastic's `"Text Display"`, also saved as
/// `"Text View"`).
pub struct TextDisplay;

/// What a Text Display shows for `value`: its text, or why there's none
/// (no value yet, or raw bytes that have no text form).
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::builtin::text_display::text;
/// let props = serde_json::Map::new();
/// assert_eq!(text(&props, Some(&Value::Double(2.5))), Ok("2.5".into()));
/// assert_eq!(text(&props, None), Err("no value"));
/// ```
pub fn text(props: &Map<String, Json>, value: Option<&Value>) -> Result<String, &'static str> {
    format::value_text(value.ok_or("no value")?, props).ok_or("not text")
}

/// Whether `widget` takes typed values while showing `value`: `editable`
/// is on, it isn't bound to a struct field (those can't be sent), and the
/// value is a single number, string or boolean (or there's none yet).
pub fn can_edit(widget: &WidgetContainer, value: Option<&Value>) -> bool {
    props::flag(&widget.properties, EDITABLE, false)
        && widget.struct_meta().is_none()
        && matches!(
            value,
            None | Some(
                Value::Double(_)
                    | Value::Float(_)
                    | Value::Int(_)
                    | Value::String(_)
                    | Value::Boolean(_)
            )
        )
}

/// The text a value starts as in the typing box: in full, not rounded to
/// the widget's decimal places.
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::builtin::text_display::edit_text;
/// assert_eq!(edit_text(Some(&Value::Double(0.123456))), "0.123456");
/// assert_eq!(edit_text(Some(&Value::Boolean(true))), "true");
/// assert_eq!(edit_text(None), "");
/// ```
pub fn edit_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Double(n)) => n.to_string(),
        Some(Value::Float(n)) => n.to_string(),
        Some(Value::Int(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Boolean(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// Whether `text` can be sent to a topic now showing `value`: a number for
/// a number, `true` or `false` (any case) for a boolean, anything for text
/// or when there's no value yet (the send says if the topic won't take it).
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::builtin::text_display::accepts_text;
/// assert!(accepts_text(" 2.5 ", Some(&Value::Double(1.0))));
/// assert!(!accepts_text("fast", Some(&Value::Int(1))));
/// assert!(accepts_text("False", Some(&Value::Boolean(true))));
/// assert!(accepts_text("anything", Some(&Value::String("x".into()))));
/// ```
pub fn accepts_text(text: &str, value: Option<&Value>) -> bool {
    match value {
        Some(Value::Double(_) | Value::Float(_) | Value::Int(_)) => {
            text.trim().parse::<f64>().is_ok_and(|n| n.is_finite())
        }
        Some(Value::Boolean(_)) => {
            matches!(text.trim().to_lowercase().as_str(), "true" | "false")
        }
        _ => true,
    }
}

/// A Text Display with a [`Ui`](egui::Ui): `area` takes the click that
/// opens the typing box, `body` is where the value is, `dot_in` the corner
/// the editable dot goes in. The value to send, if one was typed.
fn show_text(
    ui: &mut egui::Ui,
    area: Rect,
    body: Rect,
    dot_in: Rect,
    widget: &WidgetContainer,
    live: &Live<'_>,
) -> Option<Value> {
    let p = &widget.properties;
    let editable = can_edit(widget, live.value);
    let id = ui.scope_id().with("text_entry");
    let editing = editable && entry::is_open(ui, id);
    {
        let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
        if !editing {
            TextDisplay.draw_body(&painter, ui.visuals(), body, p, live.value);
        }
    }
    if !editable {
        return None;
    }
    entry::draw_editable_dot(ui.painter(), dot_in);
    let response = ui.interact(area, id, Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Text);
    }
    if response.clicked() && !editing {
        entry::open(ui, id, edit_text(live.value));
    }
    if !entry::is_open(ui, id) {
        return None;
    }
    let size = props::font_size(p);
    let rect = Rect::from_center_size(
        body.center(),
        egui::vec2(
            (body.width() - 2.0 * card::BODY_MARGIN).max(0.0),
            size + 8.0,
        ),
    );
    let value = live.value;
    entry::show(ui, id, rect, egui::FontId::monospace(size), |text| {
        accepts_text(text, value)
    })
    .map(Value::String)
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

    fn shows(&self, nt_type: &str) -> bool {
        self.accepts(nt_type) || nt_type == "boolean"
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        let area = if live.locked { rect } else { body };
        // The dot goes in the title bar's corner, clear of the value.
        let title = rect.with_max_y(body.min.y);
        show_text(
            ui,
            area,
            body.shrink(card::BODY_MARGIN),
            title,
            widget,
            live,
        )
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let area = live.input.unwrap_or(rect);
        show_text(ui, area, rect, rect, widget, live)
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
        // "Editable", with the details on hover.
        let hover = "Click the value to type a new one and press Enter to send it \
                     (Escape cancels). A blue dot in the corner shows it's editable. \
                     Not for arrays or struct fields.";
        ui.label("Editable").on_hover_text(hover);
        let mut on = props::flag(props, EDITABLE, false);
        if ui.checkbox(&mut on, "").on_hover_text(hover).changed() {
            props::set_property(props, EDITABLE, on.then_some(true.into()));
        }
        ui.end_row();
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        const KEYS: [&str; 6] = [
            EDITABLE,
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
    fn only_single_values_off_struct_fields_are_editable() {
        let at = raptaboard_layout::Rect {
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 128.0,
        };
        let mut w = crate::widgets::for_topic("/a", "double", None, at).unwrap();
        let one = Value::Double(1.0);
        assert!(!can_edit(&w, Some(&one)), "off unless set");
        w.properties.insert(EDITABLE.into(), true.into());
        assert!(can_edit(&w, Some(&one)));
        assert!(can_edit(&w, None));
        assert!(!can_edit(&w, Some(&Value::DoubleArray(vec![1.0]))));
        w.properties.insert(
            "struct_meta".into(),
            serde_json::json!({"path": ["x"], "schema_name": "Translation2d", "type": "double"}),
        );
        assert!(!can_edit(&w, Some(&one)));
        assert!(accepts_text("12", Some(&Value::Int(3))));
        assert!(!accepts_text("inf", Some(&Value::Double(3.0))));
        assert!(!accepts_text("yes", Some(&Value::Boolean(false))));
        assert!(accepts_text("", None));
    }

    #[test]
    fn its_settings_keys_include_the_format_keys() {
        for key in format::KEYS {
            assert!(TextDisplay.setting_keys().contains(&key), "{key}");
        }
    }
}
