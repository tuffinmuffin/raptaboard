//! Placing widgets on a tab's snap grid.
//!
//! A tab is a grid of square cells (128 pixels by default). Widgets and List
//! Layouts are moved and resized in whole cells, and two of them may never
//! overlap. This module holds those rules without any drawing, so they can be
//! tested without a window. Positions and sizes are pixels at the layout's
//! grid size, as Elastic saves them.

use raptaboard_layout::{GridLayout, Rect};

/// Something on a tab's grid: a single widget or a List Layout, by its index
/// in [`GridLayout::containers`] or [`GridLayout::layouts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    /// `containers[i]`: a single widget.
    Widget(usize),
    /// `layouts[i]`: a List Layout.
    List(usize),
}

impl Item {
    /// Where this item is on `grid`, or `None` if it doesn't exist.
    pub fn rect(self, grid: &GridLayout) -> Option<Rect> {
        match self {
            Item::Widget(i) => grid.containers.get(i).map(|w| w.rect),
            Item::List(i) => grid.layouts.get(i).map(|l| l.rect),
        }
    }

    /// Move or resize this item on `grid`. Does nothing if it doesn't exist.
    pub fn set_rect(self, grid: &mut GridLayout, rect: Rect) {
        match self {
            Item::Widget(i) => {
                if let Some(w) = grid.containers.get_mut(i) {
                    w.rect = rect;
                }
            }
            Item::List(i) => {
                if let Some(l) = grid.layouts.get_mut(i) {
                    l.rect = rect;
                }
            }
        }
    }
}

/// Every item on `grid` with its place: widgets first, then List Layouts.
pub fn items(grid: &GridLayout) -> Vec<(Item, Rect)> {
    let widgets = grid
        .containers
        .iter()
        .enumerate()
        .map(|(i, w)| (Item::Widget(i), w.rect));
    let lists = grid
        .layouts
        .iter()
        .enumerate()
        .map(|(i, l)| (Item::List(i), l.rect));
    widgets.chain(lists).collect()
}

/// Snap a moved rectangle to the nearest cell corner, keeping its size.
///
/// `area` is the width and height of the visible grid in pixels: the
/// rectangle is kept inside it where it fits, and never goes left of or
/// above the grid.
///
/// ```
/// use raptaboard::grid::snap_move;
/// use raptaboard_layout::Rect;
/// let dropped = Rect { x: 200.0, y: -20.0, width: 128.0, height: 256.0 };
/// let placed = snap_move(dropped, 128.0, (1024.0, 768.0));
/// assert_eq!((placed.x, placed.y), (256.0, 0.0));
/// assert_eq!((placed.width, placed.height), (128.0, 256.0));
/// ```
pub fn snap_move(rect: Rect, cell: f64, area: (f64, f64)) -> Rect {
    let snap = |pos: f64, size: f64, room: f64| {
        let last = ((room - size) / cell).floor().max(0.0) * cell;
        ((pos / cell).round() * cell).clamp(0.0, last)
    };
    Rect {
        x: snap(rect.x, rect.width, area.0),
        y: snap(rect.y, rect.height, area.1),
        ..rect
    }
}

/// Snap a resized rectangle to whole cells, at least one cell each way.
/// The top-left corner stays where it is.
///
/// ```
/// use raptaboard::grid::snap_resize;
/// use raptaboard_layout::Rect;
/// let dragged = Rect { x: 0.0, y: 0.0, width: 300.0, height: 20.0 };
/// let sized = snap_resize(dragged, 128.0);
/// assert_eq!((sized.width, sized.height), (256.0, 128.0));
/// ```
pub fn snap_resize(rect: Rect, cell: f64) -> Rect {
    let snap = |size: f64| ((size / cell).round() * cell).max(cell);
    Rect {
        width: snap(rect.width),
        height: snap(rect.height),
        ..rect
    }
}

/// Whether two rectangles share any area. Rectangles that only touch along
/// an edge don't overlap.
pub fn overlaps(a: Rect, b: Rect) -> bool {
    // Old layouts can have sizes a hair off whole pixels; ignore slivers.
    const SLACK: f64 = 0.5;
    a.x + a.width - SLACK > b.x
        && b.x + b.width - SLACK > a.x
        && a.y + a.height - SLACK > b.y
        && b.y + b.height - SLACK > a.y
}

/// Whether `item` can go at `rect` on `grid` without covering anything else.
///
/// ```
/// use raptaboard::grid::{fits, Item};
/// use raptaboard_layout::{GridLayout, Rect};
/// let grid: GridLayout = serde_json::from_str(r#"{"containers": [
///     {"title": "a", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"},
///     {"title": "b", "x": 128.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
/// ]}"#).unwrap();
/// let onto_b = Rect { x: 128.0, y: 0.0, width: 128.0, height: 128.0 };
/// let below = Rect { x: 0.0, y: 128.0, width: 128.0, height: 128.0 };
/// assert!(!fits(&grid, Item::Widget(0), onto_b));
/// assert!(fits(&grid, Item::Widget(0), below));
/// ```
pub fn fits(grid: &GridLayout, item: Item, rect: Rect) -> bool {
    items(grid)
        .into_iter()
        .all(|(other, r)| other == item || !overlaps(rect, r))
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn moves_snap_to_the_nearest_cell_and_stay_on_screen() {
        let area = (640.0, 384.0);
        // Just under half a cell rounds down, just over rounds up.
        assert_eq!(
            snap_move(rect(63.0, 65.0, 128.0, 128.0), 128.0, area).x,
            0.0
        );
        assert_eq!(
            snap_move(rect(63.0, 65.0, 128.0, 128.0), 128.0, area).y,
            128.0
        );
        // Dropped past the right edge: the last column it fits in.
        assert_eq!(
            snap_move(rect(600.0, 0.0, 256.0, 128.0), 128.0, area).x,
            384.0
        );
        // Taller than the window: pinned to the top rather than going negative.
        assert_eq!(
            snap_move(rect(0.0, 200.0, 128.0, 512.0), 128.0, area).y,
            0.0
        );
    }

    #[test]
    fn resizes_snap_to_whole_cells() {
        assert_eq!(
            snap_resize(rect(128.0, 0.0, 450.0, 130.0), 128.0),
            rect(128.0, 0.0, 512.0, 128.0)
        );
        // A widget can't shrink below one cell.
        assert_eq!(
            snap_resize(rect(0.0, 0.0, -50.0, 10.0), 128.0),
            rect(0.0, 0.0, 128.0, 128.0)
        );
        // Other grid sizes work the same way.
        assert_eq!(
            snap_resize(rect(0.0, 0.0, 100.0, 140.0), 64.0),
            rect(0.0, 0.0, 128.0, 128.0)
        );
    }

    #[test]
    fn touching_edges_is_not_overlapping() {
        let a = rect(0.0, 0.0, 128.0, 128.0);
        assert!(!overlaps(a, rect(128.0, 0.0, 128.0, 128.0)));
        assert!(!overlaps(a, rect(0.0, 128.0, 128.0, 128.0)));
        assert!(overlaps(a, rect(64.0, 64.0, 128.0, 128.0)));
        // One inside the other.
        assert!(overlaps(rect(0.0, 0.0, 512.0, 512.0), a));
    }

    #[test]
    fn list_layouts_block_widgets_too() {
        let grid: GridLayout = serde_json::from_str(
            r#"{"containers": [
                {"title": "a", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
            ], "layouts": [
                {"title": "list", "x": 256.0, "y": 0.0, "width": 256.0, "height": 384.0, "type": "List Layout"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            items(&grid),
            vec![
                (Item::Widget(0), rect(0.0, 0.0, 128.0, 128.0)),
                (Item::List(0), rect(256.0, 0.0, 256.0, 384.0)),
            ]
        );
        assert!(!fits(
            &grid,
            Item::Widget(0),
            rect(384.0, 128.0, 128.0, 128.0)
        ));
        // An item never blocks itself: growing in place is fine.
        assert!(fits(&grid, Item::List(0), rect(256.0, 0.0, 384.0, 384.0)));

        let mut grid = grid;
        Item::List(0).set_rect(&mut grid, rect(512.0, 0.0, 128.0, 128.0));
        assert_eq!(
            Item::List(0).rect(&grid),
            Some(rect(512.0, 0.0, 128.0, 128.0))
        );
        assert_eq!(Item::Widget(5).rect(&grid), None);
    }
}
