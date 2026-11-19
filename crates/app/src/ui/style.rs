//! How the grid looks. All sizes are screen points, not grid pixels: they stay
//! the same at any grid size (see "Grid size" in docs/spec.md).

use eframe::egui::Color32;

/// Size of the resize corner at the bottom right of each widget, in points.
pub(super) const RESIZE_HANDLE: f32 = 14.0;

/// Space between a widget's card and the edges of its cells, in points, so
/// side-by-side widgets don't touch.
pub(super) const CELL_GAP: f32 = 4.0;

/// Width of the grid lines, in points.
pub(super) const GRID_LINE_WIDTH: f32 = 1.0;

/// How strongly the grid lines show, from 0 (not at all) to 1.
pub(super) const GRID_LINE_STRENGTH: f32 = 0.3;

/// Width of the selection outline and the drop outline, in points.
pub(super) const OUTLINE_WIDTH: f32 = 2.0;

/// How far inside its cells the selection outline is drawn, in points.
pub(super) const SELECTION_INSET: f32 = 1.0;

/// How far inside its cells the drop outline is drawn, in points.
pub(super) const LANDING_INSET: f32 = 2.0;

/// Lengths of the resize corner's two diagonal marks, in points.
pub(super) const RESIZE_MARKS: [f32; 2] = [5.0, 9.0];

/// Width of the resize marks, in points.
pub(super) const RESIZE_MARK_WIDTH: f32 = 1.5;

/// How far the resize marks sit in from the card's corner, in points.
pub(super) const RESIZE_MARK_INSET: f32 = 4.0;

/// Color of a drop outline where the drop fits.
pub(super) const DROP_OK: Color32 = Color32::from_rgb(60, 180, 90);

/// Color of a drop outline where the drop would cover something.
pub(super) const DROP_BLOCKED: Color32 = Color32::from_rgb(220, 60, 60);

/// Color of the outline on widgets showing a topic picked in the tree.
pub(super) const FOUND_COLOR: Color32 = Color32::from_rgb(230, 160, 40);
