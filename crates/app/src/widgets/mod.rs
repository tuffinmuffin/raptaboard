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
//! Folders: [`builtin`] holds the widget types themselves, one file each;
//! [`shared`] holds what they share: [`card`] draws the card around a body,
//! [`format`](shared::format) writes numbers and enums as text (with their
//! settings), [`props`] reads and writes the settings many widgets have,
//! and [`settings`](shared::settings) is the settings dialog's shared rows.
//!
//! Adding a widget type:
//! 1. Make `widgets/builtin/my_widget.rs` with a unit struct implementing
//!    [`WidgetType`], and its setting keys as constants, and list the file
//!    in `builtin/mod.rs`.
//! 2. Add it to [`TYPES`].
//! 3. Add its settings to the table in `docs/spec.md`
//!    ("raptaboard widget settings").
//!
//! Widgets that change values (Toggle Switch, Toggle Button, Number
//! Slider) return the new
//! value from [`WidgetType::show`]; the grid sends it with
//! [`crate::nt::NtConnection::write`].

pub mod builtin;
pub mod list_layout;
pub mod shared;

use std::cell::RefCell;
use std::collections::HashMap;

use eframe::egui::{self, Rect};
use nt4::Value;
use raptaboard_layout::{Rect as LayoutRect, StructMeta, WidgetContainer};
use serde_json::{Map, Value as Json};

use crate::traces::SeriesKey;
use crate::tree::DraggedTopic;
use crate::types::alerts::Level;
use crate::types::structs::FieldInfo;
use shared::{card, props};

/// What a widget draws from: its own value, what every widget shares this
/// frame ([`Frame`]), and where it's drawn ([`Place`]).
#[derive(Clone, Copy)]
pub struct Live<'a> {
    /// The newest value of the widget's topic (or struct field), if any.
    pub value: Option<&'a Value>,
    /// What every widget shares this frame.
    pub frame: &'a Frame<'a>,
    /// Where the widget is drawn: on the grid, or as a List Layout child.
    pub place: Place,
}

/// What every widget draws from this frame, the same for all of them.
pub struct Frame<'a> {
    /// Recorded values over time, for graphs.
    pub traces: &'a crate::traces::Traces,
    /// The time now, on the clock the traces use (microseconds).
    pub now_us: i64,
    /// The newest values of the other topics the open tab's widgets asked
    /// for (each widget's [`extra_topics`](WidgetType::extra_topics)), by
    /// full name, such as a chooser's `options` and `active`.
    pub others: &'a HashMap<&'a str, &'a Value>,
    /// Whether the connection to the server is up, so a widget can keep
    /// showing the last thing it saw while it's down (alerts).
    pub connected: bool,
    /// What widgets ask the app to do this frame besides sending a value:
    /// play a tone.
    pub effects: &'a RefCell<Vec<Effect>>,
    /// The server's struct schemas, for widgets that decode a whole struct
    /// (a Field's pose); `None` before they arrive.
    pub structs: Option<&'a nt4::proto::structs::StructDatabase>,
}

impl<'a> Live<'a> {
    /// The newest value of `widget`'s own topic, not the one taken once per
    /// `period`, for widgets that draw it with other topics' newest values
    /// (a Field's robot under robot-relative layers). For a widget bound to
    /// part of a struct (`struct_meta`), the part: [`Live::value`], since the
    /// topic's newest value is the whole struct.
    pub fn newest(&self, widget: &WidgetContainer) -> Option<&'a Value> {
        if widget.struct_meta().is_some() {
            return self.value;
        }
        widget
            .topic()
            .and_then(|t| self.frame.others.get(t).copied())
            .or(self.value)
    }
}

impl<'a> Frame<'a> {
    /// What a widget showing `value` draws from, at `place`.
    pub fn live(&'a self, value: Option<&'a Value>, place: Place) -> Live<'a> {
        Live {
            value,
            frame: self,
            place,
        }
    }
}

/// Where a widget is drawn, which changes how it takes input.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Place {
    /// Whether the layout is locked. Locked, a widget that takes input
    /// takes it anywhere on its card; unlocked, its title bar is left for
    /// moving the widget.
    pub locked: bool,
    /// Where input may start, if not all of the rect the widget is given:
    /// a List Layout child's row less its inside label while unlocked, so
    /// the label still drags the child. `None` for all of it.
    pub input: Option<Rect>,
    /// The part of the rect not under a List Layout child's inside label,
    /// locked or not, for widgets that lay out their value beside it (a
    /// slider's number). `None` when there's no inside label.
    pub free: Option<Rect>,
}

/// Something a widget asks the app to do, through [`Frame::effects`].
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Play a tone (unless sounds are turned off with the bell button).
    Tone(crate::sound::Tone),
    /// Write `value` to `topic`, a topic besides the widget's own (a
    /// Field's target set), publishing it as `ty` if nobody has.
    Write {
        /// The topic's full name.
        topic: String,
        /// What to send.
        value: Value,
        /// Its type, if the dashboard publishes it.
        ty: nt4::NtType,
    },
    /// Publish one struct to `topic`, a topic the dashboard owns (a Field's
    /// target), with the schemas it needs (see
    /// [`NtConnection::publish_struct`](crate::nt::NtConnection::publish_struct)).
    PublishStruct {
        /// The topic's full name.
        topic: String,
        /// The struct's name: `Pose2d`.
        struct_name: &'static str,
        /// The struct's bytes.
        bytes: Vec<u8>,
        /// (struct name, schema) for it and the structs in it.
        schemas: &'static [(&'static str, &'static str)],
    },
}

/// What's on the server, for widgets whose topics depend on more than their
/// own settings (an Alerts widget showing every group).
#[derive(Debug, Clone, Copy, Default)]
pub struct Server<'a> {
    /// Every table with a `.type` topic, by full name (without `/.type`).
    pub tables: &'a [String],
    /// Every topic the server has announced.
    pub topics: &'a [nt4::Topic],
}

/// What the settings dialog knows about the value a widget shows, so a
/// widget offers only the settings that fit it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValueInfo<'a> {
    /// The NT type of the value (`"double"`), if known.
    pub nt_type: Option<&'a str>,
    /// What the schema says about the struct field shown, if it's one.
    pub field: Option<&'a FieldInfo>,
    /// Every topic the server has, as (full name, NT type), for settings
    /// that refer to other topics.
    pub topics: &'a [(String, String)],
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
    /// this widget (the default widget for that type).
    fn accepts(&self, nt_type: &str) -> bool;

    /// Whether this widget can show a value of NT type `nt_type`, for the
    /// settings dialog's choice of widget type. Every type it [`accepts`]
    /// by default, and maybe more.
    ///
    /// [`accepts`]: WidgetType::accepts
    fn shows(&self, nt_type: &str) -> bool {
        self.accepts(nt_type)
    }

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

    /// Draw the widget in `rect` with a full [`egui::Ui`], for widgets that
    /// need more than a painter (a plot, say), the values over time in
    /// `live`, or input. The default is [`draw`](Self::draw).
    ///
    /// Widgets that change their value (a toggle, a slider) return the new
    /// value when the user changes it, and the app sends it to the widget's
    /// topic. Input works locked or not: the lock only stops layout edits.
    /// A widget's input should cover its body, below the title bar, which
    /// moves the widget while unlocked. egui keeps a drag going to the widget
    /// it started on, even when the pointer leaves it.
    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.draw(ui.painter(), ui.visuals(), rect, widget, live.value);
        None
    }

    /// Show just the widget's body (no card or title) in `rect` with a
    /// full [`egui::Ui`], as List Layout children are shown, and return a
    /// value to send if the user changed it. Widgets that take input
    /// override this (and build their [`show`](Self::show) on it, so they
    /// work the same in a list and on the grid); the default is
    /// [`draw_body`](Self::draw_body).
    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.draw_body(
            ui.painter(),
            ui.visuals(),
            rect,
            &widget.properties,
            live.value,
        );
        None
    }

    /// How often this widget wants new values when its `period` isn't set,
    /// in seconds: Elastic's 0.06 unless a widget needs more.
    fn default_period(&self) -> f64 {
        props::DEFAULT_PERIOD_S
    }

    /// The `.type`s of the tables this widget is made for, when a table is
    /// dragged from the tree: a SendableChooser's is `"String Chooser"`.
    /// None by default (it's made for single topics).
    fn table_types(&self) -> &'static [&'static str] {
        &[]
    }

    /// Where the value [`show`](Self::show) returns is sent, for a widget
    /// with settings `props`, if not to the widget's `topic`: a topic under
    /// it (a chooser's `"selected"`), and the NT type it's published as if
    /// nobody has published it yet.
    fn sends_to(&self, _props: &Map<String, Json>) -> Option<(&'static str, nt4::NtType)> {
        None
    }

    /// Called when a widget is changed to this type in its settings, to set
    /// up settings that can be worked out from what's on the server (an
    /// enum widget guesses where its names come from). Nothing by default.
    fn chosen(&self, _props: &mut Map<String, Json>, _value: ValueInfo<'_>) {}

    /// Topics this widget needs values for besides its own `topic`, such as
    /// a graph's other lines, given what's on the `server`. None by default.
    fn extra_topics(&self, _props: &Map<String, Json>, _server: Server<'_>) -> Vec<String> {
        Vec::new()
    }

    /// Topics this widget needs values for while its tab isn't open, for
    /// its [`tab_mark`](Self::tab_mark). None by default.
    fn background_topics(&self, _props: &Map<String, Json>, _server: Server<'_>) -> Vec<String> {
        Vec::new()
    }

    /// The mark this widget puts on its tab, open or not, from the server's
    /// `values`: the most urgent alert it shows. None by default.
    fn tab_mark(
        &self,
        _widget: &WidgetContainer,
        _values: &HashMap<&str, &Value>,
    ) -> Option<Level> {
        None
    }

    /// The series of values over time this widget draws, and how many
    /// seconds of each to keep, for the app to record. None by default.
    fn traced(&self, _widget: &WidgetContainer) -> Vec<(SeriesKey, f64)> {
        Vec::new()
    }

    /// Whether topics dragged from the tree can be dropped onto this widget
    /// (a graph takes them as lines). No by default.
    fn takes_topics(&self) -> bool {
        false
    }

    /// Take `topics` dropped onto `widget`, if [`takes_topics`](Self::takes_topics).
    /// Returns what to say about it in the status line.
    fn add_topics(&self, _widget: &mut WidgetContainer, _topics: &[DraggedTopic]) -> String {
        String::new()
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
pub static TYPES: &[&dyn WidgetType] = &[
    &builtin::boolean_box::BooleanBox,
    &builtin::toggle::ToggleSwitch,
    &builtin::toggle::ToggleButton,
    &builtin::text_display::TextDisplay,
    &builtin::large_text_display::LargeTextDisplay,
    &builtin::number_bar::NumberBar,
    &builtin::number_slider::NumberSlider,
    &builtin::number_bar::VoltageView,
    &builtin::battery::Battery,
    &builtin::radial_gauge::RadialGauge,
    &builtin::match_time::MatchTime,
    &builtin::color_view::SingleColorView,
    &builtin::color_view::MultiColorView,
    &builtin::chooser::DropdownChooser,
    &builtin::chooser::SplitButtonChooser,
    &builtin::enums::EnumDropdown,
    &builtin::enums::EnumToggle,
    &builtin::alerts::AlertsWidget,
    &builtin::graph::Graph,
    &builtin::field::FieldWidget,
    &builtin::gyro::Gyro,
    &builtin::swerve::SwerveDrive,
    &builtin::chassis_speeds::ChassisSpeeds,
    &builtin::pose3d::Pose3dView,
    &builtin::mechanism::Mechanism2d,
    // Last: any struct no widget above is made for.
    &builtin::struct_view::StructView,
];

/// The widget type saved as `type_name` (including Elastic's other names
/// for it), or `None` if it isn't built yet.
///
/// ```
/// use raptaboard::widgets::find;
/// assert_eq!(find("Text View").unwrap().names()[0], "Text Display");
/// assert!(find("Boolean Box").is_some());
/// assert!(find("Gyro").is_some());
/// assert!(find("SwerveDrive").is_some());
/// assert!(find("DifferentialDrive").is_none());
/// ```
pub fn find(type_name: &str) -> Option<&'static dyn WidgetType> {
    TYPES
        .iter()
        .copied()
        .find(|t| t.names().contains(&type_name))
}

/// The widget type name to make for a topic dropped on the grid, by its NT
/// type string: a Boolean Box for booleans, a Text Display for numbers,
/// strings and arrays of them, a Struct for any whole struct (decided with
/// the user 2026-11-21: the Field, Gyro, 3D Pose, swerve view and Chassis
/// Speeds are picked from the tree's "Add as" menu, [`types_showing`]).
/// `None` for types with no widget (raw bytes, schemas); those can't be
/// dragged.
///
/// ```
/// use raptaboard::widgets::type_for_topic;
/// assert_eq!(type_for_topic("boolean"), Some("Boolean Box"));
/// assert_eq!(type_for_topic("double"), Some("Text Display"));
/// // Every whole struct makes a Struct; "Add as" offers the rest.
/// assert_eq!(type_for_topic("struct:Pose2d"), Some("Struct"));
/// assert_eq!(type_for_topic("struct:SwerveModuleState[]"), Some("Struct"));
/// assert_eq!(type_for_topic("struct:ArmState"), Some("Struct"));
/// assert_eq!(type_for_topic("raw"), None);
/// ```
pub fn type_for_topic(nt_type: &str) -> Option<&'static str> {
    TYPES
        .iter()
        .find(|t| t.accepts(nt_type))
        .map(|t| t.names()[0])
}

/// The widget type name to make for a table dropped on the grid, by its
/// `.type` (a SendableChooser is `"String Chooser"`), if there is one.
///
/// ```
/// use raptaboard::widgets::type_for_table;
/// assert_eq!(type_for_table("String Chooser"), Some("Dropdown Chooser"));
/// assert_eq!(type_for_table("Mechanism2d"), Some("Mechanism2d"));
/// assert_eq!(type_for_table("DifferentialDrive"), None);
/// ```
pub fn type_for_table(table_type: &str) -> Option<&'static str> {
    TYPES
        .iter()
        .find(|t| t.table_types().contains(&table_type))
        .map(|t| t.names()[0])
}

/// A new widget for the topic `name` of NT type `nt_type`, placed at `rect`.
/// It is titled with the last part of the name and saved the way Elastic
/// saves widgets: `topic`, `period` and `data_type` properties. `None` if
/// [`type_for_topic`] has no widget for the type.
///
/// `nt_type` may instead be a table's `.type` (see [`type_for_table`]):
/// the widget is bound to the whole table.
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
/// assert!(for_topic("/Arm/bytes", "raw", None, at).is_none());
/// ```
pub fn for_topic(
    name: &str,
    nt_type: &str,
    field: Option<&StructMeta>,
    rect: LayoutRect,
) -> Option<WidgetContainer> {
    let widget_type = type_for_topic(nt_type).or_else(|| type_for_table(nt_type))?;
    for_topic_as(name, nt_type, field, rect, widget_type)
}

/// Like [`for_topic`], as the widget type `widget_type` (one of
/// [`types_showing`]'s), with that type's update period. `None` if there's
/// no such type.
///
/// ```
/// use raptaboard::widgets::for_topic_as;
/// use raptaboard_layout::Rect;
/// let at = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let w = for_topic_as("/Drive/pose", "struct:Pose2d", None, at, "Field").unwrap();
/// assert_eq!(w.widget_type, "Field");
/// assert_eq!(w.properties["data_type"], "struct:Pose2d");
/// let g = for_topic_as("/Arm/angle", "double", None, at, "Graph").unwrap();
/// assert!(g.period().unwrap() < 0.06, "a graph's own period");
/// ```
pub fn for_topic_as(
    name: &str,
    nt_type: &str,
    field: Option<&StructMeta>,
    rect: LayoutRect,
    widget_type: &str,
) -> Option<WidgetContainer> {
    let ty = find(widget_type)?;
    // A table (`nt_type` is its `.type`, which no type shows as a value) is
    // saved with no data type, as Elastic saves one.
    let table = !TYPES.iter().any(|t| t.shows(nt_type)) && ty.table_types().contains(&nt_type);
    let title = source_title(name, field);
    let mut properties = Map::new();
    properties.insert("topic".into(), name.into());
    properties.insert(props::PERIOD.into(), ty.default_period().into());
    if !table {
        properties.insert(props::DATA_TYPE.into(), nt_type.into());
    }
    if let Some(field) = field {
        properties.insert("struct_meta".into(), field.to_json(nt_type));
    }
    Some(WidgetContainer {
        title,
        rect,
        widget_type: ty.names()[0].to_owned(),
        properties,
        extra: Map::new(),
    })
}

/// The widget types that can show a topic of NT type `nt_type` (or a table
/// whose `.type` it is), the one it makes by default first: what the
/// tree's "Add as" menu offers.
///
/// ```
/// use raptaboard::widgets::types_showing;
/// let pose = types_showing("struct:Pose2d");
/// assert_eq!(pose[0], "Struct");
/// assert!(pose.contains(&"Field") && pose.contains(&"3D Pose"));
/// assert_eq!(types_showing("String Chooser")[0], "Dropdown Chooser");
/// assert!(types_showing("raw").is_empty());
/// ```
pub fn types_showing(nt_type: &str) -> Vec<&'static str> {
    let default = type_for_topic(nt_type).or_else(|| type_for_table(nt_type));
    let others = TYPES
        .iter()
        .filter(|t| t.shows(nt_type) || t.table_types().contains(&nt_type))
        .map(|t| t.names()[0])
        .filter(|name| Some(*name) != default);
    default.into_iter().chain(others).collect()
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
/// let item = StructMeta { path: vec!["[2]".into(), "x".into()], schema_name: "Translation2d".into() };
/// assert_eq!(source_title("/Fleet", Some(&item)), "Fleet[2].x");
/// ```
pub fn source_title(name: &str, field: Option<&StructMeta>) -> String {
    let last = name
        .rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(name);
    match field {
        Some(field) => {
            let path = crate::types::structs::path_text(&field.path);
            if path.starts_with('[') {
                format!("{last}{path}")
            } else {
                format!("{last}.{path}")
            }
        }
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
        assert_eq!(
            drawn_text(&builtin::text_display::TextDisplay),
            ["Sine", "0.5"]
        );
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
