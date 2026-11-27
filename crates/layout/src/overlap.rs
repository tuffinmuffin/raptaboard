//! Partial overlap: when widgets on a tab may overlap, and the one rule that
//! always holds: no widget is ever hidden completely.
//!
//! Two things on a tab may overlap if either says it may (`may_overlap` in
//! its properties: raptaboard sets it on a widget dropped over others with
//! Alt held). Each one must keep some part showing in front of everything
//! drawn over it, so it can always be seen and grabbed. Things are drawn
//! in order of their `z` (0 unless set), then widgets before lists, each in
//! the order they're saved: later is in front.
//!
//! These are raptaboard's own settings; Elastic ignores them.

use serde_json::{Map, Value};

use crate::{GridLayout, Rect};

/// A widget's or list's setting: it may overlap others (`true` or unset).
pub const MAY_OVERLAP: &str = "may_overlap";
/// A widget's or list's place front to back: higher is in front (0 unset).
pub const Z: &str = "z";

/// One thing on a tab's grid, as the overlap rules see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    /// Where it is.
    pub rect: Rect,
    /// Whether it may overlap others.
    pub may_overlap: bool,
    /// Its place front to back.
    pub z: i64,
}

impl Piece {
    /// A piece from a widget's or list's `rect` and `properties`.
    pub fn of(rect: Rect, properties: &Map<String, Value>) -> Piece {
        Piece {
            rect,
            may_overlap: properties.get(MAY_OVERLAP).and_then(Value::as_bool) == Some(true),
            z: properties.get(Z).and_then(Value::as_i64).unwrap_or(0),
        }
    }
}

/// What breaks the rules, by the pieces' places in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// These two overlap, and neither may.
    Overlap(usize, usize),
    /// This one is hidden completely by those in front of it.
    Hidden(usize),
}

/// The order the pieces are drawn in, back to front: by `z`, then their
/// place in the list.
///
/// ```
/// use raptaboard_layout::overlap::{draw_order, Piece};
/// use raptaboard_layout::Rect;
/// let r = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
/// let at = |z| Piece { rect: r, may_overlap: true, z };
/// assert_eq!(draw_order(&[at(1), at(0), at(0)]), [1, 2, 0]);
/// ```
pub fn draw_order(pieces: &[Piece]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by_key(|&i| (pieces[i].z, i));
    order
}

/// The first thing breaking the rules among `pieces`, or `None` if they're
/// all fine.
///
/// ```
/// use raptaboard_layout::overlap::{problem, Piece, Problem};
/// use raptaboard_layout::Rect;
/// let r = |x, w| Rect { x, y: 0.0, width: w, height: 128.0 };
/// let big = Piece { rect: r(0.0, 256.0), may_overlap: false, z: 0 };
/// let label = Piece { rect: r(128.0, 128.0), may_overlap: true, z: 0 };
/// // The label over half of the big one: fine, half of it shows.
/// assert_eq!(problem(&[big, label]), None);
/// // Two that may not: refused.
/// let plain = Piece { may_overlap: false, ..label };
/// assert_eq!(problem(&[big, plain]), Some(Problem::Overlap(0, 1)));
/// // The big one in front hides the label completely.
/// let front = Piece { z: 1, ..big };
/// assert_eq!(problem(&[front, label]), Some(Problem::Hidden(1)));
/// ```
pub fn problem(pieces: &[Piece]) -> Option<Problem> {
    problems(pieces).into_iter().next()
}

/// Everything breaking the rules among `pieces`: each pair that overlaps
/// and may not, then each one hidden, back to front.
///
/// ```
/// use raptaboard_layout::overlap::{problems, Piece, Problem};
/// use raptaboard_layout::Rect;
/// let cell = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let plain = Piece { rect: cell, may_overlap: false, z: 0 };
/// let all = problems(&[plain, plain, plain]);
/// assert!(all.contains(&Problem::Overlap(0, 2)) && all.contains(&Problem::Hidden(0)));
/// ```
pub fn problems(pieces: &[Piece]) -> Vec<Problem> {
    let mut found = Vec::new();
    for (i, a) in pieces.iter().enumerate() {
        for (j, b) in pieces.iter().enumerate().skip(i + 1) {
            if a.rect.overlaps(&b.rect) && !(a.may_overlap || b.may_overlap) {
                found.push(Problem::Overlap(i, j));
            }
        }
    }
    let order = draw_order(pieces);
    for (place, &i) in order.iter().enumerate() {
        let front: Vec<Rect> = order[place + 1..]
            .iter()
            .map(|&j| pieces[j].rect)
            .filter(|r| r.overlaps(&pieces[i].rect))
            .collect();
        if covered(pieces[i].rect, &front) {
            found.push(Problem::Hidden(i));
        }
    }
    found
}

/// Whether `rect` is covered all over by `over`. Exact: `rect` is cut along
/// every edge of `over` inside it, and each piece is checked.
pub fn covered(rect: Rect, over: &[Rect]) -> bool {
    if over.is_empty() {
        return false;
    }
    let cuts = |start: f64, len: f64, edges: &mut dyn Iterator<Item = f64>| {
        let mut at: Vec<f64> = std::iter::once(start)
            .chain(std::iter::once(start + len))
            .chain(edges.filter(|e| *e > start && *e < start + len))
            .collect();
        at.sort_by(f64::total_cmp);
        at.dedup();
        at
    };
    let xs = cuts(
        rect.x,
        rect.width,
        &mut over.iter().flat_map(|r| [r.x, r.x + r.width]),
    );
    let ys = cuts(
        rect.y,
        rect.height,
        &mut over.iter().flat_map(|r| [r.y, r.y + r.height]),
    );
    xs.windows(2).all(|x| {
        ys.windows(2).all(|y| {
            // Slivers under a pixel (old layouts' sizes) don't count.
            if x[1] - x[0] < 0.5 || y[1] - y[0] < 0.5 {
                return true;
            }
            let (cx, cy) = ((x[0] + x[1]) / 2.0, (y[0] + y[1]) / 2.0);
            over.iter()
                .any(|r| cx > r.x && cx < r.x + r.width && cy > r.y && cy < r.y + r.height)
        })
    })
}

impl GridLayout {
    /// Everything on the tab as [`Piece`]s: widgets, then lists.
    pub fn pieces(&self) -> Vec<Piece> {
        let widgets = self
            .containers
            .iter()
            .map(|w| Piece::of(w.rect, &w.properties));
        let lists = self
            .layouts
            .iter()
            .map(|l| Piece::of(l.rect, &l.properties));
        widgets.chain(lists).collect()
    }

    /// The first thing on this tab breaking the overlap rules, by its place
    /// in [`GridLayout::pieces`], or `None`.
    pub fn overlap_problem(&self) -> Option<Problem> {
        problem(&self.pieces())
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    fn piece(rect: Rect, may_overlap: bool, z: i64) -> Piece {
        Piece {
            rect,
            may_overlap,
            z,
        }
    }

    #[test]
    fn a_small_one_on_a_big_one_is_fine_but_not_two_on_one_cell() {
        // A 1x1 over a 1x2: half the 1x2 shows.
        let tall = piece(r(0.0, 0.0, 1.0, 2.0), false, 0);
        let small = piece(r(0.0, 0.0, 1.0, 1.0), true, 0);
        assert_eq!(problem(&[tall, small]), None);
        // Two 1x1s on the same cell: the one behind is hidden.
        let other = piece(r(0.0, 0.0, 1.0, 1.0), false, 0);
        assert_eq!(problem(&[other, small]), Some(Problem::Hidden(0)));
        // A big one over a smaller one, in front: hidden.
        let big = piece(r(0.0, 0.0, 2.0, 2.0), true, 1);
        assert_eq!(problem(&[small, big]), Some(Problem::Hidden(0)));
        // Behind it instead: the small one is in front, and the big one
        // still shows around it.
        let behind = piece(r(0.0, 0.0, 2.0, 2.0), true, -1);
        assert_eq!(problem(&[small, behind]), None);
    }

    #[test]
    fn several_in_front_together_can_hide_one() {
        // Two halves over a 2x1, each allowed: together they hide it.
        let wide = piece(r(0.0, 0.0, 2.0, 1.0), false, 0);
        let left = piece(r(0.0, 0.0, 1.0, 1.0), true, 0);
        let right = piece(r(1.0, 0.0, 1.0, 1.0), true, 0);
        assert_eq!(problem(&[wide, left]), None);
        assert_eq!(problem(&[wide, left, right]), Some(Problem::Hidden(0)));
        // Plain ones may not overlap at all.
        let plain = piece(r(1.0, 0.0, 2.0, 1.0), false, 0);
        assert_eq!(problem(&[wide, plain]), Some(Problem::Overlap(0, 1)));
    }

    #[test]
    fn the_settings_read_from_properties() {
        let mut grid: GridLayout = serde_json::from_str(
            r#"{"containers": [
                {"title": "a", "x": 0, "y": 0, "width": 256, "height": 128, "type": "Text Display",
                 "properties": {"may_overlap": true, "z": 2}},
                {"title": "b", "x": 128, "y": 0, "width": 128, "height": 128, "type": "Text Display"}
            ]}"#,
        )
        .unwrap();
        let pieces = grid.pieces();
        assert!(pieces[0].may_overlap && pieces[0].z == 2);
        assert!(!pieces[1].may_overlap && pieces[1].z == 0);
        // "a" is in front and covers "b" completely.
        assert_eq!(grid.overlap_problem(), Some(Problem::Hidden(1)));
        // Sent back, "b" shows over part of it.
        grid.containers[0].properties.insert(Z.into(), (-1).into());
        assert_eq!(grid.overlap_problem(), None);
    }
}
