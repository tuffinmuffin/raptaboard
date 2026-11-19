//! The main window: settings bar, tab bar and the widget grid.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CursorIcon, Rect, Sense, Stroke, Vec2};
use nt4::{Topic, Value};
use raptaboard_layout::{
    ElasticLayout, GridLayout, LayoutContainer, Rect as LayoutRect, WidgetContainer,
};

use crate::cli::StartupOptions;
use crate::connection::{AddressMode, resolve};
use crate::edit::{self, EditAction, History};
use crate::grid::{self, Item};
use crate::layout_file::{self, FileAction, LoadError};
use crate::nt::{ConnectionStatus, NtConnection};
use crate::sampling::Sampler;
use crate::struct_view::{self, FieldInfo};
use crate::tabs::{self, TabAction};
use crate::tree::{AddTo, DraggedTopics, Folder, TopicEntry, TreeSelection};
use crate::widgets::shared::{card, format, props, settings};
use crate::widgets::{self, list_layout};
use nt4::proto::structs::StructDatabase;

// How the grid looks. All sizes are screen points, not grid pixels: they stay
// the same at any grid size (see "Grid size" in docs/spec.md).

/// Size of the resize corner at the bottom right of each widget, in points.
const RESIZE_HANDLE: f32 = 14.0;
/// Space between a widget's card and the edges of its cells, in points, so
/// side-by-side widgets don't touch.
const CELL_GAP: f32 = 4.0;
/// Width of the grid lines, in points.
const GRID_LINE_WIDTH: f32 = 1.0;
/// How strongly the grid lines show, from 0 (not at all) to 1.
const GRID_LINE_STRENGTH: f32 = 0.3;
/// Width of the selection outline and the drop outline, in points.
const OUTLINE_WIDTH: f32 = 2.0;
/// How far inside its cells the selection outline is drawn, in points.
const SELECTION_INSET: f32 = 1.0;
/// How far inside its cells the drop outline is drawn, in points.
const LANDING_INSET: f32 = 2.0;
/// Lengths of the resize corner's two diagonal marks, in points.
const RESIZE_MARKS: [f32; 2] = [5.0, 9.0];
/// Width of the resize marks, in points.
const RESIZE_MARK_WIDTH: f32 = 1.5;
/// How far the resize marks sit in from the card's corner, in points.
const RESIZE_MARK_INSET: f32 = 4.0;
/// Color of a drop outline where the drop fits.
const DROP_OK: Color32 = Color32::from_rgb(60, 180, 90);
/// Color of a drop outline where the drop would cover something.
const DROP_BLOCKED: Color32 = Color32::from_rgb(220, 60, 60);
/// Color of the outline on widgets showing a topic picked in the tree.
const FOUND_COLOR: Color32 = Color32::from_rgb(230, 160, 40);

/// What a drag on the grid is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    /// Moving the whole item.
    Move,
    /// Changing its size from the bottom-right corner.
    Resize,
}

/// A widget or List Layout being dragged.
#[derive(Debug, Clone, Copy)]
struct Drag {
    item: Item,
    mode: DragMode,
    /// Where the item was when the drag started.
    start: LayoutRect,
    /// How far the pointer has moved since then, in points.
    moved: Vec2,
}

impl Drag {
    /// Where the item is while it follows the pointer, before snapping.
    fn following(&self) -> LayoutRect {
        let (dx, dy) = (f64::from(self.moved.x), f64::from(self.moved.y));
        match self.mode {
            DragMode::Move => LayoutRect {
                x: self.start.x + dx,
                y: self.start.y + dy,
                ..self.start
            },
            DragMode::Resize => LayoutRect {
                width: (self.start.width + dx).max(1.0),
                height: (self.start.height + dy).max(1.0),
                ..self.start
            },
        }
    }

    /// Where the item lands if dropped now: snapped to the grid.
    fn target(&self, cell: f64, area: (f64, f64)) -> LayoutRect {
        match self.mode {
            DragMode::Move => grid::snap_move(self.following(), cell, area),
            DragMode::Resize => grid::snap_resize(self.following(), cell),
        }
    }
}

/// A List Layout's child being dragged: to another place in its list, into
/// another list, or out onto the grid.
#[derive(Debug, Clone, Copy)]
struct ChildDrag {
    /// The list, by its index in the tab's `layouts`.
    list: usize,
    /// The child, by its index in the list's `children`.
    child: usize,
    /// Where in its row the pointer took hold, in points, so the preview
    /// keeps the same spot under the pointer.
    grab: Vec2,
}

/// Where something dropped on the grid would go.
#[derive(Debug, Clone, Copy)]
enum DropTarget {
    /// Into List Layout `list` as child number `at`. `card` is the list on
    /// screen and `line_y` where the "goes here" line is drawn.
    List {
        list: usize,
        at: usize,
        card: Rect,
        line_y: f32,
    },
    /// Onto the grid at `rect`, if it's `free`.
    Grid { rect: LayoutRect, free: bool },
}

/// A widget whose settings dialog is open.
#[derive(Debug, Clone)]
enum Editing {
    /// A widget on the grid: its tab, its index in the tab's `containers`,
    /// and the widget as it was when the dialog opened, for Cancel.
    Widget {
        tab: usize,
        index: usize,
        original: WidgetContainer,
    },
    /// A List Layout: its tab, its index in the tab's `layouts`, and the
    /// list (with its children) as it was, for Cancel. `child` is the child
    /// whose settings are open inside the list's dialog, with that child as
    /// it was when they opened.
    List {
        tab: usize,
        index: usize,
        original: Box<LayoutContainer>,
        child: Option<(usize, WidgetContainer)>,
    },
}

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
            locked: false,
            drag: None,
            child_drag: None,
            menu_at: (0.0, 0.0),
            tree_sel: TreeSelection::default(),
            renaming: None,
            confirm_close: None,
            selected: None,
            history: History::new(&ElasticLayout::default()),
            grid_room: (0.0, 0.0),
            editing: None,
            sampler: Sampler::default(),
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
        Ok(app)
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

    fn settings_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("raptaboard");
            let (label, hint) = if self.show_tree {
                ("◀ Tree", "Hide the NetworkTables tree")
            } else {
                ("▶ Tree", "Show the NetworkTables tree")
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                self.show_tree = !self.show_tree;
            }
            let (label, hint) = if self.locked {
                (
                    "🔒 Locked",
                    "Unlock to move, resize and edit widgets and tabs",
                )
            } else {
                (
                    "🔓 Unlocked",
                    "Lock the layout so nothing moves by accident",
                )
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                self.locked = !self.locked;
                self.drag = None;
                self.child_drag = None;
                self.renaming = None;
                self.selected = None;
            }
            ui.separator();
            ui.label("Team");
            ui.add(egui::DragValue::new(&mut self.team).range(1..=99999));
            egui::ComboBox::from_id_salt("address_mode")
                .selected_text(self.mode.label())
                .show_ui(ui, |ui| {
                    for mode in AddressMode::ALL {
                        ui.selectable_value(&mut self.mode, mode, mode.label());
                    }
                });
            if self.mode == AddressMode::Custom {
                ui.text_edit_singleline(&mut self.custom_address);
            }
            let host = resolve(self.mode, self.team, &self.custom_address);
            self.nt.set_target(host.as_deref(), self.port);
            ui.separator();
            status_label(ui, &self.nt.status());
        });
        ui.horizontal(|ui| {
            let ctx = ui.ctx().clone();
            for (label, action) in [
                ("Open…", FileAction::Open),
                ("Save", FileAction::Save),
                ("Save as…", FileAction::SaveAs),
            ] {
                if ui
                    .button(label)
                    .on_hover_text(ctx.format_shortcut(&action.shortcut()))
                    .clicked()
                {
                    self.run_file_action(action);
                }
            }
            ui.separator();
            for (label, action, can) in [
                ("Undo", EditAction::Undo, self.history.can_undo()),
                ("Redo", EditAction::Redo, self.history.can_redo()),
            ] {
                let button = ui.add_enabled(!self.locked && can, egui::Button::new(label));
                if button
                    .on_hover_text(ctx.format_shortcut(&action.shortcut()))
                    .on_disabled_hover_text(if self.locked {
                        "Unlock to change the layout"
                    } else {
                        "Nothing to undo or redo"
                    })
                    .clicked()
                {
                    self.run_edit_action(&ctx, action);
                }
            }
            ui.separator();
            let name = layout_file::display_name(self.layout_path.as_deref());
            let name = ui.strong(name);
            if let Some(path) = &self.layout_path {
                name.on_hover_text(path.display().to_string());
            }
            if self.unsaved() {
                ui.label(egui::RichText::new("●").color(Color32::from_rgb(230, 160, 40)))
                    .on_hover_text("Unsaved changes");
            }
            ui.weak(&self.status);
            if !self.fixes.is_empty() {
                let n = self.fixes.len();
                let list = self.fixes.join("\n");
                let badge = ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "⚠ fixed {n} problem{}",
                                if n == 1 { "" } else { "s" }
                            ))
                            .color(Color32::from_rgb(230, 160, 40)),
                        )
                        .sense(Sense::click()),
                    )
                    .on_hover_text(format!("{list}\n\nRight-click to copy this list"));
                // Copy the list so it can be pasted into a bug report or chat.
                badge.context_menu(|ui| {
                    if ui.button("Copy list").clicked() {
                        ui.ctx().copy_text(list.clone());
                        self.status = format!(
                            "Copied {n} fix{} to the clipboard",
                            if n == 1 { "" } else { "es" }
                        );
                    }
                });
            }
        });
    }

    /// The NetworkTables tree in the left panel. Read-only: values are
    /// changed through widgets, not here.
    ///
    /// Returns the topics drawn, whose values the tree needs.
    fn tree_panel(&mut self, ui: &mut egui::Ui, topics: &[Topic]) -> Vec<String> {
        ui.horizontal(|ui| {
            ui.heading("NetworkTables");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("◀")
                    .on_hover_text("Hide the tree")
                    .clicked()
                {
                    self.show_tree = false;
                }
            });
        });
        // A plain row: a right-to-left layout here would fill the rest of the
        // panel's height and push the tree out of view.
        ui.horizontal(|ui| {
            let clear_width = 48.0;
            let filter_width = ui.available_width() - clear_width - ui.spacing().item_spacing.x;
            ui.add(
                egui::TextEdit::singleline(&mut self.tree_filter)
                    .hint_text("Filter topics")
                    .desired_width(filter_width),
            );
            if ui
                .add_enabled(
                    !self.tree_filter.is_empty(),
                    egui::Button::new("Clear").min_size(egui::vec2(clear_width, 0.0)),
                )
                .clicked()
            {
                self.tree_filter.clear();
            }
        });
        ui.checkbox(&mut self.show_metadata, "Show metadata (.type, /.schema)");
        ui.separator();
        if topics.is_empty() {
            ui.weak(match self.nt.status() {
                ConnectionStatus::Connected { .. } => "No topics yet",
                _ => "Not connected",
            });
            return Vec::new();
        }
        let total = topics.len();
        let tree = Folder::build(
            topics.iter().cloned().map(TopicEntry::from),
            self.show_metadata,
            &self.tree_filter,
        );
        let filtering = !self.tree_filter.trim().is_empty();
        // Say when topics are hidden, so a forgotten filter isn't mistaken
        // for missing data.
        let shown = tree.topic_count();
        if shown < total {
            let why = if filtering {
                "filter and hidden metadata"
            } else {
                "hidden metadata"
            };
            let why = if filtering && self.show_metadata {
                "filter"
            } else {
                why
            };
            ui.weak(format!("Showing {shown} of {total} topics ({why})"));
        }
        self.tree_sel.begin_frame();
        self.tree_sel.lists = self.list_names();
        let no_structs = StructDatabase::new();
        let structs = self.nt.structs().unwrap_or(&no_structs);
        let drawn = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Rows are picked, not their text: no text cursor or text
                // highlighting, which would look like picking.
                ui.style_mut().interaction.selectable_labels = false;
                tree.show(ui, "", filtering, structs, !self.locked, &mut self.tree_sel)
            })
            .inner;
        self.tree_sel.end_frame();
        drawn
    }

    /// The topics the widgets on the open tab are bound to, grouped by how
    /// often they want updates (each widget's `period`, in seconds). Only
    /// widgets raptaboard can draw are counted.
    fn tab_topics(&self) -> Vec<(f64, Vec<String>)> {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return Vec::new();
        };
        let grid = &tab.grid_layout;
        let drawn = grid
            .containers
            .iter()
            .chain(
                grid.layouts
                    .iter()
                    .flat_map(|l| &l.children)
                    .filter(|w| !list_layout::is_hidden(w)),
            )
            .filter(|w| widgets::find(&w.widget_type).is_some());
        let mut groups: Vec<(f64, Vec<String>)> = Vec::new();
        for w in drawn {
            let Some(topic) = w.topic() else { continue };
            let period = props::period_s(w);
            match groups.iter_mut().find(|(p, _)| *p == period) {
                Some((_, names)) => names.push(topic.to_owned()),
                None => groups.push((period, vec![topic.to_owned()])),
            }
        }
        groups
    }

    /// The row of tabs. Click to switch; when unlocked, double-click to
    /// rename, right-click for more, and "+" adds a tab.
    fn tab_bar(&mut self, ui: &mut egui::Ui) -> Option<TabAction> {
        let mut action = None;
        let names: Vec<String> = self.layout.tabs.iter().map(|t| t.name.clone()).collect();
        let count = names.len();
        // Tabs with widgets for the topics picked in the tree get a mark.
        let picked = self.tree_sel.selected();
        let has_picked: Vec<bool> = self
            .layout
            .tabs
            .iter()
            .map(|t| {
                let grid = &t.grid_layout;
                grid.containers
                    .iter()
                    .chain(grid.layouts.iter().flat_map(|l| &l.children))
                    .any(|w| picked.iter().any(|p| p.is_shown_by(w)))
            })
            .collect();
        ui.horizontal(|ui| {
            for (i, name) in names.iter().enumerate() {
                if let Some((renaming, text, focused)) = &mut self.renaming
                    && *renaming == i
                {
                    let edit = ui.add(egui::TextEdit::singleline(text).desired_width(120.0));
                    // Focus the box once, when renaming starts. Asking every
                    // frame would keep it from ever losing focus, so Enter
                    // and clicking away would do nothing.
                    if !*focused {
                        edit.request_focus();
                        *focused = true;
                    }
                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        self.renaming = None;
                    } else if edit.lost_focus() {
                        action = Some(TabAction::Rename(i, text.clone()));
                        self.renaming = None;
                    }
                    continue;
                }
                let marked = has_picked[i] && self.current_tab != i;
                let label = if marked {
                    egui::RichText::new(format!("{name} ●"))
                } else {
                    egui::RichText::new(name)
                };
                let mut hint = tabs::tab_hint(ui.ctx(), i, self.locked);
                if marked {
                    hint = format!("{hint}\nHas widgets for the topics picked in the tree");
                }
                let tab = ui
                    .selectable_label(self.current_tab == i, label)
                    .on_hover_text(hint);
                if tab.clicked() {
                    action = Some(TabAction::Select(i));
                }
                if self.locked {
                    continue;
                }
                if tab.double_clicked() {
                    self.renaming = Some((i, name.clone(), false));
                }
                tab.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        self.renaming = Some((i, name.clone(), false));
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move left"))
                        .clicked()
                    {
                        // Moves act on the open tab, so open this one first.
                        self.current_tab = i;
                        action = Some(TabAction::MoveLeft);
                    }
                    if ui
                        .add_enabled(i + 1 < count, egui::Button::new("Move right"))
                        .clicked()
                    {
                        self.current_tab = i;
                        action = Some(TabAction::MoveRight);
                    }
                    if ui
                        .add_enabled(count > 1, egui::Button::new("Close"))
                        .clicked()
                    {
                        action = Some(TabAction::Close(i));
                    }
                });
            }
            if !self.locked && ui.small_button("+").on_hover_text("New tab").clicked() {
                action = Some(TabAction::New);
            }
        });
        action
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
                    }),
                    None => None,
                };
                self.drag = None;
                self.child_drag = None;
            }
        }
    }

    /// The NT type of the value `widget` shows: its saved `data_type`, or
    /// for a struct field the type in its `struct_meta`, or else the type
    /// the server announced for its topic. `None` if none of those is known.
    fn value_type(&self, widget: &WidgetContainer) -> Option<String> {
        let saved = |v: Option<&serde_json::Value>| v?.as_str().map(str::to_owned);
        if let Some(ty) = saved(widget.properties.get(props::DATA_TYPE)) {
            return Some(ty);
        }
        if widget.struct_meta().is_some() {
            return saved(
                widget
                    .properties
                    .get("struct_meta")
                    .and_then(|m| m.get("type")),
            );
        }
        let topic = self.nt.client()?.topic(widget.topic()?)?;
        Some(topic.ty.to_string())
    }

    /// The settings dialog for a widget or List Layout, while one is open.
    /// Changes show on the grid as they're made; Cancel or Esc puts things
    /// back as they were, and the whole visit is one step for undo.
    fn widget_dialog(&mut self, ctx: &egui::Context) {
        match self.editing {
            Some(Editing::Widget { .. }) => self.single_widget_dialog(ctx),
            Some(Editing::List { .. }) => self.list_dialog(ctx),
            None => {}
        }
    }

    /// The type and struct field info for `widget`'s settings.
    fn value_facts(&self, widget: &WidgetContainer) -> (Option<String>, Option<FieldInfo>) {
        let field = widget.struct_meta().and_then(|meta| {
            struct_view::field_info(self.nt.structs()?, &meta.schema_name, &meta.path)
        });
        (self.value_type(widget), field)
    }

    /// The settings of a widget on the grid.
    fn single_widget_dialog(&mut self, ctx: &egui::Context) {
        let Some(Editing::Widget { tab, index, .. }) = self.editing else {
            return;
        };
        let Some(widget) = self.widget_at(tab, index) else {
            self.editing = None;
            return;
        };
        let (nt_type, field) = self.value_facts(widget);
        let Some(widget) = self
            .layout
            .tabs
            .get_mut(tab)
            .and_then(|t| t.grid_layout.containers.get_mut(index))
        else {
            return;
        };
        let close = widget_window(
            ctx,
            "Widget settings",
            widget,
            nt_type.as_deref(),
            field.as_ref(),
        );
        match close {
            Some(true) => self.editing = None,
            Some(false) => {
                if let Some(Editing::Widget { original, .. }) = self.editing.take() {
                    *widget = original;
                }
            }
            None => {}
        }
    }

    /// The widget at `index` in tab `tab`'s `containers`.
    fn widget_at(&self, tab: usize, index: usize) -> Option<&WidgetContainer> {
        self.layout.tabs.get(tab)?.grid_layout.containers.get(index)
    }

    /// The settings of a List Layout: its title, label position and
    /// children. A child's Edit… shows that child's widget settings in the
    /// same dialog; its Done or Cancel comes back to the list.
    fn list_dialog(&mut self, ctx: &egui::Context) {
        let Some(Editing::List {
            tab, index, child, ..
        }) = &self.editing
        else {
            return;
        };
        let (tab, index, child) = (*tab, *index, child.as_ref().map(|(c, _)| *c));
        let Some(list) = self
            .layout
            .tabs
            .get(tab)
            .and_then(|t| t.grid_layout.layouts.get(index))
        else {
            self.editing = None;
            return;
        };

        // A child's settings, inside the list's dialog.
        if let Some(c) = child {
            let Some(widget) = list.children.get(c) else {
                self.close_child();
                return;
            };
            let (nt_type, field) = self.value_facts(widget);
            let heading = format!("{}: {}", list.title, widget.title);
            let Some(widget) = self
                .layout
                .tabs
                .get_mut(tab)
                .and_then(|t| t.grid_layout.layouts.get_mut(index))
                .and_then(|l| l.children.get_mut(c))
            else {
                return;
            };
            match widget_window(ctx, &heading, widget, nt_type.as_deref(), field.as_ref()) {
                Some(true) => self.close_child(),
                Some(false) => {
                    if let Some(Editing::List {
                        child: Some((_, original)),
                        ..
                    }) = &self.editing
                    {
                        *widget = original.clone();
                    }
                    self.close_child();
                }
                None => {}
            }
            return;
        }

        let Some(list) = self
            .layout
            .tabs
            .get_mut(tab)
            .and_then(|t| t.grid_layout.layouts.get_mut(index))
        else {
            return;
        };
        let mut close = None;
        let mut action = None;
        egui::Modal::new(egui::Id::unique("list_settings")).show(ctx, |ui| {
            ui.heading("List settings");
            ui.add_space(4.0);
            action = list_layout::settings(ui, list);
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Done").clicked() {
                    close = Some(true);
                }
                if ui.button("Cancel").on_hover_text("Esc").clicked() {
                    close = Some(false);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = Some(false);
        }
        if let Some(list_layout::ListAction::EditChild(c)) = action
            && let Some(widget) = list.children.get(c)
            && let Some(Editing::List { child, .. }) = &mut self.editing
        {
            *child = Some((c, widget.clone()));
        }
        match close {
            Some(true) => self.editing = None,
            Some(false) => {
                if let Some(Editing::List { original, .. }) = self.editing.take() {
                    *list = *original;
                }
            }
            None => {}
        }
    }

    /// Names for the open tab's List Layouts, in order: their titles, with
    /// a number added where two share one ("List Layout (2)").
    fn list_names(&self) -> Vec<String> {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return Vec::new();
        };
        list_layout::menu_names(&tab.grid_layout.layouts)
    }

    /// Make widgets for `topics` on the open tab, one cell each, where `to`
    /// says: free spots nearest the tab's top left, the end of a list, or a
    /// new list. The status line says what happened.
    fn add_topics(&mut self, topics: &[crate::tree::DraggedTopic], to: AddTo) {
        let cell = f64::from(self.layout.grid_size());
        let one = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: cell,
            height: cell,
        };
        let made: Vec<WidgetContainer> = topics
            .iter()
            .filter_map(|t| widgets::for_topic(&t.name, &t.ty, t.field.as_ref(), one))
            .collect();
        let wanted = made.len();
        let room = self.grid_room;
        let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
            return;
        };
        if to == AddTo::NewList {
            match grid::add_list_holding(&mut tab.grid_layout, (0.0, 0.0), made, cell, room) {
                Some(i) => {
                    self.selected = Some(Item::List(i));
                    self.status = format!(
                        "Made a list of {wanted} widget{}",
                        if wanted == 1 { "" } else { "s" }
                    );
                }
                None => self.status = "No free space for a new list".to_owned(),
            }
            return;
        }
        if let AddTo::List(i) = to
            && let Some(l) = tab.grid_layout.layouts.get_mut(i)
        {
            l.children.extend(made);
            self.selected = Some(Item::List(i));
            self.status = format!(
                "Added {wanted} widget{} to \"{}\"",
                if wanted == 1 { "" } else { "s" },
                l.title
            );
            return;
        }
        let placed = grid::place_near(&mut tab.grid_layout, made, (0.0, 0.0), cell, room);
        self.selected = placed.first().map(|&i| Item::Widget(i));
        self.status = if placed.len() < wanted {
            format!(
                "Added {} of {wanted} widgets; no free space for the rest",
                placed.len()
            )
        } else {
            format!(
                "Added {wanted} widget{}",
                if wanted == 1 { "" } else { "s" }
            )
        };
    }

    /// Leave a child's settings and go back to its list's.
    fn close_child(&mut self) {
        if let Some(Editing::List { child, .. }) = &mut self.editing {
            *child = None;
        }
    }

    /// The "close this tab?" question, while one is open.
    fn close_dialog(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_close else { return };
        let Some(tab) = self.layout.tabs.get(i) else {
            self.confirm_close = None;
            return;
        };
        let name = tab.name.clone();
        let widgets = tab.grid_layout.containers.len() + tab.grid_layout.layouts.len();
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm_close_tab")).show(ctx, |ui| {
            ui.heading(format!("Close \"{name}\"?"));
            ui.label(format!(
                "Its {widgets} widget{} will be removed.",
                if widgets == 1 { "" } else { "s" }
            ));
            ui.horizontal(|ui| {
                if ui.button("Close tab").clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => self.run_tab_action(TabAction::Close(i)),
            Some(false) => self.confirm_close = None,
            None if modal.should_close() => self.confirm_close = None,
            None => {}
        }
    }

    /// The open tab's grid: lines, widgets, and (when unlocked) dragging to
    /// move widgets and their bottom-right corner to resize them. Drops snap
    /// to the grid and are refused if they would cover another widget.
    fn grid(&mut self, ui: &mut egui::Ui, values: &HashMap<&str, &Value>, now: Instant) {
        // Clicking empty grid drops the selection.
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        if response.clicked() {
            self.selected = None;
        }
        let area = response.rect;
        let origin = area.min;
        // Right-clicking empty grid: a menu to make a new list there.
        let mut new_list = false;
        if !self.locked {
            if response.secondary_clicked()
                && let Some(p) = response.interact_pointer_pos()
            {
                self.menu_at = (f64::from(p.x - origin.x), f64::from(p.y - origin.y));
                self.selected = None;
            }
            response.context_menu(|ui| {
                if ui
                    .button("New List Layout")
                    .on_hover_text("An empty list here, to drag widgets into")
                    .clicked()
                {
                    new_list = true;
                }
            });
        }
        let cell = f64::from(self.layout.grid_size());
        let visuals = ui.visuals().clone();
        let line = Stroke::new(
            GRID_LINE_WIDTH,
            visuals.weak_text_color().gamma_multiply(GRID_LINE_STRENGTH),
        );

        let mut x = 0.0;
        while x <= area.width() {
            painter.vline(origin.x + x, area.y_range(), line);
            x += cell as f32;
        }
        let mut y = 0.0;
        while y <= area.height() {
            painter.hline(area.x_range(), origin.y + y, line);
            y += cell as f32;
        }

        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return;
        };
        let to_screen = |r: &LayoutRect| {
            Rect::from_min_size(
                origin + Vec2::new(r.x as f32, r.y as f32),
                Vec2::new(r.width as f32, r.height as f32),
            )
        };
        let room = (f64::from(area.width()), f64::from(area.height()));
        self.grid_room = room;
        let placed = grid::items(&tab.grid_layout);

        // Dragging. Each item's corner is registered after its body so the
        // corner wins where they overlap.
        let mut dropped = None;
        let mut menu_action = None;
        let mut edit_child = None;
        let mut wrap = None;
        if !self.locked {
            for &(item, r) in &placed {
                let rect = to_screen(&r);
                let id = ui.scope_id().with(("grid_item", self.current_tab, item));
                let corner = Rect::from_min_max(rect.max - Vec2::splat(RESIZE_HANDLE), rect.max);
                let body = ui.interact(rect, id, Sense::click_and_drag());
                // A list's children, on top of its body so a child can be
                // dragged on its own, but under its resize corner.
                if let Item::List(index) = item
                    && let Some(list) = tab.grid_layout.layouts.get(index)
                {
                    let card = rect.shrink(CELL_GAP);
                    for row in list_layout::rows_on_screen(card, list) {
                        let area = row.row.intersect(card);
                        let child_id = id.with(("child", row.index));
                        let child = ui.interact(area, child_id, Sense::click_and_drag());
                        if child.drag_started()
                            && let Some(p) = child.interact_pointer_pos()
                        {
                            self.child_drag = Some(ChildDrag {
                                list: index,
                                child: row.index,
                                grab: p - row.row.min,
                            });
                        }
                        if child.clicked() || child.secondary_clicked() || child.drag_started() {
                            self.selected = Some(item);
                        }
                        if child.clicked()
                            && let Some(w) = list.children.get(row.index)
                            && let Some(topic) = w.topic()
                        {
                            self.tree_sel.reveal(topic, w.struct_meta().map(|m| m.path));
                        }
                        if child.double_clicked() {
                            edit_child = Some((index, row.index));
                        }
                        if child.dragged() {
                            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                        } else if child.hovered() {
                            ui.ctx().set_cursor_icon(CursorIcon::Grab);
                        }
                    }
                }
                let corner = ui.interact(corner, id.with("resize"), Sense::drag());
                for (handle, mode) in [(&body, DragMode::Move), (&corner, DragMode::Resize)] {
                    if handle.drag_started() {
                        self.drag = Some(Drag {
                            item,
                            mode,
                            start: r,
                            moved: Vec2::ZERO,
                        });
                    }
                    if let Some(drag) = &mut self.drag
                        && drag.item == item
                        && drag.mode == mode
                    {
                        drag.moved += handle.drag_delta();
                        if handle.drag_stopped() {
                            dropped = self.drag.take();
                        }
                    }
                }
                if body.clicked() || body.secondary_clicked() || body.drag_started() {
                    self.selected = Some(item);
                }
                // Clicking a widget finds its topic in the tree.
                if body.clicked()
                    && let Item::Widget(i) = item
                    && let Some(w) = tab.grid_layout.containers.get(i)
                    && let Some(topic) = w.topic()
                {
                    self.tree_sel.reveal(topic, w.struct_meta().map(|m| m.path));
                }
                if body.double_clicked() {
                    menu_action = Some((item, EditAction::Edit));
                }
                body.context_menu(|ui| {
                    let ctx = ui.ctx().clone();
                    let actions = [
                        ("Edit…", EditAction::Edit),
                        ("Duplicate", EditAction::Duplicate),
                        ("Delete", EditAction::Delete),
                    ];
                    for (label, action) in actions {
                        let button = egui::Button::new(label)
                            .shortcut_text(ctx.format_shortcut(&action.shortcut()));
                        if ui.add(button).clicked() {
                            menu_action = Some((item, action));
                        }
                    }
                    if let Item::Widget(widget) = item
                        && ui
                            .button("Put in a new list")
                            .on_hover_text("Make a List Layout here holding this widget")
                            .clicked()
                    {
                        wrap = Some(widget);
                    }
                });
                if corner.hovered() || corner.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
                } else if body.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                } else if body.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
            }
        }

        // A widget's value: its topic's, taken once per the widget's period,
        // or one field of it for widgets bound to a struct field.
        let structs = self.nt.structs();
        let sampler = RefCell::new(&mut self.sampler);
        let value_of = |w: &WidgetContainer| -> Option<Value> {
            let topic = w.topic()?;
            let latest = values.get(topic).copied();
            let value = sampler
                .borrow_mut()
                .sample(topic, props::period_s(w), latest, now)?;
            match (w.struct_meta(), &value) {
                (None, _) => Some(value),
                (Some(meta), Value::Raw(bytes)) => struct_view::field_value_shown(
                    structs?,
                    &meta.schema_name,
                    bytes,
                    &meta.path,
                    format::enum_display(&w.properties),
                    format::int_style(&w.properties),
                ),
                (Some(_), _) => None,
            }
        };
        let dragging = self.drag.map(|d| d.item);
        for &(item, r) in &placed {
            if Some(item) != dragging {
                draw_item(
                    &painter,
                    &visuals,
                    to_screen(&r),
                    &tab.grid_layout,
                    item,
                    &value_of,
                );
                if !self.locked {
                    draw_resize_handle(&painter, &visuals, to_screen(&r));
                }
            }
        }
        // The selected item gets an outline in the selection color.
        if let Some(item) = self.selected
            && Some(item) != dragging
            && let Some(r) = item.rect(&tab.grid_layout)
        {
            painter.rect_stroke(
                to_screen(&r).shrink(SELECTION_INSET),
                egui::CornerRadius::same(card::CORNER_RADIUS),
                Stroke::new(OUTLINE_WIDTH, visuals.selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }
        // Widgets showing the topics picked in the tree get an outline, so
        // it's easy to see where a value is already shown.
        let picked = self.tree_sel.selected();
        if !picked.is_empty() {
            let shows = |w: &WidgetContainer| picked.iter().any(|t| t.is_shown_by(w));
            let stroke = Stroke::new(OUTLINE_WIDTH, FOUND_COLOR);
            for w in tab.grid_layout.containers.iter().filter(|w| shows(w)) {
                painter.rect_stroke(
                    to_screen(&w.rect).shrink(SELECTION_INSET),
                    egui::CornerRadius::same(card::CORNER_RADIUS),
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            for l in &tab.grid_layout.layouts {
                let card = to_screen(&l.rect).shrink(CELL_GAP);
                for row in list_layout::rows_on_screen(card, l) {
                    if shows(&l.children[row.index]) {
                        painter.rect_stroke(
                            row.row.expand(2.0).intersect(card),
                            egui::CornerRadius::same(card::BODY_CORNER_RADIUS),
                            stroke,
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
        }

        // Where the pointer is, in grid pixels, and the list under it as a
        // drop target, if any.
        let pointer_screen = ui.ctx().pointer_latest_pos();
        let to_grid = |p: egui::Pos2| (f64::from(p.x - origin.x), f64::from(p.y - origin.y));
        let list_drop = |p: egui::Pos2| -> Option<DropTarget> {
            if !area.contains(p) {
                return None;
            }
            let list = grid::list_at(&tab.grid_layout, to_grid(p))?;
            let l = &tab.grid_layout.layouts[list];
            let card = to_screen(&l.rect).shrink(CELL_GAP);
            let rows = list_layout::rows_on_screen(card, l);
            let at = list_layout::insert_index(&rows, l.children.len(), p.y);
            let line_y = list_layout::insert_line_y(&rows, at, list_layout::body_rect(card));
            Some(DropTarget::List {
                list,
                at,
                card,
                line_y,
            })
        };

        // The dragged item follows the pointer on top of everything, with an
        // outline where it will land: green if it fits, red if not. A
        // widget over a list goes into the list instead.
        let mut widget_into = None;
        if let Some(drag) = self.drag {
            let into_list = match (drag.item, drag.mode, pointer_screen) {
                (Item::Widget(_), DragMode::Move, Some(p)) => list_drop(p),
                _ => None,
            };
            match into_list {
                Some(target) => {
                    draw_drop(&painter, &target, &to_screen);
                    widget_into = Some(target);
                }
                None => {
                    let target = drag.target(cell, room);
                    let ok = grid::fits(&tab.grid_layout, drag.item, target);
                    draw_landing(&painter, to_screen(&target), ok);
                }
            }
            draw_item(
                &painter,
                &visuals,
                to_screen(&drag.following()),
                &tab.grid_layout,
                drag.item,
                &value_of,
            );
        }

        // A list child being dragged: a preview follows the pointer; it goes
        // into the list under the pointer, or onto free grid.
        let mut child_dropped = None;
        if let Some(cd) = self.child_drag
            && let Some(p) = pointer_screen
            && let Some(child) = tab
                .grid_layout
                .layouts
                .get(cd.list)
                .and_then(|l| l.children.get(cd.child))
        {
            let size = grid::snap_resize(child.rect, cell);
            let corner = p - cd.grab;
            let target = list_drop(p).or_else(|| {
                area.contains(p).then(|| {
                    let (x, y) = to_grid(corner);
                    let rect = grid::snap_move(LayoutRect { x, y, ..size }, cell, room);
                    DropTarget::Grid {
                        rect,
                        free: grid::is_free(&tab.grid_layout, rect),
                    }
                })
            });
            if let Some(target) = &target {
                draw_drop(&painter, target, &to_screen);
            }
            let preview =
                Rect::from_min_size(corner, Vec2::new(size.width as f32, size.height as f32));
            card::draw(
                &painter,
                &visuals,
                preview.shrink(CELL_GAP),
                child,
                value_of(child).as_ref(),
            );
            // Let go: here, or outside the window, where the release may not
            // reach us (it then stays where it was).
            if !ui.input(|i| i.pointer.primary_down()) {
                child_dropped = Some((cd, target));
            }
        }

        // Topics dragged from the NT tree: show the cell under the pointer
        // where the first widget would go, and make the widgets when they're
        // dropped, each in the free spot nearest there. Over a list, they go
        // into the list.
        let pointer = ui.ctx().pointer_hover_pos().filter(|p| area.contains(*p));
        if !self.locked
            && let Some(p) = pointer
            && let Some(topics) = egui::DragAndDrop::payload::<DraggedTopics>(ui.ctx())
        {
            let under = LayoutRect {
                x: f64::from(p.x - origin.x) - cell / 2.0,
                y: f64::from(p.y - origin.y) - cell / 2.0,
                width: cell,
                height: cell,
            };
            let into_list = list_drop(p);
            let target = grid::snap_move(under, cell, room);
            let free = grid::is_free(&tab.grid_layout, target);
            match &into_list {
                Some(drop) => draw_drop(&painter, drop, &to_screen),
                // Several go around the first, so only the first must fit.
                None => draw_landing(&painter, to_screen(&target), free),
            }
            if ui.input(|i| i.pointer.any_released())
                && let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
            {
                // One cell each; in a list only the size matters.
                let one = LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: cell,
                    height: cell,
                };
                let made: Vec<WidgetContainer> = topics
                    .0
                    .iter()
                    .filter_map(|t| widgets::for_topic(&t.name, &t.ty, t.field.as_ref(), one))
                    .collect();
                let grid = &mut tab.grid_layout;
                match into_list {
                    Some(DropTarget::List { list, at, .. }) => {
                        let at = at.min(grid.layouts[list].children.len());
                        let children = &mut grid.layouts[list].children;
                        children.splice(at..at, made);
                        self.selected = Some(Item::List(list));
                    }
                    None => {
                        let placed = grid::place_near(grid, made, (target.x, target.y), cell, room);
                        self.selected = placed.first().map(|&i| Item::Widget(i));
                        if placed.len() < topics.0.len() {
                            self.status = format!(
                                "Added {} of {} widgets; no free space for the rest",
                                placed.len(),
                                topics.0.len()
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(drag) = dropped
            && let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
        {
            let grid = &mut tab.grid_layout;
            match (drag.item, widget_into) {
                (Item::Widget(widget), Some(DropTarget::List { list, at, .. })) => {
                    if grid::widget_into_list(grid, widget, list, at).is_some() {
                        self.selected = Some(Item::List(list));
                    }
                }
                _ => {
                    let target = drag.target(cell, room);
                    if grid::fits(grid, drag.item, target) {
                        drag.item.set_rect(grid, target);
                    }
                }
            }
        }
        if let Some((cd, target)) = child_dropped {
            self.child_drag = None;
            if let Some(tab) = self.layout.tabs.get_mut(self.current_tab) {
                let grid = &mut tab.grid_layout;
                match target {
                    Some(DropTarget::List { list, at, .. }) => {
                        if grid::child_to_list(grid, cd.list, cd.child, list, at).is_some() {
                            self.selected = Some(Item::List(list));
                        }
                    }
                    Some(DropTarget::Grid { rect, free: true }) => {
                        if let Some(w) = grid::child_out_of_list(grid, cd.list, cd.child, rect) {
                            self.selected = Some(Item::Widget(w));
                        }
                    }
                    // Not over the grid, or no room there: it stays put.
                    _ => {}
                }
            }
        }
        if let Some((list, child)) = edit_child
            && let Some(l) = self
                .layout
                .tabs
                .get(self.current_tab)
                .and_then(|t| t.grid_layout.layouts.get(list))
            && let Some(c) = l.children.get(child)
        {
            // Straight to the child's settings; Done goes back to the list's.
            self.selected = Some(Item::List(list));
            self.editing = Some(Editing::List {
                tab: self.current_tab,
                index: list,
                original: Box::new(l.clone()),
                child: Some((child, c.clone())),
            });
        }
        if (new_list || wrap.is_some())
            && let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
        {
            let grid = &mut tab.grid_layout;
            let made = match wrap {
                Some(widget) => grid::wrap_in_list(grid, widget, cell, room),
                None => grid::add_list(grid, self.menu_at, cell, room),
            };
            match made {
                Some(list) => self.selected = Some(Item::List(list)),
                None => self.status = "No free space for a new list".to_owned(),
            }
        }
        if let Some((item, action)) = menu_action {
            self.selected = Some(item);
            self.run_edit_action(ui.ctx(), action);
        }
    }
}

/// One widget's settings in a dialog titled `heading`, with Done, Cancel
/// and Reset settings. Returns `Some(true)` for Done, `Some(false)` for
/// Cancel or Esc, `None` while it stays open. Changes are made to `widget`
/// as they happen; on Cancel the caller puts the original back.
fn widget_window(
    ctx: &egui::Context,
    heading: &str,
    widget: &mut WidgetContainer,
    nt_type: Option<&str>,
    field: Option<&FieldInfo>,
) -> Option<bool> {
    let mut close = None;
    egui::Modal::new(egui::Id::unique("widget_settings")).show(ctx, |ui| {
        ui.heading(heading);
        ui.add_space(4.0);
        settings::show(ui, widget, nt_type, field);
        ui.add_space(4.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Done").clicked() {
                close = Some(true);
            }
            if ui.button("Cancel").on_hover_text("Esc").clicked() {
                close = Some(false);
            }
            ui.separator();
            let custom = settings::has_custom_settings(widget);
            if ui
                .add_enabled(custom, egui::Button::new("Reset settings"))
                .on_hover_text(
                    "Put every setting back to its default. The title is kept; \
                     Cancel still undoes this.",
                )
                .on_disabled_hover_text("All settings are at their defaults")
                .clicked()
            {
                settings::reset_settings(widget);
            }
        });
    });
    // Esc cancels. A click outside does nothing, so a stray click doesn't
    // throw away the changes.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = Some(false);
    }
    close
}

/// Draw one grid item (a widget, or a List Layout with its children) in
/// `rect`, in screen points.
fn draw_item(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    grid: &GridLayout,
    item: Item,
    value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
) {
    let rect = rect.shrink(CELL_GAP);
    match item {
        Item::Widget(i) => {
            let Some(w) = grid.containers.get(i) else {
                return;
            };
            card::draw(painter, visuals, rect, w, value_of(w).as_ref());
        }
        Item::List(i) => {
            let Some(l) = grid.layouts.get(i) else { return };
            list_layout::draw(painter, visuals, rect, l, value_of);
        }
    }
}

/// The outline where a dragged widget or topic will land: green if it fits
/// there, red if it would cover another widget.
fn draw_landing(painter: &egui::Painter, rect: Rect, ok: bool) {
    let color = if ok { DROP_OK } else { DROP_BLOCKED };
    painter.rect_stroke(
        rect.shrink(LANDING_INSET),
        egui::CornerRadius::same(card::CORNER_RADIUS),
        Stroke::new(OUTLINE_WIDTH, color),
        egui::StrokeKind::Inside,
    );
}

/// Show where a drop goes: for a list, the list outlined in green with a
/// line where the new child goes; for the grid, the cells outlined green if
/// free, red if not.
fn draw_drop(
    painter: &egui::Painter,
    target: &DropTarget,
    to_screen: &dyn Fn(&LayoutRect) -> Rect,
) {
    match *target {
        DropTarget::List { card, line_y, .. } => {
            draw_landing(painter, card, true);
            let body = list_layout::body_rect(card);
            painter.hline(body.x_range(), line_y, Stroke::new(OUTLINE_WIDTH, DROP_OK));
        }
        DropTarget::Grid { rect, free } => draw_landing(painter, to_screen(&rect), free),
    }
}

/// Two short diagonal lines in a widget's bottom-right corner, showing
/// where to drag to resize it.
fn draw_resize_handle(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect) {
    let corner = rect.shrink(CELL_GAP).max - Vec2::splat(RESIZE_MARK_INSET);
    let stroke = Stroke::new(RESIZE_MARK_WIDTH, visuals.weak_text_color());
    for size in RESIZE_MARKS {
        painter.line_segment(
            [corner - Vec2::new(size, 0.0), corner - Vec2::new(0.0, size)],
            stroke,
        );
    }
}

/// Show the connection state as a colored dot and a line of text.
fn status_label(ui: &mut egui::Ui, status: &ConnectionStatus) {
    let (color, text) = match status {
        ConnectionStatus::NoTarget => (Color32::GRAY, "Waiting for Driver Station".to_owned()),
        ConnectionStatus::Connecting { host, port } => (
            Color32::from_rgb(230, 160, 40),
            format!("Connecting to {host}:{port}..."),
        ),
        ConnectionStatus::Connected { host, port, topics } => (
            Color32::from_rgb(60, 180, 90),
            format!("Connected to {host}:{port}, {topics} topics"),
        ),
    };
    ui.label(egui::RichText::new("●").color(color));
    ui.label(text);
}

impl eframe::App for DashboardApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
        // "Add to grid" from the tree's right-click menu: widgets in the
        // free spots nearest the top left of the open tab.
        let add = std::mem::take(&mut self.tree_sel.add);
        if !add.is_empty() && !self.locked {
            self.add_topics(&add, self.tree_sel.add_to);
        }
        let now = Instant::now();
        egui::CentralPanel::default().show(ui, |ui| self.grid(ui, &values, now));
        let next_value = self.sampler.finish_frame(now);
        // Whatever changed the layout this frame can be undone. Changes in
        // the settings dialog are recorded once, when it closes.
        if self.editing.is_none() {
            self.history.record(&self.layout);
        }

        self.nt.watch_values(tree_topics);
        self.nt.watch_widget_values(self.tab_topics());

        // The client runs on its own thread, so egui doesn't know when values
        // change. Redraw when the next widget is due for a new value, and at
        // least a few times a second so connection changes and first values
        // still show.
        let idle = Duration::from_millis(250);
        let wait = next_value.map_or(idle, |due| due.min(idle));
        ui.ctx().request_repaint_after(wait);
    }
}
