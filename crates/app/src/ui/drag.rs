//! Dragging on the grid: moving and resizing widgets, List Layout children
//! and topics from the tree, what a drop lands on, the outlines and
//! previews drawn while dragging, and the drop itself.

use eframe::egui::{self, Pos2, Rect, Stroke, Vec2};
use nt4::Value;
use raptaboard_layout::{Rect as LayoutRect, WidgetContainer};

use crate::grid::{self, Item};
use crate::tree::DraggedTopics;
use crate::widgets::shared::card;
use crate::widgets::{self, group, list_layout};

use super::DashboardApp;
use super::grid_view::{GridView, Requests, draw_item};

use super::style::{
    CELL_GAP, DROP_BLOCKED, DROP_OK, LANDING_INSET, OUTLINE_WIDTH, RESIZE_MARK_INSET,
    RESIZE_MARK_WIDTH, RESIZE_MARKS,
};

/// What a drag on the grid is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DragMode {
    /// Moving the whole item.
    Move,
    /// Changing its size from the bottom-right corner.
    Resize,
}

/// A widget or List Layout being dragged.
#[derive(Debug, Clone, Copy)]
pub(super) struct Drag {
    pub(super) item: Item,
    pub(super) mode: DragMode,
    /// Where the item was when the drag started.
    pub(super) start: LayoutRect,
    /// How far the pointer has moved since then, in points.
    pub(super) moved: Vec2,
}

impl Drag {
    /// Where the item is while it follows the pointer, before snapping.
    pub(super) fn following(&self) -> LayoutRect {
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
    pub(super) fn target(&self, cell: f64, area: (f64, f64)) -> LayoutRect {
        match self.mode {
            DragMode::Move => grid::snap_move(self.following(), cell, area),
            DragMode::Resize => grid::snap_resize(self.following(), cell),
        }
    }
}

/// A List Layout's or group's child being dragged: to another place in its
/// list or group, into another, or out onto the grid; or (a group's, by its
/// corner) resized within its group.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChildDrag {
    /// The list or group, by its index in the tab's `layouts`.
    pub(super) list: usize,
    /// The child, by its index in the list's `children`.
    pub(super) child: usize,
    /// Where in its row the pointer took hold, in points, so the preview
    /// keeps the same spot under the pointer.
    pub(super) grab: Vec2,
    /// Resizing a group's child by its corner, not moving it.
    pub(super) resize: bool,
}

/// Where something dropped on the grid would go.
#[derive(Debug, Clone, Copy)]
pub(super) enum DropTarget {
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
    /// Onto the widget at `index` in the tab's `containers`, one that takes
    /// topics (a graph, as more lines); `card` is the widget on screen.
    Widget { index: usize, card: Rect },
    /// Into group `group` (its index in the tab's `layouts`) in the cells
    /// `rect` (saved inner pixels), if there's room (`ok`); `card` is those
    /// cells on screen.
    Group {
        group: usize,
        rect: LayoutRect,
        ok: bool,
        card: Rect,
    },
}

/// Whether a drag places over others instead of pushing them aside: Alt
/// (Option on a Mac) held.
pub(super) fn overlapping(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.modifiers.alt)
}

/// The outline where a dragged widget or topic will land: green if it fits
/// there, red if it would cover another widget.
pub(super) fn draw_landing(painter: &egui::Painter, rect: Rect, ok: bool) {
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
pub(super) fn draw_drop(
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
        DropTarget::Widget { card, .. } => draw_landing(painter, card, true),
        DropTarget::Group { card, ok, .. } => draw_landing(painter, card, ok),
    }
}

/// Two short diagonal lines in a widget's bottom-right corner, showing
/// where to drag to resize it.
pub(super) fn draw_resize_handle(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect) {
    let corner = rect.shrink(CELL_GAP).max - Vec2::splat(RESIZE_MARK_INSET);
    let stroke = Stroke::new(RESIZE_MARK_WIDTH, visuals.weak_text_color());
    for size in RESIZE_MARKS {
        painter.line_segment(
            [corner - Vec2::new(size, 0.0), corner - Vec2::new(0.0, size)],
            stroke,
        );
    }
}

/// Drag and drop on the grid: where a dragged widget, list child or tree
/// topic would land, its preview, and the drop itself.
impl DashboardApp {
    /// The list under screen point `p` as a drop target, with where in it a
    /// drop would go, if there's a list there.
    pub(super) fn list_drop(&self, view: GridView, p: Pos2) -> Option<DropTarget> {
        if !view.visible.contains(p) {
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

    /// The group under screen point `p` as a drop target for something
    /// `size` big (saved pixels), in the cells under the pointer, with
    /// whether there's room. `moving` is a group's child already being
    /// moved (group, child), left out of its own way.
    pub(super) fn group_drop(
        &self,
        view: GridView,
        p: Pos2,
        size: LayoutRect,
        moving: Option<(usize, usize)>,
    ) -> Option<DropTarget> {
        if !view.visible.contains(p) {
            return None;
        }
        let tab = self.tab()?;
        let index = grid::group_at(&tab.grid_layout, view.to_grid(p))?;
        let g = &tab.grid_layout.layouts[index];
        let m = group::Mapping::of(view.to_screen(&g.rect).shrink(CELL_GAP), g);
        let at = m.to_inner(p);
        let except = moving.filter(|&(from, _)| from == index).map(|(_, c)| c);
        let rect = group::cell_rect(g, size, at);
        let ok = group::place(g, size, at, except).is_some();
        Some(DropTarget::Group {
            group: index,
            rect,
            ok,
            card: m.to_screen(&rect),
        })
    }

    /// The widget under screen point `p` that takes dropped topics (a
    /// graph), as a drop target, if there is one.
    pub(super) fn topic_taker_drop(&self, view: GridView, p: Pos2) -> Option<DropTarget> {
        let at = view.to_grid(p);
        self.tab()?
            .grid_layout
            .containers
            .iter()
            .enumerate()
            .find(|(_, w)| {
                let r = w.rect;
                widgets::find(&w.widget_type).is_some_and(|t| t.takes_topics())
                    && at.0 >= r.x
                    && at.0 < r.x + r.width
                    && at.1 >= r.y
                    && at.1 < r.y + r.height
            })
            .map(|(index, w)| DropTarget::Widget {
                index,
                card: view.to_screen(&w.rect).shrink(CELL_GAP),
            })
    }

    /// The list or group a widget being moved would go into: the one under
    /// the pointer, if any. Resizing, lists being dragged, and anything dragged
    /// with Alt held (placed over what's there: [`overlapping`]) never go
    /// into a list.
    pub(super) fn drag_into_list(
        &self,
        ctx: &egui::Context,
        drag: Drag,
        view: GridView,
    ) -> Option<DropTarget> {
        if overlapping(ctx) {
            return None;
        }
        match (drag.item, drag.mode, ctx.pointer_latest_pos()) {
            (Item::Widget(_), DragMode::Move, Some(p)) => self
                .list_drop(view, p)
                .or_else(|| self.group_drop(view, p, drag.start, None)),
            _ => None,
        }
    }

    /// Where the items a drag pushes aside are, if it were dropped now on
    /// the grid with the pointer at `pointer` ([`grid::reflow`]): each one
    /// moved, not the dragged item itself. Empty with no drag, a drag going
    /// into a list, or one that can't go where it is.
    pub(super) fn pushed_aside(
        &self,
        ctx: &egui::Context,
        view: GridView,
        pointer: Option<Pos2>,
    ) -> Vec<(Item, LayoutRect)> {
        let Some(tab) = self.tab() else {
            return Vec::new();
        };
        if let Some(drag) = self.grid.drag {
            // With Alt held it goes over them instead.
            if self.drag_into_list(ctx, drag, view).is_some() || overlapping(ctx) {
                return Vec::new();
            }
            let target = drag.target(view.cell, view.room);
            return grid::reflow(&tab.grid_layout, drag.item, target, view.cell, view.room)
                .map(|moves| moves.into_iter().skip(1).collect())
                .unwrap_or_default();
        }
        // Something new coming onto the grid: a list child dragged out, or
        // topics from the tree. With Alt held it goes over them instead.
        if overlapping(ctx) {
            return Vec::new();
        }
        let new = match self.grid.child_drag {
            Some(_) => pointer.and_then(|p| self.child_grid_target(view, p, false)),
            None => pointer.and_then(|p| self.tree_grid_target(ctx, view, p)),
        };
        new.and_then(|rect| grid::make_room(&tab.grid_layout, rect, view.cell, view.room))
            .unwrap_or_default()
    }

    /// Where the list child being dragged lands on the grid, with the
    /// pointer at `p`: `None` off the grid, or over a list or group (it goes
    /// in) unless placed `over` what's there (Alt held).
    pub(super) fn child_grid_target(
        &self,
        view: GridView,
        p: Pos2,
        over: bool,
    ) -> Option<LayoutRect> {
        let cd = self.grid.child_drag?;
        let child = self
            .tab()?
            .grid_layout
            .layouts
            .get(cd.list)?
            .children
            .get(cd.child)?;
        let over_group = self
            .tab()
            .and_then(|t| grid::group_at(&t.grid_layout, view.to_grid(p)))
            .is_some();
        let into = !over && (self.list_drop(view, p).is_some() || over_group);
        if into || !view.visible.contains(p) {
            return None;
        }
        let size = grid::snap_resize(child.rect, view.cell);
        let (x, y) = view.to_grid(p - cd.grab);
        Some(grid::snap_move(
            LayoutRect { x, y, ..size },
            view.cell,
            view.room,
        ))
    }

    /// Where the first widget for topics dragged from the tree lands on the
    /// grid, with the pointer at `p`: the cell under it. `None` with no
    /// topics dragged, while locked, off the grid, or over a list, group or
    /// widget that takes topics (they go into it) unless Alt is held (they
    /// go over it: [`overlapping`]).
    pub(super) fn tree_grid_target(
        &self,
        ctx: &egui::Context,
        view: GridView,
        p: Pos2,
    ) -> Option<LayoutRect> {
        if self.locked
            || !view.visible.contains(p)
            || !egui::DragAndDrop::has_payload_of_type::<DraggedTopics>(ctx)
        {
            return None;
        }
        let over_group = self
            .tab()
            .and_then(|t| grid::group_at(&t.grid_layout, view.to_grid(p)))
            .is_some();
        if !overlapping(ctx)
            && (self.topic_taker_drop(view, p).is_some()
                || self.list_drop(view, p).is_some()
                || over_group)
        {
            return None;
        }
        let (x, y) = view.to_grid(p);
        let under = LayoutRect {
            x: x - view.cell / 2.0,
            y: y - view.cell / 2.0,
            width: view.cell,
            height: view.cell,
        };
        Some(grid::snap_move(under, view.cell, view.room))
    }

    /// The dragged item follows the pointer on top of everything, with an
    /// outline where it will land: green if it can go there (pushing what's
    /// in the way aside, drawn where it would go), red if not. A widget over
    /// a list goes into the list instead.
    pub(super) fn drag_preview(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        view: GridView,
        value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
    ) {
        let (Some(drag), Some(tab)) = (self.grid.drag, self.tab()) else {
            return;
        };
        let to_screen = |r: &LayoutRect| view.to_screen(r);
        match self.drag_into_list(painter.ctx(), drag, view) {
            Some(target) => draw_drop(painter, &target, &to_screen),
            None => {
                let target = drag.target(view.cell, view.room);
                let grid = &tab.grid_layout;
                let ok = if overlapping(painter.ctx()) {
                    grid::fits_over(grid, drag.item, target)
                } else {
                    grid::reflow(grid, drag.item, target, view.cell, view.room).is_some()
                };
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
    pub(super) fn child_drag_preview(
        &self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        view: GridView,
        value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
        requests: &mut Requests,
    ) {
        let Some(cd) = self.grid.child_drag else {
            return;
        };
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
        let from = &tab.grid_layout.layouts[cd.list];
        let target = if cd.resize {
            // A group's child by its corner: its new size, in its group.
            let m = group::Mapping::of(view.to_screen(&from.rect).shrink(CELL_GAP), from);
            let rect = group::resized(from, child.rect, m.to_inner(p));
            let inner = group::inner_grid(from);
            let ok = grid::reflow(
                &inner,
                Item::Widget(cd.child),
                rect,
                group::INNER_CELL,
                group::inner_area(from),
            )
            .is_some();
            Some(DropTarget::Group {
                group: cd.list,
                rect,
                ok,
                card: m.to_screen(&rect),
            })
        } else if overlapping(ui.ctx()) {
            // Alt held: onto the grid over what's there, never into a list.
            self.child_grid_target(view, p, true)
                .map(|rect| DropTarget::Grid {
                    rect,
                    free: grid::fits_new_over(&tab.grid_layout, rect),
                })
        } else {
            // Into a list or group, or onto the grid pushing what's in the
            // way aside if it can.
            self.list_drop(view, p)
                .or_else(|| self.group_drop(view, p, child.rect, Some((cd.list, cd.child))))
                .or_else(|| {
                    self.child_grid_target(view, p, false)
                        .map(|rect| DropTarget::Grid {
                            rect,
                            free: grid::make_room(&tab.grid_layout, rect, view.cell, view.room)
                                .is_some(),
                        })
                })
        };
        if let Some(target) = &target {
            draw_drop(painter, target, &|r: &LayoutRect| view.to_screen(r));
        }
        if !cd.resize {
            // A group's child is drawn at its size there; a list's at its own.
            let size = if group::is_group(from) {
                let m = group::Mapping::of(view.to_screen(&from.rect).shrink(CELL_GAP), from);
                m.to_screen(&child.rect).size()
            } else {
                let size = grid::snap_resize(child.rect, view.cell);
                Vec2::new(size.width as f32, size.height as f32)
            };
            let preview = Rect::from_min_size(p - cd.grab, size);
            card::draw(
                painter,
                ui.visuals(),
                preview.shrink(CELL_GAP),
                child,
                value_of(child).as_ref(),
            );
        }
        // Let go: here, or outside the window, where the release may not
        // reach us (it then stays where it was).
        if !ui.input(|i| i.pointer.primary_down()) {
            requests.child_dropped = Some((cd, target));
        }
    }

    /// Topics dragged from the NT tree: show the cell under the pointer
    /// where the first widget would go, or the list or graph they'd go
    /// into, and ask for the widgets when they're let go.
    pub(super) fn tree_drop(
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
            .filter(|p| view.visible.contains(*p))
        else {
            return;
        };
        let Some(topics) = egui::DragAndDrop::payload::<DraggedTopics>(ui.ctx()) else {
            return;
        };
        // Over a widget that takes topics, they go to it (a graph's lines).
        let one = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: group::INNER_CELL,
            height: group::INNER_CELL,
        };
        // With Alt held they go onto the grid over what's there.
        let over = overlapping(ui.ctx());
        let into = if over {
            None
        } else {
            self.topic_taker_drop(view, p)
                .or_else(|| self.list_drop(view, p))
                .or_else(|| self.group_drop(view, p, one, None))
        };
        let Some(target) = self.tree_grid_target(ui.ctx(), view, p).or_else(|| {
            let (x, y) = view.to_grid(p);
            Some(LayoutRect {
                x,
                y,
                width: view.cell,
                height: view.cell,
            })
        }) else {
            return;
        };
        match &into {
            Some(drop) => draw_drop(painter, drop, &|r: &LayoutRect| view.to_screen(r)),
            // The first goes here, pushing what's in the way aside; several
            // go around it, so only the first must have room.
            None => {
                let room = self.tab().is_some_and(|t| {
                    if over {
                        grid::fits_new_over(&t.grid_layout, target)
                    } else {
                        grid::make_room(&t.grid_layout, target, view.cell, view.room).is_some()
                    }
                });
                draw_landing(painter, view.to_screen(&target), room);
            }
        }
        if ui.input(|i| i.pointer.any_released()) {
            requests.topics_dropped = Some((topics, into, target));
        }
    }

    /// Put a widget or list let go after a drag `into` a list, or where it
    /// was dragged to on the grid, pushing what's in the way to the nearest
    /// free spots ([`grid::reflow`]); if something has nowhere to go, it
    /// goes back.
    pub(super) fn drop_dragged(
        &mut self,
        drag: Drag,
        into: Option<DropTarget>,
        view: GridView,
        over: bool,
    ) {
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
            (
                Item::Widget(widget),
                Some(DropTarget::Group {
                    group, rect, ok, ..
                }),
            ) => {
                if ok && grid::widget_into_group(grid, widget, group, rect).is_some() {
                    self.selected = Some(Item::List(group));
                } else {
                    self.status = "No room in that group".to_owned();
                }
            }
            // Alt held: over what's there, in front, unless that would
            // hide something.
            _ if over => {
                let target = drag.target(view.cell, view.room);
                if grid::fits_over(grid, drag.item, target) {
                    grid::place_over(grid, drag.item, target);
                } else {
                    self.status = "It would hide a widget completely there".to_owned();
                }
            }
            _ => {
                let target = drag.target(view.cell, view.room);
                if let Some(moves) = grid::reflow(grid, drag.item, target, view.cell, view.room) {
                    for &(item, rect) in &moves {
                        item.set_rect(grid, rect);
                    }
                    grid::tidy_overlaps(grid);
                    self.status = match moves.len() - 1 {
                        0 => String::new(),
                        1 => {
                            "Moved 1 widget out of the way (hold Alt to overlap instead)".to_owned()
                        }
                        n => format!(
                            "Moved {n} widgets out of the way (hold Alt to overlap instead)"
                        ),
                    };
                }
            }
        }
    }

    /// Make widgets for topics dropped from the tree: into the list, as
    /// lines on the graph, or on the grid, each in the free spot nearest
    /// `target`. Placed `over` what's there (Alt held), the first goes at
    /// `target` in front (if that hides nothing completely); the rest go
    /// in free spots near it.
    pub(super) fn add_dropped_topics(
        &mut self,
        topics: &DraggedTopics,
        into: Option<DropTarget>,
        target: LayoutRect,
        view: GridView,
        over: bool,
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
            Some(DropTarget::Group {
                group,
                rect,
                ok: true,
                ..
            }) => {
                let count = made.len();
                let added = grid::widgets_into_group(grid, group, rect, made);
                self.selected = Some(Item::List(group));
                if added < count {
                    self.status = format!("Added {added} of {count} widgets; the group is full");
                }
            }
            Some(DropTarget::Widget { index, .. }) => {
                let widget = &mut grid.containers[index];
                if let Some(ty) = widgets::find(&widget.widget_type) {
                    self.status = ty.add_topics(widget, &topics.0);
                }
                self.selected = Some(Item::Widget(index));
            }
            None if over => {
                if !grid::fits_new_over(grid, target) {
                    self.status = "It would hide a widget completely there".to_owned();
                    return;
                }
                let mut made = made.into_iter();
                let Some(mut first) = made.next() else { return };
                first.rect = target;
                grid.containers.push(first);
                let at = grid.containers.len() - 1;
                grid::place_over(grid, Item::Widget(at), target);
                let rest = made.collect();
                let more = grid::place_near(grid, rest, (target.x, target.y), cell, view.room);
                self.selected = Some(Item::Widget(at));
                if 1 + more.len() < topics.0.len() {
                    self.status = format!(
                        "Added {} of {} widgets; no free space for the rest",
                        1 + more.len(),
                        topics.0.len()
                    );
                }
            }
            None => {
                // Room for the first, pushing what's in the way aside.
                let pushed = grid::make_room(grid, target, cell, view.room).unwrap_or_default();
                for &(item, rect) in &pushed {
                    item.set_rect(grid, rect);
                }
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
