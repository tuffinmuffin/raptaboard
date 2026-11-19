//! The widget grid: drawing widgets and handling clicks, drags and menus on it.
//!
//! [`DashboardApp::grid`] runs once a frame in steps: input on empty grid,
//! input on each widget, drawing the widgets, drawing outlines and what's
//! being dragged, then [`DashboardApp::apply`] makes the changes asked for.
//! Steps that only look record what they want in [`Requests`], so the layout
//! changes in one place, after everything is drawn.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{self, CursorIcon, Pos2, Rect, Sense, Stroke, Vec2};
use nt4::Value;
use raptaboard_layout::{GridLayout, Rect as LayoutRect, Tab, WidgetContainer};

use crate::edit::EditAction;
use crate::grid::{self, Item};
use crate::nt::WriteError;
use crate::sampling::Sampler;
use crate::struct_view;
use crate::tree::DraggedTopics;
use crate::widgets::builtin::graph;
use crate::widgets::shared::{card, format, props};
use crate::widgets::{self, list_layout};

use super::DashboardApp;
use super::dialogs::Editing;
use super::drag::{
    ChildDrag, Drag, DragMode, DropTarget, draw_drop, draw_landing, draw_resize_handle,
};
use super::style::{
    CELL_GAP, FAILED_COLOR, FLASH_S, FLASH_STEP_S, FLASH_WIDTH, FOUND_COLOR, GRID_LINE_STRENGTH,
    GRID_LINE_WIDTH, OUTLINE_WIDTH, RESIZE_HANDLE, SELECTION_INSET,
};

/// Draw one grid item (a widget, or a List Layout with its children) in
/// `rect`, in screen points.
pub(super) fn draw_item(
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

/// Where the grid is on screen this frame, and its cell size, for turning
/// grid pixels into screen points and back.
#[derive(Debug, Clone, Copy)]
struct GridView {
    /// The grid's area on screen, in points.
    area: Rect,
    /// Grid cell size, in grid pixels.
    cell: f64,
    /// The visible grid's width and height, in grid pixels.
    room: (f64, f64),
}

impl GridView {
    /// A rect in grid pixels, on screen.
    fn to_screen(self, r: &LayoutRect) -> Rect {
        Rect::from_min_size(
            self.area.min + Vec2::new(r.x as f32, r.y as f32),
            Vec2::new(r.width as f32, r.height as f32),
        )
    }

    /// A screen point, in grid pixels from the grid's top left.
    fn to_grid(self, p: Pos2) -> (f64, f64) {
        (
            f64::from(p.x - self.area.min.x),
            f64::from(p.y - self.area.min.y),
        )
    }
}

/// A widget that sends a value: one on the grid (its index in the tab's
/// `containers`), or a List Layout's child.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Sender {
    Widget(usize),
    Child { list: usize, child: usize },
}

/// What the grid's input asked for this frame, done by
/// [`DashboardApp::apply`] once everything is drawn.
#[derive(Default)]
struct Requests {
    /// "New List Layout" from the empty grid's menu.
    new_list: bool,
    /// A widget or list let go after a drag.
    dropped: Option<Drag>,
    /// A list child let go, and where (if anywhere it can go).
    child_dropped: Option<(ChildDrag, Option<DropTarget>)>,
    /// Topics let go after dragging from the tree, and where: a list or
    /// graph, or else the grid cell under the pointer.
    topics_dropped: Option<(Arc<DraggedTopics>, Option<DropTarget>, LayoutRect)>,
    /// An action from a widget's menu, or a double-click (Edit).
    menu_action: Option<(Item, EditAction)>,
    /// A list child double-clicked: (list, child), to edit.
    edit_child: Option<(usize, usize)>,
    /// "Put in a new list" from a widget's menu.
    wrap: Option<usize>,
    /// A widget clicked, to find its topic in the tree.
    find_in_tree: Option<WidgetContainer>,
    /// Values widgets want sent, and which widget sent each.
    writes: Vec<(Sender, Value)>,
}

impl DashboardApp {
    /// The open tab's grid: lines, widgets, and (when unlocked) dragging to
    /// move widgets and their bottom-right corner to resize them. Drops snap
    /// to the grid and are refused if they would cover another widget.
    pub(super) fn grid(&mut self, ui: &mut egui::Ui, values: &HashMap<&str, &Value>, now: Instant) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        let area = response.rect;
        let view = GridView {
            area,
            cell: f64::from(self.layout.grid_size()),
            room: (f64::from(area.width()), f64::from(area.height())),
        };
        self.grid_area = area;
        let mut requests = Requests::default();
        self.empty_grid_input(&response, view, &mut requests);
        draw_lines(&painter, ui.visuals(), view);
        if self.tab().is_none() {
            return;
        }
        self.grid_room = view.room;
        if !self.locked {
            self.item_input(ui, view, &mut requests);
        }

        // The drawing steps only look at the app, so lend them the sampler.
        let sampler = RefCell::new(std::mem::take(&mut self.sampler));
        {
            let value_of = self.value_of(&sampler, values, now);
            requests.writes = self.draw_items(ui, &painter, view, &value_of);
            self.draw_outlines(&painter, ui.visuals(), view);
            self.drag_preview(&painter, ui.visuals(), view, &value_of);
            self.child_drag_preview(ui, &painter, view, &value_of, &mut requests);
        }
        self.sampler = sampler.into_inner();
        self.tree_drop(ui, &painter, view, &mut requests);

        self.apply(ui, view, requests);
    }

    /// The open tab, if there is one.
    fn tab(&self) -> Option<&Tab> {
        self.layout.tabs.get(self.current_tab)
    }

    /// Clicks on empty grid: a click drops the selection, and (unlocked) a
    /// right-click opens a menu to make a new list there.
    fn empty_grid_input(
        &mut self,
        response: &egui::Response,
        view: GridView,
        requests: &mut Requests,
    ) {
        if response.clicked() {
            self.selected = None;
            self.tree_sel.clear();
        }
        if self.locked {
            return;
        }
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            self.menu_at = view.to_grid(p);
            self.selected = None;
        }
        response.context_menu(|ui| {
            if ui
                .button("New List Layout")
                .on_hover_text("An empty list here, to drag widgets into")
                .clicked()
            {
                requests.new_list = true;
            }
        });
    }

    /// Input on each widget and list (unlocked only): selecting, dragging
    /// to move, dragging the corner to resize, dragging list children,
    /// double-clicking to edit, and the right-click menu. Each item's corner
    /// is registered after its body so the corner wins where they overlap.
    fn item_input(&mut self, ui: &mut egui::Ui, view: GridView, requests: &mut Requests) {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return;
        };
        for (item, r) in grid::items(&tab.grid_layout) {
            let rect = view.to_screen(&r);
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
                    {
                        requests.find_in_tree = Some(w.clone());
                    }
                    if child.double_clicked() {
                        requests.edit_child = Some((index, row.index));
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
                        requests.dropped = self.drag.take();
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
            {
                requests.find_in_tree = Some(w.clone());
            }
            if body.double_clicked() {
                requests.menu_action = Some((item, EditAction::Edit));
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
                        requests.menu_action = Some((item, action));
                    }
                }
                if let Item::Widget(widget) = item
                    && ui
                        .button("Put in a new list")
                        .on_hover_text("Make a List Layout here holding this widget")
                        .clicked()
                {
                    requests.wrap = Some(widget);
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

    /// A widget's value: its topic's, taken once per the widget's period,
    /// or one field of it for widgets bound to a struct field.
    fn value_of<'a>(
        &'a self,
        sampler: &'a RefCell<Sampler>,
        values: &'a HashMap<&str, &Value>,
        now: Instant,
    ) -> impl Fn(&WidgetContainer) -> Option<Value> + 'a {
        let structs = self.nt.structs();
        move |w: &WidgetContainer| -> Option<Value> {
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
        }
    }

    /// Draw every item but the one being dragged (drawn on top later),
    /// with resize corners when unlocked. Returns the values widgets (and
    /// list children) want sent.
    fn draw_items(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        view: GridView,
        value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
    ) -> Vec<(Sender, Value)> {
        let mut writes = Vec::new();
        let Some(tab) = self.tab() else {
            return writes;
        };
        let visuals = ui.visuals().clone();
        let dragging = self.drag.map(|d| d.item);
        let now_us = self.nt.client().map_or(0, |c| c.now_us());
        for (item, r) in grid::items(&tab.grid_layout) {
            if Some(item) == dragging {
                continue;
            }
            // Single widgets get a Ui of their own, for those that need
            // one (graphs, toggles), and so does each list child.
            if let Item::Widget(i) = item
                && let Some(w) = tab.grid_layout.containers.get(i)
            {
                let rect = view.to_screen(&r).shrink(CELL_GAP);
                let value = value_of(w);
                let live = widgets::Live {
                    value: value.as_ref(),
                    traces: &self.traces,
                    now_us,
                    locked: self.locked,
                };
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt((
                    "widget",
                    self.current_tab,
                    i,
                )));
                child.set_clip_rect(rect.intersect(painter.clip_rect()));
                if let Some(value) = card::show(&mut child, rect, w, &live) {
                    writes.push((Sender::Widget(i), value));
                }
            } else if let Item::List(i) = item
                && let Some(l) = tab.grid_layout.layouts.get(i)
            {
                let rect = view.to_screen(&r).shrink(CELL_GAP);
                let live = widgets::Live {
                    value: None,
                    traces: &self.traces,
                    now_us,
                    locked: self.locked,
                };
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt((
                    "list",
                    self.current_tab,
                    i,
                )));
                child.set_clip_rect(rect.intersect(painter.clip_rect()));
                for (c, value) in list_layout::show(&mut child, rect, l, value_of, &live) {
                    writes.push((Sender::Child { list: i, child: c }, value));
                }
            }
            if !self.locked {
                draw_resize_handle(painter, &visuals, view.to_screen(&r));
            }
        }
        writes
    }

    /// Outlines: the selected item in the selection color, and widgets
    /// showing the topics picked in the tree, so it's easy to see where a
    /// value is already shown.
    fn draw_outlines(&self, painter: &egui::Painter, visuals: &egui::Visuals, view: GridView) {
        let Some(tab) = self.tab() else { return };
        let dragging = self.drag.map(|d| d.item);
        if let Some(item) = self.selected
            && Some(item) != dragging
            && let Some(r) = item.rect(&tab.grid_layout)
        {
            painter.rect_stroke(
                view.to_screen(&r).shrink(SELECTION_INSET),
                egui::CornerRadius::same(card::CORNER_RADIUS),
                Stroke::new(OUTLINE_WIDTH, visuals.selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }
        // Widgets whose value just failed to send flash red a few times.
        let now = Instant::now();
        for (&(t, sender), &when) in &self.write_failed {
            let age = now.duration_since(when).as_secs_f32();
            if t != self.current_tab || age >= FLASH_S {
                continue;
            }
            // The widget's card, or the list child's row.
            let (outline, radius) = match sender {
                Sender::Widget(i) => {
                    let Some(w) = tab.grid_layout.containers.get(i) else {
                        continue;
                    };
                    (
                        view.to_screen(&w.rect).shrink(SELECTION_INSET),
                        card::CORNER_RADIUS,
                    )
                }
                Sender::Child { list, child } => {
                    let Some(l) = tab.grid_layout.layouts.get(list) else {
                        continue;
                    };
                    let card = view.to_screen(&l.rect).shrink(CELL_GAP);
                    let Some(row) = list_layout::rows_on_screen(card, l)
                        .into_iter()
                        .find(|row| row.index == child)
                    else {
                        continue;
                    };
                    (
                        row.row.expand(2.0).intersect(card),
                        card::BODY_CORNER_RADIUS,
                    )
                }
            };
            if ((age / FLASH_STEP_S) as u32).is_multiple_of(2) {
                painter.rect_stroke(
                    outline,
                    egui::CornerRadius::same(radius),
                    Stroke::new(FLASH_WIDTH, FAILED_COLOR),
                    egui::StrokeKind::Inside,
                );
            }
            painter
                .ctx()
                .request_repaint_after(std::time::Duration::from_millis(30));
        }
        let picked = self.tree_sel.selected();
        if picked.is_empty() {
            return;
        }
        let shows = |w: &WidgetContainer| picked.iter().any(|t| t.is_shown_by(w));
        let stroke = Stroke::new(OUTLINE_WIDTH, FOUND_COLOR);
        for w in tab.grid_layout.containers.iter().filter(|w| shows(w)) {
            painter.rect_stroke(
                view.to_screen(&w.rect).shrink(SELECTION_INSET),
                egui::CornerRadius::same(card::CORNER_RADIUS),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        for l in &tab.grid_layout.layouts {
            let card = view.to_screen(&l.rect).shrink(CELL_GAP);
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

    /// The list under screen point `p` as a drop target, with where in it a
    /// drop would go, if there's a list there.
    fn list_drop(&self, view: GridView, p: Pos2) -> Option<DropTarget> {
        if !view.area.contains(p) {
            return None;
        }
        let tab = self.tab()?;
        let list = grid::list_at(&tab.grid_layout, view.to_grid(p))?;
        let l = &tab.grid_layout.layouts[list];
        let card = view.to_screen(&l.rect).shrink(CELL_GAP);
        let rows = list_layout::rows_on_screen(card, l);
        let at = list_layout::insert_index(&rows, l.children.len(), p.y);
        let line_y = list_layout::insert_line_y(&rows, at, list_layout::body_rect(card));
        Some(DropTarget::List {
            list,
            at,
            card,
            line_y,
        })
    }

    /// The graph under screen point `p` as a drop target, if there is one.
    fn graph_drop(&self, view: GridView, p: Pos2) -> Option<DropTarget> {
        let at = view.to_grid(p);
        self.tab()?
            .grid_layout
            .containers
            .iter()
            .enumerate()
            .find(|(_, w)| {
                let r = w.rect;
                w.widget_type == "Graph"
                    && at.0 >= r.x
                    && at.0 < r.x + r.width
                    && at.1 >= r.y
                    && at.1 < r.y + r.height
            })
            .map(|(index, w)| DropTarget::Graph {
                index,
                card: view.to_screen(&w.rect).shrink(CELL_GAP),
            })
    }

    /// The list a widget being moved would go into, with the pointer at
    /// `pointer`: the list under the pointer, if any. Resizing, and lists
    /// being dragged, never go into a list.
    fn drag_into_list(
        &self,
        drag: Drag,
        view: GridView,
        pointer: Option<Pos2>,
    ) -> Option<DropTarget> {
        match (drag.item, drag.mode, pointer) {
            (Item::Widget(_), DragMode::Move, Some(p)) => self.list_drop(view, p),
            _ => None,
        }
    }

    /// The dragged item follows the pointer on top of everything, with an
    /// outline where it will land: green if it fits, red if not. A widget
    /// over a list goes into the list instead.
    fn drag_preview(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        view: GridView,
        value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
    ) {
        let (Some(drag), Some(tab)) = (self.drag, self.tab()) else {
            return;
        };
        let to_screen = |r: &LayoutRect| view.to_screen(r);
        match self.drag_into_list(drag, view, painter.ctx().pointer_latest_pos()) {
            Some(target) => draw_drop(painter, &target, &to_screen),
            None => {
                let target = drag.target(view.cell, view.room);
                let ok = grid::fits(&tab.grid_layout, drag.item, target);
                draw_landing(painter, view.to_screen(&target), ok);
            }
        }
        draw_item(
            painter,
            visuals,
            view.to_screen(&drag.following()),
            &tab.grid_layout,
            drag.item,
            value_of,
        );
    }

    /// A list child being dragged: a preview follows the pointer; it goes
    /// into the list under the pointer, or onto free grid.
    fn child_drag_preview(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        view: GridView,
        value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
        requests: &mut Requests,
    ) {
        let Some(cd) = self.child_drag else { return };
        let Some(p) = ui.ctx().pointer_latest_pos() else {
            return;
        };
        let Some(tab) = self.tab() else { return };
        let Some(child) = tab
            .grid_layout
            .layouts
            .get(cd.list)
            .and_then(|l| l.children.get(cd.child))
        else {
            return;
        };
        let size = grid::snap_resize(child.rect, view.cell);
        let corner = p - cd.grab;
        let target = self.list_drop(view, p).or_else(|| {
            view.area.contains(p).then(|| {
                let (x, y) = view.to_grid(corner);
                let rect = grid::snap_move(LayoutRect { x, y, ..size }, view.cell, view.room);
                DropTarget::Grid {
                    rect,
                    free: grid::is_free(&tab.grid_layout, rect),
                }
            })
        });
        if let Some(target) = &target {
            draw_drop(painter, target, &|r: &LayoutRect| view.to_screen(r));
        }
        let preview = Rect::from_min_size(corner, Vec2::new(size.width as f32, size.height as f32));
        card::draw(
            painter,
            ui.visuals(),
            preview.shrink(CELL_GAP),
            child,
            value_of(child).as_ref(),
        );
        // Let go: here, or outside the window, where the release may not
        // reach us (it then stays where it was).
        if !ui.input(|i| i.pointer.primary_down()) {
            requests.child_dropped = Some((cd, target));
        }
    }

    /// Topics dragged from the NT tree: show the cell under the pointer
    /// where the first widget would go, or the list or graph they'd go
    /// into, and ask for the widgets when they're let go.
    fn tree_drop(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        view: GridView,
        requests: &mut Requests,
    ) {
        if self.locked || self.tab().is_none() {
            return;
        }
        let Some(p) = ui
            .ctx()
            .pointer_hover_pos()
            .filter(|p| view.area.contains(*p))
        else {
            return;
        };
        let Some(topics) = egui::DragAndDrop::payload::<DraggedTopics>(ui.ctx()) else {
            return;
        };
        let (x, y) = view.to_grid(p);
        let under = LayoutRect {
            x: x - view.cell / 2.0,
            y: y - view.cell / 2.0,
            width: view.cell,
            height: view.cell,
        };
        // Over a graph, the topics become more lines on it.
        let into = self.graph_drop(view, p).or_else(|| self.list_drop(view, p));
        let target = grid::snap_move(under, view.cell, view.room);
        match &into {
            Some(drop) => draw_drop(painter, drop, &|r: &LayoutRect| view.to_screen(r)),
            // Several go around the first, so only the first must fit.
            None => {
                let free = self
                    .tab()
                    .is_some_and(|t| grid::is_free(&t.grid_layout, target));
                draw_landing(painter, view.to_screen(&target), free);
            }
        }
        if ui.input(|i| i.pointer.any_released()) {
            requests.topics_dropped = Some((topics, into, target));
        }
    }

    /// Make the changes the grid's input asked for this frame.
    fn apply(&mut self, ui: &egui::Ui, view: GridView, requests: Requests) {
        let Requests {
            new_list,
            dropped,
            child_dropped,
            topics_dropped,
            menu_action,
            edit_child,
            wrap,
            find_in_tree,
            writes,
        } = requests;
        for (sender, value) in writes {
            self.send(sender, value);
        }
        if let Some((topics, into, target)) = topics_dropped {
            self.add_dropped_topics(&topics, into, target, view);
        }
        if let Some(drag) = dropped {
            // Worked out here, where it's let go: the preview stops
            // drawing on the frame the drag ends, so it can't say.
            let into = self.drag_into_list(drag, view, ui.ctx().pointer_latest_pos());
            self.drop_dragged(drag, into, view);
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
                Some(widget) => grid::wrap_in_list(grid, widget, view.cell, view.room),
                None => grid::add_list(grid, self.menu_at, view.cell, view.room),
            };
            match made {
                Some(list) => self.selected = Some(Item::List(list)),
                None => self.status = "No free space for a new list".to_owned(),
            }
        }
        if let Some(widget) = find_in_tree {
            self.find_in_tree(&widget);
        }
        if let Some((item, action)) = menu_action {
            self.selected = Some(item);
            self.run_edit_action(ui.ctx(), action);
        }
    }

    /// Put a widget or list let go after a drag `into` a list, or where it
    /// was dragged to on the grid if it fits there (else it goes back).
    fn drop_dragged(&mut self, drag: Drag, into: Option<DropTarget>, view: GridView) {
        let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
            return;
        };
        let grid = &mut tab.grid_layout;
        match (drag.item, into) {
            (Item::Widget(widget), Some(DropTarget::List { list, at, .. })) => {
                if grid::widget_into_list(grid, widget, list, at).is_some() {
                    self.selected = Some(Item::List(list));
                }
            }
            _ => {
                let target = drag.target(view.cell, view.room);
                if grid::fits(grid, drag.item, target) {
                    drag.item.set_rect(grid, target);
                }
            }
        }
    }

    /// Send `value` from widget `index` on the open tab to its topic. If it
    /// can't be sent, say why in the status line, flash the widget's border
    /// red and play the alert tone.
    fn send(&mut self, sender: Sender, value: Value) {
        let grid = self.tab().map(|t| &t.grid_layout);
        let widget = match sender {
            Sender::Widget(i) => grid.and_then(|g| g.containers.get(i)),
            Sender::Child { list, child } => grid
                .and_then(|g| g.layouts.get(list))
                .and_then(|l| l.children.get(child)),
        };
        let Some(widget) = widget else {
            return;
        };
        // Copied out, so the widget's borrow of the layout ends before the
        // connection is used.
        let topic = widget.topic().map(str::to_owned);
        let is_field = widget.struct_meta().is_some();
        let result = match (topic, is_field) {
            (Some(topic), false) => self.nt.write(&topic, value),
            (Some(topic), true) => Err(WriteError::Struct(topic)),
            (None, _) => Err(WriteError::NotPublished("(no topic)".to_owned())),
        };
        match result {
            Ok(()) => {
                self.write_failed.remove(&(self.current_tab, sender));
                // A failure's message is out of date once a send works
                // (unless something else has said something since).
                if self.send_failure.take().is_some_and(|m| m == self.status) {
                    self.status.clear();
                }
            }
            Err(why) => {
                self.status = why.to_string();
                self.send_failure = Some(self.status.clone());
                self.write_failed
                    .insert((self.current_tab, sender), Instant::now());
                if self.alert_sound {
                    self.alert.play();
                }
            }
        }
    }

    /// Make widgets for topics dropped from the tree: into the list, as
    /// lines on the graph, or on the grid, each in the free spot nearest
    /// `target`.
    fn add_dropped_topics(
        &mut self,
        topics: &DraggedTopics,
        into: Option<DropTarget>,
        target: LayoutRect,
        view: GridView,
    ) {
        let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
            return;
        };
        let cell = view.cell;
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
        match into {
            Some(DropTarget::List { list, at, .. }) => {
                let at = at.min(grid.layouts[list].children.len());
                let children = &mut grid.layouts[list].children;
                children.splice(at..at, made);
                self.selected = Some(Item::List(list));
            }
            Some(DropTarget::Graph { index, .. }) => {
                let graph = &mut grid.containers[index];
                let plottable = widgets::find("Graph").expect("graphs are built");
                let added = topics
                    .0
                    .iter()
                    .filter(|t| plottable.shows(&t.ty))
                    .filter(|t| graph::add_series(graph, &t.name, &t.ty, t.field.as_ref()))
                    .count();
                self.selected = Some(Item::Widget(index));
                self.status = match added {
                    0 => "Nothing new to add to that graph (numbers and booleans only)".to_owned(),
                    1 => format!("Added a line to \"{}\"", graph.title),
                    n => format!("Added {n} lines to \"{}\"", graph.title),
                };
            }
            None => {
                let placed = grid::place_near(grid, made, (target.x, target.y), cell, view.room);
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

/// The grid lines, one every cell across and down the grid's area.
fn draw_lines(painter: &egui::Painter, visuals: &egui::Visuals, view: GridView) {
    let line = Stroke::new(
        GRID_LINE_WIDTH,
        visuals.weak_text_color().gamma_multiply(GRID_LINE_STRENGTH),
    );
    let (area, step) = (view.area, view.cell as f32);
    let mut x = 0.0;
    while x <= area.width() {
        painter.vline(area.min.x + x, area.y_range(), line);
        x += step;
    }
    let mut y = 0.0;
    while y <= area.height() {
        painter.hline(area.x_range(), area.min.y + y, line);
        y += step;
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::cli::StartupOptions;
    use eframe::egui::{Event, PointerButton, RawInput};
    use raptaboard_layout::ElasticLayout;

    /// A 128-pixel grid with a Text Display at (0, 0), another at (384, 0),
    /// and an empty List Layout at (0, 256), all one cell (the list two
    /// tall).
    const LAYOUT: &str = r#"{
        "version": 1.0, "grid_size": 128,
        "tabs": [{ "name": "Test", "grid_layout": {
            "layouts": [{ "title": "List", "x": 0, "y": 256, "width": 128, "height": 256,
                          "type": "List Layout", "properties": {}, "children": [] }],
            "containers": [
                { "title": "A", "x": 0, "y": 0, "width": 128, "height": 128,
                  "type": "Text Display", "properties": { "topic": "/A" } },
                { "title": "B", "x": 384, "y": 0, "width": 128, "height": 128,
                  "type": "Text Display", "properties": { "topic": "/B" } }
            ]
        }}]
    }"#;

    /// The app with [`LAYOUT`] open and unlocked, and nowhere to save
    /// settings.
    fn app() -> DashboardApp {
        let mut app = DashboardApp::new(StartupOptions::default()).expect("app starts");
        app.layout = ElasticLayout::from_json(LAYOUT).expect("test layout parses");
        app.current_tab = 0;
        app.locked = false;
        app
    }

    /// One frame of just the grid, with `events`; returns the grid's top
    /// left on screen.
    fn frame(ctx: &egui::Context, app: &mut DashboardApp, events: Vec<Event>) -> Pos2 {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut origin = Pos2::ZERO;
        let mut output = ctx.run_ui(raw, |ui| {
            origin = ui.available_rect_before_wrap().min;
            app.grid(ui, &HashMap::new(), Instant::now());
        });
        output.textures_delta.clear();
        origin
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// Press at `from` (grid pixels), move to `to` in steps, and let go.
    fn drag(ctx: &egui::Context, app: &mut DashboardApp, from: (f32, f32), to: (f32, f32)) {
        let origin = frame(ctx, app, vec![]);
        let start = origin + Vec2::new(from.0, from.1);
        let end = origin + Vec2::new(to.0, to.1);
        frame(ctx, app, vec![Event::PointerMoved(start)]);
        frame(ctx, app, vec![button(start, true)]);
        for step in 1..=8 {
            let at = start + (end - start) * (step as f32 / 8.0);
            frame(ctx, app, vec![Event::PointerMoved(at)]);
        }
        frame(ctx, app, vec![button(end, false)]);
        frame(ctx, app, vec![]);
    }

    fn click(ctx: &egui::Context, app: &mut DashboardApp, at: (f32, f32)) {
        let origin = frame(ctx, app, vec![]);
        let p = origin + Vec2::new(at.0, at.1);
        frame(ctx, app, vec![Event::PointerMoved(p)]);
        frame(ctx, app, vec![button(p, true)]);
        frame(ctx, app, vec![button(p, false)]);
        frame(ctx, app, vec![]);
    }

    fn rect_of(app: &DashboardApp, widget: usize) -> (f64, f64, f64, f64) {
        let r = app.layout.tabs[0].grid_layout.containers[widget].rect;
        (r.x, r.y, r.width, r.height)
    }

    #[test]
    fn dragging_a_widget_moves_it_to_the_nearest_cell() {
        let ctx = egui::Context::default();
        let mut app = app();
        // Two cells right and a bit: snaps to x = 256.
        drag(&ctx, &mut app, (40.0, 40.0), (40.0 + 270.0, 40.0));
        assert_eq!(rect_of(&app, 0), (256.0, 0.0, 128.0, 128.0));
        assert_eq!(app.selected, Some(Item::Widget(0)));
        assert!(app.drag.is_none());
    }

    #[test]
    fn dragging_the_corner_resizes() {
        let ctx = egui::Context::default();
        let mut app = app();
        // The resize corner is the bottom right few points of the widget.
        drag(&ctx, &mut app, (124.0, 124.0), (124.0 + 128.0, 124.0));
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 256.0, 128.0));
    }

    #[test]
    fn a_drop_onto_another_widget_is_refused() {
        let ctx = egui::Context::default();
        let mut app = app();
        // A onto B: it stays where it was.
        drag(&ctx, &mut app, (40.0, 40.0), (40.0 + 384.0, 40.0));
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 128.0, 128.0));
    }

    #[test]
    fn a_widget_dropped_on_a_list_goes_into_it() {
        let ctx = egui::Context::default();
        let mut app = app();
        drag(&ctx, &mut app, (40.0, 40.0), (40.0, 256.0 + 100.0));
        let grid = &app.layout.tabs[0].grid_layout;
        assert_eq!(grid.layouts[0].children.len(), 1);
        assert_eq!(grid.layouts[0].children[0].title, "A");
        assert_eq!(grid.containers.len(), 1);
        assert_eq!(app.selected, Some(Item::List(0)));
    }

    #[test]
    fn clicks_select_widgets_and_empty_grid_clears_the_selection() {
        let ctx = egui::Context::default();
        let mut app = app();
        click(&ctx, &mut app, (420.0, 60.0));
        assert_eq!(app.selected, Some(Item::Widget(1)));
        click(&ctx, &mut app, (650.0, 400.0));
        assert_eq!(app.selected, None);
    }

    #[test]
    fn nothing_moves_while_locked() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.locked = true;
        drag(&ctx, &mut app, (40.0, 40.0), (40.0 + 270.0, 40.0));
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 128.0, 128.0));
    }
}
