//! A widget's text style: the font (Sans, Mono, Serif, Narrow or Digital), color, bold and italic,
//! for its value text and for its title, set from the settings dialog's
//! "Style…" buttons. Sizes are kept apart: the value's is `font_size`
//! ([`props::font_size`](super::props::font_size)), the title's
//! `title_font_size` ([`title_size`]).
//!
//! Saved as raptaboard's own settings (Elastic ignores them):
//!
//! | | Value text | Title |
//! | --- | --- | --- |
//! | font | `font_family` (`"sans"`, `"mono"`, `"serif"`, `"narrow"` or `"digital"`) | `title_font_family` |
//! | size | `font_size` | `title_font_size` |
//! | color | `font_color` (ARGB, as Elastic saves colors) | `title_color` |
//! | bold | `font_bold` | `title_bold` |
//! | italic | `font_italic` | `title_italic` |
//!
//! Not set, each is the widget's own: a Text Display's value is Mono, a
//! title is Sans at 14 points, colors follow the theme.
//!
//! Fonts other than egui's own two (Ubuntu Light and Hack) are files of ours
//! (`media/fonts/`: Ubuntu and Hack Bold, PT Serif, PT Sans Narrow, DSEG7
//! Classic), added to egui by [`install_fonts`] as their own font families;
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
    /// PT Serif: letters with serifs, for a book-like look.
    Serif,
    /// PT Sans Narrow: tall and thin, so more fits in a small tile.
    Narrow,
    /// DSEG7 Classic: a seven-segment display, for timers and scores.
    /// It has digits and a few letters; the rest come from Sans.
    Digital,
}

impl Family {
    /// Every family, in the order the Font row shows them.
    pub const ALL: [Family; 5] = [
        Family::Sans,
        Family::Mono,
        Family::Serif,
        Family::Narrow,
        Family::Digital,
    ];

    /// Its name in a layout file.
    pub fn key(self) -> &'static str {
        match self {
            Family::Sans => "sans",
            Family::Mono => "mono",
            Family::Serif => "serif",
            Family::Narrow => "narrow",
            Family::Digital => "digital",
        }
    }

    /// The family a layout file names, if it's one of ours.
    ///
    /// ```
    /// use raptaboard::widgets::shared::text_style::Family;
    /// assert_eq!(Family::from_key("mono"), Some(Family::Mono));
    /// assert_eq!(Family::from_key("digital"), Some(Family::Digital));
    /// assert_eq!(Family::from_key("comic"), None);
    /// ```
    pub fn from_key(key: &str) -> Option<Family> {
        Family::ALL.into_iter().find(|f| f.key() == key)
    }

    /// Its name for people.
    fn label(self) -> &'static str {
        match self {
            Family::Sans => "Sans",
            Family::Mono => "Mono",
            Family::Serif => "Serif",
            Family::Narrow => "Narrow",
            Family::Digital => "Digital",
        }
    }

    /// egui's family for it, regular or bold: egui's own two for regular
    /// Sans and Mono, else one of ours ([`install_fonts`]).
    fn egui_family(self, bold: bool) -> FontFamily {
        match (self, bold) {
            (Family::Sans, false) => FontFamily::Proportional,
            (Family::Mono, false) => FontFamily::Monospace,
            _ => FontFamily::Name(Self::font_name(self, bold).into()),
        }
    }

    /// Our name for its font file, regular or bold (not used for regular
    /// Sans and Mono, which are egui's).
    fn font_name(self, bold: bool) -> &'static str {
        match (self, bold) {
            (Family::Sans, _) => SANS_BOLD,
            (Family::Mono, _) => MONO_BOLD,
            (Family::Serif, false) => "raptaboard-serif",
            (Family::Serif, true) => "raptaboard-serif-bold",
            (Family::Narrow, false) => "raptaboard-narrow",
            (Family::Narrow, true) => "raptaboard-narrow-bold",
            (Family::Digital, false) => "raptaboard-digital",
            (Family::Digital, true) => "raptaboard-digital-bold",
        }
    }
}

/// egui's name for the bold Sans family.
const SANS_BOLD: &str = "raptaboard-sans-bold";
/// egui's name for the bold Mono family.
const MONO_BOLD: &str = "raptaboard-mono-bold";

/// Our font files (`media/fonts/`): the family each is in, bold or not,
/// and the egui family that draws the characters it lacks.
const FONT_FILES: [(Family, bool, FontFamily, &[u8]); 8] = [
    (
        Family::Sans,
        true,
        FontFamily::Proportional,
        include_bytes!("../../../../../media/fonts/Ubuntu-Bold.ttf"),
    ),
    (
        Family::Mono,
        true,
        FontFamily::Monospace,
        include_bytes!("../../../../../media/fonts/Hack-Bold.ttf"),
    ),
    (
        Family::Serif,
        false,
        FontFamily::Proportional,
        include_bytes!("../../../../../media/fonts/PTSerif-Regular.ttf"),
    ),
    (
        Family::Serif,
        true,
        FontFamily::Proportional,
        include_bytes!("../../../../../media/fonts/PTSerif-Bold.ttf"),
    ),
    (
        Family::Narrow,
        false,
        FontFamily::Proportional,
        include_bytes!("../../../../../media/fonts/PTSansNarrow-Regular.ttf"),
    ),
    (
        Family::Narrow,
        true,
        FontFamily::Proportional,
        include_bytes!("../../../../../media/fonts/PTSansNarrow-Bold.ttf"),
    ),
    (
        Family::Digital,
        false,
        FontFamily::Proportional,
        include_bytes!("../../../../../media/fonts/DSEG7Classic-Regular.ttf"),
    ),
    (
        Family::Digital,
        true,
        FontFamily::Proportional,
        include_bytes!("../../../../../media/fonts/DSEG7Classic-Bold.ttf"),
    ),
];

/// Add our fonts to egui, once per window: each in a family of its own,
/// followed by egui's fonts for the characters it lacks. egui takes them
/// from the next frame; until then text in them is drawn in egui's own
/// ([`TextStyle::font`]).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = ctx.fonts(|f| f.definitions().clone());
    for (family, bold, fallback, bytes) in FONT_FILES {
        let name = family.font_name(bold);
        fonts
            .font_data
            .entry(name.to_owned())
            .or_insert_with(|| Arc::new(egui::FontData::from_static(bytes)));
        let mut chain = vec![name.to_owned()];
        chain.extend(
            fonts
                .families
                .get(&fallback)
                .into_iter()
                .flatten()
                .filter(|font| font.as_str() != name)
                .cloned(),
        );
        fonts.families.insert(family.egui_family(bold), chain);
    }
    ctx.set_fonts(fonts);
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
        let wanted = family.egui_family(self.bold);
        if matches!(wanted, FontFamily::Name(_)) {
            if ctx.fonts(|f| f.definitions().families.contains_key(&wanted)) {
                return FontId::new(size, wanted);
            }
            install_fonts(ctx);
            // Until then, the nearest of egui's own.
            let stand_in = if family == Family::Mono {
                FontFamily::Monospace
            } else {
                FontFamily::Proportional
            };
            return FontId::new(size, stand_in);
        }
        FontId::new(size, wanted)
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

/// A "Style…" button that shows or hides `keys`' style rows
/// ([`style_rows`]), whether they're shown being kept in `ui`'s memory;
/// returns whether to show them. It says "(set)" while any is set.
fn style_toggle(ui: &mut egui::Ui, props: &Map<String, Json>, keys: Keys) -> bool {
    let id = ui.scope_id().with(("text style shown", keys.family));
    let mut shown = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let label = if styled(props, keys) {
        "Style… (set)"
    } else {
        "Style…"
    };
    if ui
        .add(egui::Button::new(label).selected(shown))
        .on_hover_text("Font, color, bold and italic")
        .clicked()
    {
        shown = !shown;
        ui.data_mut(|d| d.insert_temp(id, shown));
    }
    shown
}

/// The settings dialog's rows for `keys`' style, under the row with its
/// Style… button: Font (`default_family` is the widget's own), Color (none
/// without `default_color`: a widget whose text colors are its own
/// settings), Bold and Italic, and Reset style. Rows in the dialog, not a
/// pop-up, so the color picker and each change leave them open.
fn style_rows(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    keys: Keys,
    default_family: Family,
    default_color: Option<Color32>,
) {
    ui.label("    Font");
    let now = TextStyle::read(props, keys)
        .family
        .unwrap_or(default_family);
    let size = egui::TextStyle::Button.resolve(ui.style()).size;
    // Each name in its own font, so you see what you'd get.
    let label = |ui: &egui::Ui, family: Family| {
        let text = if family == default_family {
            format!("{} (usual)", family.label())
        } else {
            family.label().to_owned()
        };
        let font = TextStyle {
            family: Some(family),
            ..Default::default()
        }
        .font(ui.ctx(), family, size);
        egui::RichText::new(text).font(font)
    };
    let mut picked = now;
    let font_set_by_shared = props::is_controlled(ui, keys.family);
    ui.add_enabled_ui(!font_set_by_shared, |ui| {
        let response = egui::ComboBox::from_id_salt(("text style font", keys.family))
            .selected_text(label(ui, now))
            .show_ui(ui, |ui| {
                for family in Family::ALL {
                    let text = label(ui, family);
                    ui.selectable_value(&mut picked, family, text);
                }
            })
            .response;
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, "Font")
        });
        response.on_disabled_hover_text(props::SET_BY_SHARED);
    });
    if picked != now {
        let value = (picked != default_family).then(|| Json::from(picked.key()));
        set_property(props, keys.family, value);
    }
    ui.end_row();

    if let Some(default_color) = default_color {
        ui.label("    Color");
        ui.horizontal(|ui| {
            if props::is_controlled(ui, keys.color) {
                ui.disable();
            }
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
            let enabled = !props::is_controlled(ui, key);
            if ui
                .add_enabled(enabled, egui::Checkbox::new(&mut on, label))
                .on_disabled_hover_text(props::SET_BY_SHARED)
                .changed()
            {
                set_property(props, key, on.then_some(Json::Bool(true)));
            }
        }
        if ui
            .add_enabled(styled(props, keys), egui::Button::new("Reset style"))
            .clicked()
        {
            for k in [keys.family, keys.color, keys.bold, keys.italic] {
                set_property(props, k, None);
            }
        }
    });
    ui.end_row();
}

/// The value text's settings row, "Text": its size (when `size` gives the
/// usual one; `None` for text fitted to the card) and a "Style…" button
/// showing rows for its font (`family` usually), bold, italic and, with
/// `color`, color.
pub fn text_row(
    ui: &mut egui::Ui,
    p: &mut Map<String, Json>,
    size: Option<f32>,
    family: Family,
    color: bool,
) {
    ui.label("Text");
    let shown = ui
        .horizontal(|ui| {
            if let Some(usual) = size {
                let set_by_shared = props::is_controlled(ui, VALUE.size);
                let mut now = props::font_size_set(p).unwrap_or(usual);
                if ui
                    .add_enabled(
                        !set_by_shared,
                        egui::DragValue::new(&mut now)
                            .range(props::MIN_FONT_SIZE..=props::MAX_FONT_SIZE)
                            .speed(0.5)
                            .fixed_decimals(0)
                            .suffix(" pt"),
                    )
                    .on_hover_text("Text size")
                    .on_disabled_hover_text(props::SET_BY_SHARED)
                    .changed()
                {
                    let value = (now != usual).then(|| f64::from(now).into());
                    set_property(p, VALUE.size, value);
                }
                if ui
                    .add_enabled(
                        p.contains_key(VALUE.size) && !set_by_shared,
                        egui::Button::new("Reset"),
                    )
                    .on_hover_text("The usual size")
                    .clicked()
                {
                    set_property(p, VALUE.size, None);
                }
            }
            style_toggle(ui, p, VALUE)
        })
        .inner;
    ui.end_row();
    if shown {
        let color = color.then(|| ui.visuals().text_color());
        style_rows(ui, p, VALUE, family, color);
    }
}

/// The title's settings: its size and a "Style…" button showing its style
/// rows, under the title's text box in the settings dialog.
pub fn title_style_row(ui: &mut egui::Ui, props: &mut Map<String, Json>, text_color: Color32) {
    title_style_row_as(ui, props, text_color, "Title text", DEFAULT_TITLE_SIZE);
}

/// [`title_style_row`] named `label`, with `usual` the size when none is
/// set (a list's labels are smaller than titles).
pub fn title_style_row_as(
    ui: &mut egui::Ui,
    props: &mut Map<String, Json>,
    text_color: Color32,
    label: &str,
    usual: f32,
) {
    ui.label(label);
    let shown = ui
        .horizontal(|ui| {
            let mut size = props
                .get(TITLE.size)
                .and_then(Json::as_f64)
                .map_or(usual, |s| s as f32)
                .clamp(props::MIN_FONT_SIZE, MAX_TITLE_SIZE);
            if ui
                .add_enabled(
                    !props::is_controlled(ui, TITLE.size),
                    egui::DragValue::new(&mut size)
                        .range(props::MIN_FONT_SIZE..=MAX_TITLE_SIZE)
                        .speed(0.5)
                        .fixed_decimals(0)
                        .suffix(" pt"),
                )
                .on_hover_text("Title size")
                .on_disabled_hover_text(props::SET_BY_SHARED)
                .changed()
            {
                let value = (size != usual).then(|| f64::from(size).into());
                set_property(props, TITLE.size, value);
            }
            style_toggle(ui, props, TITLE)
        })
        .inner;
    ui.end_row();
    if shown {
        style_rows(ui, props, TITLE, Family::Sans, Some(text_color));
    }
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
    fn each_font_is_its_own_once_in_and_falls_back_to_sans() {
        let ctx = egui::Context::default();
        // Ask once so the fonts are installed, then they're in next frame.
        for _ in 0..2 {
            frame(&ctx, |ui| {
                for family in Family::ALL {
                    for bold in [false, true] {
                        let style = TextStyle {
                            family: Some(family),
                            bold,
                            ..Default::default()
                        };
                        style.font(ui.ctx(), Family::Sans, 16.0);
                    }
                }
            });
        }
        frame(&ctx, |ui| {
            let width = |family: Family, text: &str| {
                let style = TextStyle {
                    family: Some(family),
                    ..Default::default()
                };
                let font = style.font(ui.ctx(), Family::Sans, 20.0);
                style
                    .galley(ui.painter(), text, font, Color32::WHITE)
                    .size()
                    .x
            };
            for family in [Family::Serif, Family::Narrow, Family::Digital] {
                for bold in [false, true] {
                    let style = TextStyle {
                        family: Some(family),
                        bold,
                        ..Default::default()
                    };
                    assert_eq!(
                        style.font(ui.ctx(), Family::Sans, 16.0),
                        FontId::new(16.0, FontFamily::Name(family.font_name(bold).into())),
                        "{family:?} bold {bold}"
                    );
                }
                assert_ne!(
                    width(family, "12.5"),
                    width(Family::Sans, "12.5"),
                    "{family:?} draws its own digits"
                );
            }
            // Each is a font of its own: no two the same width.
            let widths: Vec<f32> = Family::ALL
                .iter()
                .map(|&f| width(f, "Speed 12.5"))
                .collect();
            for (i, a) in widths.iter().enumerate() {
                for b in &widths[i + 1..] {
                    assert_ne!(a, b, "{widths:?}");
                }
            }
            assert!(width(Family::Narrow, "Speed") < width(Family::Sans, "Speed"));
            // What DSEG7 lacks comes from egui's Sans, not a missing box.
            let chain = ctx.fonts(|f| {
                f.definitions().families[&FontFamily::Name("raptaboard-digital".into())].clone()
            });
            assert_eq!(chain[0], "raptaboard-digital");
            assert!(chain.contains(&"Ubuntu-Light".to_owned()), "{chain:?}");
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
    fn settings_a_group_sets_are_greyed_out_and_left_alone() {
        use egui_kittest::kittest::{NodeT, Queryable};
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui_state(
                |ui, p: &mut Map<String, Json>| {
                    // As a widget in a group setting bold and the text size.
                    props::set_controlled(ui.ctx(), &[VALUE.bold, VALUE.size]);
                    egui::Grid::new("rows").show(ui, |ui| {
                        text_row(ui, p, Some(16.0), Family::Mono, true);
                    });
                    props::set_controlled(ui.ctx(), &[]);
                },
                // Its own size, so Reset would be on.
                serde_json::json!({"font_size": 20.0})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        harness.get_by_label("Style…").click();
        harness.run();
        assert!(harness.get_by_label("Bold").accesskit_node().is_disabled());
        assert!(
            !harness
                .get_by_label("Italic")
                .accesskit_node()
                .is_disabled()
        );
        harness.get_by_label("Bold").click();
        harness.run();
        harness.get_by_label("Italic").click();
        harness.run();
        assert_eq!(harness.state().get(VALUE.bold), None);
        assert_eq!(harness.state().get(VALUE.italic), Some(&Json::Bool(true)));
        // The text size's Reset too, though it has its own size.
        assert!(harness.get_by_label("Reset").accesskit_node().is_disabled());
        assert_eq!(harness.state().get(VALUE.size), Some(&Json::from(20.0)));
    }

    #[test]
    fn the_style_rows_stay_open_while_its_settings_change() {
        use egui_kittest::kittest::Queryable;
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(600.0, 400.0))
            .build_ui_state(
                |ui, p: &mut Map<String, Json>| {
                    egui::Grid::new("rows").show(ui, |ui| {
                        text_row(ui, p, Some(16.0), Family::Mono, true);
                    });
                    ui.label("elsewhere");
                },
                Map::new(),
            );
        let open =
            |h: &egui_kittest::Harness<'_, Map<String, Json>>| h.query_by_label("Italic").is_some();
        assert!(!open(&harness));
        harness.get_by_label("Style…").click();
        harness.run();
        assert!(open(&harness));
        // Each change leaves them open, clicks elsewhere too.
        harness.get_by_label("Bold").click();
        harness.run();
        assert!(open(&harness), "closed after Bold");
        assert_eq!(harness.state().get(VALUE.bold), Some(&Json::Bool(true)));
        harness.get_by_label("Italic").click();
        harness.run();
        let font_list = |h: &egui_kittest::Harness<'_, Map<String, Json>>| {
            h.get_by(|n| {
                n.role() == egui::accesskit::Role::ComboBox && n.label().as_deref() == Some("Font")
            })
            .click();
        };
        font_list(&harness);
        harness.run();
        harness.get_by_label("Sans").click();
        harness.run();
        assert!(open(&harness), "closed after picking a font");
        assert!(
            harness.query_by_label("Digital").is_none(),
            "the list closed"
        );
        assert_eq!(harness.state().get(VALUE.family), Some(&Json::from("sans")));
        harness.get_by_label("elsewhere").click();
        harness.run();
        assert!(open(&harness), "closed after a click elsewhere");
        // The usual font removes the setting; Reset style removes the rest.
        font_list(&harness);
        harness.run();
        harness.get_by_label("Mono (usual)").click();
        harness.run();
        assert_eq!(harness.state().get(VALUE.family), None);
        harness.get_by_label("Reset style").click();
        harness.run();
        assert!(harness.state().is_empty(), "{:?}", harness.state());
        // The button hides them again.
        harness.get_by_label("Style…").click();
        harness.run();
        assert!(!open(&harness));
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
