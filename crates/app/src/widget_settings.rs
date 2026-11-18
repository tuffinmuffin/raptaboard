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

use crate::struct_view::{EnumDisplay, FieldInfo};
use crate::widgets::{
    self, DEFAULT_FALSE_COLOR, DEFAULT_FONT_SIZE, DEFAULT_PERIOD_S, DEFAULT_TRUE_COLOR, IntFormat,
    MAX_DECIMAL_PLACES, MAX_FONT_SIZE, MAX_INT_DIGITS, MIN_FONT_SIZE, MIN_PERIOD_S, ValueKind,
    WidgetKind, keys,
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

/// Settings [`reset_settings`] removes when they're set: raptaboard's own,
/// which all default to "not set".
const RESETTABLE: [&str; 5] = [
    keys::DECIMAL_PLACES,
    keys::FONT_SIZE,
    keys::ENUM_DISPLAY,
    keys::INT_FORMAT,
    keys::INT_DIGITS,
];

/// Whether any of `widget`'s settings differ from the defaults, so
/// [`reset_settings`] would change something. The title doesn't count.
pub fn has_custom_settings(widget: &WidgetContainer) -> bool {
    let mut reset = widget.clone();
    reset_settings(&mut reset);
    reset != *widget
}

/// Put `widget`'s settings back to their defaults: update period 0.06 s,
/// Boolean Box colors green and red, and none of raptaboard's own settings.
/// The title and what the widget shows (`topic`, `struct_meta`,
/// `data_type`) are kept, as are settings for widget types not built yet.
///
/// ```
/// use raptaboard::widget_settings::{has_custom_settings, reset_settings};
/// use raptaboard::widgets::for_topic;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let mut w = for_topic("/Sine", "double", None, at).unwrap();
/// assert!(!has_custom_settings(&w));
/// w.title = "Wave".into();
/// w.properties.insert("decimal_places".into(), 2.into());
/// w.properties.insert("period".into(), 1.0.into());
/// assert!(has_custom_settings(&w));
/// reset_settings(&mut w);
/// assert!(!has_custom_settings(&w));
/// assert_eq!(w.title, "Wave");
/// assert_eq!(w.period(), Some(0.06));
/// ```
pub fn reset_settings(widget: &mut WidgetContainer) {
    let props = &mut widget.properties;
    for key in RESETTABLE {
        props.remove(key);
    }
    if widget_period(props) != Some(DEFAULT_PERIOD_S) {
        set_property(props, keys::PERIOD, Some(DEFAULT_PERIOD_S.into()));
    }
    // Elastic saves the default colors in every Boolean Box; leave those.
    for (key, default) in [
        (keys::TRUE_COLOR, DEFAULT_TRUE_COLOR),
        (keys::FALSE_COLOR, DEFAULT_FALSE_COLOR),
    ] {
        if props.contains_key(key) && widgets::color_property(props, key) != Some(default) {
            props.remove(key);
        }
    }
}

fn widget_period(props: &serde_json::Map<String, Json>) -> Option<f64> {
    props.get(keys::PERIOD).and_then(Json::as_f64)
}

/// Draw the settings for `widget` and apply any changes to it at once.
///
/// `nt_type` is the NT type of the value it shows (`"double"`), if known;
/// number settings are offered only for numbers of the right kind, or all
/// of them if the type isn't known. `field_info` is what the schema says
/// about the struct field it shows, if it shows one; an enum field adds the
/// choice of name, number or both, and the type row lists its values.
pub fn show(
    ui: &mut egui::Ui,
    widget: &mut WidgetContainer,
    nt_type: Option<&str>,
    field_info: Option<&FieldInfo>,
) {
    let is_enum = field_info.is_some_and(FieldInfo::is_enum);
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
            // The schema's type for a struct field (`enum uint8`), else the
            // NT type (`double`).
            let ty = field_info
                .map(FieldInfo::label)
                .or(nt_type.map(str::to_owned));
            ui.vertical(|ui| {
                match ty {
                    Some(ty) => ui.label(format!("{}, {ty}", widget.widget_type)),
                    None => ui.label(&widget.widget_type),
                };
                if let Some(info) = field_info.filter(|f| f.is_enum()) {
                    ui.weak(info.enum_list());
                }
            });
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
    // An enum shown by name has no number to format.
    let whole = matches!(kind, None | Some(ValueKind::Whole))
        && !(is_enum && widgets::enum_display(props) == EnumDisplay::Name);

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

        ui.label("Digits");
        ui.horizontal(|ui| {
            let saved = widgets::int_digits(props);
            let mut padded = saved > 0;
            let mut digits = if padded { saved } else { 2 };
            let ticked = ui
                .checkbox(&mut padded, "Pad with zeros")
                .on_hover_text("Show at least this many digits, like %08X: 0x0000001A.")
                .changed();
            let moved = padded
                && ui
                    .add(egui::DragValue::new(&mut digits).range(1..=MAX_INT_DIGITS))
                    .changed();
            if ticked || moved {
                let value = padded.then(|| digits.into());
                set_property(props, keys::INT_DIGITS, value);
            }
        });
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
        let info = FieldInfo {
            type_name: "uint8".into(),
            array: None,
            enum_values: vec![("kStow".into(), 0), ("kIntake".into(), 3)],
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, w, nt_type, is_enum.then_some(&info));
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
        let rows_for = |w: &mut WidgetContainer, nt_type: Option<&str>, is_enum: bool| {
            let labels = draw(w, nt_type, is_enum);
            [
                "Decimal places",
                "Whole numbers",
                "Digits",
                "Enum shows",
                "Text size",
            ]
            .into_iter()
            .filter(|row| labels.iter().any(|l| l == row))
            .collect::<Vec<_>>()
        };
        let rows = |nt_type: Option<&str>, is_enum: bool| {
            rows_for(&mut widget("Text Display", is_enum), nt_type, is_enum)
        };
        assert_eq!(rows(Some("double"), false), ["Decimal places", "Text size"]);
        assert_eq!(
            rows(Some("int[]"), false),
            ["Whole numbers", "Digits", "Text size"]
        );
        assert_eq!(rows(Some("string"), false), ["Text size"]);
        // An enum shown by name has no number settings, until it shows its
        // number.
        assert_eq!(rows(Some("int"), true), ["Enum shows", "Text size"]);
        let mut by_number = widget("Text Display", true);
        by_number
            .properties
            .insert(keys::ENUM_DISPLAY.into(), "both".into());
        assert_eq!(
            rows_for(&mut by_number, Some("int"), true),
            ["Whole numbers", "Digits", "Enum shows", "Text size"]
        );
        // The type row names the enum and lists its values.
        let labels = draw(&mut widget("Text Display", true), Some("int"), true);
        assert!(
            labels.iter().any(|l| l == "Text Display, enum uint8"),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == "kStow = 0, kIntake = 3"),
            "{labels:?}"
        );
        // Not known yet: offer both kinds of number setting.
        assert_eq!(
            rows(None, false),
            ["Decimal places", "Whole numbers", "Digits", "Text size"]
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

    #[test]
    fn reset_keeps_bindings_and_elastic_defaults() {
        let mut w = widget("Boolean Box", false);
        // As Elastic saves it: the default colors written out. Nothing to reset.
        w.properties
            .insert(keys::TRUE_COLOR.into(), 4283215696u32.into());
        w.properties
            .insert(keys::FALSE_COLOR.into(), 4294198070u32.into());
        assert!(!has_custom_settings(&w));
        // A custom color and a setting for some other widget type.
        w.properties
            .insert(keys::TRUE_COLOR.into(), 0xFF2196F3u32.into());
        w.properties.insert("num_of_ticks".into(), 5.into());
        assert!(has_custom_settings(&w));
        reset_settings(&mut w);
        assert!(!w.properties.contains_key(keys::TRUE_COLOR));
        assert_eq!(w.properties[keys::FALSE_COLOR], 4294198070u32);
        assert_eq!(w.properties["num_of_ticks"], 5);
        assert_eq!(w.topic(), Some("/Arm/state"));
        assert_eq!(w.properties[keys::DATA_TYPE], "boolean");

        // A struct field keeps its field.
        let mut field = widget("Text Display", true);
        field
            .properties
            .insert(keys::ENUM_DISPLAY.into(), "both".into());
        reset_settings(&mut field);
        assert!(field.struct_meta().is_some());
        assert!(!has_custom_settings(&field));
    }
}
