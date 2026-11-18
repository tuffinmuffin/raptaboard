//! The main window: settings bar, tab bar and the widget grid.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Color32, CursorIcon, Rect, Sense, Stroke, Vec2};
use nt4::{Topic, Value};
use raptaboard_layout::{ElasticLayout, GridLayout, Rect as LayoutRect};

use crate::cli::StartupOptions;
use crate::connection::{AddressMode, resolve};
use crate::grid::{self, Item};
use crate::layout_file::{self, FileAction, LoadError};
use crate::nt::{ConnectionStatus, NtConnection};
use crate::tabs::{self, TabAction};
use crate::tree::{Folder, TopicEntry};
use crate::widgets::{self, WidgetKind};
use nt4::proto::structs::StructDatabase;

/// Elastic's default widget update period, in seconds.
const DEFAULT_PERIOD_S: f64 = 0.06;
/// Space between List Layout children, in points.
const LIST_GAP: f32 = 6.0;
/// Size of the resize corner at the bottom right of each widget, in points.
const RESIZE_HANDLE: f32 = 14.0;

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
    /// The tab being renamed, the name typed so far, and whether its text
    /// box has been given the keyboard yet.
    renaming: Option<(usize, String, bool)>,
    /// A tab with widgets on it, waiting for the user to confirm closing it.
    confirm_close: Option<usize>,
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
            renaming: None,
            confirm_close: None,
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
    fn show_layout(&mut self, layout: ElasticLayout, path: PathBuf) {
        self.status = format!(
            "Opened {} tabs, {} widgets",
            layout.tabs.len(),
            layout.widgets().count()
        );
        self.saved = layout.clone();
        self.layout = layout;
        self.layout_path = Some(path);
        self.current_tab = 0;
        self.drag = None;
        self.renaming = None;
        self.confirm_close = None;
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
                self.renaming = None;
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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.nt.structs() {
                Some(structs) => tree.show(ui, "", filtering, structs),
                None => tree.show(ui, "", filtering, &StructDatabase::new()),
            })
            .inner
    }

    /// The topics the widgets on the open tab are bound to, and how often
    /// they want updates (the shortest `period` of them, in seconds).
    /// Only widgets raptaboard can draw are counted.
    fn tab_topics(&self) -> (Vec<String>, f64) {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return (Vec::new(), DEFAULT_PERIOD_S);
        };
        let grid = &tab.grid_layout;
        let drawn = grid
            .containers
            .iter()
            .chain(grid.layouts.iter().flat_map(|l| &l.children))
            .filter(|w| WidgetKind::from_type(&w.widget_type).is_some());
        let mut names = Vec::new();
        let mut period = f64::INFINITY;
        for w in drawn {
            if let Some(topic) = w.topic() {
                names.push(topic.to_owned());
                period = period.min(w.period().unwrap_or(DEFAULT_PERIOD_S));
            }
        }
        // A period of 0 or less would ask for every change; Elastic's floor is
        // a few milliseconds, so keep it sane.
        let period = if period.is_finite() {
            period.max(0.01)
        } else {
            DEFAULT_PERIOD_S
        };
        (names, period)
    }

    /// The row of tabs. Click to switch; when unlocked, double-click to
    /// rename, right-click for more, and "+" adds a tab.
    fn tab_bar(&mut self, ui: &mut egui::Ui) -> Option<TabAction> {
        let mut action = None;
        let names: Vec<String> = self.layout.tabs.iter().map(|t| t.name.clone()).collect();
        let count = names.len();
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
                let tab = ui
                    .selectable_label(self.current_tab == i, name)
                    .on_hover_text(tabs::tab_hint(ui.ctx(), i, self.locked));
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
        self.current_tab = tabs::apply(&mut self.layout, self.current_tab, action);
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
    fn grid(&mut self, ui: &mut egui::Ui, values: &HashMap<&str, &Value>) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let area = response.rect;
        let origin = area.min;
        let cell = f64::from(self.layout.grid_size());
        let visuals = ui.visuals().clone();
        let line = Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3));

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
        let placed = grid::items(&tab.grid_layout);

        // Dragging. Each item's corner is registered after its body so the
        // corner wins where they overlap.
        let mut dropped = None;
        if !self.locked {
            for &(item, r) in &placed {
                let rect = to_screen(&r);
                let id = ui.scope_id().with(("grid_item", self.current_tab, item));
                let corner = Rect::from_min_max(rect.max - Vec2::splat(RESIZE_HANDLE), rect.max);
                let body = ui.interact(rect, id, Sense::drag());
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
                if corner.hovered() || corner.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
                } else if body.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                } else if body.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
            }
        }

        let value_of = |topic: Option<&str>| topic.and_then(|t| values.get(t).copied());
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
        // The dragged item follows the pointer on top of everything, with an
        // outline where it will land: green if it fits, red if not.
        if let Some(drag) = self.drag {
            let target = drag.target(cell, room);
            let ok = grid::fits(&tab.grid_layout, drag.item, target);
            let color = if ok {
                Color32::from_rgb(60, 180, 90)
            } else {
                Color32::from_rgb(220, 60, 60)
            };
            painter.rect_stroke(
                to_screen(&target).shrink(2.0),
                egui::CornerRadius::same(10),
                Stroke::new(2.0, color),
                egui::StrokeKind::Inside,
            );
            draw_item(
                &painter,
                &visuals,
                to_screen(&drag.following()),
                &tab.grid_layout,
                drag.item,
                &value_of,
            );
        }

        if let Some(drag) = dropped {
            let target = drag.target(cell, room);
            if let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
                && grid::fits(&tab.grid_layout, drag.item, target)
            {
                drag.item.set_rect(&mut tab.grid_layout, target);
            }
        }
    }
}

/// Draw one grid item (a widget, or a List Layout with its children) in
/// `rect`, in screen points.
fn draw_item<'v>(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    grid: &GridLayout,
    item: Item,
    value_of: &dyn Fn(Option<&str>) -> Option<&'v Value>,
) {
    let rect = rect.shrink(4.0);
    match item {
        Item::Widget(i) => {
            let Some(w) = grid.containers.get(i) else {
                return;
            };
            widgets::draw(painter, visuals, rect, w, value_of(w.topic()));
        }
        // A List Layout stacks its children top to bottom, each at its own
        // height and the layout's full width; children that don't fit are cut off.
        Item::List(i) => {
            let Some(l) = grid.layouts.get(i) else { return };
            let body = widgets::draw_card(painter, visuals, rect, &l.title).shrink(LIST_GAP);
            let inside = painter.with_clip_rect(body.intersect(painter.clip_rect()));
            let mut top = body.min.y;
            for child in &l.children {
                let height = child.rect.height as f32;
                let child_rect = Rect::from_min_size(
                    egui::pos2(body.min.x, top),
                    Vec2::new(body.width(), height),
                );
                widgets::draw(&inside, visuals, child_rect, child, value_of(child.topic()));
                top += height + LIST_GAP;
            }
        }
    }
}

/// Two short diagonal lines in a widget's bottom-right corner, showing
/// where to drag to resize it.
fn draw_resize_handle(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect) {
    let corner = rect.shrink(4.0).max - Vec2::splat(4.0);
    let stroke = Stroke::new(1.5, visuals.weak_text_color());
    for size in [5.0, 9.0] {
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
        let dialog_open = self.confirm_close.is_some() || self.unsaved_prompt.is_some();
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
        // Closing the window with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && self.unsaved() && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.unsaved_prompt = Some(AfterSaving::Quit);
        }
        self.close_dialog(&ctx);
        self.unsaved_dialog(&ctx);
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
        egui::CentralPanel::default().show(ui, |ui| self.grid(ui, &values));

        self.nt.watch_values(tree_topics);
        let (widget_topics, period_s) = self.tab_topics();
        let has_widgets = !widget_topics.is_empty();
        self.nt.watch_widget_values(widget_topics, period_s);

        // The client runs on its own thread, so egui doesn't know when values
        // change. Redraw at the widgets' period while widgets are showing, and
        // a few times a second otherwise so connection changes still show.
        let connected = matches!(self.nt.status(), ConnectionStatus::Connected { .. });
        let wait = if has_widgets && connected {
            Duration::from_secs_f64(period_s)
        } else {
            Duration::from_millis(250)
        };
        ui.ctx().request_repaint_after(wait);
    }
}
