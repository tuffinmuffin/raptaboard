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

use crate::binding::{self, WidgetValues, WriteTarget};
use crate::edit::EditAction;
use crate::grid::{self, Item};
use crate::traces::SeriesKey;
use crate::tree::DraggedTopics;
use crate::widgets::shared::card;
use crate::widgets::{self, group, list_layout};

use super::DashboardApp;
use super::dialogs::Editing;
use super::drag::{ChildDrag, Drag, DragMode, DropTarget, draw_resize_handle};
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
            if group::is_group(l) {
                group::draw(painter, visuals, rect, l, value_of);
            } else {
                list_layout::draw(painter, visuals, rect, l, value_of);
            }
        }
    }
}

/// Where the grid is on screen this frame, and its cell size, for turning
/// grid pixels into screen points and back.
#[derive(Debug, Clone, Copy)]
pub(super) struct GridView {
    /// The whole grid's area, in screen points, scrolled: its top left is
    /// the grid's origin, and it can reach past what's on screen.
    pub area: Rect,
    /// The part of the grid on screen, in screen points.
    pub visible: Rect,
    /// Grid cell size, in grid pixels.
    pub cell: f64,
    /// How far right and down the grid shows, in grid pixels: drags and
    /// new widgets stay within it.
    pub room: (f64, f64),
}

impl GridView {
    /// A rect in grid pixels, on screen.
    pub(super) fn to_screen(self, r: &LayoutRect) -> Rect {
        Rect::from_min_size(
            self.area.min + Vec2::new(r.x as f32, r.y as f32),
            Vec2::new(r.width as f32, r.height as f32),
        )
    }

    /// A screen point, in grid pixels from the grid's top left.
    pub(super) fn to_grid(self, p: Pos2) -> (f64, f64) {
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
pub(super) struct Requests {
    /// "New List Layout" from the empty grid's menu.
    pub new_list: bool,
    /// "Pack widgets" from the empty grid's menu.
    pub pack: bool,
    /// "New Group" from the empty grid's menu.
    pub new_group: bool,
    /// "Remove from group" from a group child's menu: (group, child).
    pub remove_child: Option<(usize, usize)>,
    /// "Fill space" from a widget's or list's menu.
    pub fill: Option<Item>,
    /// "Pin in place" ticked or unticked in a widget's or list's menu.
    pub pin: Option<(Item, bool)>,
    /// "Bring to front" (`true`) or "Send to back" from a widget's or list's
    /// menu, when it overlaps another.
    pub front_back: Option<(Item, bool)>,
    /// A widget or list let go after a drag.
    pub dropped: Option<Drag>,
    /// A list child let go, and where (if anywhere it can go).
    pub child_dropped: Option<(ChildDrag, Option<DropTarget>)>,
    /// Topics let go after dragging from the tree, and where: a list or
    /// graph, or else the grid cell under the pointer.
    pub topics_dropped: Option<(Arc<DraggedTopics>, Option<DropTarget>, LayoutRect)>,
    /// An action from a widget's menu, or a double-click (Edit).
    pub menu_action: Option<(Item, EditAction)>,
    /// A list child double-clicked: (list, child), to edit.
    pub edit_child: Option<(usize, usize)>,
    /// "Put in a new list" from a widget's menu.
    pub wrap: Option<usize>,
    /// "Put in a new group" from a widget's menu.
    pub wrap_group: Option<usize>,
    /// A widget clicked, to find its topic in the tree.
    pub find_in_tree: Option<WidgetContainer>,
    /// A widget whose click it used itself, to find in the tree without
    /// saying anything if it isn't there.
    pub find_quietly: Option<WidgetContainer>,
    /// Values widgets want sent, and which widget sent each.
    pub writes: Vec<(Sender, Value)>,
    /// Series whose range and statistics to start again: "Reset scale" or
    /// "Reset min/max" from a widget's menu.
    pub reset_ranges: Vec<SeriesKey>,
}

/// `item`'s settings on `grid`, to change.
fn item_props(
    grid: &mut GridLayout,
    item: Item,
) -> Option<&mut serde_json::Map<String, serde_json::Value>> {
    match item {
        Item::Widget(i) => grid.containers.get_mut(i).map(|w| &mut w.properties),
        Item::List(i) => grid.layouts.get_mut(i).map(|l| &mut l.properties),
    }
}

/// A small pin in `card`'s top-left corner: the item is pinned in place.
fn draw_pin(painter: &egui::Painter, visuals: &egui::Visuals, card: Rect) {
    let head = card.min + Vec2::new(PIN_INSET + PIN_HEAD, PIN_INSET + PIN_HEAD);
    let color = visuals.text_color();
    let tip = head + Vec2::new(-PIN_LENGTH, PIN_LENGTH) * std::f32::consts::FRAC_1_SQRT_2;
    painter.line_segment([head, tip], Stroke::new(2.0, color));
    painter.circle_filled(head, PIN_HEAD, color);
}

/// How far the pin sits in from a card's corner, its head's radius and its
/// length, in points.
const PIN_INSET: f32 = 5.0;
const PIN_HEAD: f32 = 4.5;
const PIN_LENGTH: f32 = 10.0;

/// The reset in a widget's right-click menu ("Reset scale", "Reset
/// min/max"), if it has one ([`widgets::WidgetType::resettable`]); clicked,
/// its series go in `reset`.
fn reset_button(ui: &mut egui::Ui, widget: Option<&WidgetContainer>, reset: &mut Vec<SeriesKey>) {
    let Some(w) = widget else { return };
    let Some(r) = widgets::find(&w.widget_type).and_then(|ty| ty.resettable(w)) else {
        return;
    };
    if ui.button(r.label).on_hover_text(r.hover).clicked() {
        reset.extend(r.series);
    }
}

impl DashboardApp {
    /// The open tab's grid: lines, widgets, and (when unlocked) dragging to
    /// move widgets and their bottom-right corner to resize them. Drops snap
    /// to the grid and are refused if they would cover another widget.
    pub(super) fn grid(&mut self, ui: &mut egui::Ui, values: &HashMap<&str, &Value>, now: Instant) {
        self.cancel_drag_on_escape(ui.ctx());
        let tab = self.current_tab;
        let shown = ui.available_size();
        let mut offset = self.grid.scroll.get(&tab).copied().unwrap_or_default();
        // Dragging near an edge scrolls that way, and the grid grows a cell
        // past what shows, so a widget can be dragged beyond the screen.
        let dragging = !self.locked
            && (self.grid.drag.is_some()
                || self.grid.child_drag.is_some()
                || egui::DragAndDrop::has_any_payload(ui.ctx()));
        let push = if dragging {
            edge_push(self.grid.visible, ui.ctx().pointer_hover_pos())
        } else {
            Vec2::ZERO
        };
        let dt = ui.ctx().input(|i| i.stable_dt).min(0.05);
        if push != Vec2::ZERO {
            offset = (offset + push * dt * EDGE_SCROLL_SPEED).max(Vec2::ZERO);
            ui.ctx().request_repaint();
        }
        let cell = self.layout.grid_size() as f32;
        // What showed last frame (the bars take some of `shown`).
        let seen = if self.grid.visible.is_positive() {
            self.grid.visible.size()
        } else {
            shown
        };
        // At least what shows (not counting the bars, or the grid would
        // always be a bar's width wider and scroll a little), and as far as
        // the widgets reach.
        let mut content = seen.max(self.tab().map_or(Vec2::ZERO, |t| extent(&t.grid_layout)));
        if dragging {
            content = content.max(offset + seen + Vec2::splat(cell));
        }
        let mut scroll = egui::ScrollArea::both()
            .id_salt(("grid", tab))
            .auto_shrink([false, false]);
        scroll = if self.locked {
            // Locked for driving: the grid stays where it is, so a stray
            // swipe can't move it, and there are no bars to grab.
            scroll
                .scroll_source(egui::scroll_area::ScrollSource::NONE)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        } else {
            // Dragging moves widgets; only the bars and the wheel scroll.
            scroll.scroll_source(egui::scroll_area::ScrollSource {
                scroll_bar: true,
                mouse_wheel: true,
                ..egui::scroll_area::ScrollSource::NONE
            })
        };
        if push != Vec2::ZERO {
            scroll = scroll.scroll_offset(offset);
        }
        let out = ui
            .scope(|ui| {
                // Solid bars, so a grid that runs off screen shows it.
                ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
                scroll.show(ui, |ui| self.grid_content(ui, values, now, content))
            })
            .inner;
        let now = out.state.clamped_offset();
        // A widget being moved or resized keeps up with the grid scrolling
        // under it (its drag counts pointer moves on screen).
        if let Some(drag) = &mut self.grid.drag {
            drag.moved += now - self.grid.scroll.get(&tab).copied().unwrap_or_default();
        }
        self.grid.scroll.insert(tab, now);
        self.grid.visible = out.inner_rect;
    }

    /// Escape while dragging calls the drag off: a widget or list moved or
    /// resized, a list child, or topics from the tree. Nothing changes, and
    /// letting go afterwards drops nothing. Escape is used up, so it doesn't
    /// also deselect.
    fn cancel_drag_on_escape(&mut self, ctx: &egui::Context) {
        let tree = egui::DragAndDrop::has_payload_of_type::<DraggedTopics>(ctx);
        if self.grid.drag.is_none() && self.grid.child_drag.is_none() && !tree {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.grid.drag = None;
            self.grid.child_drag = None;
            egui::DragAndDrop::clear_payload(ctx);
            self.status = "Drag cancelled".to_owned();
        }
    }

    /// "Pack widgets", "Fill space" and "Pin in place" from the right-click
    /// menus, each one undo step, said in the status line.
    fn arrange(
        &mut self,
        pack: bool,
        fill: Option<Item>,
        pin: Option<(Item, bool)>,
        view: GridView,
    ) {
        let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
            return;
        };
        let grid = &mut tab.grid_layout;
        if let Some((item, pinned)) = pin {
            item.set_pinned(grid, pinned);
            self.selected = Some(item);
        }
        if let Some(item) = fill {
            self.selected = Some(item);
            self.status = match grid::fill_space(grid, item, view.cell, view.room) {
                Some(rect) => {
                    item.set_rect(grid, rect);
                    String::new()
                }
                None => "No empty space next to it".to_owned(),
            };
        }
        if pack {
            let moves = grid::pack(grid, view.cell);
            for &(item, rect) in &moves {
                item.set_rect(grid, rect);
            }
            self.status = match moves.len() {
                0 => "Already packed".to_owned(),
                1 => "Packed: moved 1 widget".to_owned(),
                n => format!("Packed: moved {n} widgets"),
            };
        }
    }

    /// The grid itself, `content` points big, inside its scroll area.
    fn grid_content(
        &mut self,
        ui: &mut egui::Ui,
        values: &HashMap<&str, &Value>,
        now: Instant,
        content: Vec2,
    ) {
        let (response, painter) = ui.allocate_painter(content, Sense::click());
        let area = response.rect;
        let visible = ui.clip_rect().intersect(area);
        let view = GridView {
            area,
            visible,
            cell: f64::from(self.layout.grid_size()),
            room: (
                f64::from(visible.max.x - area.min.x),
                f64::from(visible.max.y - area.min.y),
            ),
        };
        self.grid.area = area;
        let mut requests = Requests::default();
        self.empty_grid_input(&response, view, &mut requests);
        draw_lines(&painter, ui.visuals(), view);
        if self.tab().is_none() {
            return;
        }
        self.grid.room = view.room;
        if !self.locked {
            self.item_input(ui, view, &mut requests);
        }

        // The drawing steps only look at the app, so lend them the sampler.
        let sampler = RefCell::new(std::mem::take(&mut self.sampler));
        let effects = RefCell::new(Vec::new());
        {
            let bound = WidgetValues::new(&sampler, values, self.nt.structs(), now);
            let value_of = |w: &WidgetContainer| bound.of(w);
            let (writes, top) = self.draw_items(ui, &painter, view, &value_of, values, &effects);
            requests.writes = writes;
            // Over every item, groups' widgets too.
            self.draw_outlines(&top, ui.visuals(), view);
            // Over everything on the grid, groups' layers too.
            let top = painter.clone().with_layer_id(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("grid_drag_preview"),
            ));
            self.drag_preview(&top, ui.visuals(), view, &value_of);
            self.child_drag_preview(ui, &top, view, &value_of, &mut requests);
        }
        self.sampler = sampler.into_inner();
        for effect in effects.into_inner() {
            self.apply_effect(effect);
        }
        let top = painter.clone().with_layer_id(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("grid_drag_preview"),
        ));
        self.tree_drop(ui, &top, view, &mut requests);
        // Any click on a widget finds it in the tree, locked or not, and
        // whatever the widget does with the click (a toggle, a chooser's
        // button). Clicks in a popup or dialog over the grid don't count.
        if requests.find_in_tree.is_none()
            && let Some(pos) = ui
                .input(|i| i.pointer.primary_clicked())
                .then(|| {
                    // Where it was let go: after a touchscreen tap the
                    // pointer is already back where the mouse was.
                    crate::widgets::shared::pointer::released_at(ui)
                        .or(ui.input(|i| i.pointer.interact_pos()))
                })
                .flatten()
            && visible.contains(pos)
            && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
        {
            requests.find_quietly = self.widget_under(view, pos);
        }

        self.apply(ui, view, requests);
    }

    /// The widget (or List Layout child) under `pos` on screen, if any.
    fn widget_under(&self, view: GridView, pos: Pos2) -> Option<WidgetContainer> {
        let grid = &self.tab()?.grid_layout;
        // Front first: what's drawn on top is what's under the pointer.
        for (item, r) in grid::draw_order(grid).into_iter().rev() {
            let rect = view.to_screen(&r);
            if !rect.contains(pos) {
                continue;
            }
            return match item {
                Item::Widget(i) => grid.containers.get(i).cloned(),
                Item::List(i) => {
                    let list = grid.layouts.get(i)?;
                    if group::is_group(list) {
                        let m = group::Mapping::of(rect.shrink(CELL_GAP), list);
                        return list
                            .children
                            .iter()
                            .find(|c| m.to_screen(&c.rect).contains(pos))
                            .cloned();
                    }
                    list_layout::rows_on_screen(rect.shrink(CELL_GAP), list)
                        .into_iter()
                        .find(|row| row.row.contains(pos))
                        .and_then(|row| list.children.get(row.index).cloned())
                }
            };
        }
        None
    }

    /// The open tab, if there is one.
    pub(super) fn tab(&self) -> Option<&Tab> {
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
            self.tree.selection.clear();
        }
        if self.locked {
            return;
        }
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            self.grid.menu_at = view.to_grid(p);
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
            if ui
                .button("New Group")
                .on_hover_text("An empty group here: a card with a small grid of its own")
                .clicked()
            {
                requests.new_group = true;
            }
            if ui
                .button("Pack widgets")
                .on_hover_text(
                    "Slide every widget up, then left, closing the gaps (pinned ones stay put)",
                )
                .clicked()
            {
                requests.pack = true;
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
        for (item, r) in grid::draw_order(&tab.grid_layout) {
            let rect = view.to_screen(&r);
            let id = ui.scope_id().with(("grid_item", self.current_tab, item));
            let corner = Rect::from_min_max(rect.max - Vec2::splat(RESIZE_HANDLE), rect.max);
            let body = ui.interact(rect, id, Sense::click_and_drag());
            // A group's children: each moved by dragging it, resized by its
            // own corner, on top of the group's body.
            if let Item::List(index) = item
                && let Some(g) = tab.grid_layout.layouts.get(index)
                && group::is_group(g)
            {
                let card = rect.shrink(CELL_GAP);
                let m = group::Mapping::of(card, g);
                for (c, child) in g.children.iter().enumerate() {
                    let at = m.to_screen(&child.rect).intersect(card);
                    let child_id = id.with(("group_child", c));
                    let body = ui.interact(at, child_id, Sense::click_and_drag());
                    let corner = Rect::from_min_max(at.max - Vec2::splat(RESIZE_HANDLE), at.max);
                    let corner = ui.interact(corner, child_id.with("resize"), Sense::drag());
                    for (handle, resize) in [(&body, false), (&corner, true)] {
                        if handle.drag_started()
                            && let Some(p) = handle.interact_pointer_pos()
                        {
                            self.grid.child_drag = Some(ChildDrag {
                                list: index,
                                child: c,
                                grab: p - at.min,
                                resize,
                            });
                        }
                    }
                    if body.clicked() || body.secondary_clicked() || body.drag_started() {
                        self.selected = Some(item);
                    }
                    if body.clicked() {
                        requests.find_in_tree = Some(child.clone());
                    }
                    if body.double_clicked() {
                        requests.edit_child = Some((index, c));
                    }
                    body.context_menu(|ui| {
                        if ui
                            .button("Edit…")
                            .on_hover_text("This widget's settings")
                            .clicked()
                        {
                            requests.edit_child = Some((index, c));
                        }
                        if ui
                            .button("Edit group…")
                            .on_hover_text("The group's settings")
                            .clicked()
                        {
                            requests.menu_action = Some((item, EditAction::Edit));
                        }
                        if ui.button("Remove from group").clicked() {
                            requests.remove_child = Some((index, c));
                        }
                        reset_button(ui, Some(child), &mut requests.reset_ranges);
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
            // A list's children, on top of its body so a child can be
            // dragged on its own, but under its resize corner.
            if let Item::List(index) = item
                && let Some(list) = tab.grid_layout.layouts.get(index)
                && !group::is_group(list)
            {
                let card = rect.shrink(CELL_GAP);
                for row in list_layout::rows_on_screen(card, list) {
                    let area = row.row.intersect(card);
                    let child_id = id.with(("child", row.index));
                    let child = ui.interact(area, child_id, Sense::click_and_drag());
                    if child.drag_started()
                        && let Some(p) = child.interact_pointer_pos()
                    {
                        self.grid.child_drag = Some(ChildDrag {
                            list: index,
                            child: row.index,
                            grab: p - row.row.min,
                            resize: false,
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
                    // Its own right-click menu: the child sits over the
                    // list, so the list's menu can't open here.
                    child.context_menu(|ui| {
                        if ui
                            .button("Edit…")
                            .on_hover_text("This widget's settings")
                            .clicked()
                        {
                            requests.edit_child = Some((index, row.index));
                        }
                        if ui
                            .button("Edit list…")
                            .on_hover_text("The list's settings")
                            .clicked()
                        {
                            requests.menu_action = Some((item, EditAction::Edit));
                        }
                        reset_button(ui, list.children.get(row.index), &mut requests.reset_ranges);
                    });
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
                    self.grid.drag = Some(Drag {
                        item,
                        mode,
                        start: r,
                        moved: Vec2::ZERO,
                    });
                }
                if let Some(drag) = &mut self.grid.drag
                    && drag.item == item
                    && drag.mode == mode
                {
                    drag.moved += handle.drag_delta();
                    if handle.drag_stopped() {
                        requests.dropped = self.grid.drag.take();
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
                if let Item::Widget(widget) = item
                    && ui
                        .button("Put in a new group")
                        .on_hover_text("Make a Group here holding this widget")
                        .clicked()
                {
                    requests.wrap_group = Some(widget);
                }
                if ui
                    .button("Fill space")
                    .on_hover_text("Grow into the empty cells around it")
                    .clicked()
                {
                    requests.fill = Some(item);
                }
                // Overlapping another: front and back.
                let overlaps = grid::items(&tab.grid_layout)
                    .into_iter()
                    .any(|(other, o)| other != item && o.overlaps(&r));
                if overlaps {
                    if ui.button("Bring to front").clicked() {
                        requests.front_back = Some((item, true));
                    }
                    if ui.button("Send to back").clicked() {
                        requests.front_back = Some((item, false));
                    }
                }
                let mut pinned = item.pinned(&tab.grid_layout);
                if ui
                    .checkbox(&mut pinned, "Pin in place")
                    .on_hover_text(
                        "Never pushed aside by another widget or moved by Pack widgets; \
                         it still moves when you drag it",
                    )
                    .clicked()
                {
                    requests.pin = Some((item, pinned));
                }
                if let Item::Widget(widget) = item {
                    reset_button(
                        ui,
                        tab.grid_layout.containers.get(widget),
                        &mut requests.reset_ranges,
                    );
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

    /// Draw every item but the one being dragged (drawn on top later),
    /// front to back ([`grid::draw_order`]), with resize corners when
    /// unlocked. A group's widgets are on a layer of their own above the
    /// grid's ([`group::show`]), so whatever comes after a group in the
    /// order is drawn on a new layer above that, keeping things in front of
    /// a group in front of it. Returns the values widgets (and list
    /// children) want sent, and a painter over everything drawn, for the
    /// outlines.
    fn draw_items(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        view: GridView,
        value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
        others: &HashMap<&str, &Value>,
        effects: &RefCell<Vec<widgets::Effect>>,
    ) -> (Vec<(Sender, Value)>, egui::Painter) {
        let mut writes = Vec::new();
        let Some(tab) = self.tab() else {
            return (writes, painter.clone());
        };
        let visuals = ui.visuals().clone();
        let dragging = self.grid.drag.map(|d| d.item);
        // What the drag pushes aside shows where it would go.
        let pushed: HashMap<Item, LayoutRect> = self
            .pushed_aside(ui.ctx(), view, ui.ctx().pointer_latest_pos())
            .into_iter()
            .collect();
        let frame = widgets::Frame {
            traces: &self.traces,
            now_us: self.nt.client().map_or(0, |c| c.now_us()),
            others,
            connected: matches!(
                self.nt.status(),
                crate::nt::ConnectionStatus::Connected { .. }
            ),
            effects,
            structs: self.nt.structs(),
        };
        let place = widgets::Place {
            locked: self.locked,
            ..Default::default()
        };
        let base = ui.layer_id();
        let clip = painter.clip_rect();
        let tab_id = self.current_tab;
        // The layers above the grid's, bottom to top, named by their place
        // in that stack. egui keeps layers in the order it first saw them,
        // and these are first seen bottom to top; but it moves a layer that
        // asks to be on top (a new area, one clicked) past the others, so
        // each frame every one of them asks, and none passes the others.
        let mut stacked = Vec::new();
        let mut next_layer = |ctx: &egui::Context| {
            let id = egui::Id::unique(("grid_layer", stacked.len()));
            let layer = egui::LayerId::new(base.order, id);
            ctx.set_sublayer(base, layer);
            stacked.push(layer);
            id
        };
        // Where items are drawn: the grid's own Ui until a group, then one
        // on a layer above the last group's.
        let mut above: Option<egui::Ui> = None;
        for (item, r) in grid::draw_order(&tab.grid_layout) {
            if Some(item) == dragging {
                continue;
            }
            let r = pushed.get(&item).copied().unwrap_or(r);
            let rect = view.to_screen(&r).shrink(CELL_GAP);
            let on: &mut egui::Ui = match above.as_mut() {
                Some(layer_ui) => layer_ui,
                None => &mut *ui,
            };
            let mut new_group = false;
            // Single widgets get a Ui of their own, for those that need
            // one (graphs, toggles), and so does each list child. Their ids
            // stay the same whichever layer they're on.
            if let Item::Widget(i) = item
                && let Some(w) = tab.grid_layout.containers.get(i)
            {
                let value = value_of(w);
                let live = frame.live(value.as_ref(), place);
                let mut child = on.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .scope_id(egui::Id::unique(("widget", tab_id, i))),
                );
                child.set_clip_rect(rect.intersect(clip));
                if let Some(value) = card::show(&mut child, rect, w, &live) {
                    writes.push((Sender::Widget(i), value));
                }
            } else if let Item::List(i) = item
                && let Some(l) = tab.grid_layout.layouts.get(i)
                && group::is_group(l)
            {
                // Its card here, its widgets on a layer of their own above,
                // scaled onto it; they take input when locked (unlocked,
                // the grid moves them).
                let layer = next_layer(on.ctx());
                let key = egui::Id::unique(("group", tab_id, i));
                let shown = group::show(
                    on,
                    rect,
                    l,
                    layer,
                    base,
                    key,
                    self.locked,
                    value_of,
                    &frame,
                    place,
                );
                for (c, value) in shown {
                    writes.push((Sender::Child { list: i, child: c }, value));
                }
                new_group = true;
            } else if let Item::List(i) = item
                && let Some(l) = tab.grid_layout.layouts.get(i)
            {
                let mut child = on.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .scope_id(egui::Id::unique(("list", tab_id, i))),
                );
                child.set_clip_rect(rect.intersect(clip));
                for (c, value) in list_layout::show(&mut child, rect, l, value_of, &frame, place) {
                    writes.push((Sender::Child { list: i, child: c }, value));
                }
            }
            if !self.locked {
                let corners = on.painter().with_clip_rect(clip);
                draw_resize_handle(&corners, &visuals, view.to_screen(&r));
            }
            if new_group {
                // What comes next goes in front of the group's widgets.
                let id = next_layer(ui.ctx());
                let mut layer_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .layer_id(egui::LayerId::new(base.order, id))
                        .max_rect(ui.max_rect()),
                );
                layer_ui.set_clip_rect(clip);
                above = Some(layer_ui);
            }
        }
        for layer in stacked {
            ui.ctx().move_to_top(layer);
        }
        let top = above.map_or_else(|| painter.clone(), |u| u.painter().with_clip_rect(clip));
        (writes, top)
    }

    /// Outlines: the selected item in the selection color, and widgets
    /// showing the topics picked in the tree, so it's easy to see where a
    /// value is already shown.
    fn draw_outlines(&self, painter: &egui::Painter, visuals: &egui::Visuals, view: GridView) {
        let Some(tab) = self.tab() else { return };
        let dragging = self.grid.drag.map(|d| d.item);
        // Unlocked, a pin in the top-left corner of each pinned item.
        if !self.locked {
            for (item, r) in grid::draw_order(&tab.grid_layout) {
                if Some(item) != dragging && item.pinned(&tab.grid_layout) {
                    draw_pin(painter, visuals, view.to_screen(&r).shrink(CELL_GAP));
                }
            }
        }
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
        for (&(t, sender), &when) in &self.sends.failed {
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
        let picked = self.tree.selection.selected();
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

    /// Make the changes the grid's input asked for this frame.
    fn apply(&mut self, ui: &egui::Ui, view: GridView, requests: Requests) {
        let Requests {
            new_list,
            pack,
            new_group,
            remove_child,
            fill,
            pin,
            front_back,
            dropped,
            child_dropped,
            topics_dropped,
            menu_action,
            edit_child,
            wrap,
            wrap_group,
            find_in_tree,
            find_quietly,
            writes,
            reset_ranges,
        } = requests;
        for key in &reset_ranges {
            self.traces.reset(key);
        }
        self.arrange(pack, fill, pin, view);
        if let Some((item, front)) = front_back
            && let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
        {
            let grid = &mut tab.grid_layout;
            let was = item.z(grid);
            item.to_front_or_back(grid, front);
            // Sent back, it mustn't end up hidden; nor may it hide one.
            if grid.overlap_problem().is_some() {
                if let Some(props) = item_props(grid, item) {
                    if was == 0 {
                        props.remove(raptaboard_layout::overlap::Z);
                    } else {
                        props.insert(raptaboard_layout::overlap::Z.into(), was.into());
                    }
                }
                self.status = "That would hide a widget completely".to_owned();
            }
            self.selected = Some(item);
        }
        for (sender, value) in writes {
            self.send(sender, value);
        }
        if let Some((topics, into, target)) = topics_dropped {
            let over = super::drag::overlapping(ui.ctx());
            self.add_dropped_topics(&topics, into, target, view, over);
        }
        if let Some(drag) = dropped {
            // Worked out here, where it's let go: the preview stops
            // drawing on the frame the drag ends, so it can't say.
            let into = self.drag_into_list(ui.ctx(), drag, view);
            self.drop_dragged(drag, into, view, super::drag::overlapping(ui.ctx()));
        }
        if let Some((cd, target)) = child_dropped {
            self.grid.child_drag = None;
            if let Some(tab) = self.layout.tabs.get_mut(self.current_tab) {
                let grid = &mut tab.grid_layout;
                match target {
                    Some(DropTarget::List { list, at, .. }) => {
                        if grid::child_to_list(grid, cd.list, cd.child, list, at).is_some() {
                            self.selected = Some(Item::List(list));
                        }
                    }
                    // A group's child resized within it.
                    Some(DropTarget::Group {
                        group,
                        rect,
                        ok: true,
                        ..
                    }) if cd.resize => {
                        grid::resize_in_group(grid, group, cd.child, rect);
                        self.selected = Some(Item::List(group));
                    }
                    Some(DropTarget::Group {
                        group,
                        rect,
                        ok: true,
                        ..
                    }) => {
                        if grid::child_into_group(grid, cd.list, cd.child, group, rect).is_some() {
                            self.selected = Some(Item::List(group));
                        }
                    }
                    // Alt held: over what's there, in front.
                    Some(DropTarget::Grid { rect, free: true })
                        if super::drag::overlapping(ui.ctx()) =>
                    {
                        if let Some(w) = grid::child_out_of_list(grid, cd.list, cd.child, rect) {
                            grid::place_over(grid, Item::Widget(w), rect);
                            self.selected = Some(Item::Widget(w));
                        }
                    }
                    Some(DropTarget::Grid { rect, free: true }) => {
                        // Room first, pushing what's in the way aside.
                        let pushed = grid::make_room(grid, rect, view.cell, view.room);
                        for &(item, to) in pushed.iter().flatten() {
                            item.set_rect(grid, to);
                        }
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
            // Straight to the child's settings, and closing them closes
            // the dialog (the list's settings weren't asked for).
            self.selected = Some(Item::List(list));
            self.editing = Some(Editing::List {
                tab: self.current_tab,
                index: list,
                original: Box::new(l.clone()),
                child: Some((child, c.clone())),
                child_only: true,
            });
        }
        if (new_list || wrap.is_some())
            && let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
        {
            let grid = &mut tab.grid_layout;
            let made = match wrap {
                Some(widget) => grid::wrap_in_list(grid, widget, view.cell, view.room),
                None => grid::add_list(grid, self.grid.menu_at, view.cell, view.room),
            };
            match made {
                Some(list) => self.selected = Some(Item::List(list)),
                None => self.status = "No free space for a new list".to_owned(),
            }
        }
        if let Some(widget) = wrap_group
            && let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
        {
            match grid::wrap_in_group(&mut tab.grid_layout, widget, view.cell, view.room) {
                Some(g) => self.selected = Some(Item::List(g)),
                None => self.status = "No free space for a new group".to_owned(),
            }
        }
        if let Some((g, c)) = remove_child
            && let Some(tab) = self.layout.tabs.get_mut(self.current_tab)
            && let Some(l) = tab.grid_layout.layouts.get_mut(g)
            && c < l.children.len()
        {
            l.children.remove(c);
            self.selected = Some(Item::List(g));
        }
        if new_group && let Some(tab) = self.layout.tabs.get_mut(self.current_tab) {
            match grid::add_group(
                &mut tab.grid_layout,
                self.grid.menu_at,
                view.cell,
                view.room,
            ) {
                Some(g) => self.selected = Some(Item::List(g)),
                None => self.status = "No free space for a new group".to_owned(),
            }
        }
        if let Some(widget) = find_quietly {
            self.find_in_tree_quietly(&widget);
        }
        if let Some(widget) = find_in_tree {
            self.find_in_tree(&widget);
        }
        if let Some((item, action)) = menu_action {
            self.selected = Some(item);
            self.run_edit_action(ui.ctx(), action);
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
        // Worked out first, so the widget's borrow of the layout ends
        // before the connection is used.
        let result = match binding::write_target(widget) {
            Ok(WriteTarget::Topic(topic)) => self.nt.write(&topic, value),
            Ok(WriteTarget::Publish(topic, ty)) => self.nt.write_or_publish(&topic, value, ty),
            Err(why) => Err(why),
        };
        match result {
            Ok(()) => {
                self.sends.failed.remove(&(self.current_tab, sender));
                // A failure's message is out of date once a send works
                // (unless something else has said something since).
                if self
                    .sends
                    .last_failure
                    .take()
                    .is_some_and(|m| m == self.status)
                {
                    self.status.clear();
                }
            }
            Err(why) => {
                self.status = why.to_string();
                self.sends.last_failure = Some(self.status.clone());
                self.sends
                    .failed
                    .insert((self.current_tab, sender), Instant::now());
                if self.alert_sound {
                    self.sound.play();
                }
            }
        }
    }
}

/// How close to the grid's edge (points) a drag starts it scrolling.
const EDGE: f32 = 40.0;
/// How fast a drag at the very edge scrolls, points per second.
const EDGE_SCROLL_SPEED: f32 = 900.0;

/// Which way, and how hard (0 to 1 each way), a drag at `pointer` pushes
/// the grid `visible` on screen to scroll: toward an edge it's within
/// [`EDGE`] of, harder the closer.
fn edge_push(visible: Rect, pointer: Option<Pos2>) -> Vec2 {
    let Some(p) = pointer else {
        return Vec2::ZERO;
    };
    if !visible.is_positive() || !visible.expand(EDGE).contains(p) {
        return Vec2::ZERO;
    }
    let toward = |near_min: f32, near_max: f32| {
        if near_max < EDGE {
            1.0 - near_max.max(0.0) / EDGE
        } else if near_min < EDGE {
            -(1.0 - near_min.max(0.0) / EDGE)
        } else {
            0.0
        }
    };
    Vec2::new(
        toward(p.x - visible.min.x, visible.max.x - p.x),
        toward(p.y - visible.min.y, visible.max.y - p.y),
    )
}

/// How far right and down `grid`'s widgets and lists reach, in points.
fn extent(grid: &raptaboard_layout::GridLayout) -> Vec2 {
    grid::items(grid).iter().fold(Vec2::ZERO, |most, (_, r)| {
        most.max(Vec2::new((r.x + r.width) as f32, (r.y + r.height) as f32))
    })
}

/// The grid lines, one every cell across and down the grid's area.
fn draw_lines(painter: &egui::Painter, visuals: &egui::Visuals, view: GridView) {
    let line = Stroke::new(
        GRID_LINE_WIDTH,
        visuals.weak_text_color().gamma_multiply(GRID_LINE_STRENGTH),
    );
    // Only over the part on screen, lined up with the grid's origin.
    let (area, shown, step) = (view.area, view.visible, view.cell as f32);
    let first = |from: f32, origin: f32| origin + ((from - origin) / step).floor().max(0.0) * step;
    let mut x = first(shown.min.x, area.min.x);
    while x <= shown.max.x.min(area.max.x) {
        painter.vline(x, shown.y_range(), line);
        x += step;
    }
    let mut y = first(shown.min.y, area.min.y);
    while y <= shown.max.y.min(area.max.y) {
        painter.hline(shown.x_range(), y, line);
        y += step;
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn a_drag_near_an_edge_pushes_that_way_harder_closer() {
        let shown = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(400.0, 300.0));
        let push = |x: f32, y: f32| edge_push(shown, Some(Pos2::new(x, y)));
        assert_eq!(push(300.0, 200.0), Vec2::ZERO, "the middle");
        assert!(push(495.0, 200.0).x > 0.8 && push(495.0, 200.0).y == 0.0);
        assert!(push(480.0, 200.0).x < push(495.0, 200.0).x, "harder closer");
        assert!(push(105.0, 200.0).x < -0.8, "left");
        assert!(push(300.0, 345.0).y > 0.8, "bottom");
        let corner = push(495.0, 55.0);
        assert!(
            corner.x > 0.8 && corner.y < -0.8,
            "a corner pushes both ways"
        );
        // Past the edge (the pointer left the grid) it pushes fully.
        assert_eq!(push(520.0, 200.0).x, 1.0);
        // Far away, or no pointer: nothing.
        assert_eq!(push(900.0, 200.0), Vec2::ZERO);
        assert_eq!(edge_push(shown, None), Vec2::ZERO);
    }

    #[test]
    fn the_extent_is_how_far_widgets_and_lists_reach() {
        let mut grid = raptaboard_layout::GridLayout::default();
        assert_eq!(extent(&grid), Vec2::ZERO);
        let mut w: WidgetContainer = serde_json::from_value(serde_json::json!({
            "title": "A", "x": 3200, "y": 128, "width": 256, "height": 128,
            "type": "Text Display", "properties": {}
        }))
        .unwrap();
        grid.containers.push(w.clone());
        w.rect.x = 0.0;
        w.rect.y = 2400.0;
        grid.containers.push(w);
        assert_eq!(extent(&grid), Vec2::new(3456.0, 2528.0));
    }
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

    /// One frame with `events`, and where the text `title` was drawn (its
    /// left edge on screen), if it was.
    fn drawn_at(
        ctx: &egui::Context,
        app: &mut DashboardApp,
        events: Vec<Event>,
        title: &str,
    ) -> Option<f32> {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| {
            app.grid(ui, &HashMap::new(), Instant::now());
        });
        output.textures_delta.clear();
        fn find(shape: &egui::Shape, title: &str) -> Option<f32> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == title => Some(t.pos.x),
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, title)),
                _ => None,
            }
        }
        output.shapes.iter().find_map(|s| find(&s.shape, title))
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
        assert!(app.grid.drag.is_none());
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
    fn a_drop_onto_another_widget_pushes_it_aside_in_one_undo() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.history = crate::edit::History::new(&app.layout);
        // A onto B: A goes there, and B to the nearest free cell, beside it
        // on the left (as near as right; on the same row, the left wins).
        drag(&ctx, &mut app, (40.0, 40.0), (40.0 + 384.0, 40.0));
        assert_eq!(rect_of(&app, 0), (384.0, 0.0, 128.0, 128.0));
        assert_eq!(rect_of(&app, 1), (256.0, 0.0, 128.0, 128.0));
        assert_eq!(
            app.status,
            "Moved 1 widget out of the way (hold Alt to overlap instead)"
        );
        // One undo puts both back.
        app.history.record(&app.layout);
        assert!(app.history.undo(&mut app.layout));
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 128.0, 128.0));
        assert_eq!(rect_of(&app, 1), (384.0, 0.0, 128.0, 128.0));
    }

    /// The grid's view in a window 768 by 512 points, as the tests draw it.
    fn test_view(origin: Pos2) -> GridView {
        let window = Rect::from_min_size(origin, egui::vec2(768.0, 512.0));
        GridView {
            area: window,
            visible: window,
            cell: 128.0,
            room: (768.0, 512.0),
        }
    }

    #[test]
    fn pack_fill_and_pin_from_the_menus_each_say_what_they_did() {
        let ctx = egui::Context::default();
        let mut app = app();
        let origin = frame(&ctx, &mut app, vec![]);
        let view = test_view(origin);
        // B pinned: Pack slides A and the list but not B.
        app.arrange(false, None, Some((Item::Widget(1), true)), view);
        assert!(Item::Widget(1).pinned(&app.layout.tabs[0].grid_layout));
        app.layout.tabs[0].grid_layout.containers[0].rect.y = 128.0;
        app.arrange(true, None, None, view);
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 128.0, 128.0));
        assert_eq!(rect_of(&app, 1), (384.0, 0.0, 128.0, 128.0), "pinned");
        let list = app.layout.tabs[0].grid_layout.layouts[0].rect;
        assert_eq!((list.x, list.y), (0.0, 128.0), "the list slid up under A");
        assert_eq!(app.status, "Packed: moved 2 widgets");
        app.arrange(true, None, None, view);
        assert_eq!(app.status, "Already packed");
        // A fills the space left of B: three columns, the whole window's
        // height but the list's column below it.
        app.arrange(false, Some(Item::Widget(0)), None, view);
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 384.0, 128.0));
        assert_eq!(app.selected, Some(Item::Widget(0)));
        app.arrange(false, Some(Item::Widget(0)), None, view);
        assert_eq!(app.status, "No empty space next to it");
    }

    #[test]
    fn a_drop_onto_a_pinned_widget_is_refused_and_pins_show_while_unlocked() {
        let ctx = egui::Context::default();
        let mut app = app();
        Item::Widget(1).set_pinned(&mut app.layout.tabs[0].grid_layout, true);
        drag(&ctx, &mut app, (40.0, 40.0), (40.0 + 384.0, 40.0));
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 128.0, 128.0), "refused");
        assert_eq!(rect_of(&app, 1), (384.0, 0.0, 128.0, 128.0));
        // The pin is drawn unlocked, not locked.
        let pins = |app: &mut DashboardApp| {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| app.grid(ui, &HashMap::new(), Instant::now()));
            output.textures_delta.clear();
            fn count(shape: &egui::Shape) -> usize {
                match shape {
                    egui::Shape::Circle(c) if c.radius == PIN_HEAD => 1,
                    egui::Shape::Vec(v) => v.iter().map(count).sum(),
                    _ => 0,
                }
            }
            output.shapes.iter().map(|s| count(&s.shape)).sum::<usize>()
        };
        assert_eq!(pins(&mut app), 1);
        app.locked = true;
        assert_eq!(pins(&mut app), 0);
    }

    /// [`drag`] with Alt held all the way.
    fn alt_drag(ctx: &egui::Context, app: &mut DashboardApp, from: (f32, f32), to: (f32, f32)) {
        let held = |m| Event::ModifiersChanged(m);
        let origin = frame(ctx, app, vec![held(egui::Modifiers::ALT)]);
        let start = origin + Vec2::new(from.0, from.1);
        let end = origin + Vec2::new(to.0, to.1);
        frame(ctx, app, vec![Event::PointerMoved(start)]);
        frame(ctx, app, vec![button(start, true)]);
        for step in 1..=8 {
            let at = start + (end - start) * (step as f32 / 8.0);
            frame(ctx, app, vec![Event::PointerMoved(at)]);
        }
        frame(ctx, app, vec![button(end, false)]);
        frame(ctx, app, vec![held(egui::Modifiers::NONE)]);
    }

    #[test]
    fn with_alt_a_widget_goes_over_others_instead_of_pushing_them() {
        let ctx = egui::Context::default();
        let mut app = app();
        // B made two cells wide, so A can cover half of it.
        app.layout.tabs[0].grid_layout.containers[1].rect.width = 256.0;
        // Alt: A over B's left half, nothing pushed, A in front.
        alt_drag(&ctx, &mut app, (40.0, 40.0), (40.0 + 384.0, 40.0));
        assert_eq!(rect_of(&app, 0), (384.0, 0.0, 128.0, 128.0));
        assert_eq!(rect_of(&app, 1), (384.0, 0.0, 256.0, 128.0), "B stayed");
        let grid = &app.layout.tabs[0].grid_layout;
        assert!(Item::Widget(0).may_overlap(grid));
        assert_eq!(grid::draw_order(grid).last().unwrap().0, Item::Widget(0));
        assert_eq!(grid.overlap_problem(), None);
        // A click there picks A, drawn on top.
        click(&ctx, &mut app, (430.0, 60.0));
        assert_eq!(app.selected, Some(Item::Widget(0)));

        // Alt over all of B would hide it: refused, and said.
        app.layout.tabs[0].grid_layout.containers[0].rect.width = 256.0;
        alt_drag(&ctx, &mut app, (420.0, 40.0), (420.0, 40.0 + 128.0));
        alt_drag(&ctx, &mut app, (420.0, 168.0), (420.0, 40.0));
        assert_eq!(app.status, "It would hide a widget completely there");

        // Without Alt, A moved back onto B pushes B aside as before.
        let mut app = super::tests::app();
        drag(&ctx, &mut app, (40.0, 40.0), (40.0 + 384.0, 40.0));
        assert_ne!(rect_of(&app, 1), (384.0, 0.0, 128.0, 128.0));
        assert!(
            app.status.ends_with("(hold Alt to overlap instead)"),
            "{}",
            app.status
        );
    }

    #[test]
    fn with_alt_a_widget_goes_over_a_list_not_into_it() {
        let ctx = egui::Context::default();
        let mut app = app();
        // A onto the list's top half (the list is two cells tall).
        alt_drag(&ctx, &mut app, (40.0, 40.0), (40.0, 256.0 + 40.0));
        let grid = &app.layout.tabs[0].grid_layout;
        assert!(grid.layouts[0].children.is_empty(), "not put in the list");
        assert_eq!(grid.containers.len(), 2);
        assert_eq!(rect_of(&app, 0), (0.0, 256.0, 128.0, 128.0));
        assert_eq!(grid::draw_order(grid).last().unwrap().0, Item::Widget(0));
    }

    /// The texts drawn in one frame, in the order they're painted (layer
    /// by layer, bottom to top).
    fn painted_texts(ctx: &egui::Context, app: &mut DashboardApp) -> Vec<String> {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| app.grid(ui, &HashMap::new(), Instant::now()));
        output.textures_delta.clear();
        fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for s in &output.shapes {
            texts(&s.shape, &mut out);
        }
        out
    }

    #[test]
    fn a_widget_over_a_group_is_drawn_in_front_or_behind_its_widgets_by_its_place() {
        let ctx = egui::Context::default();
        let mut app = app();
        // A group, four cells by two, at (256, 256), with a widget inside;
        // A moved over the group's top-left cell, in front.
        let grid = &mut app.layout.tabs[0].grid_layout;
        let g = grid::add_group(grid, (256.0, 256.0), 128.0, (768.0, 512.0)).unwrap();
        let mut inside = grid.containers[1].clone();
        inside.title = "Inside".into();
        inside.rect = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: group::INNER_CELL,
            height: group::INNER_CELL,
        };
        grid.layouts[g].children.push(inside);
        let over = LayoutRect {
            x: 256.0,
            y: 256.0,
            width: 128.0,
            height: 128.0,
        };
        assert!(grid::fits_over(grid, Item::Widget(0), over));
        grid::place_over(grid, Item::Widget(0), over);
        assert_eq!(grid.overlap_problem(), None);
        let at = |texts: &[String], t: &str| {
            texts
                .iter()
                .position(|x| x == t)
                .unwrap_or_else(|| panic!("{t} not drawn: {texts:?}"))
        };
        // In front: painted after the group's widgets.
        frame(&ctx, &mut app, vec![]);
        let texts = painted_texts(&ctx, &mut app);
        assert!(at(&texts, "A") > at(&texts, "Inside"), "{texts:?}");
        // Sent back: painted before them, under the group.
        Item::Widget(0).to_front_or_back(&mut app.layout.tabs[0].grid_layout, false);
        frame(&ctx, &mut app, vec![]);
        let texts = painted_texts(&ctx, &mut app);
        assert!(at(&texts, "A") < at(&texts, "Inside"), "{texts:?}");
        // A click where it's in front of the group picks it; behind, the
        // group.
        click(&ctx, &mut app, (256.0 + 20.0, 256.0 + 60.0));
        assert_eq!(app.selected, Some(Item::List(g)));
        Item::Widget(0).to_front_or_back(&mut app.layout.tabs[0].grid_layout, true);
        click(&ctx, &mut app, (256.0 + 20.0, 256.0 + 60.0));
        assert_eq!(app.selected, Some(Item::Widget(0)));
    }

    #[test]
    fn with_alt_a_widget_goes_over_part_of_a_group_and_a_group_over_a_widget() {
        let ctx = egui::Context::default();
        let mut app = app();
        let g = grid::add_group(
            &mut app.layout.tabs[0].grid_layout,
            (256.0, 256.0),
            128.0,
            (768.0, 512.0),
        )
        .unwrap();
        // A, Alt-dragged onto the group's top-left cell: over it, nothing
        // pushed, and not into it.
        alt_drag(&ctx, &mut app, (40.0, 40.0), (256.0 + 40.0, 256.0 + 40.0));
        let grid = &app.layout.tabs[0].grid_layout;
        assert_eq!(rect_of(&app, 0), (256.0, 256.0, 128.0, 128.0));
        assert!(grid.layouts[g].children.is_empty(), "not into the group");
        assert_eq!(
            (grid.layouts[g].rect.x, grid.layouts[g].rect.y),
            (256.0, 256.0)
        );
        assert_eq!(grid::draw_order(grid).last().unwrap().0, Item::Widget(0));

        // A group two cells wide, Alt-dragged by its title over half of B
        // (two cells wide too): over it, B stays.
        let mut app = super::tests::app();
        let grid = &mut app.layout.tabs[0].grid_layout;
        grid.containers[1].rect.width = 256.0;
        let g = grid::add_group(grid, (256.0, 256.0), 128.0, (768.0, 512.0)).unwrap();
        grid.layouts[g].rect.width = 256.0;
        let b = grid.containers[1].rect;
        alt_drag(
            &ctx,
            &mut app,
            (256.0 + 40.0, 256.0 + 12.0),
            (512.0 + 40.0, 12.0),
        );
        let grid = &app.layout.tabs[0].grid_layout;
        assert_eq!(
            (grid.layouts[g].rect.x, grid.layouts[g].rect.y),
            (512.0, 0.0),
            "{}",
            app.status
        );
        assert_eq!(grid.containers[1].rect, b, "B stayed");
        assert_eq!(grid::draw_order(grid).last().unwrap().0, Item::List(g));
        assert_eq!(grid.overlap_problem(), None);
    }

    #[test]
    fn a_widget_goes_into_a_group_moves_and_resizes_there_and_comes_back_out() {
        let ctx = egui::Context::default();
        let mut app = app();
        // A group, four cells by two, at (256, 256); inside, a 4 by 2 grid.
        let g = grid::add_group(
            &mut app.layout.tabs[0].grid_layout,
            (256.0, 256.0),
            128.0,
            (768.0, 512.0),
        )
        .unwrap();
        let grid_of = |app: &DashboardApp| app.layout.tabs[0].grid_layout.clone();
        let group_rect = grid_of(&app).layouts[g].rect;
        assert_eq!((group_rect.x, group_rect.y), (256.0, 256.0));
        // Inner cells in grid coordinates (the tests' drag positions).
        let m = |app: &DashboardApp| {
            let l = &grid_of(app).layouts[g];
            let card = Rect::from_min_size(
                Pos2::new(l.rect.x as f32, l.rect.y as f32),
                egui::vec2(l.rect.width as f32, l.rect.height as f32),
            )
            .shrink(CELL_GAP);
            group::Mapping::of(card, l)
        };
        let cell = |app: &DashboardApp, c: f64, r: f64| {
            let p = m(app)
                .to_screen(&LayoutRect {
                    x: c * group::INNER_CELL,
                    y: r * group::INNER_CELL,
                    width: group::INNER_CELL,
                    height: group::INNER_CELL,
                })
                .center();
            (p.x, p.y)
        };
        let inner = |c: f64, r: f64, w: f64, h: f64| LayoutRect {
            x: c * group::INNER_CELL,
            y: r * group::INNER_CELL,
            width: w * group::INNER_CELL,
            height: h * group::INNER_CELL,
        };

        // A into the group's second cell.
        let to = cell(&app, 1.0, 0.0);
        drag(&ctx, &mut app, (40.0, 40.0), to);
        let now = grid_of(&app);
        assert_eq!(now.containers.len(), 1, "A left the grid");
        assert_eq!(now.layouts[g].children.len(), 1);
        assert_eq!(now.layouts[g].children[0].title, "A");
        assert_eq!(now.layouts[g].children[0].rect, inner(1.0, 0.0, 1.0, 1.0));

        // Moved within the group, to the last cell of the second row.
        let from = cell(&app, 1.0, 0.0);
        let to = cell(&app, 3.0, 1.0);
        drag(&ctx, &mut app, from, to);
        assert_eq!(
            grid_of(&app).layouts[g].children[0].rect,
            inner(3.0, 1.0, 1.0, 1.0)
        );
        // And back to the first cell.
        let from = cell(&app, 3.0, 1.0);
        let to = cell(&app, 0.0, 0.0);
        drag(&ctx, &mut app, from, to);
        assert_eq!(
            grid_of(&app).layouts[g].children[0].rect,
            inner(0.0, 0.0, 1.0, 1.0)
        );

        // Its corner dragged to the middle of the second row's second cell:
        // two cells by two.
        let corner = m(&app).to_screen(&inner(0.0, 0.0, 1.0, 1.0)).max - egui::vec2(3.0, 3.0);
        let to = cell(&app, 1.0, 1.0);
        drag(&ctx, &mut app, (corner.x, corner.y), to);
        assert_eq!(
            grid_of(&app).layouts[g].children[0].rect,
            inner(0.0, 0.0, 2.0, 2.0)
        );

        // Out onto the empty grid at the top right.
        let from = cell(&app, 0.0, 0.0);
        drag(&ctx, &mut app, from, (from.0 + 300.0, 20.0));
        let now = grid_of(&app);
        assert!(now.layouts[g].children.is_empty(), "out of the group");
        let a = now.containers.iter().find(|w| w.title == "A").unwrap();
        assert_eq!(
            (a.rect.width, a.rect.height),
            (256.0, 256.0),
            "two cells by two"
        );
    }

    #[test]
    fn escape_calls_a_drag_off_and_letting_go_then_drops_nothing() {
        let escape = Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        let ctx = egui::Context::default();
        let mut app = app();
        app.selected = None;
        // Hold A over B (pushing B aside), then Escape, then let go there.
        let origin = frame(&ctx, &mut app, vec![]);
        let start = origin + Vec2::new(40.0, 40.0);
        let over_b = start + Vec2::new(384.0, 0.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=8 {
            let at = start + (over_b - start) * (step as f32 / 8.0);
            frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        }
        assert!(app.grid.drag.is_some());
        frame(&ctx, &mut app, vec![escape.clone()]);
        assert!(app.grid.drag.is_none(), "called off");
        assert_eq!(app.status, "Drag cancelled");
        // B is drawn back in its own place at once.
        let b_x = drawn_at(&ctx, &mut app, vec![], "B").expect("B drawn") - origin.x;
        assert!(b_x >= 384.0, "B's title at {b_x}");
        frame(&ctx, &mut app, vec![button(over_b, false)]);
        frame(&ctx, &mut app, vec![]);
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 128.0, 128.0));
        assert_eq!(rect_of(&app, 1), (384.0, 0.0, 128.0, 128.0));

        // A resize called off the same way keeps the old size.
        let corner = origin + Vec2::new(124.0, 124.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(corner)]);
        frame(&ctx, &mut app, vec![button(corner, true)]);
        for step in 1..=8 {
            let at = corner + Vec2::new(256.0 * step as f32 / 8.0, 0.0);
            frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        }
        frame(&ctx, &mut app, vec![escape]);
        frame(
            &ctx,
            &mut app,
            vec![button(corner + Vec2::new(256.0, 0.0), false)],
        );
        frame(&ctx, &mut app, vec![]);
        assert_eq!(rect_of(&app, 0), (0.0, 0.0, 128.0, 128.0));
    }

    #[test]
    fn a_list_child_dragged_out_onto_a_widget_pushes_it_aside() {
        let ctx = egui::Context::default();
        let mut app = app();
        let child: WidgetContainer = serde_json::from_value(serde_json::json!({
            "title": "C", "x": 0, "y": 0, "width": 128, "height": 128,
            "type": "Text Display", "properties": { "topic": "/C" }
        }))
        .unwrap();
        app.layout.tabs[0].grid_layout.layouts[0]
            .children
            .push(child);
        let origin = frame(&ctx, &mut app, vec![]);
        let list = app.layout.tabs[0].grid_layout.layouts[0].clone();
        let card = Rect::from_min_size(
            origin + Vec2::new(list.rect.x as f32, list.rect.y as f32),
            Vec2::new(list.rect.width as f32, list.rect.height as f32),
        )
        .shrink(CELL_GAP);
        let row = list_layout::rows_on_screen(card, &list)[0].row;
        // Hold the row in its middle, and move it so its corner lands on
        // B's cell.
        let start = row.center();
        let b_corner = origin + Vec2::new(384.0, 0.0);
        let end = start + (b_corner - row.min);
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=8 {
            let at = start + (end - start) * (step as f32 / 8.0);
            frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        }
        assert!(app.grid.child_drag.is_some(), "the child is being dragged");
        frame(&ctx, &mut app, vec![button(end, false)]);
        frame(&ctx, &mut app, vec![]);
        let grid = &app.layout.tabs[0].grid_layout;
        assert!(grid.layouts[0].children.is_empty(), "out of the list");
        let c = grid.containers.iter().find(|w| w.title == "C").unwrap();
        assert_eq!((c.rect.x, c.rect.y), (384.0, 0.0));
        assert_ne!(rect_of(&app, 1), (384.0, 0.0, 128.0, 128.0), "B moved");
    }

    #[test]
    fn with_alt_list_children_go_over_widgets_and_lists_not_aside_or_into_them() {
        let ctx = egui::Context::default();
        let mut app = app();
        // B two cells wide, so the child can cover half of it.
        app.layout.tabs[0].grid_layout.containers[1].rect.width = 256.0;
        let child: WidgetContainer = serde_json::from_value(serde_json::json!({
            "title": "C", "x": 0, "y": 0, "width": 128, "height": 128,
            "type": "Text Display", "properties": { "topic": "/C" }
        }))
        .unwrap();
        let mut d = child.clone();
        d.title = "D".into();
        app.layout.tabs[0].grid_layout.layouts[0]
            .children
            .extend([child, d]);
        let held = |m| Event::ModifiersChanged(m);
        let origin = frame(&ctx, &mut app, vec![held(egui::Modifiers::ALT)]);
        let list = app.layout.tabs[0].grid_layout.layouts[0].clone();
        let card = Rect::from_min_size(
            origin + Vec2::new(list.rect.x as f32, list.rect.y as f32),
            Vec2::new(list.rect.width as f32, list.rect.height as f32),
        )
        .shrink(CELL_GAP);
        let row = list_layout::rows_on_screen(card, &list)[0].row;
        let start = row.center();
        let end = start + (origin + Vec2::new(384.0, 0.0) - row.min);
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=8 {
            let at = start + (end - start) * (step as f32 / 8.0);
            frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        }
        // Nothing drawn pushed aside while it's held over B.
        assert!(app.grid.child_drag.is_some());
        frame(&ctx, &mut app, vec![button(end, false)]);
        frame(&ctx, &mut app, vec![held(egui::Modifiers::NONE)]);
        let grid = &app.layout.tabs[0].grid_layout;
        assert_eq!(grid.layouts[0].children.len(), 1, "C out of the list");
        let c = grid.containers.iter().position(|w| w.title == "C").unwrap();
        assert_eq!(rect_of(&app, c), (384.0, 0.0, 128.0, 128.0));
        assert_eq!(rect_of(&app, 1), (384.0, 0.0, 256.0, 128.0), "B stayed");
        assert!(Item::Widget(c).may_overlap(grid));
        assert_eq!(grid::draw_order(grid).last().unwrap().0, Item::Widget(c));
        assert_eq!(grid.overlap_problem(), None);
        // D, held over its own list with Alt (one cell by two): onto the
        // grid over the list's top cell, not back into the list.
        frame(&ctx, &mut app, vec![held(egui::Modifiers::ALT)]);
        let list = app.layout.tabs[0].grid_layout.layouts[0].clone();
        let card = Rect::from_min_size(
            origin + Vec2::new(list.rect.x as f32, list.rect.y as f32),
            Vec2::new(list.rect.width as f32, list.rect.height as f32),
        )
        .shrink(CELL_GAP);
        let row = list_layout::rows_on_screen(card, &list)[0].row;
        let start = row.center();
        let corner = origin + Vec2::new(list.rect.x as f32, list.rect.y as f32);
        let end = start + (corner - row.min);
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=8 {
            let at = start + (end - start) * (step as f32 / 8.0);
            frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        }
        frame(&ctx, &mut app, vec![button(end, false)]);
        frame(&ctx, &mut app, vec![held(egui::Modifiers::NONE)]);
        let grid = &app.layout.tabs[0].grid_layout;
        assert!(grid.layouts[0].children.is_empty(), "D out of the list");
        let d = grid.containers.iter().position(|w| w.title == "D").unwrap();
        assert_eq!(rect_of(&app, d), (list.rect.x, list.rect.y, 128.0, 128.0));
        assert_eq!(grid.overlap_problem(), None);
    }

    #[test]
    fn widgets_in_the_way_are_drawn_where_they_will_go_during_the_drag() {
        let ctx = egui::Context::default();
        let mut app = app();
        // Press on A and hold it over B.
        let origin = frame(&ctx, &mut app, vec![]);
        let start = origin + Vec2::new(40.0, 40.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(start)]);
        frame(&ctx, &mut app, vec![button(start, true)]);
        for step in 1..=8 {
            let at = start + Vec2::new(384.0 * step as f32 / 8.0, 0.0);
            frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        }
        let window = Rect::from_min_size(origin, egui::vec2(768.0, 512.0));
        let grid_view = GridView {
            area: window,
            visible: window,
            cell: 128.0,
            room: (768.0, 512.0),
        };
        let view = |app: &DashboardApp| app.pushed_aside(&ctx, grid_view, None);
        let pushed = view(&app);
        assert_eq!(pushed.len(), 1);
        assert_eq!(pushed[0].0, Item::Widget(1));
        assert_eq!(pushed[0].1.x, 256.0);
        // Nothing has moved yet, but B is drawn where it would go: a cell
        // and a half left of its own middle.
        assert_eq!(rect_of(&app, 1), (384.0, 0.0, 128.0, 128.0));
        let b_x = drawn_at(&ctx, &mut app, vec![], "B").expect("B drawn") - origin.x;
        assert!((256.0..384.0).contains(&b_x), "B's title at {b_x}");
        // Back off B: it stays put.
        let back = start + Vec2::new(128.0, 0.0);
        frame(&ctx, &mut app, vec![Event::PointerMoved(back)]);
        assert!(view(&app).is_empty());
        let b_x = drawn_at(&ctx, &mut app, vec![], "B").expect("B drawn") - origin.x;
        assert!(b_x >= 384.0, "B's title back at {b_x}");
        frame(&ctx, &mut app, vec![button(back, false)]);
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
