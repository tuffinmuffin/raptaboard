//! Dragging on the grid: moving and resizing widgets, List Layout children,
//! and what a drop lands on, with the outlines drawn while dragging.

use eframe::egui::{self, Rect, Stroke, Vec2};
use raptaboard_layout::Rect as LayoutRect;

use crate::grid::{self, Item};
use crate::widgets::list_layout;
use crate::widgets::shared::card;

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
    /// Into the graph at `index` in the tab's `containers`, as more lines;
    /// `card` is the graph on screen.
    Graph { index: usize, card: Rect },
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
        DropTarget::Graph { card, .. } => draw_landing(painter, card, true),
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
