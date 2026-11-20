//! Undo and redo for layout edits, and the keyboard shortcuts for editing
//! widgets on the grid.
//!
//! Every change to the layout can be undone: moving, resizing, deleting or
//! adding widgets, and adding, closing or renaming tabs. [`History`] notices
//! changes by comparing the layout with the last one it saw, so nothing that
//! edits the layout has to remember to record itself.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use raptaboard_layout::ElasticLayout;

/// The most edits [`History`] keeps. Older ones are forgotten.
pub const MAX_UNDO: usize = 100;

/// Earlier and later versions of the layout, for undo and redo.
///
/// ```
/// use raptaboard::edit::History;
/// use raptaboard_layout::{ElasticLayout, Tab};
/// let mut layout = ElasticLayout::default();
/// let mut history = History::new(&layout);
/// layout.tabs.push(Tab::new("Pit"));
/// history.record(&layout);
/// assert!(history.undo(&mut layout));
/// assert_eq!(layout.tabs.len(), 2);
/// assert!(history.redo(&mut layout));
/// assert_eq!(layout.tabs.len(), 3);
/// ```
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<ElasticLayout>,
    redo: Vec<ElasticLayout>,
    /// The layout as it was last seen by [`History::record`].
    last: ElasticLayout,
}

impl History {
    /// An empty history starting from `layout`.
    pub fn new(layout: &ElasticLayout) -> Self {
        History {
            undo: Vec::new(),
            redo: Vec::new(),
            last: layout.clone(),
        }
    }

    /// Note the layout as it is now. If it changed since the last call, the
    /// old version can be undone back to, and anything undone before can't
    /// be redone any more. Call this once a frame, when no drag is part way.
    pub fn record(&mut self, layout: &ElasticLayout) {
        if *layout == self.last {
            return;
        }
        self.undo
            .push(std::mem::replace(&mut self.last, layout.clone()));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Put `layout` back the way it was before the last edit. Returns
    /// whether there was anything to undo.
    pub fn undo(&mut self, layout: &mut ElasticLayout) -> bool {
        self.record(layout);
        let Some(before) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(layout, before.clone()));
        self.last = before;
        true
    }

    /// Make the last undone edit again. Returns whether there was one.
    pub fn redo(&mut self, layout: &mut ElasticLayout) -> bool {
        self.record(layout);
        let Some(after) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(layout, after.clone()));
        self.last = after;
        true
    }

    /// Whether [`History::undo`] would do anything.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether [`History::redo`] would do anything.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// An editing command from the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAction {
    /// Undo the last layout edit (Ctrl+Z).
    Undo,
    /// Redo the last undone edit (Ctrl+Shift+Z or Ctrl+Y).
    Redo,
    /// Delete the selected widget (Delete or Backspace).
    Delete,
    /// Copy the selected widget into a free spot (Ctrl+D).
    Duplicate,
    /// Stop selecting the selected widget (Esc).
    Deselect,
    /// Open the selected widget's settings (Enter).
    Edit,
}

impl EditAction {
    /// This action's main keyboard shortcut. Ctrl is Cmd on macOS.
    pub fn shortcut(self) -> KeyboardShortcut {
        let cmd = Modifiers::COMMAND;
        match self {
            EditAction::Undo => KeyboardShortcut::new(cmd, Key::Z),
            EditAction::Redo => KeyboardShortcut::new(cmd | Modifiers::SHIFT, Key::Z),
            EditAction::Delete => KeyboardShortcut::new(Modifiers::NONE, Key::Delete),
            EditAction::Duplicate => KeyboardShortcut::new(cmd, Key::D),
            EditAction::Deselect => KeyboardShortcut::new(Modifiers::NONE, Key::Escape),
            EditAction::Edit => KeyboardShortcut::new(Modifiers::NONE, Key::Enter),
        }
    }
}

/// Read this frame's editing shortcut, if any, and use up its key press.
/// None while a text box has the keyboard, so typing there edits the text.
pub fn shortcut(ctx: &egui::Context) -> Option<EditAction> {
    if ctx.egui_wants_keyboard_input() {
        return None;
    }
    let popup_open = egui::Popup::is_any_open(ctx);
    ctx.input_mut(|input| {
        // Escape belongs to what's open or held: a drop-down or menu
        // (which it closes) or a drag (which it cancels).
        let escape_taken = popup_open || input.pointer.any_down();
        let mut pressed = |s: KeyboardShortcut| input.consume_shortcut(&s);
        // Redo first: Ctrl+Z would also match Ctrl+Shift+Z.
        if pressed(EditAction::Redo.shortcut())
            || pressed(KeyboardShortcut::new(Modifiers::COMMAND, Key::Y))
        {
            Some(EditAction::Redo)
        } else if pressed(EditAction::Undo.shortcut()) {
            Some(EditAction::Undo)
        } else if pressed(EditAction::Delete.shortcut())
            || pressed(KeyboardShortcut::new(Modifiers::NONE, Key::Backspace))
        {
            Some(EditAction::Delete)
        } else if pressed(EditAction::Duplicate.shortcut()) {
            Some(EditAction::Duplicate)
        } else if !escape_taken && pressed(EditAction::Deselect.shortcut()) {
            Some(EditAction::Deselect)
        } else if pressed(EditAction::Edit.shortcut()) {
            Some(EditAction::Edit)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use raptaboard_layout::Tab;

    fn with_tabs(n: usize) -> ElasticLayout {
        ElasticLayout {
            tabs: (0..n).map(|i| Tab::new(format!("T{i}"))).collect(),
            ..ElasticLayout::default()
        }
    }

    #[test]
    fn undo_and_redo_walk_back_and_forth() {
        let mut layout = with_tabs(1);
        let mut history = History::new(&layout);
        assert!(!history.can_undo());
        for n in 2..=4 {
            layout = with_tabs(n);
            history.record(&layout);
        }
        // Recording the same layout again adds nothing.
        history.record(&layout);
        for n in [3, 2, 1] {
            assert!(history.undo(&mut layout));
            assert_eq!(layout.tabs.len(), n);
        }
        assert!(!history.undo(&mut layout));
        assert_eq!(layout.tabs.len(), 1);
        assert!(history.redo(&mut layout));
        assert!(history.redo(&mut layout));
        assert_eq!(layout.tabs.len(), 3);
        assert!(history.can_redo());
    }

    #[test]
    fn a_new_edit_ends_redo() {
        let mut layout = with_tabs(1);
        let mut history = History::new(&layout);
        layout = with_tabs(2);
        history.record(&layout);
        history.undo(&mut layout);
        layout = with_tabs(5);
        history.record(&layout);
        assert!(!history.redo(&mut layout));
        assert!(history.undo(&mut layout));
        assert_eq!(layout.tabs.len(), 1);
    }

    #[test]
    fn edits_not_yet_recorded_are_undone_too() {
        // An edit made the same frame as Ctrl+Z is undone, not lost.
        let mut layout = with_tabs(1);
        let mut history = History::new(&layout);
        layout = with_tabs(2);
        assert!(history.undo(&mut layout));
        assert_eq!(layout.tabs.len(), 1);
        assert!(history.redo(&mut layout));
        assert_eq!(layout.tabs.len(), 2);
    }

    #[test]
    fn old_edits_are_forgotten() {
        let mut layout = with_tabs(0);
        let mut history = History::new(&layout);
        for n in 1..=MAX_UNDO + 10 {
            layout = with_tabs(n);
            history.record(&layout);
        }
        let mut undone = 0;
        while history.undo(&mut layout) {
            undone += 1;
        }
        assert_eq!(undone, MAX_UNDO);
        assert_eq!(layout.tabs.len(), 10);
    }
}
