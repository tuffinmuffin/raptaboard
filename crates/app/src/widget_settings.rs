//! The widget settings dialog's contents: title, update period, and the
//! settings for the widget's type.
//!
//! Settings are edited in the widget's `properties` as they change, so the
//! grid shows them right away. A setting at its default value is removed
//! instead of saved, which keeps layout files short. The window around
//! these controls (Done and Cancel) is in `ui.rs`.

use eframe::egui;
use raptaboard_layout::WidgetContainer;
use serde_json::Value as Json;

use crate::struct_view::EnumDisplay;
use crate::widgets::{
    self, DEFAULT_FALSE_COLOR, DEFAULT_FONT_SIZE, DEFAULT_TRUE_COLOR, IntFormat,
    MAX_DECIMAL_PLACES, MAX_FONT_SIZE, MIN_FONT_SIZE, MIN_PERIOD_S, ValueKind, WidgetKind, keys,
};

/// The longest update period the dialog allows, in seconds.
pub const MAX_PERIOD_S: f64 = 5.0;
/// Decimal places offered when "fixed" is first ticked.
const FIRST_DECIMAL_PLACES: usize = 2;

/// Set the property `key` to `value`, or remove it if `value` is `None`.
///
/// ```
/// use raptaboard::widget_settings::set_property;
/// let mut props = serde_json::Map::new();
/// set_property(&mut props, "font_size", Some(20.0.into()));
/// assert_eq!(props["font_size"], 20.0);
/// set_property(&mut props, "font_size", None);
/// assert!(props.is_empty());
/// ```
pub fn set_property(props: &mut serde_json::Map<String, Json>, key: &str, value: Option<Json>) {
    match value {
        Some(value) => {
            props.insert(key.to_owned(), value);
        }
        None => {
            props.remove(key);
        }
    }
}

/// Draw the settings for `widget` and apply any changes to it at once.
///
/// `nt_type` is the NT type of the value it shows (`"double"`), if known;
/// number settings are offered only for numbers of the right kind, or all
/// of them if the type isn't known. `is_enum` says whether it shows an enum
/// struct field, which adds the choice of name, number or both.
pub fn show(ui: &mut egui::Ui, widget: &mut WidgetContainer, nt_type: Option<&str>, is_enum: bool) {
    let source = widget.topic().unwrap_or_default().to_owned();
    let field = widget.struct_meta();
    let source_title = widgets::source_title(&source, field.as_ref());
    let kind = WidgetKind::from_type(&widget.widget_type);

    egui::Grid::new("widget_settings")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Title");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut widget.title).desired_width(180.0));
                let reset =
                    ui.add_enabled(widget.title != source_title, egui::Button::new("Reset"));
                if reset
                    .on_hover_text(format!("Use the name from the source: {source_title}"))
                    .clicked()
                {
                    widget.title = source_title.clone();
                }
            });
            ui.end_row();

            ui.label("Shows");
            ui.vertical(|ui| {
                ui.monospace(&source);
                if let Some(field) = &field {
                    ui.weak(format!(
                        "field {} of {}",
                        field.path.join("."),
                        field.schema_name
                    ));
                }
            });
            ui.end_row();

            ui.label("Type");
            match nt_type {
                Some(ty) => ui.label(format!("{}, {ty}", widget.widget_type)),
                None => ui.label(&widget.widget_type),
            };
            ui.end_row();

            ui.label("Update period");
            let mut period = widgets::period_s(widget);
            let changed = ui
                .add(
                    egui::DragValue::new(&mut period)
                        .range(MIN_PERIOD_S..=MAX_PERIOD_S)
                        .speed(0.005)
                        .fixed_decimals(2)
                        .suffix(" s"),
                )
                .on_hover_text(
                    "How often new values are sent, at most. Shorter is smoother \
                     but uses more of the robot's network.",
                )
                .changed();
            if changed {
                set_property(&mut widget.properties, keys::PERIOD, Some(period.into()));
            }
            ui.end_row();

            match kind {
                Some(WidgetKind::TextDisplay) => {
                    text_display(ui, widget, nt_type.map(ValueKind::of), is_enum);
                }
                Some(WidgetKind::BooleanBox) => boolean_box(ui, widget),
                None => {}
            }
        });
    if kind.is_none() {
        ui.weak("This widget type has no other settings yet.");
    }
}

/// Decimal places for fractional numbers, hex or binary for whole ones, how
/// enum fields show, and text size. `kind` is `None` if the value's type
/// isn't known, which offers both number settings.
fn text_display(
    ui: &mut egui::Ui,
    widget: &mut WidgetContainer,
    kind: Option<ValueKind>,
    is_enum: bool,
) {
    let props = &mut widget.properties;
    let decimals = matches!(kind, None | Some(ValueKind::Decimal));
    let whole = matches!(kind, None | Some(ValueKind::Whole));

    if is_enum {
        ui.label("Enum shows");
        let mut display = widgets::enum_display(props);
        egui::ComboBox::from_id_salt("enum_display")
            .selected_text(display.label())
            .show_ui(ui, |ui| {
                for choice in EnumDisplay::ALL {
                    ui.selectable_value(&mut display, choice, choice.label());
                }
            });
        if display != widgets::enum_display(props) {
            let value = (display != EnumDisplay::default()).then(|| display.key().into());
            set_property(props, keys::ENUM_DISPLAY, value);
        }
        ui.end_row();
    }

    if whole {
        ui.label("Whole numbers");
        let mut format = widgets::int_format(props);
        egui::ComboBox::from_id_salt("int_format")
            .selected_text(format.label())
            .show_ui(ui, |ui| {
                for choice in IntFormat::ALL {
                    ui.selectable_value(&mut format, choice, choice.label());
                }
            });
        if format != widgets::int_format(props) {
            let value = (format != IntFormat::default()).then(|| format.key().into());
            set_property(props, keys::INT_FORMAT, value);
        }
        ui.end_row();
    }

    if decimals {
        decimal_places(ui, props);
    }
    text_size(ui, props);
}

/// The "fixed decimal places" row.
fn decimal_places(ui: &mut egui::Ui, props: &mut serde_json::Map<String, Json>) {
    ui.label("Decimal places");
    ui.horizontal(|ui| {
        let saved = widgets::decimal_places(props);
        let mut fixed = saved.is_some();
        let mut places = saved.unwrap_or(FIRST_DECIMAL_PLACES);
        let ticked = ui
            .checkbox(&mut fixed, "Fixed")
            .on_hover_text(
                "Always show this many decimal places. Off: up to 4, without trailing zeros.",
            )
            .changed();
        let moved = fixed
            && ui
                .add(egui::DragValue::new(&mut places).range(0..=MAX_DECIMAL_PLACES))
                .changed();
        if ticked || moved {
            let value = fixed.then(|| places.into());
            set_property(props, keys::DECIMAL_PLACES, value);
        }
    });
    ui.end_row();
}

/// The value text size row.
fn text_size(ui: &mut egui::Ui, props: &mut serde_json::Map<String, Json>) {
    ui.label("Text size");
    ui.horizontal(|ui| {
        let mut size = widgets::font_size(props);
        if ui
            .add(
                egui::DragValue::new(&mut size)
                    .range(MIN_FONT_SIZE..=MAX_FONT_SIZE)
                    .speed(0.5)
                    .fixed_decimals(0)
                    .suffix(" pt"),
            )
            .changed()
        {
            let value = (size != DEFAULT_FONT_SIZE).then(|| f64::from(size).into());
            set_property(props, keys::FONT_SIZE, value);
        }
        if ui
            .add_enabled(
                props.contains_key(keys::FONT_SIZE),
                egui::Button::new("Reset"),
            )
            .clicked()
        {
            set_property(props, keys::FONT_SIZE, None);
        }
    });
    ui.end_row();
}

/// The true and false colors.
fn boolean_box(ui: &mut egui::Ui, widget: &mut WidgetContainer) {
    let props = &mut widget.properties;
    for (label, key, default) in [
        ("When true", keys::TRUE_COLOR, DEFAULT_TRUE_COLOR),
        ("When false", keys::FALSE_COLOR, DEFAULT_FALSE_COLOR),
    ] {
        ui.label(label);
        ui.horizontal(|ui| {
            let mut color = widgets::color_property(props, key).unwrap_or(default);
            let picked = egui::color_picker::color_edit_button_srgba(
                ui,
                &mut color,
                egui::color_picker::Alpha::Opaque,
            )
            .changed();
            if picked {
                set_property(props, key, Some(widgets::color_json(color)));
            }
            let custom = widgets::color_property(props, key).is_some_and(|c| c != default);
            if ui.add_enabled(custom, egui::Button::new("Reset")).clicked() {
                set_property(props, key, None);
            }
        });
        ui.end_row();
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use raptaboard_layout::Rect;

    fn widget(ty: &str, field: bool) -> WidgetContainer {
        let at = Rect {
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 128.0,
        };
        let meta = field.then(|| raptaboard_layout::StructMeta {
            path: vec!["mode".into()],
            schema_name: "Arm".into(),
        });
        let nt_type = if ty == "Boolean Box" {
            "boolean"
        } else {
            "int"
        };
        let mut w = widgets::for_topic("/Arm/state", nt_type, meta.as_ref(), at).unwrap();
        w.widget_type = ty.to_owned();
        w
    }

    /// Draw the settings once in a window-less egui frame, and return the
    /// row labels shown.
    fn draw(w: &mut WidgetContainer, nt_type: Option<&str>, is_enum: bool) -> Vec<String> {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, w, nt_type, is_enum);
        });
        // No window to upload the font texture to.
        output.textures_delta.clear();
        let mut labels = Vec::new();
        for shape in output.shapes {
            if let egui::epaint::Shape::Text(text) = shape.shape {
                labels.push(text.galley.text().to_owned());
            }
        }
        labels
    }

    #[test]
    fn drawing_changes_nothing_by_itself() {
        // Opening the dialog mustn't add settings, or every widget opened
        // once would gain properties and look like an unsaved change.
        for (ty, field, is_enum) in [
            ("Text Display", false, false),
            ("Text Display", true, true),
            ("Boolean Box", false, false),
            ("Graph", false, false),
        ] {
            let mut w = widget(ty, field);
            let before = w.clone();
            draw(&mut w, None, is_enum);
            assert_eq!(w, before, "{ty}");
        }
    }

    #[test]
    fn number_settings_match_the_value_type() {
        let rows = |nt_type: Option<&str>, is_enum: bool| {
            let labels = draw(&mut widget("Text Display", is_enum), nt_type, is_enum);
            ["Decimal places", "Whole numbers", "Enum shows", "Text size"]
                .into_iter()
                .filter(|row| labels.iter().any(|l| l == row))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(Some("double"), false), ["Decimal places", "Text size"]);
        assert_eq!(rows(Some("int[]"), false), ["Whole numbers", "Text size"]);
        assert_eq!(rows(Some("string"), false), ["Text size"]);
        assert_eq!(
            rows(Some("int"), true),
            ["Whole numbers", "Enum shows", "Text size"]
        );
        // Not known yet: offer both kinds of number setting.
        assert_eq!(
            rows(None, false),
            ["Decimal places", "Whole numbers", "Text size"]
        );
    }

    #[test]
    fn properties_are_set_and_removed() {
        let mut props = serde_json::Map::new();
        set_property(&mut props, keys::ENUM_DISPLAY, Some("both".into()));
        assert_eq!(widgets::enum_display(&props), EnumDisplay::Both);
        set_property(&mut props, keys::ENUM_DISPLAY, None);
        assert_eq!(widgets::enum_display(&props), EnumDisplay::Name);
        // Removing a key that isn't there is fine.
        set_property(&mut props, "nothing", None);
        assert!(props.is_empty());
    }
}
