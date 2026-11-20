//! The main window: settings bar, tab bar and the widget grid.

mod dialogs;
mod drag;
mod grid_view;
mod history_panel;
mod live;
mod panels;
mod style;
mod tree_panel;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use nt4::Value;
use raptaboard_layout::ElasticLayout;

use crate::cli::StartupOptions;
use crate::connection::AddressMode;
use crate::edit::{self, EditAction, History};
use crate::grid::Item;
use crate::layout_file::{self, FileAction, LoadError};
use crate::nt::NtConnection;
use crate::sampling::Sampler;
use crate::settings::{self, Settings};
use crate::sound::Sound;
use crate::tabs::{self, TabAction};
use crate::traces::Traces;
use crate::tree::TreeSelection;

use self::dialogs::Editing;
use self::drag::{ChildDrag, Drag};
use self::grid_view::Sender;

/// The shortest time between redraws: about 60 a second.
const MIN_REDRAW: Duration = Duration::from_millis(16);

/// What to do once the user has answered "save changes?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterSaving {
    /// Open another layout file.
    Open,
    /// Close the window.
    Quit,
}

/// Top-level app state.
pub struct DashboardApp {
    layout: ElasticLayout,
    /// The file the layout was opened from or last saved to, if any.
    layout_path: Option<PathBuf>,
    /// The layout as it is in the file, to tell when there are unsaved changes.
    saved: ElasticLayout,
    /// The last thing that happened to the layout file, shown in the settings bar.
    status: String,
    /// What [`ElasticLayout::sanitize`] fixed in the open file, until it's saved.
    fixes: Vec<String>,
    /// Waiting for an answer to "save changes?" before doing this.
    unsaved_prompt: Option<AfterSaving>,
    /// The user chose to quit without saving, so let the window close.
    allow_close: bool,
    /// The window title last set, so it's only sent when it changes.
    title: String,
    current_tab: usize,
    team: u32,
    mode: AddressMode,
    custom_address: String,
    port: u16,
    nt: NtConnection,
    show_tree: bool,
    tree_filter: String,
    show_metadata: bool,
    /// When locked, widgets and tabs can't be moved, resized or edited.
    locked: bool,
    drag: Option<Drag>,
    /// A List Layout child being dragged, if any.
    child_drag: Option<ChildDrag>,
    /// Where empty grid was last right-clicked, in grid pixels, for the
    /// menu's "New List Layout".
    menu_at: (f64, f64),
    /// The NT tree rows picked, for adding several at once and for
    /// highlighting their widgets.
    tree_sel: TreeSelection,
    /// Whether the status line shows why a clicked widget couldn't be found
    /// in the tree, so the next click that finds one clears it.
    find_status: bool,
    /// Saves settings changed in the app to the settings file.
    settings: settings::Tracker,
    /// The tab being renamed, the name typed so far, and whether its text
    /// box has been given the keyboard yet.
    renaming: Option<(usize, String, bool)>,
    /// A tab with widgets on it, waiting for the user to confirm closing it.
    confirm_close: Option<usize>,
    /// The widget or List Layout picked on the open tab, for Delete and
    /// Duplicate. Always `None` while locked.
    selected: Option<Item>,
    /// Earlier versions of the layout, for undo and redo.
    history: History,
    /// The visible grid's width and height last frame, in pixels, for
    /// finding room for a duplicate.
    grid_room: (f64, f64),
    /// The widget whose settings dialog is open, if any.
    editing: Option<Editing>,
    /// The values widgets show, taken once per each widget's period.
    sampler: Sampler,
    /// Recent values of the series graphs on the open tab show.
    traces: Traces,
    /// Whether a tone plays when a widget's value can't be sent.
    alert_sound: bool,
    /// Plays the tones (a failed send's, and Alerts widgets').
    sound: Sound,
    /// Every alert seen since connecting, from every group.
    alert_history: crate::alert_history::AlertHistory,
    /// Whether the alert history panel shows.
    show_alert_history: bool,
    /// What the alert history panel shows.
    history_filter: history_panel::HistoryFilter,
    /// When each widget (or list child) on a tab last failed to send its value: (tab,
    /// sender) to the time, so its border flashes red for a moment.
    write_failed: HashMap<(usize, Sender), Instant>,
    /// The status message the last failed send left, so the next send that
    /// works clears it (if it's still showing).
    send_failure: Option<String>,
    /// Where the grid was drawn last frame, in screen points.
    grid_area: egui::Rect,
}

impl DashboardApp {
    /// Create the app from the startup options, loading the layout file if one is given.
    ///
    /// Fails only if the tokio runtime for the NT client cannot start.
    pub fn new(options: StartupOptions) -> std::io::Result<Self> {
        let mut app = DashboardApp {
            layout: ElasticLayout::default(),
            layout_path: None,
            saved: ElasticLayout::default(),
            status: String::new(),
            fixes: Vec::new(),
            unsaved_prompt: None,
            allow_close: false,
            title: String::new(),
            current_tab: 0,
            team: options.team,
            mode: options.mode,
            custom_address: options.custom_address,
            port: options.port,
            nt: NtConnection::new()?,
            show_tree: options.show_tree,
            tree_filter: String::new(),
            show_metadata: false,
            locked: options.locked,
            drag: None,
            child_drag: None,
            menu_at: (0.0, 0.0),
            tree_sel: TreeSelection::default(),
            find_status: false,
            // A stand-in with nowhere to save, replaced below once the
            // starting settings are known.
            settings: settings::Tracker::new(settings::Store::default(), Settings::default()),
            renaming: None,
            confirm_close: None,
            selected: None,
            history: History::new(&ElasticLayout::default()),
            grid_room: (0.0, 0.0),
            editing: None,
            sampler: Sampler::default(),
            traces: Traces::default(),
            alert_sound: options.alert_sound,
            sound: Sound::new(),
            alert_history: crate::alert_history::AlertHistory::new(options.alert_history),
            show_alert_history: false,
            history_filter: Default::default(),
            write_failed: HashMap::new(),
            send_failure: None,
            grid_area: egui::Rect::NOTHING,
        };
        if let Some(path) = options.layout_path {
            let path = PathBuf::from(path);
            match layout_file::load(&path) {
                Ok(layout) => app.show_layout(layout, path),
                // A new file named on the command line: start empty and
                // save there.
                Err(LoadError::NotFound) => {
                    app.status = format!("New layout; Save writes {}", path.display());
                    app.layout_path = Some(path);
                }
                Err(e) => app.status = format!("Could not open {}: {e}", path.display()),
            }
        }
        let names: Vec<&str> = app.layout.tabs.iter().map(|t| t.name.as_str()).collect();
        app.current_tab = settings::pick_tab(&names, options.tab.as_deref(), options.tab_index);
        if !options.notes.is_empty() {
            app.status = options.notes.join("; ");
        }
        // What the app started with isn't a change to save, except the
        // layout: the file opened is remembered however it was named, the
        // command line included.
        let start = Settings {
            layout: None,
            ..app.current_settings(None)
        };
        app.settings = settings::Tracker::new(options.store, start);
        Ok(app)
    }

    /// The settings as the app has them now. `window` is the window's size
    /// and place, if known.
    fn current_settings(&self, window: Option<settings::Window>) -> Settings {
        Settings {
            team: Some(self.team),
            address: Some(self.mode.keyword(&self.custom_address)),
            port: Some(self.port),
            show_tree: Some(self.show_tree),
            locked: Some(self.locked),
            alert_sound: Some(self.alert_sound),
            alert_history: Some(self.alert_history.capacity()),
            tab: self
                .layout
                .tabs
                .get(self.current_tab)
                .map(|t| t.name.clone()),
            tab_index: Some(self.current_tab),
            layout: self
                .layout_path
                .as_ref()
                .map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone()))
                .map(|p| p.to_string_lossy().into_owned()),
            window,
        }
    }

    /// Whether the layout has changed since it was opened or saved.
    fn unsaved(&self) -> bool {
        self.layout != self.saved
    }

    /// Show a layout just read from `path`.
    /// Values raptaboard can't show are fixed first; the fixes count as
    /// unsaved changes, so saving writes them to the file.
    fn show_layout(&mut self, layout: ElasticLayout, path: PathBuf) {
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        self.status = format!(
            "Opened {}, {}",
            plural(layout.tabs.len(), "tab"),
            plural(layout.widgets().count(), "widget")
        );
        self.saved = layout.clone();
        let mut layout = layout;
        self.fixes = layout.sanitize();
        self.layout = layout;
        self.layout_path = Some(path);
        self.current_tab = 0;
        self.drag = None;
        self.child_drag = None;
        self.renaming = None;
        self.confirm_close = None;
        self.selected = None;
        // Undo goes back to the file as opened, fixes included.
        self.history = History::new(&self.layout);
    }

    /// Carry out Open, Save or Save as. Opening with unsaved changes asks
    /// to save them first.
    fn run_file_action(&mut self, action: FileAction) {
        match action {
            FileAction::Open if self.unsaved() => self.unsaved_prompt = Some(AfterSaving::Open),
            FileAction::Open => self.open(),
            FileAction::Save => {
                self.save(false);
            }
            FileAction::SaveAs => {
                self.save(true);
            }
        }
    }

    /// Ask for a layout file and open it.
    fn open(&mut self) {
        let Some(path) = layout_file::pick_open(self.layout_path.as_deref()) else {
            return;
        };
        match layout_file::load(&path) {
            Ok(layout) => self.show_layout(layout, path),
            Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
        }
    }

    /// Save the layout, asking where if `choose` is set or it has no file
    /// yet. Returns whether it was saved.
    fn save(&mut self, choose: bool) -> bool {
        let path = match &self.layout_path {
            Some(path) if !choose => path.clone(),
            near => match layout_file::pick_save(near.as_deref()) {
                Some(path) => path,
                None => return false,
            },
        };
        match layout_file::save(&self.layout, &path) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.saved = self.layout.clone();
                self.fixes.clear();
                self.layout_path = Some(path);
                true
            }
            Err(e) => {
                self.status = format!("Could not save {}: {e}", path.display());
                false
            }
        }
    }

    /// The "save changes?" question, while one is open.
    fn unsaved_dialog(&mut self, ctx: &egui::Context) {
        let Some(after) = self.unsaved_prompt else {
            return;
        };
        let name = layout_file::display_name(self.layout_path.as_deref());
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("unsaved_changes")).show(ctx, |ui| {
            ui.heading(format!("Save changes to {name}?"));
            ui.label(match after {
                AfterSaving::Open => "Opening another layout replaces this one.",
                AfterSaving::Quit => "Your changes will be lost if you don't save them.",
            });
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(true);
                }
                if ui.button("Don't save").clicked() {
                    answer = Some(false);
                }
                if ui.button("Cancel").clicked() {
                    self.unsaved_prompt = None;
                }
            });
        });
        if answer.is_none() {
            if modal.should_close() {
                self.unsaved_prompt = None;
            }
            return;
        }
        self.unsaved_prompt = None;
        if answer == Some(true) && !self.save(false) {
            return;
        }
        match after {
            AfterSaving::Open => self.open(),
            AfterSaving::Quit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Carry out a tab action from the tab bar or a shortcut. Edits are
    /// ignored while locked, and closing a tab that has widgets asks first.
    fn run_tab_action(&mut self, action: TabAction) {
        if self.locked && action.edits_layout() {
            return;
        }
        if let TabAction::Close(i) = action
            && self.confirm_close != Some(i)
            && self.layout.tabs.get(i).is_some_and(|t| {
                !t.grid_layout.containers.is_empty() || !t.grid_layout.layouts.is_empty()
            })
        {
            self.confirm_close = Some(i);
            return;
        }
        self.confirm_close = None;
        self.drag = None;
        self.child_drag = None;
        self.selected = None;
        self.current_tab = tabs::apply(&mut self.layout, self.current_tab, action);
    }

    /// Carry out an editing command: undo, redo, or Delete, Duplicate and
    /// Deselect on the selected widget. Ignored while locked.
    fn run_edit_action(&mut self, ctx: &egui::Context, action: EditAction) {
        // Esc also unpicks tree rows, which can be picked while locked.
        if action == EditAction::Deselect {
            self.tree_sel.clear();
        }
        if self.locked {
            return;
        }
        let undo_key = ctx.format_shortcut(&EditAction::Undo.shortcut());
        match action {
            EditAction::Undo | EditAction::Redo => {
                let done = if action == EditAction::Undo {
                    self.history.undo(&mut self.layout)
                } else {
                    self.history.redo(&mut self.layout)
                };
                if done {
                    self.current_tab = self
                        .current_tab
                        .min(self.layout.tabs.len().saturating_sub(1));
                    self.selected = None;
                    self.drag = None;
                    self.child_drag = None;
                    self.renaming = None;
                    self.confirm_close = None;
                }
            }
            EditAction::Delete => {
                let Some(item) = self.selected.take() else {
                    return;
                };
                let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
                    return;
                };
                let title = item.title(&tab.grid_layout).unwrap_or_default().to_owned();
                item.remove(&mut tab.grid_layout);
                self.drag = None;
                self.child_drag = None;
                self.status = format!("Deleted \"{title}\"; {undo_key} brings it back");
            }
            EditAction::Duplicate => {
                let Some(item) = self.selected else { return };
                let cell = f64::from(self.layout.grid_size());
                let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
                    return;
                };
                match item.duplicate(&mut tab.grid_layout, cell, self.grid_room) {
                    Some(copy) => self.selected = Some(copy),
                    None => {
                        let title = item.title(&tab.grid_layout).unwrap_or_default();
                        self.status = format!("No free space for a copy of \"{title}\"");
                    }
                }
            }
            EditAction::Deselect => self.selected = None,
            EditAction::Edit => {
                let tab = self.current_tab;
                let Some(grid) = self.layout.tabs.get(tab).map(|t| &t.grid_layout) else {
                    return;
                };
                self.editing = match self.selected {
                    Some(Item::Widget(index)) => {
                        grid.containers.get(index).map(|w| Editing::Widget {
                            tab,
                            index,
                            original: w.clone(),
                        })
                    }
                    Some(Item::List(index)) => grid.layouts.get(index).map(|l| Editing::List {
                        tab,
                        index,
                        original: Box::new(l.clone()),
                        child: None,
                        child_only: false,
                    }),
                    None => None,
                };
                self.drag = None;
                self.child_drag = None;
            }
        }
    }
}

impl eframe::App for DashboardApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

impl DashboardApp {
    /// One frame of the whole window: bars, tree, grid and dialogs, and
    /// saving settings and asking for values. Kept apart from
    /// [`eframe::App::ui`] so tests can run frames without an
    /// [`eframe::Frame`].
    fn show(&mut self, ui: &mut egui::Ui) {
        let clicked = egui::Panel::top("settings")
            .show(ui, |ui| {
                self.settings_bar(ui);
                self.tab_bar(ui)
            })
            .inner;
        let ctx = ui.ctx().clone();
        let dialog_open =
            self.confirm_close.is_some() || self.unsaved_prompt.is_some() || self.editing.is_some();
        let pressed = if dialog_open {
            None
        } else {
            tabs::shortcut(&ctx)
        };
        if let Some(action) = clicked.or(pressed) {
            self.run_tab_action(action);
        }
        if !dialog_open && let Some(action) = layout_file::shortcut(&ctx) {
            self.run_file_action(action);
        }
        if !dialog_open && let Some(action) = edit::shortcut(&ctx) {
            self.run_edit_action(&ctx, action);
        }
        // Closing the window with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && self.unsaved() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.unsaved_prompt = Some(AfterSaving::Quit);
        }
        self.close_dialog(&ctx);
        self.unsaved_dialog(&ctx);
        self.widget_dialog(&ctx);
        let title = format!(
            "{}{} - raptaboard",
            layout_file::display_name(self.layout_path.as_deref()),
            if self.unsaved() { " •" } else { "" }
        );
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        // One copy of every topic per frame, shared by the tree and the grid.
        let topics = self.nt.topics();
        self.nt.update_structs(&topics);
        let values: HashMap<&str, &Value> = topics
            .iter()
            .filter_map(|t| Some((&*t.name, &t.value.as_ref()?.value)))
            .collect();
        self.record_alerts(&values);
        // A hidden tree costs nothing: it isn't built and asks for no values.
        let tree_topics = if self.show_tree {
            egui::Panel::left("nt_tree")
                .resizable(true)
                .default_size(300.0)
                .show(ui, |ui| self.tree_panel(ui, &topics))
                .inner
        } else {
            Vec::new()
        };
        if self.show_alert_history {
            egui::Panel::right("alert_history")
                .resizable(true)
                .default_size(460.0)
                .show(ui, |ui| self.alert_history_panel(ui));
        }
        // "Add to grid" from the tree's right-click menu: widgets in the
        // free spots nearest the top left of the open tab.
        let add = std::mem::take(&mut self.tree_sel.add);
        if !add.is_empty() && !self.locked {
            self.add_topics(&add, self.tree_sel.add_to);
        }
        self.record_traces(&topics);
        let now = Instant::now();
        egui::CentralPanel::default().show(ui, |ui| self.grid(ui, &values, now));
        let next_value = self.sampler.finish_frame(now);
        let closing = ctx.input(|i| i.viewport().close_requested());
        let now = self.current_settings(settings::window_of(&ctx));
        if let Err(e) = self.settings.update(now, closing) {
            self.status = e;
        }
        // Whatever changed the layout this frame can be undone. Changes in
        // the settings dialog are recorded once, when it closes.
        if self.editing.is_none() {
            self.history.record(&self.layout);
        }

        self.nt.watch_values(tree_topics);
        // The history records every alert group, whatever the tab shows.
        let mut wanted = self.tab_topics();
        wanted.push((
            history_panel::HISTORY_PERIOD_S,
            self.history_topics(&topics),
        ));
        self.nt.watch_widget_values(wanted);

        // The client runs on its own thread, so egui doesn't know when values
        // change. Redraw when the next widget is due for a new value, and at
        // least a few times a second so connection changes and first values
        // still show.
        // At most about 60 a second: graphs' values are recorded as they
        // arrive, so drawing them more often adds nothing.
        let idle = Duration::from_millis(250);
        let wait = next_value.map_or(idle, |due| due.min(idle)).max(MIN_REDRAW);
        ui.ctx().request_repaint_after(wait);
    }
}
