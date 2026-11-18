//! Placing widgets on a tab's snap grid.
//!
//! A tab is a grid of square cells (128 pixels by default). Widgets and List
//! Layouts are moved and resized in whole cells, and two of them may never
//! overlap. This module holds those rules without any drawing, so they can be
//! tested without a window. Positions and sizes are pixels at the layout's
//! grid size, as Elastic saves them.

use raptaboard_layout::{GridLayout, MAX_GRID_CELLS, Rect};

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

/// The List Layout on `grid` under the point (`x`, `y`) in grid pixels,
/// if any.
///
/// ```
/// use raptaboard::grid::list_at;
/// use raptaboard_layout::GridLayout;
/// let grid: GridLayout = serde_json::from_str(r#"{"layouts": [
///     {"title": "list", "x": 256.0, "y": 0.0, "width": 256.0, "height": 384.0,
///      "type": "List Layout"}]}"#).unwrap();
/// assert_eq!(list_at(&grid, (300.0, 100.0)), Some(0));
/// assert_eq!(list_at(&grid, (100.0, 100.0)), None);
/// ```
pub fn list_at(grid: &GridLayout, (x, y): (f64, f64)) -> Option<usize> {
    grid.layouts.iter().position(|l| {
        let r = l.rect;
        x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
    })
}

/// Move widget `widget` (an index in `containers`) into List Layout `list`,
/// as child number `at` (or last, if `at` is past the end). It keeps its
/// size. Returns its index among the list's children, or `None` (changing
/// nothing) if either doesn't exist.
///
/// Removing the widget shifts later widgets' indexes down by one.
pub fn widget_into_list(
    grid: &mut GridLayout,
    widget: usize,
    list: usize,
    at: usize,
) -> Option<usize> {
    if widget >= grid.containers.len() || list >= grid.layouts.len() {
        return None;
    }
    let child = grid.containers.remove(widget);
    let children = &mut grid.layouts[list].children;
    let at = at.min(children.len());
    children.insert(at, child);
    Some(at)
}

/// Move child `child` of List Layout `from` to List Layout `to`, as child
/// number `at` there (or last, if past the end). Within one list this
/// reorders: `at` is counted before the child is taken out, so dropping
/// a child just below itself leaves it where it is. Returns its new index,
/// or `None` (changing nothing) if any of them doesn't exist.
pub fn child_to_list(
    grid: &mut GridLayout,
    from: usize,
    child: usize,
    to: usize,
    at: usize,
) -> Option<usize> {
    if to >= grid.layouts.len() || child >= grid.layouts.get(from)?.children.len() {
        return None;
    }
    let moving = grid.layouts[from].children.remove(child);
    // Taking it out shifts everything after it up one in its own list.
    let at = if from == to && at > child { at - 1 } else { at };
    let children = &mut grid.layouts[to].children;
    let at = at.min(children.len());
    children.insert(at, moving);
    Some(at)
}

/// Take child `child` out of List Layout `list` and put it on the grid as a
/// widget at `rect`. The caller checks `rect` is free first ([`is_free`]).
/// Returns its index in `containers`, or `None` (changing nothing) if there
/// is no such child.
pub fn child_out_of_list(
    grid: &mut GridLayout,
    list: usize,
    child: usize,
    rect: Rect,
) -> Option<usize> {
    let children = &mut grid.layouts.get_mut(list)?.children;
    if child >= children.len() {
        return None;
    }
    let mut widget = children.remove(child);
    widget.rect = rect;
    grid.containers.push(widget);
    Some(grid.containers.len() - 1)
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
/// rectangle's top-left cell stays among the cells that show (counting a
/// last row or column the window cuts off part way), so it can always be
/// grabbed again; the rest may run past the right or bottom of the window,
/// so a big widget can show just its first row or column. It never goes
/// left of or above the grid, or past [`MAX_GRID_CELLS`] cells, which
/// opening a layout would undo.
///
/// ```
/// use raptaboard::grid::snap_move;
/// use raptaboard_layout::Rect;
/// let dropped = Rect { x: 200.0, y: -20.0, width: 128.0, height: 256.0 };
/// let placed = snap_move(dropped, 128.0, (1024.0, 768.0));
/// assert_eq!((placed.x, placed.y), (256.0, 0.0));
/// assert_eq!((placed.width, placed.height), (128.0, 256.0));
/// // Most of a tall widget can hang off the bottom: only its top row shows.
/// let low = Rect { y: 700.0, ..dropped };
/// assert_eq!(snap_move(low, 128.0, (1024.0, 768.0)).y, 640.0);
/// ```
pub fn snap_move(rect: Rect, cell: f64, area: (f64, f64)) -> Rect {
    let snap = |pos: f64, size: f64, room: f64| {
        let shown = (room / cell).ceil().max(1.0);
        // The top-left cell must show...
        let last_shown = (shown - 1.0) * cell;
        // ...and the far edge stay inside the layout's limit.
        let last_allowed = ((cell * MAX_GRID_CELLS - size) / cell).floor().max(0.0) * cell;
        ((pos / cell).round() * cell).clamp(0.0, last_shown.min(last_allowed))
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
    //
    // Everything here is written in grid cells and run at several grid
    // sizes, so the rules are checked to scale with the cell size rather
    // than only at Elastic's default of 128 pixels.
    use super::*;
    use raptaboard_layout::{LayoutContainer, WidgetContainer};
    use serde_json::Map;

    /// Grid sizes every test runs at, in pixels.
    const SIZES: [f64; 4] = [16.0, 64.0, 128.0, 256.0];

    /// A grid of one cell size, for writing positions in cells.
    #[derive(Clone, Copy)]
    struct G {
        cell: f64,
    }

    impl G {
        /// A rectangle at cell (`col`, `row`), `wide` by `high` cells.
        /// Fractions are allowed, for "dropped part way into a cell".
        fn at(self, col: f64, row: f64, wide: f64, high: f64) -> Rect {
            Rect {
                x: col * self.cell,
                y: row * self.cell,
                width: wide * self.cell,
                height: high * self.cell,
            }
        }

        /// A window `cols` by `rows` cells big, in pixels.
        fn window(self, cols: f64, rows: f64) -> (f64, f64) {
            (cols * self.cell, rows * self.cell)
        }

        /// A tab with these widgets, then these List Layouts (one child each).
        fn grid(self, widgets: &[(&str, Rect)], lists: &[(&str, Rect)]) -> GridLayout {
            let widget = |title: &str, rect: Rect| WidgetContainer {
                title: title.into(),
                rect,
                widget_type: "Text Display".into(),
                properties: Map::new(),
                extra: Map::new(),
            };
            GridLayout {
                containers: widgets.iter().map(|&(t, r)| widget(t, r)).collect(),
                layouts: lists
                    .iter()
                    .map(|&(t, r)| LayoutContainer {
                        title: t.into(),
                        rect: r,
                        layout_type: "List Layout".into(),
                        properties: Map::new(),
                        children: vec![widget("child", self.at(0.0, 0.0, 1.0, 0.5))],
                        extra: Map::new(),
                    })
                    .collect(),
                extra: Map::new(),
            }
        }
    }

    /// Run `test` once for each grid size in [`SIZES`].
    fn each_size(test: impl Fn(G)) {
        for cell in SIZES {
            test(G { cell });
        }
    }

    #[test]
    fn moves_snap_to_the_nearest_cell_and_stay_on_screen() {
        each_size(|g| {
            let window = g.window(5.0, 3.0);
            let snap = |r: Rect| snap_move(r, g.cell, window);
            // Just under half a cell rounds down, just over rounds up.
            let dropped = snap(g.at(0.49, 0.51, 1.0, 1.0));
            assert_eq!((dropped.x, dropped.y), (0.0, g.cell));
            // Dropped past the right edge: its first column stays in the
            // last column that shows, the rest off the window.
            assert_eq!(snap(g.at(4.7, 0.0, 2.0, 1.0)), g.at(4.0, 0.0, 2.0, 1.0));
            assert_eq!(snap(g.at(40.0, 0.0, 2.0, 1.0)), g.at(4.0, 0.0, 2.0, 1.0));
            // Taller than the window: it can still move down; only its top shows.
            assert_eq!(snap(g.at(0.0, 1.6, 1.0, 4.0)), g.at(0.0, 2.0, 1.0, 4.0));
            // Never left of or above the grid.
            assert_eq!(snap(g.at(-2.3, -2.3, 1.0, 1.0)), g.at(0.0, 0.0, 1.0, 1.0));

            // A window 7.8 cells tall shows 7 full rows and part of an 8th;
            // widgets can go in that last row.
            let window = g.window(8.0, 7.8);
            let low = snap_move(g.at(0.0, 7.4, 1.0, 1.0), g.cell, window);
            assert_eq!(low.y, 7.0 * g.cell);
            let low = snap_move(g.at(0.0, 7.4, 1.0, 2.0), g.cell, window);
            assert_eq!(low.y, 7.0 * g.cell);

            // A huge window can't push a widget past the layout's 100-cell
            // limit: a 100-cell-wide widget stays at the left edge, and a
            // 2-cell one stops with its right side on the limit.
            let huge = g.window(200.0, 8.0);
            let full = g.at(5.0, 0.0, MAX_GRID_CELLS, 1.0);
            assert_eq!(snap_move(full, g.cell, huge).x, 0.0);
            let far = snap_move(g.at(150.0, 0.0, 2.0, 1.0), g.cell, huge);
            assert_eq!(far.x, (MAX_GRID_CELLS - 2.0) * g.cell);
        });
    }

    #[test]
    fn resizes_snap_to_whole_cells() {
        each_size(|g| {
            let resize = |r: Rect| snap_resize(r, g.cell);
            // Rounded to the nearest whole cells; the corner doesn't move.
            assert_eq!(resize(g.at(1.0, 0.0, 3.52, 1.02)), g.at(1.0, 0.0, 4.0, 1.0));
            // A widget can't shrink below one cell.
            assert_eq!(resize(g.at(0.0, 0.0, -0.4, 0.08)), g.at(0.0, 0.0, 1.0, 1.0));
        });
    }

    #[test]
    fn list_layouts_block_widgets_too() {
        each_size(|g| {
            let a = g.at(0.0, 0.0, 1.0, 1.0);
            let list = g.at(2.0, 0.0, 2.0, 3.0);
            let mut grid = g.grid(&[("a", a)], &[("list", list)]);
            assert_eq!(
                items(&grid),
                vec![(Item::Widget(0), a), (Item::List(0), list)]
            );
            assert!(!fits(&grid, Item::Widget(0), g.at(3.0, 1.0, 1.0, 1.0)));
            // Something new can't go on the list, but can go beside it.
            assert!(!is_free(&grid, g.at(2.0, 1.0, 1.0, 1.0)));
            assert!(is_free(&grid, g.at(1.0, 0.0, 1.0, 1.0)));
            // An item never blocks itself: growing in place is fine.
            assert!(fits(&grid, Item::List(0), g.at(2.0, 0.0, 3.0, 3.0)));

            let moved = g.at(4.0, 0.0, 1.0, 1.0);
            Item::List(0).set_rect(&mut grid, moved);
            assert_eq!(Item::List(0).rect(&grid), Some(moved));
            assert_eq!(Item::Widget(5).rect(&grid), None);
        });
    }

    #[test]
    fn copies_go_to_the_nearest_free_spot() {
        each_size(|g| {
            let mut grid = g.grid(
                &[
                    ("a", g.at(1.0, 1.0, 2.0, 1.0)),
                    ("b", g.at(3.0, 1.0, 1.0, 1.0)),
                ],
                &[],
            );
            let window = g.window(4.0, 3.0);
            let copy = |grid: &mut GridLayout, item: Item| item.duplicate(grid, g.cell, window);
            // Right of "a" is taken by "b", so the copy goes just above "a".
            let first = copy(&mut grid, Item::Widget(0)).unwrap();
            assert_eq!(first.rect(&grid), Some(g.at(1.0, 0.0, 2.0, 1.0)));
            assert_eq!(first.title(&grid), Some("a"));
            // Then just below it.
            let second = copy(&mut grid, Item::Widget(0)).unwrap();
            assert_eq!(second.rect(&grid), Some(g.at(1.0, 2.0, 2.0, 1.0)));
            // Fill the rest; then there's no room left. 12 cells, 7 used.
            while copy(&mut grid, Item::Widget(1)).is_some() {}
            assert_eq!(grid.containers.len(), 4 + 5);
            assert_eq!(copy(&mut grid, Item::Widget(0)), None);
            // Too big for the window at all.
            let wide = g.at(0.0, 0.0, 8.0, 1.0);
            assert_eq!(
                free_spot_near(&GridLayout::default(), wide, g.cell, window),
                None
            );
        });
    }

    #[test]
    fn removing_shifts_later_items() {
        each_size(|g| {
            let mut grid = g.grid(
                &[
                    ("a", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("b", g.at(1.0, 0.0, 1.0, 1.0)),
                ],
                &[("list", g.at(2.0, 0.0, 1.0, 1.0))],
            );
            Item::Widget(0).remove(&mut grid);
            assert_eq!(Item::Widget(0).title(&grid), Some("b"));
            Item::Widget(7).remove(&mut grid);
            assert_eq!(grid.containers.len(), 1);
            let copy = Item::List(0)
                .duplicate(&mut grid, g.cell, g.window(4.0, 1.0))
                .unwrap();
            assert_eq!(grid.layouts[1].children.len(), 1);
            Item::List(0).remove(&mut grid);
            assert_eq!(copy.title(&grid), None);
            assert_eq!(Item::List(0).title(&grid), Some("list"));
        });
    }

    #[test]
    fn widgets_go_into_lists_and_back_out() {
        each_size(|g| {
            let mut grid = g.grid(
                &[
                    ("a", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("b", g.at(1.0, 0.0, 1.0, 2.0)),
                ],
                &[("list", g.at(3.0, 0.0, 2.0, 4.0))],
            );
            let titles = |grid: &GridLayout| -> Vec<String> {
                grid.layouts[0]
                    .children
                    .iter()
                    .map(|c| c.title.clone())
                    .collect()
            };
            // Over the list, not over the widgets.
            assert_eq!(list_at(&grid, (3.5 * g.cell, 3.9 * g.cell)), Some(0));
            assert_eq!(list_at(&grid, (5.0 * g.cell, 0.0)), None);

            // "b" goes in first, keeping its size; "a" goes past the end, so last.
            assert_eq!(widget_into_list(&mut grid, 1, 0, 0), Some(0));
            assert_eq!(widget_into_list(&mut grid, 0, 0, 99), Some(2));
            assert_eq!(titles(&grid), ["b", "child", "a"]);
            assert!(grid.containers.is_empty());
            assert_eq!(grid.layouts[0].children[0].rect.height, 2.0 * g.cell);
            assert_eq!(widget_into_list(&mut grid, 0, 0, 0), None);

            // Reordering: "a" up to the top; "b" dropped just below itself stays.
            assert_eq!(child_to_list(&mut grid, 0, 2, 0, 0), Some(0));
            assert_eq!(titles(&grid), ["a", "b", "child"]);
            assert_eq!(child_to_list(&mut grid, 0, 1, 0, 2), Some(1));
            assert_eq!(titles(&grid), ["a", "b", "child"]);
            // Moving down: "a" dropped below "child" (at 3) ends up last.
            assert_eq!(child_to_list(&mut grid, 0, 0, 0, 3), Some(2));
            assert_eq!(titles(&grid), ["b", "child", "a"]);
            assert_eq!(child_to_list(&mut grid, 0, 9, 0, 0), None);
            assert_eq!(child_to_list(&mut grid, 0, 0, 4, 0), None);

            // Out onto the grid, wherever the caller says.
            let spot = g.at(0.0, 2.0, 1.0, 1.0);
            let out = child_out_of_list(&mut grid, 0, 2, spot).unwrap();
            assert_eq!(grid.containers[out].title, "a");
            assert_eq!(grid.containers[out].rect, spot);
            assert_eq!(titles(&grid), ["b", "child"]);
            assert_eq!(child_out_of_list(&mut grid, 0, 5, spot), None);
        });
    }

    #[test]
    fn children_move_between_lists() {
        each_size(|g| {
            let mut grid = g.grid(
                &[],
                &[
                    ("one", g.at(0.0, 0.0, 2.0, 3.0)),
                    ("two", g.at(2.0, 0.0, 2.0, 3.0)),
                ],
            );
            assert_eq!(child_to_list(&mut grid, 0, 0, 1, 0), Some(0));
            assert!(grid.layouts[0].children.is_empty());
            assert_eq!(grid.layouts[1].children.len(), 2);
        });
    }
}
