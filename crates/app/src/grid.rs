//! Placing widgets on a tab's snap grid.
//!
//! A tab is a grid of square cells (128 pixels by default). Widgets and List
//! Layouts are moved and resized in whole cells, and two of them may never
//! overlap: one moved or resized onto others pushes them aside to the
//! nearest free spots ([`reflow`]). A widget can grow into the empty cells
//! around it ([`fill_space`]), and a tab's widgets can slide up and left to
//! close gaps ([`pack`]). A pinned item ([`PINNED`]) is never moved by any
//! of these, only by being dragged itself. This module holds those rules without any drawing, so they can be
//! tested without a window. Positions and sizes are pixels at the layout's
//! grid size, as Elastic saves them.

use raptaboard_layout::{GridLayout, MAX_GRID_CELLS, Rect};

use crate::widgets::list_layout::{NEW_LIST_CELLS, cells_tall_for, labels, new_list};

/// The setting that pins a widget or List Layout in place (raptaboard's,
/// `true` or unset): pushing aside and packing leave it where it is.
pub const PINNED: &str = "pinned";

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
    /// Whether this item is pinned in place ([`PINNED`]).
    pub fn pinned(self, grid: &GridLayout) -> bool {
        let props = match self {
            Item::Widget(i) => grid.containers.get(i).map(|w| &w.properties),
            Item::List(i) => grid.layouts.get(i).map(|l| &l.properties),
        };
        props.and_then(|p| p.get(PINNED)).and_then(|v| v.as_bool()) == Some(true)
    }

    /// Pin this item in place, or unpin it (the setting removed).
    pub fn set_pinned(self, grid: &mut GridLayout, pinned: bool) {
        let props = match self {
            Item::Widget(i) => grid.containers.get_mut(i).map(|w| &mut w.properties),
            Item::List(i) => grid.layouts.get_mut(i).map(|l| &mut l.properties),
        };
        if let Some(props) = props {
            if pinned {
                props.insert(PINNED.into(), true.into());
            } else {
                props.remove(PINNED);
            }
        }
    }

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
/// widget at `rect`. The caller makes room there first ([`make_room`]).
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

/// Add an empty List Layout ([`NEW_LIST_CELLS`] cells each way) in the
/// free spot nearest the cell at (`x`, `y`) grid pixels, within `area` (the
/// visible grid's width and height in pixels). Returns its index in
/// `layouts`, or `None` if there's no room.
///
/// ```
/// use raptaboard::grid::add_list;
/// use raptaboard_layout::GridLayout;
/// let mut grid = GridLayout::default();
/// assert_eq!(add_list(&mut grid, (300.0, 10.0), 128.0, (1024.0, 512.0)), Some(0));
/// let r = grid.layouts[0].rect;
/// assert_eq!((r.x, r.y, r.width, r.height), (256.0, 0.0, 256.0, 256.0));
/// ```
pub fn add_list(
    grid: &mut GridLayout,
    at: (f64, f64),
    cell: f64,
    area: (f64, f64),
) -> Option<usize> {
    add_list_holding(grid, at, Vec::new(), cell, area)
}

/// Like [`add_list`], but the new list holds `children` and is tall
/// enough to show them all ([`cells_tall_for`]). `None` (and the children
/// are dropped) if there's no room.
pub fn add_list_holding(
    grid: &mut GridLayout,
    (x, y): (f64, f64),
    children: Vec<raptaboard_layout::WidgetContainer>,
    cell: f64,
    area: (f64, f64),
) -> Option<usize> {
    let mut list = new_list(
        Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        },
        children,
    );
    let tall = cells_tall_for(&list.children, labels(&list.properties), cell);
    let at = Rect {
        x: (x / cell).floor() * cell,
        y: (y / cell).floor() * cell,
        width: NEW_LIST_CELLS * cell,
        height: tall * cell,
    };
    list.rect = free_spot_near(grid, at, cell, area)?;
    grid.layouts.push(list);
    Some(grid.layouts.len() - 1)
}

/// Put widget `widget` (an index in `containers`) into a new List Layout of
/// its own, in its place: as wide as the widget and a cell taller (room for
/// the label), at least [`NEW_LIST_CELLS`] cells each way. If that isn't
/// free, the list goes to the nearest free spot within `area`. Returns the
/// list's index in `layouts`, or `None` (changing nothing) if there's no
/// room or no such widget.
///
/// Removing the widget shifts later widgets' indexes down by one.
pub fn wrap_in_list(
    grid: &mut GridLayout,
    widget: usize,
    cell: f64,
    area: (f64, f64),
) -> Option<usize> {
    let w = grid.containers.get(widget)?.rect;
    let min = NEW_LIST_CELLS * cell;
    let wanted = Rect {
        width: w.width.max(min),
        height: (w.height + cell).max(min),
        ..w
    };
    let child = grid.containers.remove(widget);
    let spot = if is_free(grid, wanted) {
        Some(wanted)
    } else {
        free_spot_near(grid, wanted, cell, area)
    };
    let Some(spot) = spot else {
        grid.containers.insert(widget, child);
        return None;
    };
    grid.layouts.push(new_list(spot, vec![child]));
    Some(grid.layouts.len() - 1)
}

/// Put new `widgets` on `grid`, each in the free spot nearest the cell
/// corner `near` (grid pixels), in order, within `area` (the visible grid's
/// width and height in pixels). Each keeps its size. Stops when there's no
/// room; returns the new widgets' indexes in `containers`.
///
/// ```
/// use raptaboard::grid::place_near;
/// use raptaboard::widgets::for_topic;
/// use raptaboard_layout::{GridLayout, Rect};
/// let one = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let new = ["/a", "/b"].map(|t| for_topic(t, "double", None, one).unwrap());
/// let mut grid = GridLayout::default();
/// assert_eq!(place_near(&mut grid, new.to_vec(), (128.0, 0.0), 128.0, (512.0, 512.0)), [0, 1]);
/// assert_eq!(grid.containers[0].rect.x, 128.0);
/// ```
pub fn place_near(
    grid: &mut GridLayout,
    widgets: Vec<raptaboard_layout::WidgetContainer>,
    near: (f64, f64),
    cell: f64,
    area: (f64, f64),
) -> Vec<usize> {
    let mut placed = Vec::new();
    for mut widget in widgets {
        let want = Rect {
            x: near.0,
            y: near.1,
            ..widget.rect
        };
        let Some(spot) = free_spot_near(grid, want, cell, area) else {
            break;
        };
        widget.rect = spot;
        grid.containers.push(widget);
        placed.push(grid.containers.len() - 1);
    }
    placed
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

/// Where things go if `item` is put at `target`, moving whatever is in the
/// way: `item` at `target`, then each item it would cover at the free spot
/// nearest where it was (on whole cells inside `area`, the visible grid's
/// width and height in pixels). Bigger items in the way are placed first,
/// as they're the hardest to fit. Only the items that move are listed.
/// `None` if one of them has nowhere to go.
///
/// ```
/// use raptaboard::grid::{reflow, Item};
/// use raptaboard_layout::{GridLayout, Rect};
/// let grid: GridLayout = serde_json::from_str(r#"{"containers": [
///     {"title": "a", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"},
///     {"title": "b", "x": 128.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
/// ]}"#).unwrap();
/// // "a" moved onto "b": "b" goes to the nearest free spot, where "a" was.
/// let onto_b = Rect { x: 128.0, y: 0.0, width: 128.0, height: 128.0 };
/// let moves = reflow(&grid, Item::Widget(0), onto_b, 128.0, (256.0, 128.0)).unwrap();
/// assert_eq!(moves[0], (Item::Widget(0), onto_b));
/// assert_eq!(moves[1], (Item::Widget(1), Rect { x: 0.0, ..onto_b }));
/// ```
pub fn reflow(
    grid: &GridLayout,
    item: Item,
    target: Rect,
    cell: f64,
    area: (f64, f64),
) -> Option<Vec<(Item, Rect)>> {
    let mut placed = grid.clone();
    item.rect(&placed)?;
    item.set_rect(&mut placed, target);
    let pushed = push_aside(&mut placed, target, Some(item), cell, area)?;
    Some(std::iter::once((item, target)).chain(pushed).collect())
}

/// Make room at `target` for something new (a widget dropped from the tree,
/// or a list child dragged out): each item it would cover goes to the free
/// spot nearest where it was, as [`reflow`] does. Returns only the items
/// that move, or `None` if one has nowhere to go.
///
/// ```
/// use raptaboard::grid::{make_room, Item};
/// use raptaboard_layout::{GridLayout, Rect};
/// let grid: GridLayout = serde_json::from_str(r#"{"containers": [
///     {"title": "a", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
/// ]}"#).unwrap();
/// let cell = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let moves = make_room(&grid, cell, 128.0, (256.0, 128.0)).unwrap();
/// assert_eq!(moves, [(Item::Widget(0), Rect { x: 128.0, ..cell })]);
/// ```
pub fn make_room(
    grid: &GridLayout,
    target: Rect,
    cell: f64,
    area: (f64, f64),
) -> Option<Vec<(Item, Rect)>> {
    let mut placed = grid.clone();
    // The new thing holds its place while the others are placed: a widget
    // standing in for it.
    placed.containers.push(raptaboard_layout::WidgetContainer {
        title: String::new(),
        rect: target,
        widget_type: String::new(),
        properties: Default::default(),
        extra: Default::default(),
    });
    let new = Item::Widget(placed.containers.len() - 1);
    push_aside(&mut placed, target, Some(new), cell, area)
}

/// Move every item on `placed` that overlaps `target` (but `keep`) to the
/// free spot nearest where it was, biggest first, and say where each went.
/// `None` if one has nowhere to go, or is pinned.
fn push_aside(
    placed: &mut GridLayout,
    target: Rect,
    keep: Option<Item>,
    cell: f64,
    area: (f64, f64),
) -> Option<Vec<(Item, Rect)>> {
    let mut in_the_way: Vec<(Item, Rect)> = items(placed)
        .into_iter()
        .filter(|&(other, r)| Some(other) != keep && target.overlaps(&r))
        .collect();
    if in_the_way.iter().any(|&(other, _)| other.pinned(placed)) {
        return None;
    }
    // Biggest first; equal sizes in the order they're on the tab.
    in_the_way.sort_by(|a, b| (b.1.width * b.1.height).total_cmp(&(a.1.width * a.1.height)));
    // Off the grid while they're placed, so none blocks another.
    for &(other, r) in &in_the_way {
        let away = Rect {
            x: -r.width - 1.0e9,
            y: -r.height - 1.0e9,
            ..r
        };
        other.set_rect(placed, away);
    }
    let mut moves = Vec::new();
    for (other, was) in in_the_way {
        let spot = free_spot_near(placed, was, cell, area)?;
        other.set_rect(placed, spot);
        moves.push((other, spot));
    }
    Some(moves)
}

/// Which whole cells are taken, by every item but `except`: `taken[row][col]`
/// for the first `cols` by `rows` cells.
fn taken_cells(
    grid: &GridLayout,
    except: Option<Item>,
    cell: f64,
    cols: usize,
    rows: usize,
) -> Vec<Vec<bool>> {
    let mut taken = vec![vec![false; cols]; rows];
    for (item, r) in items(grid) {
        if Some(item) == except {
            continue;
        }
        let from = |pos: f64| (pos / cell).floor().max(0.0) as usize;
        let to = |end: f64| (end / cell).ceil().max(0.0) as usize;
        for row in taken
            .iter_mut()
            .take(to(r.y + r.height).min(rows))
            .skip(from(r.y))
        {
            for c in row
                .iter_mut()
                .take(to(r.x + r.width).min(cols))
                .skip(from(r.x))
            {
                *c = true;
            }
        }
    }
    taken
}

/// Where `item` goes if it grows to fill the empty cells around it: the
/// biggest rectangle of free whole cells holding its own, within the cells
/// of `area` (the visible grid, in pixels) that show in full. Ties keep its
/// top-left corner if they can, then the higher, then the further left.
/// `None` if it can't grow (or doesn't exist).
///
/// ```
/// use raptaboard::grid::{fill_space, Item};
/// use raptaboard_layout::{GridLayout, Rect};
/// let grid: GridLayout = serde_json::from_str(r#"{"containers": [
///     {"title": "a", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0, "type": "Text Display"},
///     {"title": "b", "x": 384.0, "y": 0.0, "width": 128.0, "height": 256.0, "type": "Text Display"}
/// ]}"#).unwrap();
/// // Four cells across, two down: "a" grows to fill the three columns left of "b".
/// let grown = fill_space(&grid, Item::Widget(0), 128.0, (512.0, 256.0)).unwrap();
/// assert_eq!((grown.width, grown.height), (384.0, 256.0));
/// ```
pub fn fill_space(grid: &GridLayout, item: Item, cell: f64, area: (f64, f64)) -> Option<Rect> {
    let own = item.rect(grid)?;
    let cols = ((area.0 / cell).floor() as usize).min(MAX_GRID_CELLS as usize);
    let rows = ((area.1 / cell).floor() as usize).min(MAX_GRID_CELLS as usize);
    let taken = taken_cells(grid, Some(item), cell, cols, rows);
    // Free cells above and left of each corner, for checking a rectangle
    // in one step: free[r][c] counts cells in rows < r and columns < c.
    let mut free = vec![vec![0usize; cols + 1]; rows + 1];
    for r in 0..rows {
        for c in 0..cols {
            free[r + 1][c + 1] =
                free[r][c + 1] + free[r + 1][c] - free[r][c] + usize::from(!taken[r][c]);
        }
    }
    let all_free = |top: usize, left: usize, bottom: usize, right: usize| {
        let count = free[bottom][right] + free[top][left] - free[top][right] - free[bottom][left];
        count == (bottom - top) * (right - left)
    };
    // Its own cells (it must fit on screen to grow).
    let left0 = (own.x / cell).round() as usize;
    let top0 = (own.y / cell).round() as usize;
    let right0 = left0 + ((own.width / cell).round() as usize).max(1);
    let bottom0 = top0 + ((own.height / cell).round() as usize).max(1);
    if right0 > cols || bottom0 > rows || !all_free(top0, left0, bottom0, right0) {
        return None;
    }
    // How good a fill is, best highest: its area, then keeping the corner,
    // then higher up, then further left.
    type Rank = (
        usize,
        bool,
        std::cmp::Reverse<usize>,
        std::cmp::Reverse<usize>,
    );
    let mut best: Option<(Rank, Rect)> = None;
    for top in (0..=top0).rev() {
        for left in (0..=left0).rev() {
            if !all_free(top, left, bottom0, right0) {
                continue;
            }
            for bottom in bottom0..=rows {
                if !all_free(top, left, bottom, right0) {
                    break;
                }
                for right in right0..=cols {
                    if !all_free(top, left, bottom, right) {
                        break;
                    }
                    let area = (bottom - top) * (right - left);
                    let key = (
                        area,
                        top == top0 && left == left0,
                        std::cmp::Reverse(top),
                        std::cmp::Reverse(left),
                    );
                    if best.as_ref().is_none_or(|(k, _)| key > *k) {
                        let rect = Rect {
                            x: left as f64 * cell,
                            y: top as f64 * cell,
                            width: (right - left) as f64 * cell,
                            height: (bottom - top) as f64 * cell,
                        };
                        best = Some((key, rect));
                    }
                }
            }
        }
    }
    best.map(|(_, r)| r).filter(|r| *r != own)
}

/// Close the gaps on `grid`: in reading order (top to bottom, then left to
/// right), each item slides up as far as it can, then left as far as it
/// can, a whole cell at a time, until nothing more moves. Sizes and the
/// arrangement stay; pinned items stay put and others pack around them.
/// Returns where each item that moved goes.
///
/// ```
/// use raptaboard::grid::{pack, Item};
/// use raptaboard_layout::GridLayout;
/// let grid: GridLayout = serde_json::from_str(r#"{"containers": [
///     {"title": "a", "x": 256.0, "y": 256.0, "width": 128.0, "height": 128.0, "type": "Text Display"}
/// ]}"#).unwrap();
/// let moves = pack(&grid, 128.0);
/// assert_eq!(moves.len(), 1);
/// assert_eq!((moves[0].1.x, moves[0].1.y), (0.0, 0.0));
/// ```
pub fn pack(grid: &GridLayout, cell: f64) -> Vec<(Item, Rect)> {
    let mut placed = grid.clone();
    let mut order: Vec<(Item, Rect)> = items(&placed)
        .into_iter()
        .filter(|&(item, _)| !item.pinned(&placed))
        .collect();
    order.sort_by(|a, b| {
        (a.1.y, a.1.x)
            .partial_cmp(&(b.1.y, b.1.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Each pass can open room for one already done; stop when still.
    loop {
        let mut moved = false;
        for &(item, _) in &order {
            let Some(mut r) = item.rect(&placed) else {
                continue;
            };
            for (dx, dy) in [(0.0, -cell), (-cell, 0.0)] {
                loop {
                    let next = Rect {
                        x: r.x + dx,
                        y: r.y + dy,
                        ..r
                    };
                    if next.x < 0.0 || next.y < 0.0 || !fits(&placed, item, next) {
                        break;
                    }
                    r = next;
                    item.set_rect(&mut placed, r);
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    items(&placed)
        .into_iter()
        .filter(|&(item, r)| item.rect(grid) != Some(r))
        .collect()
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
    fn a_move_onto_others_pushes_them_to_the_nearest_free_spots() {
        each_size(|g| {
            // a b c
            // . . .
            let grid = g.grid(
                &[
                    ("a", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("b", g.at(1.0, 0.0, 1.0, 1.0)),
                    ("c", g.at(2.0, 0.0, 1.0, 1.0)),
                ],
                &[],
            );
            let window = g.window(3.0, 2.0);
            let flow = |grid: &GridLayout, item, to| reflow(grid, item, to, g.cell, window);
            // Nothing in the way: just the move.
            let below = g.at(0.0, 1.0, 1.0, 1.0);
            assert_eq!(
                flow(&grid, Item::Widget(0), below),
                Some(vec![(Item::Widget(0), below)])
            );
            // "a" onto "b": "b" takes the free spot nearest it, where "a" was
            // (as near as the one below it; the higher wins).
            let onto_b = g.at(1.0, 0.0, 1.0, 1.0);
            let moves = flow(&grid, Item::Widget(0), onto_b).unwrap();
            assert_eq!(
                moves,
                vec![
                    (Item::Widget(0), onto_b),
                    (Item::Widget(1), g.at(0.0, 0.0, 1.0, 1.0)),
                ]
            );
            // "a" grown to two cells wide covers "b": "b" goes below.
            let wide = g.at(0.0, 0.0, 2.0, 1.0);
            let moves = flow(&grid, Item::Widget(0), wide).unwrap();
            assert_eq!(moves[1], (Item::Widget(1), g.at(1.0, 1.0, 1.0, 1.0)));
            // Over "b" and "c" at once: neither lands on the other.
            let row = g.at(1.0, 0.0, 2.0, 1.0);
            let moves = flow(&grid, Item::Widget(0), row).unwrap();
            let mut after = grid.clone();
            for &(item, r) in &moves {
                item.set_rect(&mut after, r);
            }
            let rects: Vec<Rect> = items(&after).into_iter().map(|(_, r)| r).collect();
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(!a.overlaps(b), "{a:?} on {b:?}");
                }
            }
            assert_eq!(moves.len(), 3);
            // Partly covered, a tall widget slides down past its own old
            // place (which no longer counts as taken).
            let tall = g.grid(
                &[
                    ("a", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("t", g.at(1.0, 0.0, 1.0, 2.0)),
                    ("x", g.at(0.0, 1.0, 1.0, 1.0)),
                ],
                &[],
            );
            let moves = reflow(
                &tall,
                Item::Widget(0),
                g.at(1.0, 0.0, 1.0, 1.0),
                g.cell,
                g.window(2.0, 3.0),
            )
            .unwrap();
            assert_eq!(moves[1], (Item::Widget(1), g.at(1.0, 1.0, 1.0, 2.0)));
            // Room for something new on "b": "b" moves, the rest stay.
            let new = g.at(1.0, 0.0, 1.0, 1.0);
            let moves = make_room(&grid, new, g.cell, window).unwrap();
            assert_eq!(moves, vec![(Item::Widget(1), g.at(1.0, 1.0, 1.0, 1.0))]);
            assert_eq!(make_room(&grid, below, g.cell, window), Some(vec![]));
            // No room for a full window's worth.
            assert_eq!(
                make_room(&grid, g.at(0.0, 0.0, 3.0, 2.0), g.cell, window),
                None
            );
            // Grown over everything with nowhere left: refused.
            assert_eq!(flow(&grid, Item::Widget(0), g.at(0.0, 0.0, 3.0, 2.0)), None);
            // An item that isn't there: nothing.
            assert_eq!(flow(&grid, Item::Widget(9), below), None);
        });
    }

    #[test]
    fn bigger_items_in_the_way_are_placed_first() {
        each_size(|g| {
            // m L . x      m: grown to two by two, over the list L and "s".
            // s L . .      Free after: the third column and the bottom right.
            let grid = g.grid(
                &[
                    ("m", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("s", g.at(0.0, 1.0, 1.0, 1.0)),
                    ("x", g.at(3.0, 0.0, 1.0, 1.0)),
                ],
                &[("L", g.at(1.0, 0.0, 1.0, 2.0))],
            );
            let window = g.window(4.0, 2.0);
            let grown = g.at(0.0, 0.0, 2.0, 2.0);
            let moves = reflow(&grid, Item::Widget(0), grown, g.cell, window).unwrap();
            // The list first, to the only column tall enough; then "s" to
            // the bottom right. "s" first would take the third column's
            // bottom, leaving the list nowhere.
            assert_eq!(
                moves,
                vec![
                    (Item::Widget(0), grown),
                    (Item::List(0), g.at(2.0, 0.0, 1.0, 2.0)),
                    (Item::Widget(1), g.at(3.0, 1.0, 1.0, 1.0)),
                ]
            );
        });
    }

    #[test]
    fn a_widget_fills_the_biggest_empty_space_around_it() {
        each_size(|g| {
            // . . . . b        "a" at (1, 1); "b" in the last column; "c"
            // . a . . b        under "a"'s right. The window is 5 by 3 and
            // . . c . .        a bit (the part-shown row doesn't count).
            let grid = g.grid(
                &[
                    ("a", g.at(1.0, 1.0, 1.0, 1.0)),
                    ("b", g.at(4.0, 0.0, 1.0, 2.0)),
                    ("c", g.at(2.0, 2.0, 1.0, 1.0)),
                ],
                &[],
            );
            let window = g.window(5.0, 3.4);
            let fill = |grid: &GridLayout, item| fill_space(grid, item, g.cell, window);
            // The biggest: four across the top two rows, left of "b".
            assert_eq!(fill(&grid, Item::Widget(0)), Some(g.at(0.0, 0.0, 4.0, 2.0)));
            // "c" can't take "a"'s cell: the two columns above and beside it.
            assert_eq!(fill(&grid, Item::Widget(2)), Some(g.at(2.0, 0.0, 2.0, 3.0)));
            // Hemmed in: no growing.
            let boxed = g.grid(
                &[
                    ("a", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("b", g.at(1.0, 0.0, 1.0, 1.0)),
                    ("c", g.at(0.0, 1.0, 2.0, 1.0)),
                ],
                &[],
            );
            assert_eq!(
                fill_space(&boxed, Item::Widget(0), g.cell, g.window(2.0, 2.0)),
                None
            );
            // Off screen: can't grow; missing: nothing.
            assert_eq!(fill(&grid, Item::Widget(9)), None);
            let far = g.grid(&[("a", g.at(6.0, 0.0, 1.0, 1.0))], &[]);
            assert_eq!(fill(&far, Item::Widget(0)), None);
        });
    }

    #[test]
    fn equal_fills_keep_the_corner() {
        each_size(|g| {
            // "a" in the middle of an empty 3 by 1 row could take the whole
            // row, the only biggest; in a 2 by 2 square from (0, 0), the
            // biggest holding it keeps its corner.
            let grid = g.grid(&[("a", g.at(1.0, 0.0, 1.0, 1.0))], &[]);
            assert_eq!(
                fill_space(&grid, Item::Widget(0), g.cell, g.window(3.0, 1.0)),
                Some(g.at(0.0, 0.0, 3.0, 1.0))
            );
            let grid = g.grid(&[("a", g.at(1.0, 1.0, 1.0, 1.0))], &[]);
            // 2 by 2 window: the whole window, whatever the corner.
            assert_eq!(
                fill_space(&grid, Item::Widget(0), g.cell, g.window(2.0, 2.0)),
                Some(g.at(0.0, 0.0, 2.0, 2.0))
            );
            // Two equal biggest: up (moving its corner) or right (keeping it).
            // x .      ". x" above, "a ." below with the right taken: it
            // a . x    grows right, keeping its corner.
            let grid = g.grid(
                &[
                    ("a", g.at(0.0, 1.0, 1.0, 1.0)),
                    ("x", g.at(1.0, 0.0, 1.0, 1.0)),
                    ("y", g.at(2.0, 1.0, 1.0, 1.0)),
                ],
                &[],
            );
            assert_eq!(
                fill_space(&grid, Item::Widget(0), g.cell, g.window(3.0, 2.0)),
                Some(g.at(0.0, 1.0, 2.0, 1.0))
            );
            // 3 by 2 with the first column taken: the free 2 by 2 holding it.
            let grid = g.grid(
                &[
                    ("a", g.at(1.0, 0.0, 1.0, 1.0)),
                    ("x", g.at(0.0, 0.0, 1.0, 2.0)),
                ],
                &[],
            );
            assert_eq!(
                fill_space(&grid, Item::Widget(0), g.cell, g.window(3.0, 2.0)),
                Some(g.at(1.0, 0.0, 2.0, 2.0))
            );
        });
    }

    #[test]
    fn packing_slides_up_then_left_around_pinned_items() {
        each_size(|g| {
            // . . . .      "a" slides up to row 0, then left to column 0;
            // . p a .      "b" up, then left until "a". "p" is pinned: "c"
            // . c . b      can't go up past it, so goes left, then up.
            let mut grid = g.grid(
                &[
                    ("a", g.at(2.0, 1.0, 1.0, 1.0)),
                    ("p", g.at(1.0, 1.0, 1.0, 1.0)),
                    ("b", g.at(3.0, 2.0, 1.0, 1.0)),
                    ("c", g.at(1.0, 2.0, 1.0, 1.0)),
                ],
                &[],
            );
            Item::Widget(1).set_pinned(&mut grid, true);
            assert!(Item::Widget(1).pinned(&grid));
            let moves = pack(&grid, g.cell);
            let mut after = grid.clone();
            for &(item, r) in &moves {
                item.set_rect(&mut after, r);
            }
            assert_eq!(Item::Widget(0).rect(&after), Some(g.at(0.0, 0.0, 1.0, 1.0)));
            assert_eq!(Item::Widget(1).rect(&after), Some(g.at(1.0, 1.0, 1.0, 1.0)));
            assert_eq!(Item::Widget(2).rect(&after), Some(g.at(1.0, 0.0, 1.0, 1.0)));
            assert_eq!(Item::Widget(3).rect(&after), Some(g.at(0.0, 1.0, 1.0, 1.0)));
            assert!(moves.iter().all(|&(item, _)| item != Item::Widget(1)));
            // Packed already: nothing moves.
            assert!(pack(&after, g.cell).is_empty());
            // Unpinned, the setting goes.
            Item::Widget(1).set_pinned(&mut grid, false);
            assert!(!grid.containers[1].properties.contains_key(PINNED));
        });
    }

    #[test]
    fn pinned_items_are_never_pushed_aside() {
        each_size(|g| {
            let mut grid = g.grid(
                &[
                    ("a", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("p", g.at(1.0, 0.0, 1.0, 1.0)),
                ],
                &[("list", g.at(2.0, 0.0, 1.0, 1.0))],
            );
            let window = g.window(3.0, 2.0);
            let onto_p = g.at(1.0, 0.0, 1.0, 1.0);
            assert!(reflow(&grid, Item::Widget(0), onto_p, g.cell, window).is_some());
            Item::Widget(1).set_pinned(&mut grid, true);
            assert_eq!(reflow(&grid, Item::Widget(0), onto_p, g.cell, window), None);
            assert_eq!(make_room(&grid, onto_p, g.cell, window), None);
            // A pinned list too.
            Item::List(0).set_pinned(&mut grid, true);
            let onto_list = g.at(2.0, 0.0, 1.0, 1.0);
            assert_eq!(make_room(&grid, onto_list, g.cell, window), None);
            // A pinned item can still be dragged itself.
            let free = g.at(1.0, 1.0, 1.0, 1.0);
            assert!(reflow(&grid, Item::Widget(1), free, g.cell, window).is_some());
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

    #[test]
    fn new_lists_go_near_the_click() {
        each_size(|g| {
            let mut grid = g.grid(&[("a", g.at(0.0, 0.0, 1.0, 1.0))], &[]);
            let window = g.window(5.0, 3.0);
            // Clicked in cell (0, 0), which is taken: the nearest free 2 x 2.
            let list = add_list(&mut grid, (0.5 * g.cell, 0.5 * g.cell), g.cell, window).unwrap();
            assert_eq!(grid.layouts[list].rect, g.at(1.0, 0.0, 2.0, 2.0));
            assert!(grid.layouts[list].children.is_empty());
            // Clicked in free cell (3, 1): it starts there.
            let next = add_list(&mut grid, (3.2 * g.cell, 1.9 * g.cell), g.cell, window).unwrap();
            assert_eq!(grid.layouts[next].rect, g.at(3.0, 1.0, 2.0, 2.0));
            // The window is full now.
            assert_eq!(add_list(&mut grid, (0.0, 0.0), g.cell, window), None);
        });
    }

    #[test]
    fn widgets_wrap_into_a_list_in_their_place() {
        each_size(|g| {
            let mut grid = g.grid(
                &[
                    ("small", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("wide", g.at(2.0, 0.0, 3.0, 1.0)),
                ],
                &[],
            );
            let window = g.window(6.0, 4.0);
            // 1 x 1 becomes a 2 x 2 list where it was.
            let list = wrap_in_list(&mut grid, 0, g.cell, window).unwrap();
            assert_eq!(grid.layouts[list].rect, g.at(0.0, 0.0, 2.0, 2.0));
            assert_eq!(grid.layouts[list].children[0].title, "small");
            // "wide" is now widget 0: 3 wide, and a cell taller.
            let list = wrap_in_list(&mut grid, 0, g.cell, window).unwrap();
            assert_eq!(grid.layouts[list].rect, g.at(2.0, 0.0, 3.0, 2.0));
            assert!(grid.containers.is_empty());
            assert_eq!(wrap_in_list(&mut grid, 0, g.cell, window), None);
        });
    }

    #[test]
    fn wrapping_with_no_room_changes_nothing() {
        each_size(|g| {
            // Neighbours on two sides block its own spot growing to 2 x 2, and
            // a 2 x 2 window has nowhere else.
            let mut grid = g.grid(
                &[
                    ("a", g.at(0.0, 0.0, 1.0, 1.0)),
                    ("right", g.at(1.0, 0.0, 1.0, 1.0)),
                    ("below", g.at(0.0, 1.0, 1.0, 1.0)),
                ],
                &[],
            );
            let before = grid.clone();
            assert_eq!(wrap_in_list(&mut grid, 0, g.cell, g.window(2.0, 2.0)), None);
            assert_eq!(grid, before);
        });
    }

    #[test]
    fn several_new_widgets_fill_in_around_the_drop() {
        each_size(|g| {
            let one = g.at(0.0, 0.0, 1.0, 1.0);
            let new = |n: usize| -> Vec<raptaboard_layout::WidgetContainer> {
                (0..n)
                    .map(|i| {
                        crate::widgets::for_topic(&format!("/t{i}"), "double", None, one).unwrap()
                    })
                    .collect()
            };
            let mut grid = g.grid(&[("taken", g.at(1.0, 1.0, 1.0, 1.0))], &[]);
            let window = g.window(3.0, 2.0);
            // Dropped on the taken cell: they fill the nearest free ones.
            let placed = place_near(&mut grid, new(3), (g.cell, g.cell), g.cell, window);
            assert_eq!(placed, [1, 2, 3]);
            let spots: Vec<Rect> = placed.iter().map(|&i| grid.containers[i].rect).collect();
            assert_eq!(
                spots,
                [
                    g.at(1.0, 0.0, 1.0, 1.0),
                    g.at(0.0, 1.0, 1.0, 1.0),
                    g.at(2.0, 1.0, 1.0, 1.0)
                ]
            );
            // Two cells left; three more only place two.
            assert_eq!(
                place_near(&mut grid, new(3), (0.0, 0.0), g.cell, window).len(),
                2
            );
            assert!(place_near(&mut grid, new(1), (0.0, 0.0), g.cell, window).is_empty());
        });
    }

    #[test]
    fn new_lists_holding_topics_are_tall_enough() {
        each_size(|g| {
            let mut grid = g.grid(&[], &[]);
            let one = g.at(0.0, 0.0, 1.0, 1.0);
            let kids: Vec<raptaboard_layout::WidgetContainer> = (0..5)
                .map(|i| crate::widgets::for_topic(&format!("/t{i}"), "double", None, one).unwrap())
                .collect();
            // Drawing sizes are in points, so small grids need many rows.
            let window = g.window(8.0, 60.0);
            let list = add_list_holding(&mut grid, (0.0, 0.0), kids, g.cell, window).unwrap();
            let l = &grid.layouts[list];
            assert_eq!(l.children.len(), 5);
            assert_eq!(l.rect.width, 2.0 * g.cell);
            assert_eq!(
                l.rect.height,
                cells_tall_for(&l.children, labels(&l.properties), g.cell) * g.cell
            );
            // Five one-cell children need more than an empty list's two cells.
            assert!(l.rect.height > 2.0 * g.cell);
        });
    }
}
