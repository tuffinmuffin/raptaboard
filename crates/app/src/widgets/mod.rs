//! Widgets drawn on the grid: Text Display and Boolean Box so far.
//!
//! Each widget is a card with its title on top and a body that shows the
//! bound topic's newest value. Widget types use Elastic's exact names
//! (`"Text Display"`, `"Boolean Box"`) so Elastic layouts open unchanged.
//! Types that aren't built yet are drawn as a placeholder card.
//!
//! Every widget type is one file implementing [`WidgetType`], listed in
//! [`TYPES`]. The grid and the settings dialog only talk to that trait, so
//! a widget's drawing, settings and setting keys all live in its own file.
//! Plugin widgets (milestone 4) will implement the same trait.
//!
//! Adding a widget type:
//! 1. Make `widgets/my_widget.rs` with a unit struct implementing
//!    [`WidgetType`], and its setting keys as constants.
//! 2. Add it to [`TYPES`].
//! 3. Add its settings to the table in `docs/spec.md`
//!    ("raptaboard widget settings").
//!
//! Shared pieces: [`card`] draws the card around a body, [`format`] writes
//! numbers and enums as text (with their settings), [`props`] reads and
//! writes the settings every widget has, and [`settings`] is the settings
//! dialog's shared rows.
//!
//! Widgets are read-only for now: typing into a Text Display to publish a
//! value comes later.

pub mod boolean_box;
pub mod card;
pub mod format;
pub mod props;
pub mod settings;
pub mod text_display;

use eframe::egui::{self, Rect};
use nt4::Value;
use raptaboard_layout::{Rect as LayoutRect, StructMeta, WidgetContainer};
use serde_json::{Map, Value as Json};

use crate::struct_view::FieldInfo;

/// What the settings dialog knows about the value a widget shows, so a
/// widget offers only the settings that fit it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValueInfo<'a> {
    /// The NT type of the value (`"double"`), if known.
    pub nt_type: Option<&'a str>,
    /// What the schema says about the struct field shown, if it's one.
    pub field: Option<&'a FieldInfo>,
}

impl ValueInfo<'_> {
    /// Whether the value is an enum struct field.
    pub fn is_enum(&self) -> bool {
        self.field.is_some_and(FieldInfo::is_enum)
    }
}

/// One kind of widget: how it's named in layouts, which topics become it,
/// how it draws, and its settings. Built-in widgets are unit structs listed
/// in [`TYPES`].
pub trait WidgetType: Sync {
    /// The `type` names this widget is saved under. The first is the one
    /// raptaboard writes; the rest are Elastic's other names for it.
    fn names(&self) -> &'static [&'static str];

    /// Whether a topic of NT type `nt_type` dragged from the tree becomes
    /// this widget.
    fn accepts(&self, nt_type: &str) -> bool;

    /// Draw the whole widget in `rect` (its grid cells, less a small gap),
    /// for the newest `value` of its topic, if any.
    ///
    /// The default is the standard card: a rounded box with the title on
    /// top and [`draw_body`](Self::draw_body) inside. Override it for a
    /// widget that needs a different frame or none, such as a camera view
    /// drawn edge to edge; [`card`] has the pieces to build one.
    fn draw(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        widget: &WidgetContainer,
        value: Option<&Value>,
    ) {
        let body = card::draw_card(painter, visuals, rect, &widget.title);
        card::draw_clipped(painter, body, |painter, body| {
            self.draw_body(painter, visuals, body, &widget.properties, value);
        });
    }

    /// Draw the widget's body (the card below the title) in `rect`, for the
    /// newest `value` of its topic, if any. Only called by the default
    /// [`draw`](Self::draw).
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    );

    /// The rows this widget adds to the settings dialog's grid, after the
    /// title and update period. Changes go straight into `props`.
    fn settings(&self, ui: &mut egui::Ui, props: &mut Map<String, Json>, value: ValueInfo<'_>);

    /// This widget's own setting keys, all of which default to "not set".
    /// "Reset settings" removes them.
    fn setting_keys(&self) -> &'static [&'static str];

    /// Put any settings that default to a saved value back to it, for
    /// "Reset settings". Most widgets have none.
    fn reset(&self, _props: &mut Map<String, Json>) {}
}

/// Every widget type raptaboard can draw. Types earlier in the list win
/// when more than one accepts a dropped topic.
pub static TYPES: &[&dyn WidgetType] = &[&boolean_box::BooleanBox, &text_display::TextDisplay];

/// The widget type saved as `type_name` (including Elastic's other names
/// for it), or `None` if it isn't built yet.
///
/// ```
/// use raptaboard::widgets::find;
/// assert_eq!(find("Text View").unwrap().names()[0], "Text Display");
/// assert!(find("Boolean Box").is_some());
/// assert!(find("Graph").is_none());
/// ```
pub fn find(type_name: &str) -> Option<&'static dyn WidgetType> {
    TYPES
        .iter()
        .copied()
        .find(|t| t.names().contains(&type_name))
}

/// The widget type name to make for a topic dropped on the grid, by its NT
/// type string: a Boolean Box for booleans, a Text Display for numbers,
/// strings and arrays of them. `None` for types with no widget yet, such as
/// structs (a Pose2d gets the Field widget in milestone 3); those can't be
/// dragged.
///
/// ```
/// use raptaboard::widgets::type_for_topic;
/// assert_eq!(type_for_topic("boolean"), Some("Boolean Box"));
/// assert_eq!(type_for_topic("double"), Some("Text Display"));
/// assert_eq!(type_for_topic("struct:Pose2d"), None);
/// ```
pub fn type_for_topic(nt_type: &str) -> Option<&'static str> {
    TYPES
        .iter()
        .find(|t| t.accepts(nt_type))
        .map(|t| t.names()[0])
}

/// A new widget for the topic `name` of NT type `nt_type`, placed at `rect`.
/// It is titled with the last part of the name and saved the way Elastic
/// saves widgets: `topic`, `period` and `data_type` properties. `None` if
/// [`type_for_topic`] has no widget for the type.
///
/// With `field`, the widget shows one field of the struct topic `name`, and
/// `nt_type` is the field's type. It's saved with Elastic's `struct_meta`
/// and titled like `pose.translation.x`.
///
/// ```
/// use raptaboard::widgets::for_topic;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 128.0, y: 0.0, width: 128.0, height: 128.0 };
/// let w = for_topic("/Drive/Health/allModulesHealthy", "boolean", None, at).unwrap();
/// assert_eq!(w.title, "allModulesHealthy");
/// assert_eq!(w.widget_type, "Boolean Box");
/// assert_eq!(w.topic(), Some("/Drive/Health/allModulesHealthy"));
/// assert_eq!(w.period(), Some(0.06));
/// assert!(for_topic("/Drive/pose", "struct:Pose2d", None, at).is_none());
/// ```
pub fn for_topic(
    name: &str,
    nt_type: &str,
    field: Option<&StructMeta>,
    rect: LayoutRect,
) -> Option<WidgetContainer> {
    let widget_type = type_for_topic(nt_type)?;
    let title = source_title(name, field);
    let mut properties = Map::new();
    properties.insert("topic".into(), name.into());
    properties.insert(props::PERIOD.into(), props::DEFAULT_PERIOD_S.into());
    properties.insert(props::DATA_TYPE.into(), nt_type.into());
    if let Some(field) = field {
        properties.insert("struct_meta".into(), field.to_json(nt_type));
    }
    Some(WidgetContainer {
        title,
        rect,
        widget_type: widget_type.to_owned(),
        properties,
        extra: Map::new(),
    })
}

/// The title a widget gets from what it shows: the last part of the topic
/// name, then the struct field's path if it shows one field of a struct.
///
/// ```
/// use raptaboard::widgets::source_title;
/// use raptaboard_layout::StructMeta;
/// assert_eq!(source_title("/Drive/Health/ok", None), "ok");
/// let x = StructMeta { path: vec!["translation".into(), "x".into()], schema_name: "Pose2d".into() };
/// assert_eq!(source_title("/Drive/pose", Some(&x)), "pose.translation.x");
/// ```
pub fn source_title(name: &str, field: Option<&StructMeta>) -> String {
    let last = name
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(name);
    match field {
        Some(field) => format!("{last}.{}", field.path.join(".")),
        None => last.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn dropped_topics_become_widgets() {
        let at = LayoutRect {
            x: 0.0,
            y: 256.0,
            width: 128.0,
            height: 128.0,
        };
        let w = for_topic("/DashTest/ModeName", "string", None, at).unwrap();
        assert_eq!(
            (w.title.as_str(), w.widget_type.as_str()),
            ("ModeName", "Text Display")
        );
        assert_eq!(w.rect, at);
        assert_eq!(w.properties["data_type"], "string");
        // It can be drawn: the type is one raptaboard knows.
        assert!(find(&w.widget_type).is_some());
        // A name ending in a slash still gets a title.
        assert_eq!(for_topic("/odd/", "double", None, at).unwrap().title, "odd");
        // A struct field keeps the struct topic and says which field.
        let field = StructMeta {
            path: vec!["translation".into(), "x".into()],
            schema_name: "Pose2d".into(),
        };
        let x = for_topic("/Drive/pose", "double", Some(&field), at).unwrap();
        assert_eq!(x.title, "pose.translation.x");
        assert_eq!(x.topic(), Some("/Drive/pose"));
        assert_eq!(x.struct_meta(), Some(field));
        assert_eq!(
            x.properties["struct_meta"],
            serde_json::json!({"path": ["translation", "x"], "schema_name": "Pose2d", "type": "double"})
        );
        // Arrays go in a Text Display; raw bytes and schemas have no widget.
        assert_eq!(type_for_topic("int[]"), Some("Text Display"));
        assert_eq!(type_for_topic("boolean[]"), Some("Text Display"));
        assert_eq!(type_for_topic("raw"), None);
        assert_eq!(type_for_topic("structschema"), None);
        // The JSON matches what Elastic writes for a dropped topic.
        let json = serde_json::to_value(&w).unwrap();
        assert_eq!(
            json["properties"],
            serde_json::json!({"topic": "/DashTest/ModeName", "period": 0.06, "data_type": "string"})
        );
    }

    /// A widget drawn edge to edge, with no card.
    struct Frameless;

    impl WidgetType for Frameless {
        fn names(&self) -> &'static [&'static str] {
            &["Frameless"]
        }
        fn accepts(&self, _: &str) -> bool {
            false
        }
        fn draw(
            &self,
            painter: &egui::Painter,
            _: &egui::Visuals,
            rect: Rect,
            _: &WidgetContainer,
            _: Option<&Value>,
        ) {
            painter.rect_filled(rect, 0, egui::Color32::BLUE);
        }
        fn draw_body(
            &self,
            _: &egui::Painter,
            _: &egui::Visuals,
            _: Rect,
            _: &Map<String, Json>,
            _: Option<&Value>,
        ) {
            unreachable!("draw is overridden")
        }
        fn settings(&self, _: &mut egui::Ui, _: &mut Map<String, Json>, _: ValueInfo<'_>) {}
        fn setting_keys(&self) -> &'static [&'static str] {
            &[]
        }
    }

    /// The text drawn by `ty.draw` for a widget titled "Sine" showing 0.5.
    fn drawn_text(ty: &dyn WidgetType) -> Vec<String> {
        let at = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 128.0,
        };
        let mut widget = for_topic("/Sine", "double", None, at).unwrap();
        widget.title = "Sine".into();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(128.0, 128.0));
            ty.draw(
                ui.painter(),
                ui.visuals(),
                rect,
                &widget,
                Some(&Value::Double(0.5)),
            );
        });
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|s| match s.shape {
                egui::epaint::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn widgets_can_replace_the_card() {
        // The default draw is the card: title, then the body.
        assert_eq!(drawn_text(&text_display::TextDisplay), ["Sine", "0.5"]);
        // An override draws instead, with no card or title.
        assert!(drawn_text(&Frameless).is_empty());
    }

    #[test]
    fn every_type_name_is_unique() {
        // A name in two types would make `find` depend on list order.
        let mut names: Vec<&str> = TYPES
            .iter()
            .flat_map(|t| t.names().iter().copied())
            .collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count);
    }
}
