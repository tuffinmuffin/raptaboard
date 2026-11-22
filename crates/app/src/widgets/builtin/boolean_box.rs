//! Boolean Box: fills with one color when the boolean is true and another
//! when false, optionally with text over the color.
//!
//! Settings: `true_color` and `false_color`, and `true_icon` and
//! `false_icon` (Elastic's: `None`, `Checkmark`, `X` or `Exclamation
//! Point`; an icon set for the current state is drawn in that state's color
//! instead of filling the box, as Elastic draws it); and raptaboard's
//! `true_image` and `false_image` (an image from the images folders,
//! [`crate::images`], in place of an icon), `icon_on_color` (fill the box
//! and draw the icon or image on it), `show_text`,
//! `true_text`, `false_text`, `font_size`, `text_backing` (a see-through
//! box behind the text, so it reads over any color) and `true_text_color`
//! and `false_text_color` (automatic black or white if not set). raptaboard
//! lets any icon be used for either state.

use eframe::egui::{self, Color32, CornerRadius, Rect};
use nt4::Value;
use serde_json::{Map, Value as Json};

use crate::images;
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
/// The image shown when true (raptaboard): a file name in the images
/// folders ([`crate::images`]).
pub const TRUE_IMAGE: &str = "true_image";
/// The image shown when false (raptaboard).
pub const FALSE_IMAGE: &str = "false_image";
/// Fill the box and draw the icon (or image) on it, instead of it alone
/// (raptaboard).
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
    /// The image's file name (in the images folders), drawn in place of an
    /// icon.
    pub image: Option<String>,
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
    let (icon_key, text_color_key, image_key) = if on {
        (TRUE_ICON, TRUE_TEXT_COLOR, TRUE_IMAGE)
    } else {
        (FALSE_ICON, FALSE_TEXT_COLOR, FALSE_IMAGE)
    };
    // An image takes the icon's place.
    let image = Some(props::text(p, image_key, "").trim())
        .filter(|n| !n.is_empty())
        .map(str::to_owned);
    let icon = Icon::from_key(props::text(p, icon_key, "")).filter(|_| image.is_none());
    let on_color = props::flag(p, ICON_ON_COLOR, false);
    // An icon alone (Elastic's way) sits on the card, so it's drawn in the
    // state's color; on a filled box it's black or white, whichever reads.
    let fill = ((icon.is_none() && image.is_none()) || on_color).then_some(color);
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
        image,
        text: text.map(|t| (t, text_color)),
        backing,
    })
}

/// Draw the image named `name` (in the images folders) in `rect`, as big
/// as fits with its shape kept; or say it isn't there.
fn draw_image(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect, name: &str) {
    let Some(path) = images::find(name, &images::default_folders()) else {
        return card::draw_message(painter, visuals, rect, &format!("no image {name}"));
    };
    // Still being read: nothing yet (a repaint comes when it's ready).
    let Some(texture) = images::texture(painter.ctx(), &path) else {
        return;
    };
    let [w, h] = texture.size().map(|n| n as f32);
    let scale = (rect.width() / w).min(rect.height() / h);
    let fitted = Rect::from_center_size(rect.center(), egui::vec2(w, h) * scale);
    painter.image(
        texture.id(),
        fitted,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
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

/// What a Boolean Box shows for one state, besides its color: nothing, one
/// of Elastic's drawn icons, or an image (built in or from the images
/// folders).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Picture {
    None,
    Icon(Icon),
    Image(String),
}

impl Picture {
    /// Its name in the list.
    fn label(&self) -> String {
        match self {
            Picture::None => "(none)".to_owned(),
            Picture::Icon(i) => i.key().to_owned(),
            Picture::Image(name) => name.clone(),
        }
    }
}

/// The picture `p` sets with `icon_key` and `image_key` (an image wins, as
/// drawn).
fn picture_of(p: &Map<String, Json>, icon_key: &str, image_key: &str) -> Picture {
    let image = props::text(p, image_key, "").trim();
    if !image.is_empty() {
        return Picture::Image(image.to_owned());
    }
    Icon::from_key(props::text(p, icon_key, "")).map_or(Picture::None, Picture::Icon)
}

/// Save `picture` with `icon_key` and `image_key`: an icon as Elastic's
/// `true_icon`/`false_icon` (so Elastic shows it too), an image by name;
/// picking one clears the other.
fn set_picture(p: &mut Map<String, Json>, icon_key: &str, image_key: &str, picture: &Picture) {
    let (icon, image) = match picture {
        Picture::None => (None, None),
        Picture::Icon(i) => (Some(i.key().into()), None),
        Picture::Image(name) => (None, Some(name.clone().into())),
    };
    set_property(p, icon_key, icon);
    set_property(p, image_key, image);
}

/// A settings row choosing what a state shows: (none); built in (Elastic's
/// drawn icons and the built-in images); a line; the images in the images
/// folders. Import… copies a picked image into `~/.raptaboard/images/` and
/// picks it.
fn picture_row(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    label: &str,
    icon_key: &str,
    image_key: &str,
) {
    // Where a failed import's message is kept, to show under the row.
    let id = ui.scope_id().with(("image import", image_key));
    ui.label(label);
    ui.horizontal(|ui| {
        let current = picture_of(props, icon_key, image_key);
        let mut picked = current.clone();
        let (built_in, external) = images::built_in_and_external(&images::default_folders());
        egui::ComboBox::from_id_salt(image_key)
            .selected_text(picked.label())
            .width(170.0)
            // Tall enough for the built-in pictures and a few of the team's.
            .height(420.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, Picture::None, "(none)");
                ui.weak("Built in");
                for icon in Icon::ALL {
                    ui.selectable_value(&mut picked, Picture::Icon(icon), icon.key());
                }
                for name in &built_in {
                    ui.selectable_value(&mut picked, Picture::Image(name.clone()), name);
                }
                ui.separator();
                if external.is_empty() {
                    ui.weak("No images of your own yet: Import… adds one");
                }
                for name in &external {
                    ui.selectable_value(&mut picked, Picture::Image(name.clone()), name);
                }
            })
            .response
            .on_hover_text(
                "Built in: Elastic's icons (drawn in the state's color) and raptaboard's \
                 images. Below the line: images in the images folders (images/ where \
                 raptaboard starts, beside the program, and ~/.raptaboard/images/).",
            );
        if ui
            .button("Import…")
            .on_hover_text("Pick a PNG, JPEG or SVG; it's copied into ~/.raptaboard/images/")
            .clicked()
            && let Some(from) = rfd::FileDialog::new()
                .add_filter("Image", &images::EXTENSIONS)
                .pick_file()
            && let Some(into) = images::import_folder()
        {
            match images::import(&from, &into) {
                Ok(name) => picked = Picture::Image(name),
                Err(why) => {
                    ui.ctx().data_mut(|d| d.insert_temp(id, why));
                }
            }
        }
        if picked != current {
            set_picture(props, icon_key, image_key, &picked);
        }
    });
    ui.end_row();
    // An import that failed says why, under the row.
    if let Some(why) = ui.ctx().data(|d| d.get_temp::<String>(id)) {
        ui.label("");
        ui.colored_label(ui.visuals().warn_fg_color, why);
        ui.end_row();
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
        // With text too, the icon (or image) takes the top two thirds.
        let pictured = look.icon.is_some() || look.image.is_some();
        let (icon_rect, text_at) = match (pictured, &look.text) {
            (true, Some(_)) => {
                let (top, bottom) = rect.split_top_bottom_at_fraction(0.68);
                (top, bottom.center())
            }
            _ => (rect, rect.center()),
        };
        if let Some((icon, color)) = look.icon {
            draw_icon(painter, icon_rect, icon, color);
        }
        if let Some(name) = &look.image {
            draw_image(painter, visuals, icon_rect.shrink(4.0), name);
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
        picture_row(ui, props, "Picture when true", TRUE_ICON, TRUE_IMAGE);
        picture_row(ui, props, "Picture when false", FALSE_ICON, FALSE_IMAGE);
        props::flag_row(
            ui,
            props,
            "Icon or image on the color",
            "Fill the box and draw the icon or image on it (off: it alone, an icon in the state's \
             color, as Elastic)",
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
            TRUE_IMAGE,
            FALSE_IMAGE,
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
    fn one_list_picks_an_icon_or_an_image_and_clears_the_other() {
        let mut p = props(serde_json::json!({"true_icon": "Checkmark"}));
        assert_eq!(
            picture_of(&p, TRUE_ICON, TRUE_IMAGE),
            Picture::Icon(Icon::Checkmark)
        );
        // An image replaces the icon.
        set_picture(
            &mut p,
            TRUE_ICON,
            TRUE_IMAGE,
            &Picture::Image("motor-on.svg".into()),
        );
        assert_eq!(p.get(TRUE_ICON), None);
        assert_eq!(p[TRUE_IMAGE], "motor-on.svg");
        assert_eq!(
            picture_of(&p, TRUE_ICON, TRUE_IMAGE),
            Picture::Image("motor-on.svg".into())
        );
        // An icon replaces the image, saved as Elastic saves it.
        set_picture(
            &mut p,
            TRUE_ICON,
            TRUE_IMAGE,
            &Picture::Icon(Icon::ExclamationPoint),
        );
        assert_eq!(p[TRUE_ICON], "Exclamation Point");
        assert_eq!(p.get(TRUE_IMAGE), None);
        // None clears both; the false state's keys are left alone.
        p.insert(FALSE_ICON.into(), "X".into());
        set_picture(&mut p, TRUE_ICON, TRUE_IMAGE, &Picture::None);
        assert!(!p.contains_key(TRUE_ICON) && !p.contains_key(TRUE_IMAGE));
        assert_eq!(p[FALSE_ICON], "X");
        // A layout with both set (by hand): the image, as drawn.
        let both = props(serde_json::json!({"true_icon": "X", "true_image": "a.png"}));
        assert_eq!(
            picture_of(&both, TRUE_ICON, TRUE_IMAGE),
            Picture::Image("a.png".into())
        );
    }

    #[test]
    fn an_image_takes_the_icons_place_for_its_state() {
        let value = |b| Some(Value::Boolean(b));
        let p = props(serde_json::json!({
            "true_icon": "Checkmark", "true_image": "intake-down.png", "false_icon": "X"
        }));
        let t = look(&p, value(true).as_ref()).unwrap();
        assert_eq!(t.image.as_deref(), Some("intake-down.png"));
        assert_eq!(t.icon, None, "the image, not the icon");
        assert_eq!(t.fill, None, "alone, as an icon is");
        // The other state keeps its icon.
        let f = look(&p, value(false).as_ref()).unwrap();
        assert_eq!(f.image, None);
        assert_eq!(f.icon, Some((Icon::X, DEFAULT_FALSE_COLOR)));
        // On the color.
        let on = props(serde_json::json!({"true_image": "a.png", "icon_on_color": true}));
        assert_eq!(
            look(&on, value(true).as_ref()).unwrap().fill,
            Some(DEFAULT_TRUE_COLOR)
        );
        // A blank name is no image.
        let blank = props(serde_json::json!({"true_image": "  "}));
        assert_eq!(look(&blank, value(true).as_ref()).unwrap().image, None);
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
