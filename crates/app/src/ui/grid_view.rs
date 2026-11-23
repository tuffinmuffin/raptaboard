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
use crate::widgets::{self, list_layout};

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
            list_layout::draw(painter, visuals, rect, l, value_of);
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
    /// A widget clicked, to find its topic in the tree.
    pub find_in_tree: Option<WidgetContainer>,
    /// A widget whose click it used itself, to find in the tree without
    /// saying anything if it isn't there.
    pub find_quietly: Option<WidgetContainer>,
    /// Values widgets want sent, and which widget sent each.
    pub writes: Vec<(Sender, Value)>,
    /// Series whose range seen to forget: "Reset scale" from a widget's
    /// menu.
    pub reset_ranges: Vec<SeriesKey>,
}

/// "Reset scale" in a widget's right-click menu, if it has a range to
/// reset ([`widgets::WidgetType::resettable`]); clicked, its series go in
/// `reset`.
fn reset_scale_button(
    ui: &mut egui::Ui,
    widget: Option<&WidgetContainer>,
    reset: &mut Vec<SeriesKey>,
) {
    let Some(w) = widget else { return };
    let Some(ty) = widgets::find(&w.widget_type) else {
        return;
    };
    let keys = ty.resettable(w);
    if !keys.is_empty()
        && ui
            .button("Reset scale")
            .on_hover_text("Start the autoscaled range again from the value now")
            .clicked()
    {
        reset.extend(keys);
    }
}

impl DashboardApp {
    /// The open tab's grid: lines, widgets, and (when unlocked) dragging to
    /// move widgets and their bottom-right corner to resize them. Drops snap
    /// to the grid and are refused if they would cover another widget.
    pub(super) fn grid(&mut self, ui: &mut egui::Ui, values: &HashMap<&str, &Value>, now: Instant) {
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
            requests.writes = self.draw_items(ui, &painter, view, &value_of, values, &effects);
            self.draw_outlines(&painter, ui.visuals(), view);
            self.drag_preview(&painter, ui.visuals(), view, &value_of);
            self.child_drag_preview(ui, &painter, view, &value_of, &mut requests);
        }
        self.sampler = sampler.into_inner();
        for effect in effects.into_inner() {
            self.apply_effect(effect);
        }
        self.tree_drop(ui, &painter, view, &mut requests);
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
        for (item, r) in grid::items(grid) {
            let rect = view.to_screen(&r);
            if !rect.contains(pos) {
                continue;
            }
            return match item {
                Item::Widget(i) => grid.containers.get(i).cloned(),
                Item::List(i) => {
                    let list = grid.layouts.get(i)?;
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
                        self.grid.child_drag = Some(ChildDrag {
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
                        reset_scale_button(
                            ui,
                            list.children.get(row.index),
                            &mut requests.reset_ranges,
                        );
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
                if let Item::Widget(widget) = item {
                    reset_scale_button(
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
    /// with resize corners when unlocked. Returns the values widgets (and
    /// list children) want sent.
    fn draw_items(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        view: GridView,
        value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
        others: &HashMap<&str, &Value>,
        effects: &RefCell<Vec<widgets::Effect>>,
    ) -> Vec<(Sender, Value)> {
        let mut writes = Vec::new();
        let Some(tab) = self.tab() else {
            return writes;
        };
        let visuals = ui.visuals().clone();
        let dragging = self.grid.drag.map(|d| d.item);
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
                let live = frame.live(value.as_ref(), place);
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
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt((
                    "list",
                    self.current_tab,
                    i,
                )));
                child.set_clip_rect(rect.intersect(painter.clip_rect()));
                for (c, value) in list_layout::show(&mut child, rect, l, value_of, &frame, place) {
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
        let dragging = self.grid.drag.map(|d| d.item);
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
            dropped,
            child_dropped,
            topics_dropped,
            menu_action,
            edit_child,
            wrap,
            find_in_tree,
            find_quietly,
            writes,
            reset_ranges,
        } = requests;
        for key in &reset_ranges {
            self.traces.reset_range(key);
        }
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
            self.grid.child_drag = None;
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
