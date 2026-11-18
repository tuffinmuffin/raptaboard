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

impl Item {
    /// The item's title, or `None` if it doesn't exist.
    pub fn title(self, grid: &GridLayout) -> Option<&str> {
        match self {
            Item::Widget(i) => grid.containers.get(i).map(|w| &*w.title),
            Item::List(i) => grid.layouts.get(i).map(|l| &*l.title),
        }
    }

    /// Take this item off `grid`. Other items of the same kind after it
    /// move down one index. Does nothing if it doesn't exist.
    pub fn remove(self, grid: &mut GridLayout) {
        match self {
            Item::Widget(i) if i < grid.containers.len() => {
                grid.containers.remove(i);
            }
            Item::List(i) if i < grid.layouts.len() => {
                grid.layouts.remove(i);
            }
            _ => {}
        }
    }

    /// Put a copy of this item (a List Layout with its children) in the
    /// free spot nearest to it, within `area` (the visible grid's width and
    /// height in pixels). Returns the copy, or `None` if there's no room or
    /// the item doesn't exist.
    ///
    /// ```
    /// use raptaboard::grid::Item;
    /// use raptaboard_layout::GridLayout;
    /// let mut grid: GridLayout = serde_json::from_str(r#"{"containers": [
    ///     {"title": "a", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
    /// ]}"#).unwrap();
    /// let copy = Item::Widget(0).duplicate(&mut grid, 128.0, (512.0, 512.0)).unwrap();
    /// assert_eq!(copy, Item::Widget(1));
    /// assert_eq!(grid.containers[1].rect.x, 128.0);
    /// ```
    pub fn duplicate(self, grid: &mut GridLayout, cell: f64, area: (f64, f64)) -> Option<Item> {
        let rect = free_spot_near(grid, self.rect(grid)?, cell, area)?;
        match self {
            Item::Widget(i) => {
                let mut copy = grid.containers[i].clone();
                copy.rect = rect;
                grid.containers.push(copy);
                Some(Item::Widget(grid.containers.len() - 1))
            }
            Item::List(i) => {
                let mut copy = grid.layouts[i].clone();
                copy.rect = rect;
                grid.layouts.push(copy);
                Some(Item::List(grid.layouts.len() - 1))
            }
        }
    }
}

/// The free spot for something `rect`'s size closest to `rect`, on whole
/// cells inside `area` (the visible grid's width and height in pixels,
/// counting a part-shown last row or column). Ties go to the spot higher
/// up, then further left. `None` if nowhere is free.
///
/// ```
/// use raptaboard::grid::free_spot_near;
/// use raptaboard_layout::{GridLayout, Rect};
/// let empty = GridLayout::default();
/// let at = Rect { x: 256.0, y: 0.0, width: 128.0, height: 128.0 };
/// assert_eq!(free_spot_near(&empty, at, 128.0, (512.0, 512.0)), Some(at));
/// ```
pub fn free_spot_near(grid: &GridLayout, rect: Rect, cell: f64, area: (f64, f64)) -> Option<Rect> {
    let cells = |size: f64| (size / cell).ceil().max(1.0) as i64;
    let (w, h) = (cells(rect.width), cells(rect.height));
    let (cols, rows) = (cells(area.0), cells(area.1));
    let (from_col, from_row) = (
        (rect.x / cell).round() as i64,
        (rect.y / cell).round() as i64,
    );
    (0..=rows - h)
        .flat_map(|row| (0..=cols - w).map(move |col| (col, row)))
        .min_by_key(|&(col, row)| {
            let (dc, dr) = (col - from_col, row - from_row);
            (
                !is_free_at(grid, rect, cell, col, row),
                dc * dc + dr * dr,
                row,
                col,
            )
        })
        .filter(|&(col, row)| is_free_at(grid, rect, cell, col, row))
        .map(|(col, row)| Rect {
            x: col as f64 * cell,
            y: row as f64 * cell,
            ..rect
        })
}

/// [`is_free`] for `rect`'s size with its corner at cell `(col, row)`.
fn is_free_at(grid: &GridLayout, rect: Rect, cell: f64, col: i64, row: i64) -> bool {
    is_free(
        grid,
        Rect {
            x: col as f64 * cell,
            y: row as f64 * cell,
            ..rect
        },
    )
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
/// `area` is the width and height of the visible grid in pixels. The
/// rectangle stays within the cells that show, counting a last row or
/// column the window cuts off part way, and never goes left of or above
/// the grid.
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
        let shown = (room / cell).ceil();
        let needed = (size / cell).ceil();
        let last = (shown - needed).max(0.0) * cell;
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

/// Whether `rect` is empty on `grid`, for placing something new there.
///
/// ```
/// use raptaboard::grid::is_free;
/// use raptaboard_layout::{GridLayout, Rect};
/// let empty = GridLayout::default();
/// assert!(is_free(&empty, Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 }));
/// ```
pub fn is_free(grid: &GridLayout, rect: Rect) -> bool {
    items(grid).into_iter().all(|(_, r)| !rect.overlaps(&r))
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
        .all(|(other, r)| other == item || !rect.overlaps(&r))
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
        // A window 1000 tall shows 7 full rows and part of an 8th; widgets
        // can go in that last row.
        let area = (1024.0, 1000.0);
        assert_eq!(
            snap_move(rect(0.0, 950.0, 128.0, 128.0), 128.0, area).y,
            896.0
        );
        assert_eq!(
            snap_move(rect(0.0, 950.0, 128.0, 256.0), 128.0, area).y,
            768.0
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
        // Something new can't go on the list, but can go beside it.
        assert!(!is_free(&grid, rect(256.0, 128.0, 128.0, 128.0)));
        assert!(is_free(&grid, rect(128.0, 0.0, 128.0, 128.0)));
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

    #[test]
    fn copies_go_to_the_nearest_free_spot() {
        let mut grid: GridLayout = serde_json::from_str(
            r#"{"containers": [
                {"title": "a", "x": 128.0, "y": 128.0, "width": 256.0, "height": 128.0, "type": "Text Display"},
                {"title": "b", "x": 384.0, "y": 128.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
            ]}"#,
        )
        .unwrap();
        let area = (512.0, 384.0);
        // Right of "a" is taken by "b", so the copy goes just above "a".
        let copy = Item::Widget(0).duplicate(&mut grid, 128.0, area).unwrap();
        assert_eq!(
            Item::Widget(2).rect(&grid),
            Some(rect(128.0, 0.0, 256.0, 128.0))
        );
        assert_eq!(copy.title(&grid), Some("a"));
        // Then just below it.
        Item::Widget(0).duplicate(&mut grid, 128.0, area).unwrap();
        assert_eq!(
            Item::Widget(3).rect(&grid),
            Some(rect(128.0, 256.0, 256.0, 128.0))
        );
        // Fill the rest; then there's no room left.
        while Item::Widget(1).duplicate(&mut grid, 128.0, area).is_some() {}
        assert_eq!(grid.containers.len(), 4 + 5); // 12 cells, 7 used
        assert_eq!(Item::Widget(0).duplicate(&mut grid, 128.0, area), None);
        // Too big for the window at all.
        let huge = rect(0.0, 0.0, 1024.0, 128.0);
        assert_eq!(
            free_spot_near(&GridLayout::default(), huge, 128.0, area),
            None
        );
    }

    #[test]
    fn removing_shifts_later_items() {
        let mut grid: GridLayout = serde_json::from_str(
            r#"{"containers": [
                {"title": "a", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"},
                {"title": "b", "x": 128.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
            ], "layouts": [
                {"title": "list", "x": 256.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "List Layout",
                 "children": [{"title": "c", "x": 0.0, "y": 0.0, "width": 128.0, "height": 64.0, "type": "Text Display"}]}
            ]}"#,
        )
        .unwrap();
        Item::Widget(0).remove(&mut grid);
        assert_eq!(Item::Widget(0).title(&grid), Some("b"));
        Item::Widget(7).remove(&mut grid);
        assert_eq!(grid.containers.len(), 1);
        let copy = Item::List(0)
            .duplicate(&mut grid, 128.0, (512.0, 128.0))
            .unwrap();
        assert_eq!(grid.layouts[1].children.len(), 1);
        Item::List(0).remove(&mut grid);
        assert_eq!(copy.title(&grid), None);
        assert_eq!(Item::List(0).title(&grid), Some("list"));
    }
}
