//! The NetworkTables tree: topics grouped into folders by their `/` paths.
//!
//! NT has no real folders. A topic like `/Drive/Module/frontLeft/speed` is
//! shown under the folders `Drive`, `Module` and `frontLeft`. Building the
//! tree is kept apart from drawing it so it can be tested without a window.
//!
//! Here: building the tree ([`Folder::build`]). Picking and dragging rows
//! are in `selection.rs` ([`TreeSelection`], [`DraggedTopic`]), drawing in
//! `view.rs` ([`Folder::show`]), and a value's short text in `format.rs`
//! ([`format_value`]).

use std::collections::BTreeMap;

use eframe::egui;
use nt4::Value;
use nt4::proto::structs::StructDatabase;
use raptaboard_layout::StructMeta;
use raptaboard_plugin_host::path;

use crate::types::structs::{self, FieldInfo, Shown};
use crate::widgets;

mod format;
mod selection;
mod view;

pub use format::format_value;
pub use selection::{AddTo, DraggedTopic, DraggedTopics, TreeSelection};

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

/// A folder: sub-folders and topics, each sorted by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Folder {
    /// Sub-folders by name.
    pub folders: BTreeMap<String, Folder>,
    /// Topics directly in this folder, by their last path segment.
    pub topics: BTreeMap<String, TopicEntry>,
    /// The folder's `.type` value, if it's a table robot code made for a
    /// widget (a SendableChooser's is `"String Chooser"`). Known even when
    /// the `.type` topic itself is hidden.
    pub table_type: Option<String>,
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
        let mut table_types = Vec::new();
        for topic in topics {
            let parts: Vec<String> = path::segments(&topic.name)
                .into_iter()
                .map(str::to_owned)
                .collect();
            let Some((key, folders)) = parts.split_last() else {
                continue;
            };
            if key == ".type"
                && let Some(Value::String(ty)) = &topic.value
            {
                table_types.push((folders.to_vec(), ty.clone()));
            }
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
        // Each table's type, on its folder if it's shown.
        for (path, ty) in table_types {
            let mut folder = Some(&mut root);
            for name in &path {
                folder = folder.and_then(|f| f.folders.get_mut(name));
            }
            if let Some(folder) = folder {
                folder.table_type = Some(ty);
            }
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
}
