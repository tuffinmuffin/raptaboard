//! The widget settings dialog's contents: title, what it shows, update
//! period, then the rows the widget's own type adds
//! ([`WidgetType::settings`](crate::widgets::WidgetType::settings)).
//!
//! Settings are edited in the widget's `properties` as they change, so the
//! grid shows them right away. A setting at its default value is removed
//! instead of saved, which keeps layout files short. The window around
//! these controls (Done, Cancel, Reset settings) is in `ui/dialogs.rs`.

use eframe::egui;
use raptaboard_layout::WidgetContainer;
use serde_json::Value as Json;

use super::props::{self, DEFAULT_PERIOD_S, MAX_PERIOD_S, MIN_PERIOD_S, PERIOD, set_property};
use crate::types::structs::FieldInfo;
use crate::widgets::{TYPES, ValueInfo, find, source_title};

/// The widest the "Shows" row's topic name gets before it wraps, in points.
const SHOWS_WIDTH: f32 = 320.0;

/// Whether any of `widget`'s settings differ from the defaults, so
/// [`reset_settings`] would change something. The title doesn't count.
pub fn has_custom_settings(widget: &WidgetContainer) -> bool {
    let mut reset = widget.clone();
    reset_settings(&mut reset);
    reset != *widget
}

/// Put `widget`'s settings back to their defaults: update period 0.06 s,
/// and its type's settings
/// ([`setting_keys`](crate::widgets::WidgetType::setting_keys) removed,
/// [`reset`](crate::widgets::WidgetType::reset) for the rest). The title and what
/// the widget shows (`topic`, `struct_meta`, `data_type`) are kept, as are
/// settings raptaboard doesn't know.
///
/// ```
/// use raptaboard::widgets::for_topic;
/// use raptaboard::widgets::shared::settings::{has_custom_settings, reset_settings};
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
    if props.get(PERIOD).and_then(Json::as_f64) != Some(DEFAULT_PERIOD_S) {
        set_property(props, PERIOD, Some(DEFAULT_PERIOD_S.into()));
    }
    if let Some(ty) = find(&widget.widget_type) {
        for key in ty.setting_keys() {
            props.remove(*key);
        }
        ty.reset(props);
    }
}

/// Draw the settings for `widget` and apply any changes to it at once.
///
/// `nt_type` is the NT type of the value it shows (`"double"`), if known;
/// number settings are offered only for numbers of the right kind, or all
/// of them if the type isn't known. `field_info` is what the schema says
/// about the struct field it shows, if it shows one; an enum field adds the
/// choice of name, number or both, and the type row lists its values.
///
/// `topics` is every topic on the server, as (full name, NT type), for
/// settings that refer to other topics.
pub fn show(
    ui: &mut egui::Ui,
    widget: &mut WidgetContainer,
    nt_type: Option<&str>,
    field_info: Option<&FieldInfo>,
    topics: &[(String, String)],
) {
    let source = widget.topic().unwrap_or_default().to_owned();
    let field = widget.struct_meta();
    let source_title = source_title(&source, field.as_ref());
    let ty = find(&widget.widget_type);

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
                // The whole name, wrapped onto more lines if it's long.
                ui.set_max_width(SHOWS_WIDTH);
                ui.add(egui::Label::new(egui::RichText::new(&source).monospace()).wrap());
                if let Some(field) = &field {
                    ui.weak(format!(
                        "field {} of {}",
                        field.path.join("."),
                        field.schema_name
                    ));
                }
            });
            ui.end_row();

            // Which widget shows it: any type that can show this value (all
            // of them if the value's type isn't known yet).
            ui.label("Widget");
            let current = ty
                .map_or(widget.widget_type.as_str(), |t| t.names()[0])
                .to_owned();
            let mut picked = current.clone();
            egui::ComboBox::from_id_salt("widget_type")
                .selected_text(&picked)
                .show_ui(ui, |ui| {
                    if ty.is_none() {
                        ui.selectable_value(&mut picked, current.clone(), &current);
                    }
                    for t in TYPES {
                        // Unlisted types (Large Text Display) only for a
                        // widget that is one, so it can stay one.
                        let offered = t.listed() || t.names().contains(&current.as_str());
                        if offered && nt_type.is_none_or(|nt| t.shows(nt)) {
                            let name = t.names()[0];
                            ui.selectable_value(&mut picked, name.to_owned(), name);
                        }
                    }
                })
                .response
                .on_hover_text("How the value is shown. Settings for other types are kept.");
            if picked != current {
                // A period left at the old type's default moves to the new
                // type's (graphs want values faster); a chosen one stays.
                let old_default = ty.map_or(DEFAULT_PERIOD_S, |t| t.default_period());
                if widget.period() == Some(old_default)
                    && let Some(new) = find(&picked)
                {
                    set_property(
                        &mut widget.properties,
                        PERIOD,
                        Some(new.default_period().into()),
                    );
                }
                widget.widget_type = picked;
                // The new type sets up what it can work out.
                if let Some(new) = find(&widget.widget_type) {
                    let value = ValueInfo {
                        nt_type,
                        field: field_info,
                        topics,
                    };
                    new.chosen(&mut widget.properties, value);
                }
            }
            ui.end_row();

            // The schema's type for a struct field (`enum uint8`), else the
            // NT type (`double`).
            let value_type = field_info
                .map(FieldInfo::label)
                .or(nt_type.map(str::to_owned));
            if let Some(value_type) = value_type {
                ui.label("Value");
                ui.vertical(|ui| {
                    ui.label(value_type);
                    if let Some(info) = field_info.filter(|f| f.is_enum()) {
                        ui.weak(info.enum_list());
                    }
                });
                ui.end_row();
            }

            ui.label("Update period");
            let mut period = props::period_s(widget);
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
                set_property(&mut widget.properties, PERIOD, Some(period.into()));
            }
            ui.end_row();

            if let Some(ty) = ty {
                let value = ValueInfo {
                    nt_type,
                    field: field_info,
                    topics,
                };
                ty.settings(ui, &mut widget.properties, value);
            }
        });
    if ty.is_none() {
        ui.weak("This widget type has no other settings yet.");
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::widgets::builtin::boolean_box::{FALSE_COLOR, TRUE_COLOR};
    use crate::widgets::for_topic;
    use crate::widgets::shared::format::ENUM_DISPLAY;
    use crate::widgets::shared::props::DATA_TYPE;
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
        let mut w = for_topic("/Arm/state", nt_type, meta.as_ref(), at).unwrap();
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
            show(ui, w, nt_type, is_enum.then_some(&info), &[]);
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
            ("Field", false, false),
            ("Large Text Display", false, false),
            ("Number Bar", false, false),
            ("Voltage View", false, false),
            ("Radial Gauge", false, false),
            ("Match Time", false, false),
            ("Single Color View", false, false),
            ("Multi Color View", false, false),
        ] {
            let mut w = widget(ty, field);
            let before = w.clone();
            draw(&mut w, None, is_enum);
            assert_eq!(w, before, "{ty}");
        }
    }

    #[test]
    fn fit_text_hides_the_text_size_and_large_text_is_a_fitted_text_display() {
        let has = |labels: &[String], row: &str| labels.iter().any(|l| l == row);
        // A Text Display: a text size until its text is fitted.
        let mut text = widget("Text Display", false);
        let labels = draw(&mut text, Some("double"), false);
        assert!(has(&labels, "Fit text") && has(&labels, "Text size"));
        text.properties.insert("fit_text".into(), true.into());
        assert!(!has(&draw(&mut text, Some("double"), false), "Text size"));
        // A Large Text Display: fitted unless set otherwise, with a Text
        // Display's settings, and still offered as its own type (so it
        // stays one) while no other widget is offered it.
        let mut large = widget("Large Text Display", false);
        let labels = draw(&mut large, Some("double"), false);
        assert!(!has(&labels, "Text size"), "{labels:?}");
        assert!(has(&labels, "Fit text") && has(&labels, "Color by value"));
        assert!(has(&labels, "Large Text Display"));
        assert!(!crate::widgets::types_showing("double").contains(&"Large Text Display"));
        assert!(crate::widgets::types_showing("double").contains(&"Text Display"));
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
            .insert(ENUM_DISPLAY.into(), "both".into());
        assert_eq!(
            rows_for(&mut by_number, Some("int"), true),
            ["Whole numbers", "Digits", "Enum shows", "Text size"]
        );
        // The type row names the enum and lists its values.
        let labels = draw(&mut widget("Text Display", true), Some("int"), true);
        assert!(labels.iter().any(|l| l == "enum uint8"), "{labels:?}");
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
    fn reset_keeps_bindings_and_elastic_defaults() {
        let mut w = widget("Boolean Box", false);
        // As Elastic saves it: the default colors written out. Nothing to reset.
        w.properties.insert(TRUE_COLOR.into(), 4283215696u32.into());
        w.properties
            .insert(FALSE_COLOR.into(), 4294198070u32.into());
        assert!(!has_custom_settings(&w));
        // A custom color and a setting for some other widget type.
        w.properties.insert(TRUE_COLOR.into(), 0xFF2196F3u32.into());
        w.properties.insert("num_of_ticks".into(), 5.into());
        assert!(has_custom_settings(&w));
        reset_settings(&mut w);
        assert!(!w.properties.contains_key(TRUE_COLOR));
        assert_eq!(w.properties[FALSE_COLOR], 4294198070u32);
        assert_eq!(w.properties["num_of_ticks"], 5);
        assert_eq!(w.topic(), Some("/Arm/state"));
        assert_eq!(w.properties[DATA_TYPE], "boolean");

        // A struct field keeps its field.
        let mut field = widget("Text Display", true);
        field.properties.insert(ENUM_DISPLAY.into(), "both".into());
        reset_settings(&mut field);
        assert!(field.struct_meta().is_some());
        assert!(!has_custom_settings(&field));

        // A type not built yet: only the period resets.
        let mut unbuilt = widget("DifferentialDrive", false);
        unbuilt.properties.insert(PERIOD.into(), 0.033.into());
        unbuilt
            .properties
            .insert("show_robot_rotation".into(), false.into());
        reset_settings(&mut unbuilt);
        assert_eq!(unbuilt.period(), Some(DEFAULT_PERIOD_S));
        assert_eq!(unbuilt.properties["show_robot_rotation"], false);

        // A graph keeps its extra lines through a reset.
        let mut lines = widget("Graph", false);
        lines
            .properties
            .insert("series".into(), serde_json::json!([{"topic": "/b"}]));
        lines.properties.insert("line_width".into(), 5.into());
        reset_settings(&mut lines);
        assert!(lines.properties.contains_key("series"));
        assert!(!lines.properties.contains_key("line_width"));
    }
}
