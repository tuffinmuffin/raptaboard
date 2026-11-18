//! The NetworkTables tree: topics grouped into folders by their `/` paths.
//!
//! NT has no real folders. A topic like `/Drive/Module/frontLeft/speed` is
//! shown under the folders `Drive`, `Module` and `frontLeft`. Building the
//! tree is kept apart from drawing it so it can be tested without a window.

use std::collections::BTreeMap;

use eframe::egui;
use nt4::Value;
use nt4::proto::structs::StructDatabase;
use raptaboard_layout::StructMeta;
use raptaboard_plugin_host::path;

use crate::struct_view::{self, FieldInfo, Shown};
use crate::widgets;

/// One topic as the tree shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TopicEntry {
    /// Full topic name, such as `/Telemetry/Arm/angle_rad`.
    pub name: String,
    /// Type string the server announced, such as `double` or `struct:Pose2d`.
    pub ty: String,
    /// Newest value, if this topic's value has been received.
    pub value: Option<Value>,
}

impl From<nt4::Topic> for TopicEntry {
    fn from(t: nt4::Topic) -> Self {
        TopicEntry {
            name: t.name.to_string(),
            ty: t.ty.to_string(),
            value: t.value.map(|v| v.value),
        }
    }
}

/// What a topic row carries while it's dragged out of the tree, for the
/// grid to make a widget from when it's dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedTopic {
    /// Full topic name.
    pub name: String,
    /// NT type string, such as `boolean`. For a struct field, the field's
    /// type.
    pub ty: String,
    /// Set when one field of a struct topic is dragged.
    pub field: Option<StructMeta>,
}

/// A folder: sub-folders and topics, each sorted by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Folder {
    /// Sub-folders by name.
    pub folders: BTreeMap<String, Folder>,
    /// Topics directly in this folder, by their last path segment.
    pub topics: BTreeMap<String, TopicEntry>,
}

impl Folder {
    /// Build the tree from a list of topics.
    ///
    /// - `show_metadata`: include hidden topics, whose path has a part starting
    ///   with a dot (`.type`, `.name`, everything under `/.schema/`).
    /// - `filter`: keep only topics whose full name contains this text,
    ///   ignoring case. Empty keeps everything.
    ///
    /// ```
    /// use raptaboard::tree::{Folder, TopicEntry};
    /// let topic = |name: &str| TopicEntry { name: name.into(), ty: "double".into(), value: None };
    /// let tree = Folder::build([topic("/Arm/angle"), topic("/Arm/.type")], false, "");
    /// assert!(tree.folders["Arm"].topics.contains_key("angle"));
    /// assert!(!tree.folders["Arm"].topics.contains_key(".type"));
    /// ```
    pub fn build(
        topics: impl IntoIterator<Item = TopicEntry>,
        show_metadata: bool,
        filter: &str,
    ) -> Folder {
        let filter = filter.trim().to_lowercase();
        let mut root = Folder::default();
        for topic in topics {
            let parts: Vec<String> = path::segments(&topic.name)
                .into_iter()
                .map(str::to_owned)
                .collect();
            let Some((key, folders)) = parts.split_last() else {
                continue;
            };
            if !show_metadata && parts.iter().any(|p| path::is_metadata(p)) {
                continue;
            }
            if !filter.is_empty() && !topic.name.to_lowercase().contains(&filter) {
                continue;
            }
            let mut folder = &mut root;
            for name in folders {
                folder = folder.folders.entry(name.clone()).or_default();
            }
            folder.topics.insert(key.clone(), topic);
        }
        root
    }

    /// How many topics are in this folder and every folder under it.
    pub fn topic_count(&self) -> usize {
        self.topics.len()
            + self
                .folders
                .values()
                .map(Folder::topic_count)
                .sum::<usize>()
    }

    /// Draw this folder's contents. `path` is the folder's own path (`""` for
    /// the root); `expand_all` opens every folder, used while filtering.
    /// `structs` holds the struct schemas the server has sent, used to open struct topics
    /// such as `Pose2d` into their fields.
    ///
    /// When `draggable` is set (the layout isn't locked), rows for topics
    /// that have a widget type can be dragged onto the grid; see
    /// [`DraggedTopic`] and [`widgets::type_for_topic`].
    ///
    /// Returns the full names of the topics that were drawn, that is, those
    /// in open folders, so their values can be requested from the server.
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        path: &str,
        expand_all: bool,
        structs: &StructDatabase,
        draggable: bool,
    ) -> Vec<String> {
        let mut visible = Vec::new();
        for (name, folder) in &self.folders {
            let folder_path = format!("{path}/{name}");
            let label = format!("{name}  ({})", folder.topic_count());
            let open = egui::CollapsingHeader::new(label)
                .id_salt(&folder_path)
                .default_open(false)
                .open(expand_all.then_some(true))
                .show(ui, |ui| {
                    folder.show(ui, &folder_path, expand_all, structs, draggable)
                });
            visible.extend(open.body_returned.unwrap_or_default());
        }
        for (key, topic) in &self.topics {
            let has_widget = widgets::type_for_topic(&topic.ty).is_some();
            if draggable && has_widget {
                let payload = DraggedTopic {
                    name: topic.name.clone(),
                    ty: topic.ty.clone(),
                    field: None,
                };
                let id = egui::Id::unique(("nt_topic", &topic.name));
                ui.dnd_drag_source(id, payload, |ui| {
                    // Plain labels, so the pointer shows the grab hand rather
                    // than the text cursor.
                    ui.style_mut().interaction.selectable_labels = false;
                    show_topic(
                        ui,
                        key,
                        topic,
                        structs,
                        "Drag onto the grid to make a widget",
                        false,
                    );
                });
            } else {
                // Rows that open (structs, schemas) must not be drag sources:
                // egui gives a drag area on top of a header the clicks.
                let note = if has_widget || struct_view::struct_type(&topic.ty).is_some() {
                    String::new()
                } else {
                    format!("No widget for {} yet", topic.ty)
                };
                show_topic(ui, key, topic, structs, &note, draggable);
            }
            visible.push(topic.name.clone());
        }
        visible
    }
}

/// One topic row. Struct values that can be decoded open into their fields,
/// which can be dragged onto the grid when `draggable` is set. `note`, if
/// not empty, is added to the hover text.
fn show_topic(
    ui: &mut egui::Ui,
    key: &str,
    topic: &TopicEntry,
    structs: &StructDatabase,
    note: &str,
    draggable: bool,
) {
    let bytes = match &topic.value {
        Some(Value::Raw(bytes)) => Some(bytes.as_slice()),
        _ => None,
    };
    let schema = struct_view::struct_type(&topic.ty)
        .and_then(|(name, _)| structs.find(name))
        .map(|d| d.schema().to_owned());
    let mut hover = match &schema {
        Some(schema) => format!("{}\nschema: {schema}", topic.name),
        None => topic.name.clone(),
    };
    if !note.is_empty() {
        hover = format!("{hover}\n{note}");
    }
    if let Some(Shown::Node { summary, children }) =
        bytes.and_then(|b| struct_view::decode(structs, &topic.ty, b))
    {
        // Fields of a single struct can be dragged; Elastic doesn't bind to
        // fields inside struct arrays either.
        let source = match (struct_view::struct_type(&topic.ty), bytes) {
            (Some((schema, false)), Some(bytes)) if draggable => Some(FieldSource {
                topic: &topic.name,
                schema,
                bytes,
                structs,
            }),
            _ => None,
        };
        let fields = struct_view::struct_type(&topic.ty).map(|(schema, _)| Fields {
            topic: &topic.name,
            schema,
            structs,
        });
        egui::CollapsingHeader::new(format!("{key}  {summary}"))
            .id_salt(&topic.name)
            .default_open(false)
            .show(ui, |ui| {
                show_children(
                    ui,
                    &topic.name,
                    &children,
                    fields.as_ref(),
                    source.as_ref(),
                    &[],
                )
            })
            .header_response
            .on_hover_text(hover);
        return;
    }
    // A schema topic opens into its fields, enum lists and nested structs.
    if topic.ty == "structschema"
        && let Some(text) = bytes.map(String::from_utf8_lossy)
        && let Ok(Shown::Node { summary, children }) = struct_view::describe_schema(structs, &text)
    {
        egui::CollapsingHeader::new(format!("{key}  schema, {summary}"))
            .id_salt(&topic.name)
            .default_open(false)
            .show(ui, |ui| {
                show_children(ui, &topic.name, &children, None, None, &[])
            })
            .header_response
            .on_hover_text(format!("{}\n{text}", topic.name));
        return;
    }
    let value = match (&topic.value, bytes) {
        // Schema text that didn't parse is shown as it is, such as "double x;double y".
        (_, Some(b)) if topic.ty == "structschema" => {
            format!("\"{}\"", shorten(&String::from_utf8_lossy(b)))
        }
        (Some(v), _) => format_value(v),
        (None, _) => "…".into(),
    };
    ui.horizontal(|ui| {
        ui.label(key);
        ui.monospace(value);
        ui.weak(&topic.ty);
    })
    .response
    .on_hover_text(hover);
}

/// A struct topic whose field rows can be dragged onto the grid.
struct FieldSource<'a> {
    topic: &'a str,
    /// The struct's schema name, such as `Pose2d`.
    schema: &'a str,
    bytes: &'a [u8],
    structs: &'a StructDatabase,
}

/// A decoded struct topic, for looking up its fields' types.
struct Fields<'a> {
    topic: &'a str,
    /// The struct's schema name, such as `Pose2d`.
    schema: &'a str,
    structs: &'a StructDatabase,
}

impl Fields<'_> {
    /// The field at `path`, if the schema has it.
    fn info(&self, path: &[String]) -> Option<FieldInfo> {
        struct_view::field_info(self.structs, self.schema, path)
    }

    /// Hover text for the field at `path`: where it is, its type, and its
    /// enum names, such as
    /// `/Arm/state → mode` / `enum uint8 field of Arm` / `kStow = 0, kIntake = 3`.
    fn hover(&self, path: &[String], info: Option<&FieldInfo>) -> String {
        let mut where_ = String::new();
        for name in path {
            if !where_.is_empty() && !name.starts_with('[') {
                where_.push('.');
            }
            where_.push_str(name);
        }
        let mut text = format!("{} → {where_}", self.topic);
        if let Some(info) = info {
            text.push_str(&format!("\n{} field of {}", info.label(), self.schema));
            if info.is_enum() {
                text.push_str(&format!("\n{}", info.enum_list()));
            }
        }
        text
    }
}

/// The fields of a decoded struct; nested structs and arrays open in turn.
/// With `fields`, each field shows its type and has hover details. With
/// `source`, plain fields (not array items) can be dragged onto the grid.
/// `path` is the field names leading here.
fn show_children(
    ui: &mut egui::Ui,
    id: &str,
    children: &[(String, Shown)],
    fields: Option<&Fields<'_>>,
    source: Option<&FieldSource<'_>>,
    path: &[String],
) {
    // Used for decoded values and for schemas; nested parts start open.
    for (name, shown) in children {
        // Array items are named [0], [1], ...; they can't be bound to.
        let source = source.filter(|_| !name.starts_with('['));
        let field_path: Vec<String> = path.iter().cloned().chain([name.clone()]).collect();
        let info = fields.and_then(|f| f.info(&field_path));
        let hover = fields.map(|f| f.hover(&field_path, info.as_ref()));
        match shown {
            Shown::Leaf(text) => {
                let row = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        ui.monospace(text);
                        if let Some(info) = &info {
                            ui.weak(info.label());
                        }
                    })
                };
                let payload = source.and_then(|src| {
                    let value =
                        struct_view::field_value(src.structs, src.schema, src.bytes, &field_path)?;
                    let ty = struct_view::nt_type_name(&value);
                    widgets::type_for_topic(ty)?;
                    Some(DraggedTopic {
                        name: src.topic.to_owned(),
                        ty: ty.to_owned(),
                        field: Some(StructMeta {
                            path: field_path.clone(),
                            schema_name: src.schema.to_owned(),
                        }),
                    })
                });
                match payload {
                    Some(payload) => {
                        let drag_id = egui::Id::unique(("nt_field", id, name));
                        let hover = hover.map_or_else(String::new, |h| h + "\n\n");
                        ui.dnd_drag_source(drag_id, payload, |ui| {
                            ui.style_mut().interaction.selectable_labels = false;
                            row(ui)
                        })
                        .response
                        .on_hover_text(format!(
                            "{hover}Drag onto the grid to make a widget for this field"
                        ));
                    }
                    None => {
                        let row = row(ui).response;
                        if let Some(hover) = hover {
                            row.on_hover_text(hover);
                        }
                    }
                }
            }
            Shown::Node { summary, children } => {
                let child_id = format!("{id}/{name}");
                // The type after the value, greyed like a leaf row's.
                let mut title = egui::text::LayoutJob::default();
                let style = ui.style();
                egui::RichText::new(format!("{name}  {summary}")).append_to(
                    &mut title,
                    style,
                    egui::FontSelection::Default,
                    egui::Align::Center,
                );
                if let Some(info) = &info {
                    egui::RichText::new(format!("  {}", info.label()))
                        .weak()
                        .append_to(
                            &mut title,
                            style,
                            egui::FontSelection::Default,
                            egui::Align::Center,
                        );
                }
                let header = egui::CollapsingHeader::new(title)
                    .id_salt(&child_id)
                    .default_open(true)
                    .show(ui, |ui| {
                        show_children(ui, &child_id, children, fields, source, &field_path)
                    })
                    .header_response;
                if let Some(hover) = hover {
                    header.on_hover_text(hover);
                }
            }
        }
    }
}

/// Most array items shown before the rest are summarized.
const MAX_ITEMS: usize = 6;
/// Longest string shown before it is cut off, in characters.
const MAX_CHARS: usize = 40;

/// A short one-line form of a value for the tree.
///
/// Numbers keep up to 4 decimal places, long strings and arrays are cut
/// short, and raw bytes show their length. Struct values are opened into
/// fields by the tree instead (see [`crate::struct_view`]).
///
/// ```
/// use raptaboard::tree::format_value;
/// use nt4::Value;
/// assert_eq!(format_value(&Value::Double(0.5)), "0.5");
/// assert_eq!(format_value(&Value::Double(1.0 / 3.0)), "0.3333");
/// assert_eq!(format_value(&Value::BooleanArray(vec![true, false])), "[true, false]");
/// assert_eq!(format_value(&Value::Raw(vec![0; 24])), "24 bytes");
/// ```
pub fn format_value(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        Value::Double(d) => format_number(*d),
        Value::Float(f) => format_number(f64::from(*f)),
        Value::Int(i) => i.to_string(),
        Value::String(s) => format!("\"{}\"", shorten(s)),
        Value::Raw(bytes) => format!("{} bytes", bytes.len()),
        Value::BooleanArray(a) => format_list(a, |b| b.to_string()),
        Value::DoubleArray(a) => format_list(a, |d| format_number(*d)),
        Value::FloatArray(a) => format_list(a, |f| format_number(f64::from(*f))),
        Value::IntArray(a) => format_list(a, |i| i.to_string()),
        Value::StringArray(a) => format_list(a, |s| format!("\"{}\"", shorten(s))),
    }
}

/// A number with up to 4 decimal places and no trailing zeros.
pub(crate) fn format_number(n: f64) -> String {
    if !n.is_finite() {
        return n.to_string();
    }
    let text = format!("{n:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

fn shorten(s: &str) -> String {
    if s.chars().count() <= MAX_CHARS {
        s.to_owned()
    } else {
        let cut: String = s.chars().take(MAX_CHARS).collect();
        format!("{cut}…")
    }
}

fn format_list<T>(items: &[T], one: impl Fn(&T) -> String) -> String {
    let shown: Vec<String> = items.iter().take(MAX_ITEMS).map(one).collect();
    let more = items.len().saturating_sub(MAX_ITEMS);
    if more == 0 {
        format!("[{}]", shown.join(", "))
    } else {
        format!("[{}, … +{more}]", shown.join(", "))
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn topic(name: &str) -> TopicEntry {
        TopicEntry {
            name: name.into(),
            ty: "double".into(),
            value: None,
        }
    }

    #[test]
    fn groups_topics_into_nested_folders() {
        let tree = Folder::build(
            [
                topic("/Drive/Module/frontLeft/speed"),
                topic("/Drive/Module/frontLeft/angle"),
                topic("/Drive/heading"),
                topic("/rootTopic"),
            ],
            false,
            "",
        );
        assert!(tree.topics.contains_key("rootTopic"));
        let drive = &tree.folders["Drive"];
        assert!(drive.topics.contains_key("heading"));
        let front_left = &drive.folders["Module"].folders["frontLeft"];
        assert_eq!(
            front_left.topics.keys().collect::<Vec<_>>(),
            ["angle", "speed"]
        );
        assert_eq!(tree.topic_count(), 4);
        assert_eq!(drive.topic_count(), 3);
    }

    #[test]
    fn metadata_is_hidden_unless_asked_for() {
        let topics = [
            topic("/Arm/angle"),
            topic("/Arm/.type"),
            topic("/.schema/struct:Pose2d"),
        ];
        assert_eq!(Folder::build(topics.clone(), false, "").topic_count(), 1);
        let all = Folder::build(topics, true, "");
        assert_eq!(all.topic_count(), 3);
        assert!(all.folders[".schema"].topics.contains_key("struct:Pose2d"));
    }

    #[test]
    fn filter_matches_full_name_ignoring_case() {
        let topics = [topic("/Drive/heading"), topic("/Arm/angle")];
        let tree = Folder::build(topics, false, "  DRIVE ");
        assert_eq!(tree.topic_count(), 1);
        assert!(tree.folders.contains_key("Drive"));
        assert!(!tree.folders.contains_key("Arm"));
    }

    #[test]
    fn values_are_short() {
        assert_eq!(format_value(&Value::Double(2.0)), "2");
        assert_eq!(format_value(&Value::Double(-0.00001)), "0");
        assert_eq!(format_value(&Value::Float(1.25)), "1.25");
        assert_eq!(format_value(&Value::Int(-3)), "-3");
        assert_eq!(format_value(&Value::Boolean(true)), "true");
        assert_eq!(format_value(&Value::String("hi".into())), "\"hi\"");
        let long = "x".repeat(50);
        assert_eq!(
            format_value(&Value::String(long)),
            format!("\"{}…\"", "x".repeat(40))
        );
        assert_eq!(
            format_value(&Value::IntArray((1..=8).collect())),
            "[1, 2, 3, 4, 5, 6, … +2]"
        );
        assert_eq!(format_value(&Value::DoubleArray(vec![])), "[]");
    }
}
