//! The widget grid: drawing widgets and handling clicks, drags and menus on it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use eframe::egui::{self, CursorIcon, Rect, Sense, Stroke, Vec2};
use nt4::Value;
use raptaboard_layout::{GridLayout, Rect as LayoutRect, WidgetContainer};

use crate::edit::EditAction;
use crate::grid::{self, Item};
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
    CELL_GAP, FOUND_COLOR, GRID_LINE_STRENGTH, GRID_LINE_WIDTH, OUTLINE_WIDTH, RESIZE_HANDLE,
    SELECTION_INSET,
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

impl DashboardApp {
    /// The open tab's grid: lines, widgets, and (when unlocked) dragging to
    /// move widgets and their bottom-right corner to resize them. Drops snap
    /// to the grid and are refused if they would cover another widget.
    pub(super) fn grid(&mut self, ui: &mut egui::Ui, values: &HashMap<&str, &Value>, now: Instant) {
        // Clicking empty grid drops the selection.
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
        if response.clicked() {
            self.selected = None;
            self.tree_sel.clear();
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
        let mut find_in_tree = None;
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
                        {
                            find_in_tree = Some(w.clone());
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
                {
                    find_in_tree = Some(w.clone());
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
        let now_us = self.nt.client().map_or(0, |c| c.now_us());
        for &(item, r) in &placed {
            if Some(item) != dragging {
                // Single widgets get a Ui of their own, for those that need
                // one (graphs); lists draw their children with a painter.
                if let Item::Widget(i) = item
                    && let Some(w) = tab.grid_layout.containers.get(i)
                {
                    let rect = to_screen(&r).shrink(CELL_GAP);
                    let value = value_of(w);
                    let live = widgets::Live {
                        value: value.as_ref(),
                        traces: &self.traces,
                        now_us,
                    };
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt((
                        "widget",
                        self.current_tab,
                        i,
                    )));
                    child.set_clip_rect(rect.intersect(painter.clip_rect()));
                    card::show(&mut child, rect, w, &live);
                } else {
                    draw_item(
                        &painter,
                        &visuals,
                        to_screen(&r),
                        &tab.grid_layout,
                        item,
                        &value_of,
                    );
                }
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
            // Over a graph, the topics become more lines on it.
            let at = to_grid(p);
            let onto_graph = tab
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
                    card: to_screen(&w.rect).shrink(CELL_GAP),
                });
            let into_list = onto_graph.or_else(|| list_drop(p));
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
                            0 => "Nothing new to add to that graph (numbers and booleans only)"
                                .to_owned(),
                            1 => format!("Added a line to \"{}\"", graph.title),
                            n => format!("Added {n} lines to \"{}\"", graph.title),
                        };
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
        if let Some(widget) = find_in_tree {
            self.find_in_tree(&widget);
        }
        if let Some((item, action)) = menu_action {
            self.selected = Some(item);
            self.run_edit_action(ui.ctx(), action);
        }
    }
}
