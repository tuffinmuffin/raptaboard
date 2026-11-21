//! Boolean Box: fills with one color when the boolean is true and another
//! when false, optionally with text over the color.
//!
//! Settings: `true_color` and `false_color`, and `true_icon` and
//! `false_icon` (Elastic's: `None`, `Checkmark`, `X` or `Exclamation
//! Point`; an icon set for the current state is drawn in that state's color
//! instead of filling the box, as Elastic draws it); and raptaboard's
//! `icon_on_color` (fill the box and draw the icon on it), `show_text`,
//! `true_text`, `false_text`, `font_size`, `text_backing` (a see-through
//! box behind the text, so it reads over any color) and `true_text_color`
//! and `false_text_color` (automatic black or white if not set). raptaboard
//! lets any icon be used for either state.

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

/// The icon drawn when true (Elastic): see [`Icon`].
pub const TRUE_ICON: &str = "true_icon";
/// The icon drawn when false (Elastic).
pub const FALSE_ICON: &str = "false_icon";
/// Fill the box and draw the icon on it, instead of the icon alone in the
/// state's color (raptaboard).
pub const ICON_ON_COLOR: &str = "icon_on_color";
/// A see-through box behind the text (raptaboard).
pub const TEXT_BACKING: &str = "text_backing";
/// The text's color when true (raptaboard); automatic if not set.
pub const TRUE_TEXT_COLOR: &str = "true_text_color";
/// The text's color when false (raptaboard).
pub const FALSE_TEXT_COLOR: &str = "false_text_color";

/// An icon a Boolean Box can draw for a state, by the names Elastic saves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// ✓
    Checkmark,
    /// ✕
    X,
    /// !
    ExclamationPoint,
}

impl Icon {
    /// Every icon, in the order the settings list them.
    pub const ALL: [Icon; 3] = [Icon::Checkmark, Icon::X, Icon::ExclamationPoint];

    /// The name Elastic saves.
    pub fn key(self) -> &'static str {
        match self {
            Icon::Checkmark => "Checkmark",
            Icon::X => "X",
            Icon::ExclamationPoint => "Exclamation Point",
        }
    }

    /// The icon saved as `key` (any case, as Elastic reads it); `None` for
    /// `None` or anything else.
    ///
    /// ```
    /// use raptaboard::widgets::builtin::boolean_box::Icon;
    /// assert_eq!(Icon::from_key("CHECKMARK"), Some(Icon::Checkmark));
    /// assert_eq!(Icon::from_key("None"), None);
    /// ```
    pub fn from_key(key: &str) -> Option<Icon> {
        Self::ALL
            .into_iter()
            .find(|i| i.key().eq_ignore_ascii_case(key.trim()))
    }
}

/// Everything a Boolean Box draws for one value.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// The box's color, if it's filled (not when an icon is drawn alone).
    pub fill: Option<Color32>,
    /// The icon and its color.
    pub icon: Option<(Icon, Color32)>,
    /// The text and its color.
    pub text: Option<(String, Color32)>,
    /// A see-through box behind the text.
    pub backing: bool,
}

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

/// Everything a Boolean Box draws for `value`, from its settings `p`, or
/// why there's nothing (as [`fill`]).
///
/// ```
/// use nt4::Value;
/// use raptaboard::widgets::builtin::boolean_box::{look, Icon, DEFAULT_FALSE_COLOR};
/// let p = serde_json::json!({"false_icon": "X"});
/// let l = look(p.as_object().unwrap(), Some(&Value::Boolean(false))).unwrap();
/// // As Elastic draws it: the icon in the state's color, no fill.
/// assert_eq!(l.fill, None);
/// assert_eq!(l.icon, Some((Icon::X, DEFAULT_FALSE_COLOR)));
/// ```
pub fn look(p: &Map<String, Json>, value: Option<&Value>) -> Result<Look, &'static str> {
    let (color, text) = fill(p, value)?;
    let on = matches!(value, Some(Value::Boolean(true)));
    let (icon_key, text_color_key) = if on {
        (TRUE_ICON, TRUE_TEXT_COLOR)
    } else {
        (FALSE_ICON, FALSE_TEXT_COLOR)
    };
    let icon = Icon::from_key(props::text(p, icon_key, ""));
    let on_color = props::flag(p, ICON_ON_COLOR, false);
    // An icon alone (Elastic's way) sits on the card, so it's drawn in the
    // state's color; on a filled box it's black or white, whichever reads.
    let fill = (icon.is_none() || on_color).then_some(color);
    let ground = fill.unwrap_or(Color32::TRANSPARENT);
    let auto = if fill.is_some() {
        props::text_color_on(color)
    } else {
        color
    };
    let icon = icon.map(|i| (i, if fill.is_some() { auto } else { color }));
    let backing = props::flag(p, TEXT_BACKING, false);
    let text_color = props::color_property(p, text_color_key).unwrap_or(if backing {
        Color32::WHITE
    } else if ground == Color32::TRANSPARENT {
        color
    } else {
        auto
    });
    Ok(Look {
        fill,
        icon,
        text: text.map(|t| (t, text_color)),
        backing,
    })
}

/// Draw `icon` centered in `rect`, as big as fits, in `color`.
fn draw_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let size = rect.width().min(rect.height()) * 0.8;
    let c = rect.center();
    let width = (size * 0.12).max(2.0);
    let stroke = egui::Stroke::new(width, color);
    match icon {
        Icon::Checkmark => {
            let h = size / 2.0;
            painter.add(egui::Shape::line(
                vec![
                    c + egui::vec2(-h * 0.8, 0.0),
                    c + egui::vec2(-h * 0.25, h * 0.55),
                    c + egui::vec2(h * 0.85, -h * 0.6),
                ],
                stroke,
            ));
        }
        Icon::X => {
            let h = size * 0.38;
            painter.line_segment([c + egui::vec2(-h, -h), c + egui::vec2(h, h)], stroke);
            painter.line_segment([c + egui::vec2(-h, h), c + egui::vec2(h, -h)], stroke);
        }
        Icon::ExclamationPoint => {
            let h = size / 2.0;
            painter.line_segment(
                [
                    c + egui::vec2(0.0, -h * 0.85),
                    c + egui::vec2(0.0, h * 0.35),
                ],
                egui::Stroke::new(width * 1.2, color),
            );
            painter.circle_filled(c + egui::vec2(0.0, h * 0.75), width * 0.8, color);
        }
    }
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
        let look = match look(props, value) {
            Ok(look) => look,
            Err(why) => return card::draw_message(painter, visuals, rect, why),
        };
        if let Some(color) = look.fill {
            painter.rect_filled(rect, CornerRadius::same(card::BODY_CORNER_RADIUS), color);
        }
        // With text too, the icon takes the top two thirds.
        let (icon_rect, text_at) = match (&look.icon, &look.text) {
            (Some(_), Some(_)) => {
                let (top, bottom) = rect.split_top_bottom_at_fraction(0.68);
                (top, bottom.center())
            }
            _ => (rect, rect.center()),
        };
        if let Some((icon, color)) = look.icon {
            draw_icon(painter, icon_rect, icon, color);
        }
        if let Some((text, color)) = look.text {
            let galley = painter.layout_no_wrap(
                text,
                egui::FontId::proportional(props::font_size(props)),
                color,
            );
            let at = text_at - galley.size() / 2.0;
            if look.backing {
                let pad = galley.size().y * 0.2;
                painter.rect_filled(
                    Rect::from_min_size(at, galley.size()).expand(pad),
                    CornerRadius::same((pad * 1.5) as u8),
                    Color32::from_black_alpha(150),
                );
            }
            painter.galley(at, galley, color);
        }
    }

    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, _value: ValueInfo<'_>) {
        props::color_row(ui, props, "When true", TRUE_COLOR, DEFAULT_TRUE_COLOR);
        props::color_row(ui, props, "When false", FALSE_COLOR, DEFAULT_FALSE_COLOR);
        for (label, key) in [
            ("Icon when true", TRUE_ICON),
            ("Icon when false", FALSE_ICON),
        ] {
            let mut choices = vec!["None"];
            choices.extend(Icon::ALL.iter().map(|i| i.key()));
            let current = Icon::from_key(props::text(props, key, "")).map_or("None", Icon::key);
            ui.label(label);
            let mut picked = current;
            egui::ComboBox::from_id_salt(key)
                .selected_text(picked)
                .show_ui(ui, |ui| {
                    for c in &choices {
                        ui.selectable_value(&mut picked, *c, *c);
                    }
                });
            if picked != current {
                // Elastic saves "None" too; not set means the same here.
                set_property(props, key, (picked != "None").then(|| picked.into()));
            }
            ui.end_row();
        }
        props::flag_row(
            ui,
            props,
            "Icon on the color",
            "Fill the box and draw the icon on it (off: the icon alone, in the state's color, as Elastic)",
            ICON_ON_COLOR,
            false,
        );

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
            props::flag_row(
                ui,
                props,
                "Text backing",
                "A see-through box behind the text, so it reads over any color",
                TEXT_BACKING,
                false,
            );
            for (label, key) in [
                ("Text color when true", TRUE_TEXT_COLOR),
                ("Text color when false", FALSE_TEXT_COLOR),
            ] {
                ui.label(label);
                ui.horizontal(|ui| {
                    let set = props::color_property(props, key);
                    let mut auto = set.is_none();
                    if ui.checkbox(&mut auto, "Automatic").changed() {
                        let start = props::color_json(Color32::WHITE);
                        set_property(props, key, (!auto).then_some(start));
                    }
                    if let Some(mut color) = set
                        && egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut color,
                            egui::color_picker::Alpha::Opaque,
                        )
                        .changed()
                    {
                        set_property(props, key, Some(props::color_json(color)));
                    }
                });
                ui.end_row();
            }
        }
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &[
            SHOW_TEXT,
            TRUE_TEXT,
            FALSE_TEXT,
            props::FONT_SIZE,
            TRUE_ICON,
            FALSE_ICON,
            ICON_ON_COLOR,
            TEXT_BACKING,
            TRUE_TEXT_COLOR,
            FALSE_TEXT_COLOR,
        ]
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
    fn icons_draw_as_elastic_draws_them_or_on_the_color() {
        let value = |b| Some(Value::Boolean(b));
        // Elastic's: the icon alone, in the state's color, only for its
        // state.
        let elastic = props(serde_json::json!({"true_icon": "Checkmark", "false_icon": "None"}));
        let t = look(&elastic, value(true).as_ref()).unwrap();
        assert_eq!(t.fill, None);
        assert_eq!(t.icon, Some((Icon::Checkmark, DEFAULT_TRUE_COLOR)));
        let f = look(&elastic, value(false).as_ref()).unwrap();
        assert_eq!(f.fill, Some(DEFAULT_FALSE_COLOR));
        assert_eq!(f.icon, None);
        // raptaboard's: on the color, in black or white, and any icon for
        // either state.
        let on =
            props(serde_json::json!({"true_icon": "exclamation point", "icon_on_color": true}));
        let t = look(&on, value(true).as_ref()).unwrap();
        assert_eq!(t.fill, Some(DEFAULT_TRUE_COLOR));
        assert_eq!(
            t.icon,
            Some((
                Icon::ExclamationPoint,
                props::text_color_on(DEFAULT_TRUE_COLOR)
            ))
        );
        // An unknown name is no icon.
        let odd = props(serde_json::json!({"true_icon": "Smiley"}));
        assert_eq!(look(&odd, value(true).as_ref()).unwrap().icon, None);
    }

    #[test]
    fn text_gets_a_backing_and_its_own_color() {
        let value = Some(Value::Boolean(false));
        let plain = props(serde_json::json!({"show_text": true, "false_text": "Jammed"}));
        let l = look(&plain, value.as_ref()).unwrap();
        assert_eq!(
            l.text,
            Some(("Jammed".into(), props::text_color_on(DEFAULT_FALSE_COLOR)))
        );
        assert!(!l.backing);
        // A backing: white text on it unless a color is set, even on a
        // yellow box, where the automatic color would be black.
        let yellow_box = props(serde_json::json!({
            "show_text": true, "false_text": "Jammed", "false_color": 0xFFFFEB3Bu32
        }));
        assert_eq!(
            look(&yellow_box, value.as_ref()).unwrap().text.unwrap().1,
            Color32::BLACK
        );
        let backed = props(serde_json::json!({
            "show_text": true, "false_text": "Jammed", "false_color": 0xFFFFEB3Bu32,
            "text_backing": true
        }));
        assert_eq!(
            look(&backed, value.as_ref()).unwrap().text.unwrap().1,
            Color32::WHITE
        );
        let yellow = props(serde_json::json!({
            "show_text": true, "false_text": "Jammed", "false_text_color": 0xFFFFEB3Bu32
        }));
        assert_eq!(
            look(&yellow, value.as_ref()).unwrap().text.unwrap().1,
            Color32::from_rgb(0xFF, 0xEB, 0x3B)
        );
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
