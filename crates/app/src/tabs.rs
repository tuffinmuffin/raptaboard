//! Tab actions: switching, adding, closing, renaming and reordering tabs.
//!
//! The tab bar and the keyboard shortcuts both produce a [`TabAction`], and
//! [`apply`] carries it out on the layout. Only switching tabs has
//! shortcuts (see [`shortcut`]): Ctrl+1 to 9 as in Elastic, and Ctrl+Left
//! and Right for the previous and next tab, where Elastic moves the tab
//! instead. Ctrl is Cmd on macOS.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use raptaboard_layout::{ElasticLayout, Tab};

/// Name for tabs added with [`TabAction::New`] (made unique with a number).
pub const NEW_TAB_NAME: &str = "New Tab";

/// Something to do to the layout's tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabAction {
    /// Show tab `i` (counting from 0).
    Select(usize),
    /// Show the next tab, wrapping to the first.
    Next,
    /// Show the previous tab, wrapping to the last.
    Previous,
    /// Swap the open tab with the one on its left.
    MoveLeft,
    /// Swap the open tab with the one on its right.
    MoveRight,
    /// Add an empty tab after the open one and show it.
    New,
    /// Close tab `i`. The last tab can't be closed.
    Close(usize),
    /// Rename tab `i`. Blank names are ignored.
    Rename(usize, String),
}

impl TabAction {
    /// Whether this action changes the layout (so the layout lock blocks it),
    /// rather than only choosing which tab is shown.
    ///
    /// ```
    /// use raptaboard::tabs::TabAction;
    /// assert!(!TabAction::Next.edits_layout());
    /// assert!(TabAction::New.edits_layout());
    /// ```
    pub fn edits_layout(&self) -> bool {
        !matches!(
            self,
            TabAction::Select(_) | TabAction::Next | TabAction::Previous
        )
    }
}

/// Carry out `action` on `layout`, where tab `current` is open. Returns the
/// tab to show afterwards.
///
/// ```
/// use raptaboard::tabs::{apply, TabAction};
/// use raptaboard_layout::ElasticLayout;
/// let mut layout = ElasticLayout::default(); // Teleoperated, Autonomous
/// let open = apply(&mut layout, 0, TabAction::New);
/// assert_eq!(open, 1);
/// assert_eq!(layout.tabs[1].name, "New Tab");
/// ```
pub fn apply(layout: &mut ElasticLayout, current: usize, action: TabAction) -> usize {
    let count = layout.tabs.len();
    if count == 0 {
        return 0;
    }
    let current = current.min(count - 1);
    match action {
        TabAction::Select(i) if i < count => i,
        TabAction::Select(_) => current,
        TabAction::Next => (current + 1) % count,
        TabAction::Previous => (current + count - 1) % count,
        TabAction::MoveLeft if current > 0 => {
            layout.tabs.swap(current, current - 1);
            current - 1
        }
        TabAction::MoveRight if current + 1 < count => {
            layout.tabs.swap(current, current + 1);
            current + 1
        }
        TabAction::MoveLeft | TabAction::MoveRight => current,
        TabAction::New => {
            let name = layout.unused_tab_name(NEW_TAB_NAME);
            layout.tabs.insert(current + 1, Tab::new(name));
            current + 1
        }
        TabAction::Close(i) if i < count && count > 1 => {
            layout.tabs.remove(i);
            // Stay on the same tab if it's still there, else its neighbour.
            if i < current {
                current - 1
            } else {
                current.min(count - 2)
            }
        }
        TabAction::Close(_) => current,
        TabAction::Rename(i, name) => {
            let name = name.trim();
            if let Some(tab) = layout.tabs.get_mut(i)
                && !name.is_empty()
            {
                tab.name = name.to_owned();
            }
            current
        }
    }
}

/// The number keys for [`shortcut`]: Ctrl+1 shows the first tab.
const NUMBER_KEYS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

/// Read this frame's tab shortcuts, if any, and use up their key presses.
/// Only switching tabs has shortcuts; adding, closing, renaming and moving
/// tabs are rare, so they live in the tab bar.
///
/// | Keys | Action |
/// | --- | --- |
/// | Ctrl+1 to Ctrl+9 | show tab 1 to 9 |
/// | Ctrl+Left, Ctrl+Right | previous, next tab |
/// | Ctrl+Tab, Ctrl+Shift+Tab | next, previous tab (Windows and Linux; macOS keeps Cmd+Tab) |
///
/// Ctrl is Cmd on macOS. Skipped while a text box has the keyboard, so
/// Ctrl+Left still moves by word there.
pub fn shortcut(ctx: &egui::Context) -> Option<TabAction> {
    if ctx.egui_wants_keyboard_input() {
        return None;
    }
    let cmd = Modifiers::COMMAND;
    let key = |modifiers, key| KeyboardShortcut::new(modifiers, key);
    ctx.input_mut(|input| {
        // The Shift version first: Ctrl+Tab would also match Ctrl+Shift+Tab.
        if input.consume_shortcut(&key(cmd | Modifiers::SHIFT, Key::Tab)) {
            return Some(TabAction::Previous);
        }
        let simple = [
            (Key::ArrowLeft, TabAction::Previous),
            (Key::ArrowRight, TabAction::Next),
            (Key::Tab, TabAction::Next),
        ];
        for (k, action) in simple {
            if input.consume_shortcut(&key(cmd, k)) {
                return Some(action);
            }
        }
        NUMBER_KEYS
            .iter()
            .position(|&k| input.consume_shortcut(&key(cmd, k)))
            .map(TabAction::Select)
    })
}

/// The hover text for tab `i`: its shortcut, if it has one, and how to edit
/// it when the layout isn't locked. `ctx` gives the platform's key names
/// (Cmd on macOS, Ctrl elsewhere).
pub fn tab_hint(ctx: &egui::Context, i: usize, locked: bool) -> String {
    let mut lines = Vec::new();
    if let Some(&k) = NUMBER_KEYS.get(i) {
        lines.push(format!(
            "{} to show this tab",
            ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, k))
        ));
    }
    let arrows = |k| ctx.format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, k));
    lines.push(format!(
        "{} / {} for the previous / next tab",
        arrows(Key::ArrowLeft),
        arrows(Key::ArrowRight)
    ));
    if !locked {
        lines.push("Double-click to rename, right-click for more".to_owned());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn layout(names: &[&str]) -> ElasticLayout {
        ElasticLayout {
            tabs: names.iter().map(|n| Tab::new(*n)).collect(),
            ..ElasticLayout::default()
        }
    }

    fn names(layout: &ElasticLayout) -> Vec<&str> {
        layout.tabs.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn next_and_previous_wrap_around() {
        let mut l = layout(&["A", "B", "C"]);
        assert_eq!(apply(&mut l, 2, TabAction::Next), 0);
        assert_eq!(apply(&mut l, 0, TabAction::Previous), 2);
        assert_eq!(apply(&mut l, 1, TabAction::Select(0)), 0);
        // Ctrl+9 with only three tabs does nothing.
        assert_eq!(apply(&mut l, 1, TabAction::Select(8)), 1);
    }

    #[test]
    fn moving_a_tab_keeps_it_open() {
        let mut l = layout(&["A", "B", "C"]);
        assert_eq!(apply(&mut l, 1, TabAction::MoveLeft), 0);
        assert_eq!(names(&l), ["B", "A", "C"]);
        // Already at the left end: nothing changes.
        assert_eq!(apply(&mut l, 0, TabAction::MoveLeft), 0);
        assert_eq!(names(&l), ["B", "A", "C"]);
        assert_eq!(apply(&mut l, 0, TabAction::MoveRight), 1);
        assert_eq!(names(&l), ["A", "B", "C"]);
    }

    #[test]
    fn closing_keeps_a_sensible_tab_open() {
        let mut l = layout(&["A", "B", "C", "D"]);
        // Closing a tab to the left of the open one: stay on the same tab.
        assert_eq!(apply(&mut l, 2, TabAction::Close(0)), 1);
        assert_eq!(names(&l), ["B", "C", "D"]);
        // Closing the open last tab: show the new last tab.
        assert_eq!(apply(&mut l, 2, TabAction::Close(2)), 1);
        assert_eq!(names(&l), ["B", "C"]);
        assert_eq!(apply(&mut l, 0, TabAction::Close(1)), 0);
        // The last tab stays.
        assert_eq!(apply(&mut l, 0, TabAction::Close(0)), 0);
        assert_eq!(names(&l), ["B"]);
    }

    #[test]
    fn new_tabs_get_unique_names() {
        let mut l = layout(&["A"]);
        assert_eq!(apply(&mut l, 0, TabAction::New), 1);
        assert_eq!(apply(&mut l, 0, TabAction::New), 1);
        assert_eq!(names(&l), ["A", "New Tab 2", "New Tab"]);
    }

    #[test]
    fn renames_are_trimmed_and_blank_ones_ignored() {
        let mut l = layout(&["A", "B"]);
        apply(&mut l, 0, TabAction::Rename(1, "  Pit  ".into()));
        apply(&mut l, 0, TabAction::Rename(0, "   ".into()));
        apply(&mut l, 0, TabAction::Rename(7, "Nowhere".into()));
        assert_eq!(names(&l), ["A", "Pit"]);
    }

    #[test]
    fn only_switching_tabs_is_allowed_when_locked() {
        assert!(!TabAction::Select(0).edits_layout());
        assert!(!TabAction::Previous.edits_layout());
        for action in [
            TabAction::MoveLeft,
            TabAction::MoveRight,
            TabAction::Close(0),
            TabAction::Rename(0, "x".into()),
        ] {
            assert!(action.edits_layout(), "{action:?}");
        }
    }
}
