//! Group (raptaboard): a card holding a small grid of its own, so widgets
//! inside can sit side by side at any size (a List Layout only stacks
//! them).
//!
//! The group's inner grid is `columns` by `rows` cells (4 by 2 unless
//! set). Widgets inside are saved as if each inner cell were
//! [`INNER_CELL`] pixels, whatever the tab's grid size, and the whole inner
//! grid is scaled to fit the group's card: resizing the group grows or
//! shrinks everything inside, text too (the widgets are drawn at full size
//! on a layer of their own and scaled, [`show`]). The grid rules
//! ([`crate::grid`]) work inside a group unchanged: its children are a
//! grid of their own ([`inner_grid`]), [`INNER_CELL`] to a cell.
//!
//! Saved like a List Layout, in the tab's `layouts`, with the type
//! `"Group"` (raptaboard's own; Elastic shows a placeholder).

use eframe::egui::{self, Pos2, Rect, Vec2};
use nt4::Value;
use raptaboard_layout::{GridLayout, LayoutContainer, Rect as LayoutRect, WidgetContainer};
use serde_json::{Map, Value as Json};

use crate::widgets::shared::{card, props};

/// A group's `type` in a layout file.
pub const TYPE_NAME: &str = "Group";
/// The inner grid's columns (raptaboard's).
pub const COLUMNS: &str = "columns";
/// The inner grid's rows (raptaboard's).
pub const ROWS: &str = "rows";
/// Columns unless set.
pub const DEFAULT_COLUMNS: u32 = 4;
/// Rows unless set.
pub const DEFAULT_ROWS: u32 = 2;
/// The most columns or rows a group can have.
pub const MAX_CELLS: u32 = 24;
/// The size of one inner cell as saved, in pixels: widgets inside are
/// drawn at this scale, then scaled to fit the group.
pub const INNER_CELL: f64 = 128.0;
/// The gap around each widget inside a group, in saved pixels (scaled with
/// it): the tab's gap between cells.
const CHILD_GAP: f32 = 4.0;
/// A new group's size on the tab, in cells: across and down. Two rows tall
/// for its title plus a 4 by 2 inner grid of fair-sized cells.
pub const NEW_GROUP_CELLS: (f64, f64) = (4.0, 2.0);
/// A widget's resize corner marks inside a group (saved pixels, scaled with
/// it): how far in from the corner, and each line's length.
const RESIZE_MARK_INSET: f32 = 4.0;
const RESIZE_MARKS: [f32; 2] = [6.0, 11.0];
/// The title a new group gets.
pub const NEW_GROUP_TITLE: &str = "Group";

/// Whether `layout` is a group (not a List Layout).
pub fn is_group(layout: &LayoutContainer) -> bool {
    layout.layout_type == TYPE_NAME
}

/// The inner grid's columns and rows, each 1 to [`MAX_CELLS`].
///
/// ```
/// use raptaboard::widgets::group::{cells, new_group};
/// use raptaboard_layout::Rect;
/// let g = new_group(Rect { x: 0.0, y: 0.0, width: 256.0, height: 128.0 });
/// assert_eq!(cells(&g.properties), (4, 2));
/// ```
pub fn cells(props: &Map<String, Json>) -> (u32, u32) {
    let get = |key, default| {
        (props::number(props, key, f64::from(default)).round() as i64)
            .clamp(1, i64::from(MAX_CELLS)) as u32
    };
    (get(COLUMNS, DEFAULT_COLUMNS), get(ROWS, DEFAULT_ROWS))
}

/// The inner grid's size, in saved pixels ([`INNER_CELL`] a cell).
pub fn inner_area(group: &LayoutContainer) -> (f64, f64) {
    let (cols, rows) = cells(&group.properties);
    (f64::from(cols) * INNER_CELL, f64::from(rows) * INNER_CELL)
}

/// A new, empty group at `rect`.
pub fn new_group(rect: LayoutRect) -> LayoutContainer {
    LayoutContainer {
        title: NEW_GROUP_TITLE.into(),
        rect,
        layout_type: TYPE_NAME.into(),
        properties: Map::new(),
        children: Vec::new(),
        extra: Map::new(),
    }
}

/// The group's children as a grid of their own, for the grid rules: each
/// child is `containers[i]`, [`INNER_CELL`] to a cell.
pub fn inner_grid(group: &LayoutContainer) -> GridLayout {
    GridLayout {
        layouts: Vec::new(),
        containers: group.children.clone(),
        extra: Map::new(),
    }
}

/// How a group's inner grid sits on screen: its top-left corner and how
/// many screen points one saved pixel is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mapping {
    /// Where inner (0, 0) is on screen.
    pub origin: Pos2,
    /// Screen points per saved pixel.
    pub scale: f32,
}

impl Mapping {
    /// Where the inner grid of `group` goes for its card on screen at
    /// `card`: in the body below the title, as large as fits with square
    /// cells, at the body's top left.
    pub fn of(card: Rect, group: &LayoutContainer) -> Mapping {
        let body = body_rect(card);
        let (w, h) = inner_area(group);
        let scale = (body.width() / w as f32)
            .min(body.height() / h as f32)
            .max(0.01);
        Mapping {
            origin: body.min,
            scale,
        }
    }

    /// An inner rectangle (saved pixels) on screen.
    ///
    /// ```
    /// use eframe::egui::pos2;
    /// use raptaboard::widgets::group::Mapping;
    /// use raptaboard_layout::Rect;
    /// let m = Mapping { origin: pos2(10.0, 20.0), scale: 0.5 };
    /// let r = m.to_screen(&Rect { x: 128.0, y: 0.0, width: 128.0, height: 256.0 });
    /// assert_eq!((r.min.x, r.min.y, r.width(), r.height()), (74.0, 20.0, 64.0, 128.0));
    /// ```
    pub fn to_screen(&self, r: &LayoutRect) -> Rect {
        Rect::from_min_size(
            self.origin + Vec2::new(r.x as f32, r.y as f32) * self.scale,
            Vec2::new(r.width as f32, r.height as f32) * self.scale,
        )
    }

    /// A point on screen in the inner grid's saved pixels.
    pub fn to_inner(&self, p: Pos2) -> (f64, f64) {
        let v = (p - self.origin) / self.scale;
        (f64::from(v.x), f64::from(v.y))
    }

    /// The layer transform drawing the inner grid at full size scaled onto
    /// the card: saved pixels to screen.
    pub fn transform(&self) -> egui::emath::TSTransform {
        egui::emath::TSTransform::new(self.origin.to_vec2(), self.scale)
    }
}

/// The part of a group's card below its title, where its grid goes.
pub fn body_rect(card: Rect) -> Rect {
    card.with_min_y(card.min.y + card::TITLE_HEIGHT)
        .shrink(card::BODY_MARGIN)
}

/// The cells a widget `size` (saved pixels) dropped with the pointer at
/// inner point `at` takes in `group`: the inner cell under the pointer as
/// its top left, its size in whole cells (at most the group's), kept
/// inside the inner grid.
///
/// ```
/// use raptaboard::widgets::group::{cell_rect, new_group, INNER_CELL};
/// use raptaboard_layout::Rect;
/// let g = new_group(Rect { x: 0.0, y: 0.0, width: 512.0, height: 256.0 });
/// let one = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let r = cell_rect(&g, one, (300.0, 140.0));
/// assert_eq!((r.x, r.y), (2.0 * INNER_CELL, INNER_CELL));
/// ```
pub fn cell_rect(group: &LayoutContainer, size: LayoutRect, at: (f64, f64)) -> LayoutRect {
    let (cols, rows) = cells(&group.properties);
    let whole = |len: f64, most: u32| ((len / INNER_CELL).round().max(1.0) as u32).min(most);
    let (w, h) = (whole(size.width, cols), whole(size.height, rows));
    let cell_at = |pos: f64, span: u32, most: u32| {
        ((pos / INNER_CELL).floor().max(0.0) as u32).min(most - span)
    };
    LayoutRect {
        x: f64::from(cell_at(at.0, w, cols)) * INNER_CELL,
        y: f64::from(cell_at(at.1, h, rows)) * INNER_CELL,
        width: f64::from(w) * INNER_CELL,
        height: f64::from(h) * INNER_CELL,
    }
}

/// Where a widget `size` dropped at inner point `at` goes in `group`
/// ([`cell_rect`]), and what that pushes aside inside the group
/// ([`crate::grid::make_room`]): `None` if there's no room. `except` is a
/// child already in the group (moving within it), left out.
///
/// ```
/// use raptaboard::widgets::group::{new_group, place, INNER_CELL};
/// use raptaboard_layout::Rect;
/// let g = new_group(Rect { x: 0.0, y: 0.0, width: 512.0, height: 256.0 });
/// let one = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
/// let (rect, pushed) = place(&g, one, (300.0, 140.0), None).unwrap();
/// assert_eq!((rect.x, rect.y), (2.0 * INNER_CELL, INNER_CELL));
/// assert!(pushed.is_empty());
/// ```
pub fn place(
    group: &LayoutContainer,
    size: LayoutRect,
    at: (f64, f64),
    except: Option<usize>,
) -> Option<(LayoutRect, Vec<(usize, LayoutRect)>)> {
    let rect = cell_rect(group, size, at);
    let mut inner = inner_grid(group);
    if let Some(i) = except {
        // Moving within the group: it isn't in its own way.
        if i < inner.containers.len() {
            inner.containers[i].rect = LayoutRect {
                x: -1.0e12,
                ..inner.containers[i].rect
            };
        }
    }
    let pushed = crate::grid::make_room(&inner, rect, INNER_CELL, inner_area(group))?;
    let pushed = pushed
        .into_iter()
        .filter_map(|(item, r)| match item {
            crate::grid::Item::Widget(i) => Some((i, r)),
            crate::grid::Item::List(_) => None,
        })
        .collect();
    Some((rect, pushed))
}

/// The cells child `child` of `group` takes if its bottom-right corner is
/// dragged to inner point `to`: whole cells, at least one each way, inside
/// the inner grid; its top left stays.
pub fn resized(group: &LayoutContainer, child: LayoutRect, to: (f64, f64)) -> LayoutRect {
    let (cols, rows) = cells(&group.properties);
    let span = |start: f64, end: f64, most: u32| {
        let first = (start / INNER_CELL).round().max(0.0);
        let cells = ((end - start) / INNER_CELL).round().max(1.0);
        cells.min(f64::from(most) - first).max(1.0) * INNER_CELL
    };
    LayoutRect {
        width: span(child.x, to.0, cols),
        height: span(child.y, to.1, rows),
        ..child
    }
}

/// Draw `group` at `card` with only a painter (while it's dragged): its card
/// and its widgets in their cells, at their small size.
pub fn draw(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    group: &LayoutContainer,
    value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
) {
    card::draw_card(painter, visuals, rect, &group.title, &group.properties);
    let m = Mapping::of(rect, group);
    let clip = body_rect(rect).intersect(painter.clip_rect());
    let inner = painter.with_clip_rect(clip);
    for child in &group.children {
        let at = m.to_screen(&child.rect).shrink(1.0);
        card::draw(&inner, visuals, at, child, value_of(child).as_ref());
    }
}

/// Show `group` at `card`: its card on `ui`, and its widgets on a layer of
/// their own, drawn at full size and scaled onto the card, so their text
/// scales too. `interactive`: whether the widgets take input (locked;
/// unlocked, the grid moves and resizes them instead). `id` tells groups
/// apart. Returns the values its widgets want sent, by child.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    card_rect: Rect,
    group: &LayoutContainer,
    id: egui::Id,
    interactive: bool,
    value_of: &dyn Fn(&WidgetContainer) -> Option<Value>,
    frame: &crate::widgets::Frame<'_>,
    place: crate::widgets::Place,
) -> Vec<(usize, Value)> {
    card::draw_card(
        ui.painter(),
        ui.visuals(),
        card_rect,
        &group.title,
        &group.properties,
    );
    let m = Mapping::of(card_rect, group);
    let clip = body_rect(card_rect).intersect(ui.clip_rect());
    let mut writes = Vec::new();
    let area = egui::Area::new(id)
        // The grid's own order, so it can sit just above the grid.
        .order(ui.layer_id().order)
        .fixed_pos(Pos2::ZERO)
        .constrain(false)
        .interactable(interactive)
        .show(ui.ctx(), |ui| {
            // The clip, in the layer's own (saved pixel) space.
            let (x0, y0) = m.to_inner(clip.min);
            let (x1, y1) = m.to_inner(clip.max);
            ui.set_clip_rect(Rect::from_min_max(
                egui::pos2(x0 as f32, y0 as f32),
                egui::pos2(x1 as f32, y1 as f32),
            ));
            for (i, child) in group.children.iter().enumerate() {
                let r = child.rect;
                let at = Rect::from_min_size(
                    egui::pos2(r.x as f32, r.y as f32),
                    egui::vec2(r.width as f32, r.height as f32),
                )
                .shrink(CHILD_GAP);
                let value = value_of(child);
                let live = frame.live(value.as_ref(), place);
                let mut child_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(at)
                        .id_salt(("group_child", i)),
                );
                child_ui.set_clip_rect(at.intersect(ui.clip_rect()));
                if let Some(v) = card::show(&mut child_ui, at, child, &live) {
                    writes.push((i, v));
                }
                // Unlocked, each has a resize corner, as widgets on the grid do.
                if !interactive {
                    let corner = at.max - Vec2::splat(RESIZE_MARK_INSET);
                    let stroke = egui::Stroke::new(1.5, ui.visuals().weak_text_color());
                    for size in RESIZE_MARKS {
                        ui.painter().line_segment(
                            [corner - Vec2::new(size, 0.0), corner - Vec2::new(0.0, size)],
                            stroke,
                        );
                    }
                }
            }
        });
    let layer = area.response.layer_id;
    ui.ctx().set_transform_layer(layer, m.transform());
    ui.ctx().set_sublayer(ui.layer_id(), layer);
    writes
}

/// A group's settings dialog rows: its title (and its style), and its inner
/// grid's columns and rows. Widgets inside are edited by double-clicking
/// them.
pub fn settings(ui: &mut egui::Ui, group: &mut LayoutContainer) {
    egui::Grid::new("group_settings")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label("Title");
            ui.add(egui::TextEdit::singleline(&mut group.title).desired_width(180.0));
            ui.end_row();
            let text_color = ui.visuals().text_color();
            crate::widgets::shared::text_style::title_style_row(
                ui,
                &mut group.properties,
                text_color,
            );
            let most = i64::from(MAX_CELLS);
            let p = &mut group.properties;
            props::whole_row(
                ui,
                p,
                "Columns",
                COLUMNS,
                i64::from(DEFAULT_COLUMNS),
                1..=most,
            );
            props::whole_row(ui, p, "Rows", ROWS, i64::from(DEFAULT_ROWS), 1..=most);
        });
    ui.weak("Double-click a widget in the group for its settings.");
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn group_with(children: &[(f64, f64, f64, f64)]) -> LayoutContainer {
        let mut g = new_group(LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 512.0,
            height: 256.0,
        });
        for (i, &(x, y, w, h)) in children.iter().enumerate() {
            g.children.push(WidgetContainer {
                title: format!("c{i}"),
                rect: LayoutRect {
                    x: x * INNER_CELL,
                    y: y * INNER_CELL,
                    width: w * INNER_CELL,
                    height: h * INNER_CELL,
                },
                widget_type: "Text Display".into(),
                properties: Map::new(),
                extra: Map::new(),
            });
        }
        g
    }

    fn cells_rect(x: f64, y: f64, w: f64, h: f64) -> LayoutRect {
        LayoutRect {
            x: x * INNER_CELL,
            y: y * INNER_CELL,
            width: w * INNER_CELL,
            height: h * INNER_CELL,
        }
    }

    #[test]
    fn the_inner_grid_scales_onto_the_card_with_square_cells() {
        let g = group_with(&[]);
        // A card 400 wide: body 388 by (300 - 24 - 12) = 264. The 4 by 2
        // grid is 512 by 256 saved pixels: the width limits it.
        let card = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let m = Mapping::of(card, &g);
        assert!((m.scale - 388.0 / 512.0).abs() < 1e-5, "{}", m.scale);
        assert_eq!(m.origin, egui::pos2(6.0, 30.0));
        // To screen and back.
        let r = m.to_screen(&cells_rect(1.0, 1.0, 1.0, 1.0));
        let (x, y) = m.to_inner(r.min);
        assert!((x - INNER_CELL).abs() < 1e-3 && (y - INNER_CELL).abs() < 1e-3);
        // Columns and rows from settings, kept in range.
        let mut g = g;
        g.properties.insert(COLUMNS.into(), 99.into());
        g.properties.insert(ROWS.into(), 0.into());
        assert_eq!(cells(&g.properties), (MAX_CELLS, 1));
    }

    #[test]
    fn a_widget_dropped_in_takes_the_cell_under_the_pointer_and_makes_room() {
        // c0 at (0, 0); c1 two wide at (1, 1).
        let g = group_with(&[(0.0, 0.0, 1.0, 1.0), (1.0, 1.0, 2.0, 1.0)]);
        let one = cells_rect(0.0, 0.0, 1.0, 1.0);
        // On c0's cell: c0 moves to the nearest free cell.
        let (rect, pushed) = place(&g, one, (60.0, 60.0), None).unwrap();
        assert_eq!(rect, cells_rect(0.0, 0.0, 1.0, 1.0));
        assert_eq!(pushed, vec![(0, cells_rect(1.0, 0.0, 1.0, 1.0))]);
        // A wide widget near the right edge stays inside.
        let wide = cells_rect(0.0, 0.0, 3.0, 1.0);
        let (rect, _) = place(&g, wide, (500.0, 10.0), None).unwrap();
        assert_eq!(rect, cells_rect(1.0, 0.0, 3.0, 1.0));
        // Bigger than the group: cut to its size; then nothing else fits.
        let huge = cells_rect(0.0, 0.0, 9.0, 9.0);
        assert_eq!(place(&g, huge, (0.0, 0.0), None), None);
        // Moving c0 within the group: it isn't in its own way.
        let (rect, pushed) = place(&g, one, (140.0, 10.0), Some(0)).unwrap();
        assert_eq!(rect, cells_rect(1.0, 0.0, 1.0, 1.0));
        assert!(pushed.is_empty());
    }
}
