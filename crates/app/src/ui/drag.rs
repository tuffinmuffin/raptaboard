//! Dragging on the grid: moving and resizing widgets, List Layout children
//! and topics from the tree, what a drop lands on, the outlines and
//! previews drawn while dragging, and the drop itself.

use eframe::egui::{self, Pos2, Rect, Stroke, Vec2};
use nt4::Value;
use raptaboard_layout::{Rect as LayoutRect, WidgetContainer};

use crate::grid::{self, Item};
use crate::tree::DraggedTopics;
use crate::widgets::shared::card;
use crate::widgets::{self, list_layout};

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

/// A List Layout's child being dragged: to another place in its list, into
/// another list, or out onto the grid.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChildDrag {
    /// The list, by its index in the tab's `layouts`.
    pub(super) list: usize,
    /// The child, by its index in the list's `children`.
    pub(super) child: usize,
    /// Where in its row the pointer took hold, in points, so the preview
    /// keeps the same spot under the pointer.
    pub(super) grab: Vec2,
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

    /// The list a widget being moved would go into, with the pointer at
    /// `pointer`: the list under the pointer, if any. Resizing, and lists
    /// being dragged, never go into a list.
    pub(super) fn drag_into_list(
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
        // Over a widget that takes topics, they go to it (a graph's lines).
        let into = self
            .topic_taker_drop(view, p)
            .or_else(|| self.list_drop(view, p));
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

    /// Put a widget or list let go after a drag `into` a list, or where it
    /// was dragged to on the grid if it fits there (else it goes back).
    pub(super) fn drop_dragged(&mut self, drag: Drag, into: Option<DropTarget>, view: GridView) {
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

    /// Make widgets for topics dropped from the tree: into the list, as
    /// lines on the graph, or on the grid, each in the free spot nearest
    /// `target`.
    pub(super) fn add_dropped_topics(
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
            Some(DropTarget::Widget { index, .. }) => {
                let widget = &mut grid.containers[index];
                if let Some(ty) = widgets::find(&widget.widget_type) {
                    self.status = ty.add_topics(widget, &topics.0);
                }
                self.selected = Some(Item::Widget(index));
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
