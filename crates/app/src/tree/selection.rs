//! Picking tree rows (click, Ctrl+click, Shift+click), what a dragged row
//! carries, and revealing a widget's topic in the tree.

use super::*;

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

impl DraggedTopic {
    /// Whether `widget` shows this topic (or struct field).
    ///
    /// ```
    /// use raptaboard::tree::DraggedTopic;
    /// use raptaboard::widgets::for_topic;
    /// use raptaboard_layout::Rect;
    /// let at = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
    /// let w = for_topic("/Arm/angle", "double", None, at).unwrap();
    /// let t = DraggedTopic { name: "/Arm/angle".into(), ty: "double".into(), field: None };
    /// assert!(t.is_shown_by(&w));
    /// ```
    pub fn is_shown_by(&self, widget: &raptaboard_layout::WidgetContainer) -> bool {
        widget.topic() == Some(self.name.as_str())
            && widget.struct_meta().map(|m| m.path) == self.field.as_ref().map(|f| f.path.clone())
    }
}

/// Topics dragged out of the tree together: one row, or every selected row
/// when a selected one is dragged. This is the drag-and-drop payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedTopics(pub Vec<DraggedTopic>);

/// Where topics added from the tree's right-click menu go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AddTo {
    /// Free spots on the open tab's grid.
    #[default]
    Grid,
    /// The end of the open tab's List Layout at this index.
    List(usize),
    /// A new List Layout holding them.
    NewList,
}

/// Which tree rows are selected, for adding several topics at once and for
/// highlighting their widgets. Only rows that can become a widget (topics
/// and struct fields) can be selected.
///
/// Click selects one row; Ctrl+click (Cmd on macOS) adds or removes a row;
/// Shift+click selects every row from the last one clicked to this one, in
/// the order the tree shows them.
///
/// ```
/// use raptaboard::tree::{DraggedTopic, TreeSelection};
/// let t = |n: &str| DraggedTopic { name: n.into(), ty: "double".into(), field: None };
/// let mut sel = TreeSelection::default();
/// sel.click(&t("/a"), false, false);
/// sel.click(&t("/b"), true, false);
/// assert_eq!(sel.selected().len(), 2);
/// sel.click(&t("/c"), false, false);
/// assert_eq!(sel.selected(), [t("/c")]);
/// ```
#[derive(Debug, Default)]
pub struct TreeSelection {
    selected: Vec<DraggedTopic>,
    /// The row Shift+click ranges start from.
    anchor: Option<DraggedTopic>,
    /// Selectable rows in the order they were drawn last frame.
    order: Vec<DraggedTopic>,
    /// The same, being collected this frame.
    drawing: Vec<DraggedTopic>,
    /// Rows whose "Add to grid" or "Add to list" was clicked this frame,
    /// for the window to add.
    pub add: Vec<DraggedTopic>,
    /// Where [`TreeSelection::add`] goes.
    pub add_to: AddTo,
    /// Names of the open tab's List Layouts, for the "Add to list" menu.
    /// The window sets these before drawing the tree.
    pub lists: Vec<String>,
    /// A row to open the tree to, scroll to and pick, set by
    /// [`TreeSelection::reveal`].
    reveal: Option<Reveal>,
    /// Whether a row was clicked (or right-clicked) in the tree this frame,
    /// so the window can drop its own selection.
    pub clicked: bool,
    /// A row [`TreeSelection::reveal`] gave up on this frame, as the text
    /// to show (`/Arm/angle` or `/Drive/pose → translation.x`), for the
    /// window to say so.
    pub not_found: Option<String>,
}

/// A row the tree should show: a topic, and the field path for a struct
/// field.
#[derive(Debug, Clone)]
struct Reveal {
    name: String,
    field: Option<Vec<String>>,
    /// Frames left to find it (folders take a frame or two to open).
    frames: u8,
}

/// How many frames [`TreeSelection::reveal`] keeps trying.
const REVEAL_FRAMES: u8 = 30;

impl TreeSelection {
    /// The selected rows, in the order they were selected.
    pub fn selected(&self) -> &[DraggedTopic] {
        &self.selected
    }

    /// Whether `row` is selected.
    pub fn is_selected(&self, row: &DraggedTopic) -> bool {
        self.selected.contains(row)
    }

    /// A click on `row`. `toggle` is Ctrl (Cmd) held, `range` is Shift held.
    pub fn click(&mut self, row: &DraggedTopic, toggle: bool, range: bool) {
        let position = |r: &DraggedTopic| self.order.iter().position(|o| o == r);
        if range
            && let Some(from) = self.anchor.as_ref().and_then(position)
            && let Some(to) = position(row)
        {
            let (a, b) = (from.min(to), from.max(to));
            self.selected = self.order[a..=b].to_vec();
            return;
        }
        if toggle {
            match self.selected.iter().position(|s| s == row) {
                Some(i) => {
                    self.selected.remove(i);
                }
                None => self.selected.push(row.clone()),
            }
        } else {
            self.selected = vec![row.clone()];
        }
        self.anchor = Some(row.clone());
    }

    /// What dragging or adding `row` takes: the whole selection if `row` is
    /// in it, otherwise just `row`.
    pub fn group(&self, row: &DraggedTopic) -> Vec<DraggedTopic> {
        if self.is_selected(row) {
            self.selected.clone()
        } else {
            vec![row.clone()]
        }
    }

    /// Open the tree down to the row for topic `name` (and `field`, a
    /// struct field's path), scroll it into view and pick it, over the next
    /// few frames. For clicking a widget to find its topic. What was picked
    /// before is unpicked at once, so if the row isn't in the tree (not
    /// published, or filtered out) nothing is left picked.
    pub fn reveal(&mut self, name: &str, field: Option<Vec<String>>) {
        self.clear();
        self.reveal = Some(Reveal {
            name: name.to_owned(),
            field,
            frames: REVEAL_FRAMES,
        });
    }

    /// Whether the row being revealed is inside the folder at `path`
    /// (`/Drive/Health`), so the folder should open.
    pub(super) fn reveals_in_folder(&self, path: &str) -> bool {
        self.reveal.as_ref().is_some_and(|r| {
            r.name
                .strip_prefix(path)
                .is_some_and(|rest| rest.starts_with('/'))
        })
    }

    /// Whether the row being revealed is a field of struct topic `topic`
    /// inside `path` (the fields leading to a nested struct; empty for the
    /// topic itself), so that header should open.
    pub(super) fn reveals_in_struct(&self, topic: &str, path: &[String]) -> bool {
        self.reveal.as_ref().is_some_and(|r| {
            r.name == topic && r.field.as_ref().is_some_and(|f| f.starts_with(path))
        })
    }

    /// If `row` is the one being revealed: pick it, stop revealing, and
    /// return `true` so the caller scrolls to it.
    pub(super) fn reveal_hit(&mut self, row: &DraggedTopic) -> bool {
        let hit = self.reveal.as_ref().is_some_and(|r| {
            r.name == row.name && r.field == row.field.as_ref().map(|f| f.path.clone())
        });
        if hit {
            self.reveal = None;
            self.selected = vec![row.clone()];
            self.anchor = Some(row.clone());
        }
        hit
    }

    /// Unselect everything.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// Call before drawing the tree each frame.
    pub fn begin_frame(&mut self) {
        self.clicked = false;
        self.drawing.clear();
        self.add.clear();
        self.add_to = AddTo::Grid;
    }

    /// Call after drawing the tree each frame.
    pub fn end_frame(&mut self) {
        self.order = std::mem::take(&mut self.drawing);
        // Give up on a row that never showed up, and say which.
        self.not_found = None;
        if let Some(r) = &mut self.reveal {
            r.frames = r.frames.saturating_sub(1);
            if r.frames == 0 {
                self.not_found = Some(match &r.field {
                    Some(path) => format!("{} → {}", r.name, structs::path_text(path)),
                    None => r.name.clone(),
                });
                self.reveal = None;
            }
        }
    }

    /// Note that `row` was drawn, in order, for Shift+click ranges.
    pub(super) fn drew(&mut self, row: &DraggedTopic) {
        self.drawing.push(row.clone());
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn row(name: &str) -> DraggedTopic {
        DraggedTopic {
            name: name.into(),
            ty: "double".into(),
            field: None,
        }
    }

    /// A selection that has seen `rows` drawn, in order.
    fn drawn(rows: &[&str]) -> TreeSelection {
        let mut sel = TreeSelection::default();
        sel.begin_frame();
        for r in rows {
            sel.drew(&row(r));
        }
        sel.end_frame();
        sel
    }

    #[test]
    fn shift_click_selects_a_range_either_way() {
        let mut sel = drawn(&["/a", "/b", "/c", "/d"]);
        sel.click(&row("/b"), false, false);
        sel.click(&row("/d"), false, true);
        assert_eq!(sel.selected(), [row("/b"), row("/c"), row("/d")]);
        // The range is from the same start: back up to "/a".
        sel.click(&row("/a"), false, true);
        assert_eq!(sel.selected(), [row("/a"), row("/b")]);
        // Shift with nothing clicked before is a plain click.
        let mut fresh = drawn(&["/a", "/b"]);
        fresh.click(&row("/b"), false, true);
        assert_eq!(fresh.selected(), [row("/b")]);
    }

    #[test]
    fn ctrl_click_adds_and_removes() {
        let mut sel = drawn(&["/a", "/b", "/c"]);
        sel.click(&row("/a"), true, false);
        sel.click(&row("/c"), true, false);
        assert_eq!(sel.selected(), [row("/a"), row("/c")]);
        sel.click(&row("/a"), true, false);
        assert_eq!(sel.selected(), [row("/c")]);
        sel.clear();
        assert!(sel.selected().is_empty());
    }

    #[test]
    fn dragging_a_selected_row_takes_the_selection() {
        let mut sel = drawn(&["/a", "/b", "/c"]);
        sel.click(&row("/a"), false, false);
        sel.click(&row("/b"), true, false);
        assert_eq!(sel.group(&row("/b")), [row("/a"), row("/b")]);
        // A row outside the selection goes alone.
        assert_eq!(sel.group(&row("/c")), [row("/c")]);
    }

    #[test]
    fn widgets_are_matched_by_topic_and_field() {
        use raptaboard_layout::Rect;
        let at = Rect {
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 128.0,
        };
        let x = StructMeta {
            path: vec!["x".into()],
            schema_name: "Translation2d".into(),
        };
        let field_widget = widgets::for_topic("/pose", "double", Some(&x), at).unwrap();
        let field = DraggedTopic {
            name: "/pose".into(),
            ty: "double".into(),
            field: Some(x),
        };
        assert!(field.is_shown_by(&field_widget));
        // The same topic without the field, or another field, doesn't match.
        assert!(!row("/pose").is_shown_by(&field_widget));
        let y = DraggedTopic {
            field: Some(StructMeta {
                path: vec!["y".into()],
                schema_name: "Translation2d".into(),
            }),
            ..field.clone()
        };
        assert!(!y.is_shown_by(&field_widget));
    }

    #[test]
    fn revealing_opens_the_way_and_picks_the_row() {
        let mut sel = drawn(&["/Elsewhere"]);
        sel.click(&row("/Elsewhere"), false, false);
        let x = StructMeta {
            path: vec!["translation".into(), "x".into()],
            schema_name: "Pose2d".into(),
        };
        sel.reveal("/Drive/pose", Some(x.path.clone()));
        // The old pick goes at once, before the row is found.
        assert!(sel.selected().is_empty());
        assert!(sel.reveals_in_folder("/Drive"));
        assert!(!sel.reveals_in_folder("/Dri"));
        assert!(!sel.reveals_in_folder("/Arm"));
        assert!(sel.reveals_in_struct("/Drive/pose", &[]));
        assert!(sel.reveals_in_struct("/Drive/pose", &["translation".to_string()]));
        assert!(!sel.reveals_in_struct("/Drive/pose", &["rotation".to_string()]));
        // The whole topic isn't the field.
        assert!(!sel.reveal_hit(&row("/Drive/pose")));
        let field = DraggedTopic {
            name: "/Drive/pose".into(),
            ty: "double".into(),
            field: Some(x),
        };
        assert!(sel.reveal_hit(&field));
        assert_eq!(sel.selected(), std::slice::from_ref(&field));
        // Found once; then it stops.
        assert!(!sel.reveal_hit(&field));
        // A row that never shows up is given up on.
        sel.reveal("/Missing", None);
        for _ in 0..REVEAL_FRAMES {
            sel.begin_frame();
            sel.end_frame();
        }
        assert!(!sel.reveals_in_folder(""));
        assert_eq!(sel.not_found.as_deref(), Some("/Missing"));
        sel.end_frame();
        assert_eq!(sel.not_found, None);
    }
}
