//! A widget's text style: the font (Sans or Mono), color, bold and italic,
//! for its value text and for its title, set from the settings dialog's
//! "Style…" buttons. Sizes are kept apart: the value's is `font_size`
//! ([`props::font_size`](super::props::font_size)), the title's
//! `title_font_size` ([`title_size`]).
//!
//! Saved as raptaboard's own settings (Elastic ignores them):
//!
//! | | Value text | Title |
//! | --- | --- | --- |
//! | font | `font_family` (`"sans"` or `"mono"`) | `title_font_family` |
//! | size | `font_size` | `title_font_size` |
//! | color | `font_color` (ARGB, as Elastic saves colors) | `title_color` |
//! | bold | `font_bold` | `title_bold` |
//! | italic | `font_italic` | `title_italic` |
//!
//! Not set, each is the widget's own: a Text Display's value is Mono, a
//! title is Sans at 14 points, colors follow the theme.
//!
//! Bold uses bold font files of our own (`media/fonts/`: Ubuntu Bold, Hack
//! Bold), added to egui by [`install_fonts`] as their own font families;
//! italic is egui's slant.

use std::sync::Arc;

use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect};
use serde_json::{Map, Value as Json};

use super::props::{self, color_json, color_property, flag, set_property};

/// The settings for one piece of text: its font, size, color, bold and
/// italic keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    pub family: &'static str,
    pub size: &'static str,
    pub color: &'static str,
    pub bold: &'static str,
    pub italic: &'static str,
}

/// A widget's value text.
pub const VALUE: Keys = Keys {
    family: "font_family",
    size: props::FONT_SIZE,
    color: "font_color",
    bold: "font_bold",
    italic: "font_italic",
};

/// The value text's style keys (size aside: each widget has its own),
/// for widgets' [`setting_keys`](crate::widgets::WidgetType::setting_keys).
pub const VALUE_STYLE_KEYS: [&str; 4] = ["font_family", "font_color", "font_bold", "font_italic"];

/// The title's keys, size included: every widget has them.
pub const TITLE_KEYS: [&str; 5] = [
    "title_font_family",
    "title_font_size",
    "title_color",
    "title_bold",
    "title_italic",
];

/// A widget's title.
pub const TITLE: Keys = Keys {
    family: "title_font_family",
    size: "title_font_size",
    color: "title_color",
    bold: "title_bold",
    italic: "title_italic",
};

/// The size of a title when `title_font_size` isn't set, points.
pub const DEFAULT_TITLE_SIZE: f32 = 14.0;
/// The largest title, points: it has to fit the card's title strip.
pub const MAX_TITLE_SIZE: f32 = 20.0;

/// The fonts text can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Ubuntu: egui's usual font.
    Sans,
    /// Hack: every letter the same width, so numbers don't jump about.
    Mono,
}

impl Family {
    /// Its name in a layout file.
    pub fn key(self) -> &'static str {
        match self {
            Family::Sans => "sans",
            Family::Mono => "mono",
        }
    }

    /// The family a layout file names, if it's one of ours.
    ///
    /// ```
    /// use raptaboard::widgets::shared::text_style::Family;
    /// assert_eq!(Family::from_key("mono"), Some(Family::Mono));
    /// assert_eq!(Family::from_key("comic"), None);
    /// ```
    pub fn from_key(key: &str) -> Option<Family> {
        [Family::Sans, Family::Mono]
            .into_iter()
            .find(|f| f.key() == key)
    }

    /// Its name for people.
    fn label(self) -> &'static str {
        match self {
            Family::Sans => "Sans",
            Family::Mono => "Mono",
        }
    }

    /// egui's family for it, regular or bold.
    fn egui_family(self, bold: bool) -> FontFamily {
        match (self, bold) {
            (Family::Sans, false) => FontFamily::Proportional,
            (Family::Mono, false) => FontFamily::Monospace,
            (Family::Sans, true) => FontFamily::Name(SANS_BOLD.into()),
            (Family::Mono, true) => FontFamily::Name(MONO_BOLD.into()),
        }
    }
}

/// egui's name for the bold Sans family.
const SANS_BOLD: &str = "raptaboard-sans-bold";
/// egui's name for the bold Mono family.
const MONO_BOLD: &str = "raptaboard-mono-bold";

/// Add the bold fonts to egui, once per window: each in a family of its
/// own. egui takes them from the next frame; until then bold text is
/// drawn regular ([`TextStyle::font`]).
pub fn install_fonts(ctx: &egui::Context) {
    for (name, bytes) in [
        (
            SANS_BOLD,
            &include_bytes!("../../../../../media/fonts/Ubuntu-Bold.ttf")[..],
        ),
        (
            MONO_BOLD,
            &include_bytes!("../../../../../media/fonts/Hack-Bold.ttf")[..],
        ),
    ] {
        ctx.add_font(egui::epaint::text::FontInsert::new(
            name,
            egui::FontData::from_static(bytes),
            vec![egui::epaint::text::InsertFontFamily {
                family: FontFamily::Name(name.into()),
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        ));
    }
}

/// How one piece of text is drawn: each part `None` or `false` where it
/// isn't set, the widget's own then used.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStyle {
    pub family: Option<Family>,
    pub color: Option<Color32>,
    pub bold: bool,
    pub italic: bool,
}

impl TextStyle {
    /// The style `props` sets for the text `keys` name.
    ///
    /// ```
    /// use raptaboard::widgets::shared::text_style::{Family, TextStyle, VALUE};
    /// let p = serde_json::json!({"font_family": "sans", "font_bold": true});
    /// let style = TextStyle::read(p.as_object().unwrap(), VALUE);
    /// assert_eq!(style.family, Some(Family::Sans));
    /// assert!(style.bold && !style.italic);
    /// ```
    pub fn read(props: &Map<String, Json>, keys: Keys) -> Self {
        TextStyle {
            family: props
                .get(keys.family)
                .and_then(Json::as_str)
                .and_then(Family::from_key),
            color: color_property(props, keys.color),
            bold: flag(props, keys.bold, false),
            italic: flag(props, keys.italic, false),
        }
    }

    /// The font at `size` points: its family, or `default` if it has none;
    /// bold once [`install_fonts`] has taken effect in `ctx`'s window.
    pub fn font(&self, ctx: &egui::Context, default: Family, size: f32) -> FontId {
        let family = self.family.unwrap_or(default);
        if self.bold {
            let bold = family.egui_family(true);
            if ctx.fonts(|f| f.definitions().families.contains_key(&bold)) {
                return FontId::new(size, bold);
            }
            install_fonts(ctx);
        }
        FontId::new(size, family.egui_family(false))
    }

    /// Its color, or `default` if it has none.
    pub fn color_or(&self, default: Color32) -> Color32 {
        self.color.unwrap_or(default)
    }

    /// `text` laid out in `font` and `color`, slanted if italic.
    pub fn galley(
        &self,
        painter: &egui::Painter,
        text: &str,
        font: FontId,
        color: Color32,
    ) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::single_section(
            text.to_owned(),
            egui::TextFormat {
                font_id: font,
                color,
                italics: self.italic,
                ..Default::default()
            },
        );
        job.wrap.max_rows = 1;
        job.break_on_newline = false;
        painter.layout_job(job)
    }

    /// Draw `text` in this style, `font` and `color`, placed at `pos` by
    /// `align` as [`egui::Painter::text`] does; returns where it went.
    pub fn text(
        &self,
        painter: &egui::Painter,
        pos: Pos2,
        align: egui::Align2,
        text: &str,
        font: FontId,
        color: Color32,
    ) -> Rect {
        let galley = self.galley(painter, text, font, color);
        let rect = align.anchor_size(pos, galley.size());
        painter.galley(rect.min, galley, color);
        rect
    }
}

/// The title's size, `title_font_size` kept between 8 points and
/// [`MAX_TITLE_SIZE`]; [`DEFAULT_TITLE_SIZE`] if not set.
///
/// ```
/// use raptaboard::widgets::shared::text_style::{title_size, DEFAULT_TITLE_SIZE};
/// assert_eq!(title_size(&serde_json::Map::new()), DEFAULT_TITLE_SIZE);
/// let p = serde_json::json!({"title_font_size": 40.0});
/// assert_eq!(title_size(p.as_object().unwrap()), 20.0);
/// ```
pub fn title_size(props: &Map<String, Json>) -> f32 {
    props
        .get(TITLE.size)
        .and_then(Json::as_f64)
        .filter(|f| f.is_finite())
        .map_or(DEFAULT_TITLE_SIZE, |f| {
            (f as f32).clamp(props::MIN_FONT_SIZE, MAX_TITLE_SIZE)
        })
}

/// Whether `props` sets any of `keys`' style (size aside).
fn styled(props: &Map<String, Json>, keys: Keys) -> bool {
    [keys.family, keys.color, keys.bold, keys.italic]
        .iter()
        .any(|k| props.contains_key(*k))
}

/// A "Style…" button whose pop-up sets `keys`' font, color, bold and
/// italic in `props`; `default_family` and `default_color` are the
/// widget's own, used when not set. With no `default_color`, no color row:
/// for a widget whose text colors are its own settings. Reset removes them
/// all.
pub fn style_button(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    keys: Keys,
    default_family: Family,
    default_color: Option<Color32>,
) {
    let label = if styled(props, keys) {
        "Style… (set)"
    } else {
        "Style…"
    };
    ui.menu_button(label, |ui| {
        egui::Grid::new(("text style", keys.family))
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Font");
                let now = TextStyle::read(props, keys).family;
                let shown = now.map_or_else(
                    || format!("{} (usual)", default_family.label()),
                    |f| f.label().to_owned(),
                );
                egui::ComboBox::from_id_salt(("font family", keys.family))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for family in [Family::Sans, Family::Mono] {
                            if ui
                                .selectable_label(now == Some(family), family.label())
                                .clicked()
                            {
                                let value =
                                    (family != default_family).then(|| Json::from(family.key()));
                                set_property(props, keys.family, value);
                            }
                        }
                    });
                ui.end_row();

                if let Some(default_color) = default_color {
                    ui.label("Color");
                    ui.horizontal(|ui| {
                        let mut color = color_property(props, keys.color).unwrap_or(default_color);
                        if egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut color,
                            egui::color_picker::Alpha::Opaque,
                        )
                        .changed()
                        {
                            set_property(props, keys.color, Some(color_json(color)));
                        }
                        if ui
                            .add_enabled(props.contains_key(keys.color), egui::Button::new("Theme"))
                            .on_hover_text("Follow the theme's text color")
                            .clicked()
                        {
                            set_property(props, keys.color, None);
                        }
                    });
                    ui.end_row();
                }

                ui.label("");
                ui.horizontal(|ui| {
                    for (key, label) in [(keys.bold, "Bold"), (keys.italic, "Italic")] {
                        let mut on = flag(props, key, false);
                        if ui.checkbox(&mut on, label).changed() {
                            set_property(props, key, on.then_some(Json::Bool(true)));
                        }
                    }
                });
                ui.end_row();
            });
        ui.separator();
        if ui
            .add_enabled(styled(props, keys), egui::Button::new("Reset style"))
            .clicked()
        {
            for k in [keys.family, keys.color, keys.bold, keys.italic] {
                set_property(props, k, None);
            }
        }
    });
}

/// The value text's settings row, "Text": its size (when `size` gives the
/// usual one; `None` for text fitted to the card) and a "Style…" button for
/// its font (`family` usually), bold, italic and, with `color`, color.
pub fn text_row(
    ui: &mut egui::Ui,
    p: &mut Map<String, Json>,
    size: Option<f32>,
    family: Family,
    color: bool,
) {
    ui.label("Text");
    ui.horizontal(|ui| {
        if let Some(usual) = size {
            let mut now = props::font_size_set(p).unwrap_or(usual);
            if ui
                .add(
                    egui::DragValue::new(&mut now)
                        .range(props::MIN_FONT_SIZE..=props::MAX_FONT_SIZE)
                        .speed(0.5)
                        .fixed_decimals(0)
                        .suffix(" pt"),
                )
                .on_hover_text("Text size")
                .changed()
            {
                let value = (now != usual).then(|| f64::from(now).into());
                set_property(p, VALUE.size, value);
            }
            if ui
                .add_enabled(p.contains_key(VALUE.size), egui::Button::new("Reset"))
                .on_hover_text("The usual size")
                .clicked()
            {
                set_property(p, VALUE.size, None);
            }
        }
        let color = color.then(|| ui.visuals().text_color());
        style_button(ui, p, VALUE, family, color);
    });
    ui.end_row();
}

/// The title's settings: its size and a "Style…" button, beside the
/// title's text box in the settings dialog.
pub fn title_style_row(ui: &mut egui::Ui, props: &mut Map<String, Json>, text_color: Color32) {
    ui.label("Title text");
    ui.horizontal(|ui| {
        let mut size = title_size(props);
        if ui
            .add(
                egui::DragValue::new(&mut size)
                    .range(props::MIN_FONT_SIZE..=MAX_TITLE_SIZE)
                    .speed(0.5)
                    .fixed_decimals(0)
                    .suffix(" pt"),
            )
            .changed()
        {
            let value = (size != DEFAULT_TITLE_SIZE).then(|| f64::from(size).into());
            set_property(props, TITLE.size, value);
        }
        style_button(ui, props, TITLE, Family::Sans, Some(text_color));
    });
    ui.end_row();
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    /// Run one frame of `ctx` with `body`.
    fn frame(ctx: &egui::Context, body: impl FnMut(&mut egui::Ui)) {
        let mut output = ctx.run_ui(egui::RawInput::default(), body);
        output.textures_delta.clear();
    }

    #[test]
    fn a_style_reads_its_own_keys_and_ignores_what_it_doesnt_know() {
        let p = serde_json::json!({
            "title_font_family": "mono", "title_bold": true, "title_color": 4294901760u32,
            "font_family": "comic", "font_italic": true,
        });
        let p = p.as_object().unwrap();
        let title = TextStyle::read(p, TITLE);
        assert_eq!(title.family, Some(Family::Mono));
        assert!(title.bold && !title.italic);
        assert_eq!(title.color, Some(Color32::from_rgb(255, 0, 0)));
        let value = TextStyle::read(p, VALUE);
        assert_eq!(value.family, None, "an unknown font is the widget's own");
        assert!(value.italic && !value.bold);
        assert_eq!(value.color_or(Color32::WHITE), Color32::WHITE);
    }

    #[test]
    fn bold_is_drawn_regular_until_the_fonts_are_in_then_bold() {
        let ctx = egui::Context::default();
        let bold = TextStyle {
            bold: true,
            ..Default::default()
        };
        let mut fonts = Vec::new();
        for _ in 0..3 {
            frame(&ctx, |ui| {
                fonts.push(bold.font(ui.ctx(), Family::Mono, 16.0));
            });
        }
        assert_eq!(fonts[0], FontId::monospace(16.0));
        assert_eq!(
            fonts[2],
            FontId::new(16.0, FontFamily::Name(MONO_BOLD.into()))
        );
        let sans = TextStyle {
            family: Some(Family::Sans),
            ..bold
        };
        frame(&ctx, |ui| {
            assert_eq!(
                sans.font(ui.ctx(), Family::Mono, 12.0),
                FontId::new(12.0, FontFamily::Name(SANS_BOLD.into()))
            );
            // Bold text really is wider than regular.
            let painter = ui.painter();
            let regular = TextStyle::default().galley(
                painter,
                "88.8",
                FontId::proportional(20.0),
                Color32::WHITE,
            );
            let thick = sans.galley(
                painter,
                "88.8",
                sans.font(ui.ctx(), Family::Sans, 20.0),
                Color32::WHITE,
            );
            assert!(
                thick.size().x > regular.size().x,
                "{} {}",
                thick.size().x,
                regular.size().x
            );
        });
    }

    #[test]
    fn italic_text_is_laid_out_slanted() {
        let ctx = egui::Context::default();
        frame(&ctx, |ui| {
            let italic = TextStyle {
                italic: true,
                ..Default::default()
            };
            let galley = italic.galley(
                ui.painter(),
                "x",
                FontId::proportional(20.0),
                Color32::WHITE,
            );
            assert!(galley.job.sections[0].format.italics);
            assert!(
                !TextStyle::default()
                    .galley(
                        ui.painter(),
                        "x",
                        FontId::proportional(20.0),
                        Color32::WHITE
                    )
                    .job
                    .sections[0]
                    .format
                    .italics
            );
        });
    }

    #[test]
    fn the_title_size_is_kept_small_enough_for_its_strip() {
        let p = serde_json::json!({"title_font_size": 2.0});
        assert_eq!(title_size(p.as_object().unwrap()), props::MIN_FONT_SIZE);
        let p = serde_json::json!({"title_font_size": "big"});
        assert_eq!(title_size(p.as_object().unwrap()), DEFAULT_TITLE_SIZE);
    }
}

#[cfg(test)]
mod drawn {
    // Tests written by Claude.
    //! Widgets drawing their title and value in the style their settings
    //! give, through the usual [`card::draw`](super::super::card::draw).

    use super::*;
    use crate::widgets::for_topic_as;
    use crate::widgets::shared::card;
    use eframe::egui::epaint::Shape;
    use nt4::Value;

    /// A widget of `ty` on `/Test/x` (`nt_type`) with `props`, drawn with
    /// `value` for three frames (the bold fonts come in on the second); each
    /// text drawn on the last, with its format.
    fn texts(
        ty: &str,
        nt_type: &str,
        props: serde_json::Value,
        value: Value,
    ) -> Vec<(String, egui::TextFormat)> {
        let at = raptaboard_layout::Rect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 256.0,
        };
        let mut w = for_topic_as("/Test/x", nt_type, None, at, ty).unwrap();
        w.properties.extend(props.as_object().unwrap().clone());
        let ctx = egui::Context::default();
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(256.0, 256.0));
                card::draw(ui.painter(), ui.visuals(), rect, &w, Some(&value));
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        shapes
            .into_iter()
            .filter_map(|s| match s.shape {
                Shape::Text(t) => Some((
                    t.galley.text().to_owned(),
                    t.galley.job.sections[0].format.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    fn find<'a>(texts: &'a [(String, egui::TextFormat)], text: &str) -> &'a egui::TextFormat {
        &texts
            .iter()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("no {text:?} in {texts:?}"))
            .1
    }

    const RED: u32 = 0xFFFF_0000;

    #[test]
    fn a_text_display_draws_its_title_and_value_in_their_styles() {
        let drawn = texts(
            "Text Display",
            "string",
            serde_json::json!({
                "title_font_size": 18.0, "title_bold": true, "title_color": RED,
                "font_family": "sans", "font_italic": true, "font_size": 30.0,
            }),
            Value::String("kShoot".into()),
        );
        let title = find(&drawn, "x");
        assert_eq!(
            title.font_id,
            FontId::new(18.0, FontFamily::Name(SANS_BOLD.into()))
        );
        assert_eq!(title.color, Color32::from_rgb(255, 0, 0));
        assert!(!title.italics);
        let value = find(&drawn, "kShoot");
        assert_eq!(value.font_id, FontId::proportional(30.0));
        assert!(value.italics);
        // Unset, the usual: a Sans 14 point title and a Mono value.
        let plain = texts(
            "Text Display",
            "string",
            serde_json::json!({}),
            Value::String("kShoot".into()),
        );
        assert_eq!(
            find(&plain, "x").font_id,
            FontId::proportional(DEFAULT_TITLE_SIZE)
        );
        assert_eq!(
            find(&plain, "kShoot").font_id,
            FontId::monospace(props::DEFAULT_FONT_SIZE)
        );
        assert!(!find(&plain, "kShoot").italics);
    }

    #[test]
    fn fitted_text_keeps_its_style_and_a_color_beats_the_background_ink() {
        let drawn = texts(
            "Text Display",
            "string",
            serde_json::json!({"fit_text": true, "font_family": "mono", "font_bold": true, "font_color": RED}),
            Value::String("GO".into()),
        );
        let value = find(&drawn, "GO");
        assert_eq!(value.font_id.family, FontFamily::Name(MONO_BOLD.into()));
        assert!(value.font_id.size > 30.0, "fitted: {}", value.font_id.size);
        assert_eq!(value.color, Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn a_boolean_box_text_is_bold_in_its_own_color() {
        let drawn = texts(
            "Boolean Box",
            "boolean",
            serde_json::json!({"show_text": true, "true_text": "OK", "font_bold": true, "font_family": "mono"}),
            Value::Boolean(true),
        );
        let ok = find(&drawn, "OK");
        assert_eq!(ok.font_id.family, FontFamily::Name(MONO_BOLD.into()));
    }

    #[test]
    fn reset_settings_clears_the_title_and_value_styles() {
        let at = raptaboard_layout::Rect {
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 128.0,
        };
        let mut w = for_topic_as("/Test/x", "string", None, at, "Text Display").unwrap();
        for key in TITLE_KEYS.iter().chain(VALUE_STYLE_KEYS.iter()) {
            w.properties.insert((*key).into(), true.into());
        }
        crate::widgets::shared::settings::reset_settings(&mut w);
        for key in TITLE_KEYS.iter().chain(VALUE_STYLE_KEYS.iter()) {
            assert!(!w.properties.contains_key(*key), "{key}");
        }
    }
}
