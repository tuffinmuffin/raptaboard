//! The window's state, in groups: the layout file, the robot's address,
//! the tree panel, the grid's interaction, the alert history and failed
//! sends. [`DashboardApp`](super::DashboardApp) holds one of each beside the
//! layout itself and the few things every part uses (the open tab, the
//! lock, the selection, the status line).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use raptaboard_layout::ElasticLayout;

use crate::alert_history::AlertHistory;
use crate::connection::AddressMode;
use crate::tree::TreeSelection;

use super::alert_history_panel::AlertHistoryFilter;
use super::drag::{ChildDrag, Drag};
use super::grid_view::Sender;
use super::{AfterSaving, LoadProblem};

/// The layout's file: where it is, what's saved in it, and the questions
/// about it waiting for an answer.
#[derive(Default)]
pub(super) struct LayoutFile {
    /// The file the layout was opened from or last saved to, if any.
    pub path: Option<PathBuf>,
    /// The layout as it is in the file, to tell when there are unsaved changes.
    pub saved: ElasticLayout,
    /// What [`ElasticLayout::sanitize`] fixed in the open file, until it's saved.
    pub fixes: Vec<String>,
    /// Waiting for an answer to "save changes?" before doing this.
    pub unsaved_prompt: Option<AfterSaving>,
    /// A layout file that couldn't be opened, until the user answers.
    pub load_problem: Option<LoadProblem>,
    /// The user chose to quit without saving, so let the window close.
    pub allow_close: bool,
    /// The window title last set, so it's only sent when it changes.
    pub title: String,
}

/// Where to find the robot, as set in the settings bar.
pub(super) struct Address {
    /// FRC team number; 0 if none has been set.
    pub team: u32,
    /// How the address is worked out.
    pub mode: AddressMode,
    /// The host for [`AddressMode::Custom`].
    pub custom: String,
    /// The NetworkTables server port.
    pub port: u16,
}

/// The NetworkTables tree panel.
#[derive(Default)]
pub(super) struct TreeView {
    /// Whether the panel shows.
    pub shown: bool,
    /// The filter typed above the tree.
    pub filter: String,
    /// Whether metadata topics (`.type`, `/.schema/`) show.
    pub show_metadata: bool,
    /// The rows picked, for adding several at once and for highlighting
    /// their widgets.
    pub selection: TreeSelection,
    /// Whether the status line shows why a clicked widget couldn't be found
    /// in the tree, so the next click that finds one clears it.
    pub find_status: bool,
}

/// The grid's interaction between frames: what's being dragged, and where
/// the grid was.
pub(super) struct GridState {
    /// A widget or list being moved or resized, if any.
    pub drag: Option<Drag>,
    /// A List Layout child being dragged, if any.
    pub child_drag: Option<ChildDrag>,
    /// Where empty grid was last right-clicked, in grid pixels, for the
    /// menu's "New List Layout".
    pub menu_at: (f64, f64),
    /// The visible grid's width and height last frame, in pixels, for
    /// finding room for a duplicate.
    pub room: (f64, f64),
    /// Where the grid was drawn last frame, in screen points.
    pub area: egui::Rect,
}

impl Default for GridState {
    fn default() -> Self {
        GridState {
            drag: None,
            child_drag: None,
            menu_at: (0.0, 0.0),
            room: (0.0, 0.0),
            area: egui::Rect::NOTHING,
        }
    }
}

/// The alert history and its panel.
pub(super) struct Alerts {
    /// Every alert seen since connecting, from every group.
    pub history: AlertHistory,
    /// Whether the history panel shows.
    pub panel_shown: bool,
    /// What the panel shows.
    pub filter: AlertHistoryFilter,
}

/// Values widgets couldn't send, so they can say so.
#[derive(Default)]
pub(super) struct Sends {
    /// When each widget (or list child) on a tab last failed to send its
    /// value: (tab, sender) to the time, so its border flashes red for a
    /// moment.
    pub failed: HashMap<(usize, Sender), Instant>,
    /// The status message the last failed send left, so the next send that
    /// works clears it (if it's still showing).
    pub last_failure: Option<String>,
}
