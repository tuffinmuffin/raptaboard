//! Settings many widgets share: update period, text size and colors, and
//! reading and writing a widget's `properties`.
//!
//! Settings live in each widget's `properties` object in the layout file.
//! Readers here never fail: a missing, wrong-typed or out-of-range value
//! gives the default or the nearest allowed value.

use eframe::egui::{self, Color32};
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

/// Update period in seconds (Elastic).
pub const PERIOD: &str = "period";
/// The NT type of the value shown, such as `"double"` (Elastic).
pub const DATA_TYPE: &str = "data_type";
/// Value text size in points (raptaboard).
pub const FONT_SIZE: &str = "font_size";
/// Values can be changed from the widget (raptaboard's). Off, a widget
/// that writes only shows its value: a Text Display is off unless set, the
/// choosers and enum widgets are on unless set.
pub const EDITABLE: &str = "editable";

/// Elastic's default widget update period, in seconds.
pub const DEFAULT_PERIOD_S: f64 = 0.06;
/// The shortest update period used, in seconds. A saved period of 0 would
/// ask for every change, so shorter ones are raised to this.
pub const MIN_PERIOD_S: f64 = 0.01;
/// The longest update period the settings dialog offers, in seconds.
pub const MAX_PERIOD_S: f64 = 5.0;

/// Size of value text when `font_size` isn't set, in points.
pub const DEFAULT_FONT_SIZE: f32 = 16.0;
/// The smallest `font_size` used, in points.
pub const MIN_FONT_SIZE: f32 = 8.0;
/// The largest `font_size` used, in points.
pub const MAX_FONT_SIZE: f32 = 96.0;

/// Set the property `key` to `value`, or remove it if `value` is `None`.
///
/// ```
/// use raptaboard::widgets::shared::props::set_property;
/// let mut props = serde_json::Map::new();
/// set_property(&mut props, "font_size", Some(20.0.into()));
/// assert_eq!(props["font_size"], 20.0);
/// set_property(&mut props, "font_size", None);
/// assert!(props.is_empty());
/// ```
pub fn set_property(props: &mut Map<String, Json>, key: &str, value: Option<Json>) {
    match value {
        Some(value) => {
            props.insert(key.to_owned(), value);
        }
        None => {
            props.remove(key);
        }
    }
}

/// How often `widget` updates, in seconds: its `period`, or its type's
/// default ([`WidgetType::default_period`]; [`DEFAULT_PERIOD_S`] for most)
/// if not set, and at least [`MIN_PERIOD_S`].
///
/// [`WidgetType::default_period`]: crate::widgets::WidgetType::default_period
///
/// ```
/// use raptaboard::widgets::for_topic;
/// use raptaboard::widgets::shared::props::period_s;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let mut w = for_topic("/Sine", "double", None, at).unwrap();
/// assert_eq!(period_s(&w), 0.06);
/// w.properties.insert("period".into(), 0.0.into());
/// assert_eq!(period_s(&w), 0.01);
/// ```
pub fn period_s(widget: &WidgetContainer) -> f64 {
    let default =
        crate::widgets::find(&widget.widget_type).map_or(DEFAULT_PERIOD_S, |t| t.default_period());
    let period = widget.period().unwrap_or(default);
    if period.is_finite() {
        period.max(MIN_PERIOD_S)
    } else {
        default
    }
}

/// The `font_size` setting in points, kept between [`MIN_FONT_SIZE`] and
/// [`MAX_FONT_SIZE`]; [`DEFAULT_FONT_SIZE`] if not set.
///
/// ```
/// use raptaboard::widgets::shared::props::{font_size, DEFAULT_FONT_SIZE};
/// let props = serde_json::json!({"font_size": 30.0});
/// assert_eq!(font_size(props.as_object().unwrap()), 30.0);
/// assert_eq!(font_size(&serde_json::Map::new()), DEFAULT_FONT_SIZE);
/// ```
pub fn font_size(props: &Map<String, Json>) -> f32 {
    props
        .get(FONT_SIZE)
        .and_then(Json::as_f64)
        .filter(|f| f.is_finite())
        .map_or(DEFAULT_FONT_SIZE, |f| {
            (f as f32).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
        })
}

/// The `font_size` setting, if one is set (`None` for widgets that fit
/// their text when it isn't).
pub fn font_size_set(props: &Map<String, Json>) -> Option<f32> {
    props.contains_key(FONT_SIZE).then(|| font_size(props))
}

/// The "Text size" row of the settings dialog, for `font_size`.
pub fn text_size_row(ui: &mut egui::Ui, props: &mut Map<String, Json>) {
    let name = ui.label("Text size").id;
    ui.horizontal(|ui| {
        if is_controlled(ui, FONT_SIZE) {
            ui.disable();
        }
        let mut size = font_size(props);
        if ui
            .add(
                egui::DragValue::new(&mut size)
                    .range(MIN_FONT_SIZE..=MAX_FONT_SIZE)
                    .speed(0.5)
                    .fixed_decimals(0)
                    .suffix(" pt"),
            )
            .labelled_by(name)
            .changed()
        {
            let value = (size != DEFAULT_FONT_SIZE).then(|| f64::from(size).into());
            set_property(props, FONT_SIZE, value);
        }
        if ui
            .add_enabled(props.contains_key(FONT_SIZE), egui::Button::new("Reset"))
            .clicked()
        {
            set_property(props, FONT_SIZE, None);
        }
    });
    ui.end_row();
}

/// Name a control `name` for screen readers (and tests), for one sharing
/// its row's label with others (a layer's kind and its topic).
pub fn name_control(response: &egui::Response, role: egui::accesskit::Role, name: &str) {
    response.widget_info(|| egui::WidgetInfo::labeled(role, response.enabled(), name));
}

/// What a greyed-out setting says on hover: a group or list sets it.
pub const SET_BY_SHARED: &str =
    "Set by its group or list; untick \"Use the shared settings\" to set it here";

fn controlled_id() -> egui::Id {
    egui::Id::unique("raptaboard settings set by a group")
}

/// Say which settings the settings dialog being drawn shows greyed out,
/// as a group or list sets them ([`is_controlled`]); `&[]` for none. Set
/// around drawing one widget's settings.
pub fn set_controlled(ctx: &egui::Context, keys: &[&str]) {
    let keys: Vec<String> = keys.iter().map(|k| (*k).to_owned()).collect();
    ctx.data_mut(|d| d.insert_temp(controlled_id(), keys));
}

/// Whether the settings dialog being drawn shows `key` greyed out
/// ([`set_controlled`]).
pub fn is_controlled(ui: &egui::Ui, key: &str) -> bool {
    ui.ctx().data(|d| {
        d.get_temp::<Vec<String>>(controlled_id())
            .is_some_and(|keys| keys.iter().any(|k| k == key))
    })
}

/// A number setting, or `default` if it's not set or not a number.
///
/// ```
/// use raptaboard::widgets::shared::props::number;
/// let props = serde_json::json!({"max_value": 13});
/// let props = props.as_object().unwrap();
/// assert_eq!(number(props, "max_value", 1.0), 13.0);
/// assert_eq!(number(props, "min_value", -1.0), -1.0);
/// ```
pub fn number(props: &Map<String, Json>, key: &str, default: f64) -> f64 {
    props
        .get(key)
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite())
        .unwrap_or(default)
}

/// A yes/no setting, or `default` if it's not set or not a boolean.
pub fn flag(props: &Map<String, Json>, key: &str, default: bool) -> bool {
    props.get(key).and_then(Json::as_bool).unwrap_or(default)
}

/// A text setting, or `default` if it's not set or not text.
pub fn text<'a>(props: &'a Map<String, Json>, key: &str, default: &'a str) -> &'a str {
    props.get(key).and_then(Json::as_str).unwrap_or(default)
}

/// A settings dialog row editing a number setting. Back at `default`, the
/// setting is removed. `range` limits what can be typed.
pub fn number_row(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    label: &str,
    key: &str,
    default: f64,
    range: std::ops::RangeInclusive<f64>,
) {
    let name = ui.label(label).id;
    let mut value = number(props, key, default);
    if ui
        .add(egui::DragValue::new(&mut value).range(range).speed(0.1))
        .labelled_by(name)
        .changed()
    {
        set_property(props, key, (value != default).then(|| value.into()));
    }
    ui.end_row();
}

/// A settings dialog row editing a whole-number setting, saved as an
/// integer. Back at `default`, the setting is removed.
pub fn whole_row(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    label: &str,
    key: &str,
    default: i64,
    range: std::ops::RangeInclusive<i64>,
) {
    let name = ui.label(label).id;
    let mut value = number(props, key, default as f64).round() as i64;
    if ui
        .add(egui::DragValue::new(&mut value).range(range))
        .labelled_by(name)
        .changed()
    {
        set_property(props, key, (value != default).then(|| value.into()));
    }
    ui.end_row();
}

/// A settings dialog row with a checkbox for a yes/no setting. Back at
/// `default`, the setting is removed.
pub fn flag_row(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    label: &str,
    text: &str,
    key: &str,
    default: bool,
) {
    ui.label(label);
    let mut on = flag(props, key, default);
    if ui.checkbox(&mut on, text).changed() {
        set_property(props, key, (on != default).then(|| on.into()));
    }
    ui.end_row();
}

/// A settings dialog row choosing one of `choices` for a text setting.
/// Back at `default`, the setting is removed.
pub fn choice_row(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    label: &str,
    key: &str,
    default: &str,
    choices: &[&str],
) {
    let name = ui.label(label).id;
    let current = text(props, key, default).to_owned();
    let mut picked = current.clone();
    egui::ComboBox::from_id_salt(key)
        .selected_text(&picked)
        .show_ui(ui, |ui| {
            for choice in choices {
                ui.selectable_value(&mut picked, (*choice).to_owned(), *choice);
            }
        })
        .response
        .labelled_by(name);
    if picked != current {
        set_property(props, key, (picked != default).then(|| picked.into()));
    }
    ui.end_row();
}

/// A topic picker: one of the server's `topics` (name, NT type) of the
/// types `fits` says, or none (an empty `topic`). Returns whether it
/// changed. `label` is its row label's id (its name for screen readers);
/// `id` tells pickers in one dialog apart.
pub fn topic_picker(
    ui: &mut egui::Ui,
    label: egui::Id,
    id: impl std::hash::Hash + std::fmt::Debug,
    topic: &mut String,
    topics: &[(String, String)],
    fits: impl Fn(&str) -> bool,
) -> bool {
    let mut changed = false;
    let shown = if topic.is_empty() {
        "(none)"
    } else {
        topic.as_str()
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(220.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(topic.is_empty(), "(none)").clicked() {
                topic.clear();
                changed = true;
            }
            for (name, ty) in topics.iter().filter(|(_, ty)| fits(ty)) {
                if ui
                    .selectable_label(topic == name, format!("{name}  ({ty})"))
                    .clicked()
                {
                    *topic = name.clone();
                    changed = true;
                }
            }
        })
        .response
        .labelled_by(label);
    changed
}

/// Read a color saved by Elastic: a 32-bit ARGB number, such as
/// `4283215696` (`0xFF4CAF50`, green).
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::shared::props::color_property;
/// let props = serde_json::json!({"true_color": 4283215696u32});
/// let props = props.as_object().unwrap();
/// assert_eq!(color_property(props, "true_color"), Some(Color32::from_rgb(0x4C, 0xAF, 0x50)));
/// assert_eq!(color_property(props, "false_color"), None);
/// ```
pub fn color_property(props: &Map<String, Json>, key: &str) -> Option<Color32> {
    let argb = u32::try_from(props.get(key)?.as_u64()?).ok()?;
    let [a, r, g, b] = argb.to_be_bytes();
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// A color as Elastic saves it: a 32-bit ARGB number. The opposite of
/// [`color_property`].
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::shared::props::color_json;
/// assert_eq!(color_json(Color32::from_rgb(0x4C, 0xAF, 0x50)), serde_json::json!(4283215696u32));
/// ```
pub fn color_json(color: Color32) -> Json {
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    u32::from_be_bytes([a, r, g, b]).into()
}

/// A settings dialog row with a color button for `key` and a Reset button
/// that removes it, so `default` is used again.
pub fn color_row(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    label: &str,
    key: &str,
    default: Color32,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut color = color_property(props, key).unwrap_or(default);
        let picked = egui::color_picker::color_edit_button_srgba(
            ui,
            &mut color,
            egui::color_picker::Alpha::Opaque,
        )
        .changed();
        if picked {
            set_property(props, key, Some(color_json(color)));
        }
        let custom = color_property(props, key).is_some_and(|c| c != default);
        if ui.add_enabled(custom, egui::Button::new("Reset")).clicked() {
            set_property(props, key, None);
        }
    });
    ui.end_row();
}

/// Like [`color_row`], with a Clear button too: the color made fully
/// see-through (alpha 0, an ARGB of 0 as Elastic saves it), so the card's
/// own background shows, dark or light.
pub fn color_row_clearable(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    label: &str,
    key: &str,
    default: Color32,
) {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut color = color_property(props, key).unwrap_or(default);
        let picked = egui::color_picker::color_edit_button_srgba(
            ui,
            &mut color,
            egui::color_picker::Alpha::Opaque,
        )
        .changed();
        if picked {
            set_property(props, key, Some(color_json(color)));
        }
        let custom = color_property(props, key).is_some_and(|c| c != default);
        if ui.add_enabled(custom, egui::Button::new("Reset")).clicked() {
            set_property(props, key, None);
        }
        let clear = color_property(props, key).is_some_and(|c| c.a() == 0);
        if ui
            .add_enabled(!clear, egui::Button::new("Clear"))
            .on_hover_text("No color: the card's own background shows")
            .clicked()
        {
            set_property(props, key, Some(color_json(Color32::TRANSPARENT)));
        }
    });
    ui.end_row();
}

/// Black or white, whichever is easier to read on `background`.
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::shared::props::text_color_on;
/// assert_eq!(text_color_on(Color32::YELLOW), Color32::BLACK);
/// assert_eq!(text_color_on(Color32::from_rgb(0x8B, 0x00, 0x00)), Color32::WHITE);
/// ```
pub fn text_color_on(background: Color32) -> Color32 {
    // Relative luminance from sRGB, roughly: green looks brightest.
    let [r, g, b, _] = background.to_srgba_unmultiplied();
    let luminance = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    if luminance > 150.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// The "Editable" settings row, for [`EDITABLE`], with `hover` saying what
/// it means for this widget. Back at `default`, the setting is removed.
pub fn editable_row(ui: &mut egui::Ui, props: &mut Map<String, Json>, default: bool, hover: &str) {
    ui.label("Editable").on_hover_text(hover);
    let mut on = flag(props, EDITABLE, default);
    if ui.checkbox(&mut on, "").on_hover_text(hover).changed() {
        set_property(props, EDITABLE, (on != default).then(|| on.into()));
    }
    ui.end_row();
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn props(json: Json) -> Map<String, Json> {
        json.as_object().unwrap().clone()
    }

    #[test]
    fn font_size_is_kept_in_range() {
        assert_eq!(
            font_size(&props(serde_json::json!({"font_size": 500}))),
            MAX_FONT_SIZE
        );
        assert_eq!(
            font_size(&props(serde_json::json!({"font_size": 1}))),
            MIN_FONT_SIZE
        );
        assert_eq!(
            font_size(&props(serde_json::json!({"font_size": "big"}))),
            DEFAULT_FONT_SIZE
        );
    }

    #[test]
    fn colors_round_trip_through_json() {
        for color in [
            Color32::from_rgb(0x4C, 0xAF, 0x50),
            Color32::from_rgb(0xF4, 0x43, 0x36),
            Color32::from_rgb(1, 2, 3),
        ] {
            let mut p = Map::new();
            p.insert("c".into(), color_json(color));
            assert_eq!(color_property(&p, "c"), Some(color));
        }
        let bad = props(serde_json::json!({"a": "green", "b": -1}));
        assert_eq!(color_property(&bad, "a"), None);
        assert_eq!(color_property(&bad, "b"), None);
    }

    #[test]
    fn properties_are_set_and_removed() {
        let mut p = Map::new();
        set_property(&mut p, FONT_SIZE, Some(20.into()));
        assert_eq!(font_size(&p), 20.0);
        set_property(&mut p, FONT_SIZE, None);
        // Removing a key that isn't there is fine.
        set_property(&mut p, "nothing", None);
        assert!(p.is_empty());
    }
}
