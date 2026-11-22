//! The main window: settings bar, tab bar and the widget grid.

mod alert_history_panel;
mod dialogs;
mod drag;
mod grid_view;
mod live;
mod panels;
mod settings_window;
mod state;
mod style;
mod taps;
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
use crate::edit::{self, EditAction, History};
use crate::grid::Item;
use crate::layout_file::{self, FileAction, LoadError};
use crate::nt::NtConnection;
use crate::sampling::Sampler;
use crate::settings::{self, Settings};
use crate::sound::Sound;
use crate::tabs::{self, TabAction};
use crate::traces::Traces;

use self::dialogs::Editing;
use self::state::{Address, Alerts, GridState, LayoutFile, Sends, TreeView};

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

/// A layout file that couldn't be opened, shown in a dialog until it's
/// answered, so it isn't missed in the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadProblem {
    /// The file asked for, as given (a relative path is from the folder
    /// raptaboard was started in).
    path: PathBuf,
    /// There's no file there (else it couldn't be read or isn't a layout).
    missing: bool,
    /// Why, in words.
    why: String,
}

impl LoadProblem {
    fn new(path: PathBuf, error: &LoadError) -> Self {
        LoadProblem {
            path,
            missing: matches!(error, LoadError::NotFound),
            why: error.to_string(),
        }
    }
}

/// Top-level app state.
pub struct DashboardApp {
    /// The layout shown, as it's being edited.
    layout: ElasticLayout,
    /// Its file, and the questions about it waiting for an answer.
    file: LayoutFile,
    /// The last thing that happened, shown in the settings bar.
    status: String,
    current_tab: usize,
    /// Where to find the robot.
    address: Address,
    nt: NtConnection,
    /// The NetworkTables tree panel.
    tree: TreeView,
    /// When locked, widgets and tabs can't be moved, resized or edited.
    locked: bool,
    /// What's being dragged on the grid, and where it was.
    grid: GridState,
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
    /// The widget whose settings dialog is open, if any.
    editing: Option<Editing>,
    /// The values widgets show, taken once per each widget's period.
    sampler: Sampler,
    /// Recent values of the series graphs on the open tab show.
    traces: Traces,
    /// Whether tones play (a failed send's, and Alerts widgets').
    alert_sound: bool,
    /// Plays the tones.
    sound: Sound,
    /// The alert history and its panel.
    alerts: Alerts,
    /// Values widgets couldn't send.
    sends: Sends,
    /// Pointer events held back after a touchscreen tap (see [`taps`]).
    taps: taps::TapHold,
    /// The ⚙ Settings and About windows.
    gear: settings_window::GearWindows,
}

impl DashboardApp {
    /// Create the app from the startup options, loading the layout file if one is given.
    ///
    /// Fails only if the tokio runtime for the NT client cannot start.
    pub fn new(options: StartupOptions) -> std::io::Result<Self> {
        let mut app = DashboardApp {
            layout: ElasticLayout::default(),
            file: LayoutFile::default(),
            status: String::new(),
            current_tab: 0,
            address: Address {
                team: options.team,
                mode: options.mode,
                custom: options.custom_address,
                port: options.port,
            },
            nt: NtConnection::new()?,
            tree: TreeView {
                shown: options.show_tree,
                ..TreeView::default()
            },
            locked: options.locked,
            grid: GridState::default(),
            // A stand-in with nowhere to save, replaced below once the
            // starting settings are known.
            settings: settings::Tracker::new(settings::Store::default(), Settings::default()),
            renaming: None,
            confirm_close: None,
            selected: None,
            history: History::new(&ElasticLayout::default()),
            editing: None,
            sampler: Sampler::default(),
            traces: Traces::default(),
            alert_sound: options.alert_sound,
            sound: Sound::new(),
            alerts: Alerts {
                history: {
                    let mut h = crate::alert_history::AlertHistory::new(options.alert_history);
                    h.set_ignored(options.alert_history_ignore);
                    h
                },
                panel_shown: false,
                filter: Default::default(),
            },
            sends: Sends::default(),
            taps: taps::TapHold::default(),
            gear: Default::default(),
        };
        if let Some(path) = options.layout_path {
            let path = PathBuf::from(path);
            match layout_file::load(&path) {
                Ok(layout) => app.show_layout(layout, path),
                // Missing or bad: an empty layout, and a dialog saying so
                // (which can make a new file there).
                Err(e) => app.cant_open(path, &e),
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
            team: Some(self.address.team),
            address: Some(self.address.mode.keyword(&self.address.custom)),
            port: Some(self.address.port),
            show_tree: Some(self.tree.shown),
            locked: Some(self.locked),
            alert_sound: Some(self.alert_sound),
            alert_history: Some(self.alerts.history.capacity()),
            alert_history_ignore: Some(self.alerts.history.ignored().to_vec()),
            tab: self
                .layout
                .tabs
                .get(self.current_tab)
                .map(|t| t.name.clone()),
            tab_index: Some(self.current_tab),
            layout: self
                .file
                .path
                .as_ref()
                .map(|p| std::path::absolute(p).unwrap_or_else(|_| p.clone()))
                .map(|p| p.to_string_lossy().into_owned()),
            window,
        }
    }

    /// Whether the layout has changed since it was opened or saved.
    fn unsaved(&self) -> bool {
        self.layout != self.file.saved
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
        // Old widget types become today's as the file is read, so they
        // aren't shown as fixes or unsaved changes; the next save writes them.
        let mut layout = layout;
        crate::widgets::upgrade(&mut layout);
        self.file.saved = layout.clone();
        self.file.fixes = layout.sanitize();
        self.layout = layout;
        self.file.path = Some(path);
        self.current_tab = 0;
        self.grid.drag = None;
        self.grid.child_drag = None;
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
            FileAction::Open if self.unsaved() => {
                self.file.unsaved_prompt = Some(AfterSaving::Open)
            }
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
        let Some(path) = layout_file::pick_open(self.file.path.as_deref()) else {
            return;
        };
        match layout_file::load(&path) {
            Ok(layout) => self.show_layout(layout, path),
            Err(e) => self.cant_open(path, &e),
        }
    }

    /// Say that the layout file at `path` couldn't be opened: in the status
    /// line, and in a dialog until it's answered.
    fn cant_open(&mut self, path: PathBuf, error: &LoadError) {
        self.status = format!("Could not open {}: {error}", path.display());
        self.file.load_problem = Some(LoadProblem::new(path, error));
    }

    /// The "couldn't open the layout" dialog, while one is open: open
    /// another, or carry on with what's open (for a missing file, also
    /// start a new layout that saves there).
    fn load_problem_dialog(&mut self, ctx: &egui::Context) {
        let Some(problem) = self.file.load_problem.clone() else {
            return;
        };
        enum Answer {
            Open,
            NewHere,
            Close,
        }
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("load_problem")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading(if problem.missing {
                "Layout file not found"
            } else {
                "Couldn't open the layout"
            });
            ui.label(if problem.missing {
                "There's no layout file at:"
            } else {
                "This file couldn't be opened as a layout:"
            });
            ui.monospace(problem.path.display().to_string());
            if problem.missing {
                if let Ok(folder) = std::env::current_dir() {
                    ui.label(format!(
                        "A path given without a folder is looked for in the folder raptaboard \
                         was started in, {}.",
                        folder.display()
                    ));
                }
            } else {
                ui.label(&problem.why);
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Open a layout…").clicked() {
                    answer = Some(Answer::Open);
                }
                if problem.missing
                    && ui
                        .button("New layout saved here")
                        .on_hover_text("Start an empty layout; Save writes it to this path")
                        .clicked()
                {
                    answer = Some(Answer::NewHere);
                }
                if ui.button("Close").clicked() {
                    answer = Some(Answer::Close);
                }
            });
        });
        let answer = match answer {
            Some(a) => a,
            None if modal.should_close() => Answer::Close,
            None => return,
        };
        self.file.load_problem = None;
        match answer {
            Answer::Open => self.run_file_action(FileAction::Open),
            Answer::NewHere => {
                self.status = format!("New layout; Save writes {}", problem.path.display());
                self.file.path = Some(problem.path);
            }
            Answer::Close => {}
        }
    }

    /// Save the layout, asking where if `choose` is set or it has no file
    /// yet. Returns whether it was saved.
    fn save(&mut self, choose: bool) -> bool {
        let path = match &self.file.path {
            Some(path) if !choose => path.clone(),
            near => match layout_file::pick_save(near.as_deref()) {
                Some(path) => path,
                None => return false,
            },
        };
        match layout_file::save(&self.layout, &path) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.file.saved = self.layout.clone();
                self.file.fixes.clear();
                self.file.path = Some(path);
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
        let Some(after) = self.file.unsaved_prompt else {
            return;
        };
        let name = layout_file::display_name(self.file.path.as_deref());
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
                    self.file.unsaved_prompt = None;
                }
            });
        });
        if answer.is_none() {
            if modal.should_close() {
                self.file.unsaved_prompt = None;
            }
            return;
        }
        self.file.unsaved_prompt = None;
        if answer == Some(true) && !self.save(false) {
            return;
        }
        match after {
            AfterSaving::Open => self.open(),
            AfterSaving::Quit => {
                self.file.allow_close = true;
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
        self.grid.drag = None;
        self.grid.child_drag = None;
        self.selected = None;
        self.current_tab = tabs::apply(&mut self.layout, self.current_tab, action);
    }

    /// Carry out an editing command: undo, redo, or Delete, Duplicate and
    /// Deselect on the selected widget. Ignored while locked.
    fn run_edit_action(&mut self, ctx: &egui::Context, action: EditAction) {
        // Esc also unpicks tree rows, which can be picked while locked.
        if action == EditAction::Deselect {
            self.tree.selection.clear();
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
                    self.grid.drag = None;
                    self.grid.child_drag = None;
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
                self.grid.drag = None;
                self.grid.child_drag = None;
                self.status = format!("Deleted \"{title}\"; {undo_key} brings it back");
            }
            EditAction::Duplicate => {
                let Some(item) = self.selected else { return };
                let cell = f64::from(self.layout.grid_size());
                let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
                    return;
                };
                match item.duplicate(&mut tab.grid_layout, cell, self.grid.room) {
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
                self.grid.drag = None;
                self.grid.child_drag = None;
            }
        }
    }
}

impl eframe::App for DashboardApp {
    /// Before egui reads a frame's input: a touchscreen tap's move back to
    /// the mouse waits a frame, so the tap lands where it was (see `ui/taps.rs`).
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        taps::hold_after_taps(&mut self.taps, ctx, raw_input);
    }

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
        // Pointer and touch events, when RAPTABOARD_LOG_INPUT is set.
        crate::input_log::log_frame(ui.ctx());
        // One copy of every topic per frame, shared by the tab marks, the tree
        // and the grid.
        let topics = self.nt.topics();
        self.nt.update_structs(&topics);
        let values: HashMap<&str, &Value> = topics
            .iter()
            .filter_map(|t| Some((&*t.name, &t.value.as_ref()?.value)))
            .collect();
        let clicked = egui::Panel::top("settings")
            .show(ui, |ui| {
                self.settings_bar(ui);
                self.tab_bar(ui, &values)
            })
            .inner;
        let ctx = ui.ctx().clone();
        let dialog_open = self.confirm_close.is_some()
            || self.file.unsaved_prompt.is_some()
            || self.file.load_problem.is_some()
            || self.editing.is_some();
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
        if ctx.input(|i| i.viewport().close_requested()) && self.unsaved() && !self.file.allow_close
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.file.unsaved_prompt = Some(AfterSaving::Quit);
        }
        self.close_dialog(&ctx);
        self.unsaved_dialog(&ctx);
        self.load_problem_dialog(&ctx);
        self.widget_dialog(&ctx);
        self.gear_windows(&ctx);
        let title = format!(
            "{}{} - raptaboard",
            layout_file::display_name(self.file.path.as_deref()),
            if self.unsaved() { " •" } else { "" }
        );
        if title != self.file.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.file.title = title;
        }
        self.record_alert_history(&values);
        // A hidden tree costs nothing: it isn't built and asks for no values.
        let tree_topics = if self.tree.shown {
            egui::Panel::left("nt_tree")
                .resizable(true)
                .default_size(300.0)
                .show(ui, |ui| self.tree_panel(ui, &topics))
                .inner
        } else {
            Vec::new()
        };
        if self.alerts.panel_shown {
            egui::Panel::right("alert_history")
                .resizable(true)
                .default_size(460.0)
                .show(ui, |ui| self.alert_history_panel(ui));
        }
        // "Add to grid" from the tree's right-click menu: widgets in the
        // free spots nearest the top left of the open tab.
        let add = std::mem::take(&mut self.tree.selection.add);
        if !add.is_empty() && !self.locked {
            let (to, as_type) = (self.tree.selection.add_to, self.tree.selection.add_as);
            self.add_topics(&add, to, as_type);
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
        let mut wanted = self.tab_topics(&topics);
        wanted.push((
            alert_history_panel::ALERT_HISTORY_PERIOD_S,
            self.alert_history_topics(&topics),
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
