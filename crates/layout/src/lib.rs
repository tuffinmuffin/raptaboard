//! Layout files for raptaboard.
//!
//! Layouts are Elastic's JSON shape (`"version": 1.0`), read and written
//! without losing anything, so Elastic layouts open in raptaboard and the
//! other way round. That's a convenience, not a promise: raptaboard may
//! grow its own format (see "Goal and decisions" in `docs/spec.md`).
//!
//! Widget settings live in each container's `properties` object. They are kept
//! as raw JSON here; each widget reads the keys it knows and keeps the rest.
//! Any field this crate does not model is kept in `extra` and written back out.
//!
//! ```
//! let json = r#"{"version":1.0,"grid_size":128,"tabs":[{"name":"Teleop",
//!   "grid_layout":{"layouts":[],"containers":[{"title":"Pose","x":0.0,"y":0.0,
//!   "width":256.0,"height":256.0,"type":"Field","properties":{"topic":"/Drive/Field"}}]}}]}"#;
//! let layout = raptaboard_layout::ElasticLayout::from_json(json).unwrap();
//! let pose = &layout.tabs[0].grid_layout.containers[0];
//! assert_eq!(pose.widget_type, "Field");
//! assert_eq!(pose.topic(), Some("/Drive/Field"));
//! assert_eq!(pose.grid_cells(layout.grid_size()), (2, 2));
//! ```

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Elastic's default grid cell size in pixels.
pub const DEFAULT_GRID_SIZE: u32 = 128;
/// The smallest grid cell size [`ElasticLayout::sanitize`] keeps, in pixels.
pub const MIN_GRID_SIZE: u32 = 16;
/// The largest grid cell size [`ElasticLayout::sanitize`] keeps, in pixels.
pub const MAX_GRID_SIZE: u32 = 1024;
/// How far right or down a widget may reach, in grid cells. Anything past
/// this is moved back by [`ElasticLayout::sanitize`]; no screen is that big.
pub const MAX_GRID_CELLS: f64 = 100.0;

/// Errors reading a layout file.
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("layout JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported layout version {0}; expected 1.x")]
    Version(f64),
}

/// A whole Elastic layout file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElasticLayout {
    pub version: f64,
    /// Grid cell size in pixels when the layout was saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_size: Option<u32>,
    pub tabs: Vec<Tab>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One tab of the dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub name: String,
    pub grid_layout: GridLayout,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Everything placed on one tab's grid.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GridLayout {
    /// Containers that hold other widgets (List Layout).
    #[serde(default)]
    pub layouts: Vec<LayoutContainer>,
    /// Single widgets.
    #[serde(default)]
    pub containers: Vec<WidgetContainer>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Position and size of a container, in pixels at the saved grid size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// Whether two rectangles share any area. Rectangles that only touch
    /// along an edge don't overlap.
    ///
    /// ```
    /// use raptaboard_layout::Rect;
    /// let a = Rect { x: 0.0, y: 0.0, width: 128.0, height: 128.0 };
    /// let beside = Rect { x: 128.0, ..a };
    /// let across = Rect { x: 64.0, y: 64.0, ..a };
    /// assert!(!a.overlaps(&beside));
    /// assert!(a.overlaps(&across));
    /// ```
    pub fn overlaps(&self, other: &Rect) -> bool {
        // Old layouts can have sizes a hair off whole pixels; ignore slivers.
        const SLACK: f64 = 0.5;
        self.x + self.width - SLACK > other.x
            && other.x + other.width - SLACK > self.x
            && self.y + self.height - SLACK > other.y
            && other.y + other.height - SLACK > self.y
    }

    /// Short text for messages: `128x256 at (0, 384)`.
    fn describe(&self) -> String {
        format!("{}x{} at ({}, {})", self.width, self.height, self.x, self.y)
    }
}

/// One widget on the grid or inside a List Layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetContainer {
    pub title: String,
    #[serde(flatten)]
    pub rect: Rect,
    /// Widget type name, like `"Boolean Box"` or `"Field"`.
    #[serde(rename = "type")]
    pub widget_type: String,
    /// Widget settings; always has `topic` and `period` in Elastic files.
    #[serde(default)]
    pub properties: Map<String, Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A container widget (Elastic's List Layout) and its children.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutContainer {
    pub title: String,
    #[serde(flatten)]
    pub rect: Rect,
    #[serde(rename = "type")]
    pub layout_type: String,
    #[serde(default)]
    pub properties: Map<String, Value>,
    #[serde(default)]
    pub children: Vec<WidgetContainer>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Tab {
    /// An empty tab called `name`.
    ///
    /// ```
    /// let tab = raptaboard_layout::Tab::new("Pit");
    /// assert_eq!(tab.name, "Pit");
    /// assert!(tab.grid_layout.containers.is_empty());
    /// ```
    pub fn new(name: impl Into<String>) -> Self {
        Tab {
            name: name.into(),
            grid_layout: GridLayout::default(),
            extra: Map::new(),
        }
    }
}

impl Default for ElasticLayout {
    /// A new layout like Elastic starts with: tabs "Teleoperated" and
    /// "Autonomous", both empty, on a 128-pixel grid.
    fn default() -> Self {
        ElasticLayout {
            version: 1.0,
            grid_size: Some(DEFAULT_GRID_SIZE),
            tabs: vec![Tab::new("Teleoperated"), Tab::new("Autonomous")],
            extra: Map::new(),
        }
    }
}

impl ElasticLayout {
    /// A tab name not used yet: `base`, or `base 2`, `base 3` and so on.
    ///
    /// ```
    /// let layout = raptaboard_layout::ElasticLayout::default();
    /// assert_eq!(layout.unused_tab_name("Pit"), "Pit");
    /// assert_eq!(layout.unused_tab_name("Autonomous"), "Autonomous 2");
    /// ```
    pub fn unused_tab_name(&self, base: &str) -> String {
        let taken = |name: &str| self.tabs.iter().any(|t| t.name == name);
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .expect("some number is free")
    }

    /// Fix values raptaboard can't show, and return a plain-language line
    /// for each fix. A layout Elastic saved normally needs no fixes and is
    /// left exactly as it was.
    ///
    /// Fixes made:
    /// - A grid size outside [`MIN_GRID_SIZE`] to [`MAX_GRID_SIZE`] becomes
    ///   [`DEFAULT_GRID_SIZE`].
    /// - A layout with no tabs gets the default "Teleoperated" and
    ///   "Autonomous" tabs.
    /// - Widths and heights of zero or less become one cell; ones over
    ///   [`MAX_GRID_CELLS`] cells are cut to that.
    /// - Negative positions become 0.
    /// - A widget reaching past [`MAX_GRID_CELLS`] cells moves to the first
    ///   free spot on its tab.
    /// - A widget or List Layout covering one before it on its tab (widgets
    ///   first, then lists, in file order) moves to the free spot nearest
    ///   where it was. With no free spot it stays, and the fix says so.
    ///
    /// ```
    /// let json = r#"{"version": 1.0, "grid_size": 0, "tabs": []}"#;
    /// let mut layout = raptaboard_layout::ElasticLayout::from_json(json).unwrap();
    /// let fixes = layout.sanitize();
    /// assert_eq!(fixes.len(), 2);
    /// assert_eq!(layout.grid_size(), 128);
    /// assert_eq!(layout.tabs.len(), 2);
    /// ```
    pub fn sanitize(&mut self) -> Vec<String> {
        let mut fixes = Vec::new();
        if let Some(size) = self.grid_size
            && !(MIN_GRID_SIZE..=MAX_GRID_SIZE).contains(&size)
        {
            fixes.push(format!(
                "grid size {size} is outside {MIN_GRID_SIZE} to {MAX_GRID_SIZE}; using {DEFAULT_GRID_SIZE}"
            ));
            self.grid_size = Some(DEFAULT_GRID_SIZE);
        }
        if self.tabs.is_empty() {
            fixes.push("no tabs; added Teleoperated and Autonomous".to_owned());
            self.tabs = ElasticLayout::default().tabs;
        }
        let cell = f64::from(self.grid_size());
        for tab in &mut self.tabs {
            tab.grid_layout.sanitize(cell, &tab.name, &mut fixes);
        }
        fixes
    }

    /// Parse an Elastic layout file. Accepts any 1.x version.
    pub fn from_json(text: &str) -> Result<Self, LayoutError> {
        let layout: Self = serde_json::from_str(text)?;
        if !(1.0..2.0).contains(&layout.version) {
            return Err(LayoutError::Version(layout.version));
        }
        Ok(layout)
    }

    /// Write the layout as pretty JSON (2-space indent, stable key order).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("layout is always valid JSON")
    }

    /// Grid size in pixels, falling back to Elastic's default of 128.
    pub fn grid_size(&self) -> u32 {
        self.grid_size.unwrap_or(DEFAULT_GRID_SIZE)
    }

    /// Every widget in the layout, including those inside List Layouts.
    pub fn widgets(&self) -> impl Iterator<Item = &WidgetContainer> {
        self.tabs.iter().flat_map(|t| {
            t.grid_layout
                .containers
                .iter()
                .chain(t.grid_layout.layouts.iter().flat_map(|l| l.children.iter()))
        })
    }
}

impl GridLayout {
    /// [`ElasticLayout::sanitize`] for one tab, with `cell` the grid size in
    /// pixels. Fixes are added to `fixes`.
    fn sanitize(&mut self, cell: f64, tab: &str, fixes: &mut Vec<String>) {
        let limit = cell * MAX_GRID_CELLS;
        for w in &mut self.containers {
            let what = format!("tab \"{tab}\", widget \"{}\"", w.title);
            fix_rect(&mut w.rect, cell, limit, true, &what, fixes);
        }
        for l in &mut self.layouts {
            let what = format!("tab \"{tab}\", list \"{}\"", l.title);
            fix_rect(&mut l.rect, cell, limit, true, &what, fixes);
            // Children are stacked by the list, so only their size matters.
            for c in &mut l.children {
                let what = format!("{what}, widget \"{}\"", c.title);
                fix_rect(&mut c.rect, cell, limit, false, &what, fixes);
            }
        }

        // Bring back anything reaching past the edge of the grid.
        let count = self.containers.len() + self.layouts.len();
        for i in 0..count {
            let rect = self.rect_at(i);
            if rect.x + rect.width <= limit && rect.y + rect.height <= limit {
                continue;
            }
            let others: Vec<Rect> = (0..count)
                .filter(|&j| j != i)
                .map(|j| self.rect_at(j))
                .collect();
            let moved = free_spot(rect, cell, limit, &others).unwrap_or(Rect {
                x: 0.0,
                y: 0.0,
                ..rect
            });
            *self.rect_at_mut(i) = moved;
            fixes.push(format!(
                "tab \"{tab}\", {}: off the grid at ({}, {}); moved to ({}, {})",
                self.name_at(i),
                rect.x,
                rect.y,
                moved.x,
                moved.y
            ));
        }

        self.fix_overlaps(cell, limit, tab, fixes);
    }

    /// [`ElasticLayout::sanitize`]'s overlap fix for one tab: anything that
    /// covers an item before it moves to the nearest free spot.
    fn fix_overlaps(&mut self, cell: f64, limit: f64, tab: &str, fixes: &mut Vec<String>) {
        let count = self.containers.len() + self.layouts.len();
        for i in 0..count {
            let rect = self.rect_at(i);
            let Some(covered) = (0..i).find(|&j| rect.overlaps(&self.rect_at(j))) else {
                continue;
            };
            let what = format!(
                "tab \"{tab}\", {}: covers {}",
                self.name_at(i),
                self.name_at(covered)
            );
            let others: Vec<Rect> = (0..count)
                .filter(|&j| j != i)
                .map(|j| self.rect_at(j))
                .collect();
            match nearest_free_spot(rect, cell, limit, &others) {
                Some(moved) => {
                    *self.rect_at_mut(i) = moved;
                    fixes.push(format!(
                        "{what}; moved from ({}, {}) to ({}, {})",
                        rect.x, rect.y, moved.x, moved.y
                    ));
                }
                None => fixes.push(format!("{what}, and there's no free space; left there")),
            }
        }
    }

    /// Widgets then lists, counted together, for [`GridLayout::sanitize`].
    fn rect_at(&self, i: usize) -> Rect {
        match self.containers.get(i) {
            Some(w) => w.rect,
            None => self.layouts[i - self.containers.len()].rect,
        }
    }

    fn rect_at_mut(&mut self, i: usize) -> &mut Rect {
        let widgets = self.containers.len();
        match self.containers.get_mut(i) {
            Some(w) => &mut w.rect,
            None => &mut self.layouts[i - widgets].rect,
        }
    }

    /// `widget "Title"` or `list "Title"`, for messages.
    fn name_at(&self, i: usize) -> String {
        match self.containers.get(i) {
            Some(w) => format!("widget \"{}\"", w.title),
            None => format!("list \"{}\"", self.layouts[i - self.containers.len()].title),
        }
    }
}

/// Fix one rectangle's size, and its position if `has_position`, adding a
/// line to `fixes` if anything changed. See [`ElasticLayout::sanitize`].
fn fix_rect(
    rect: &mut Rect,
    cell: f64,
    limit: f64,
    has_position: bool,
    what: &str,
    fixes: &mut Vec<String>,
) {
    let old = *rect;
    let size = |v: f64| {
        if v.is_finite() && v > 0.0 {
            v.min(limit)
        } else {
            cell
        }
    };
    let position = |v: f64| if v.is_finite() && v >= 0.0 { v } else { 0.0 };
    rect.width = size(rect.width);
    rect.height = size(rect.height);
    if has_position {
        rect.x = position(rect.x);
        rect.y = position(rect.y);
    }
    // Compare as text so a NaN that was replaced counts as a change.
    if old.describe() != rect.describe() {
        fixes.push(format!(
            "{what}: {} became {}",
            old.describe(),
            rect.describe()
        ));
    }
}

/// Every cell corner where `rect`'s size fits inside `limit` without
/// covering any of `others`, row by row from the top left.
fn free_spots(rect: Rect, cell: f64, limit: f64, others: &[Rect]) -> impl Iterator<Item = Rect> {
    let steps = (limit / cell) as usize;
    (0..steps)
        .flat_map(move |row| (0..steps).map(move |col| (col, row)))
        .map(move |(col, row)| Rect {
            x: col as f64 * cell,
            y: row as f64 * cell,
            ..rect
        })
        .filter(move |r| {
            r.x + r.width <= limit
                && r.y + r.height <= limit
                && !others.iter().any(|o| r.overlaps(o))
        })
}

/// The first free spot for `rect`'s size, row by row from the top left.
fn free_spot(rect: Rect, cell: f64, limit: f64, others: &[Rect]) -> Option<Rect> {
    free_spots(rect, cell, limit, others).next()
}

/// The free spot for `rect`'s size closest to where it is. Ties go to the
/// spot higher up, then further left.
fn nearest_free_spot(rect: Rect, cell: f64, limit: f64, others: &[Rect]) -> Option<Rect> {
    let distance = |r: &Rect| (r.x - rect.x).powi(2) + (r.y - rect.y).powi(2);
    // Spots come row by row, so `min_by` keeping the first of equals breaks
    // ties toward the top left.
    free_spots(rect, cell, limit, others).min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

/// Which field of a struct topic a widget shows: Elastic's `struct_meta`
/// property. The widget's `topic` is the struct topic itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructMeta {
    /// Field names from the struct down to the field, such as
    /// `["translation", "x"]` in a `Pose2d`.
    pub path: Vec<String>,
    /// The struct's schema name, such as `Pose2d`.
    pub schema_name: String,
}

impl StructMeta {
    /// The `struct_meta` JSON Elastic saves, with `nt_type` the field's NT
    /// type (`double`).
    ///
    /// ```
    /// use raptaboard_layout::StructMeta;
    /// let meta = StructMeta { path: vec!["x".into()], schema_name: "Translation2d".into() };
    /// assert_eq!(
    ///     meta.to_json("double"),
    ///     serde_json::json!({"path": ["x"], "schema_name": "Translation2d", "type": "double"})
    /// );
    /// ```
    pub fn to_json(&self, nt_type: &str) -> Value {
        serde_json::json!({
            "path": self.path,
            "schema_name": self.schema_name,
            "type": nt_type,
        })
    }
}

impl WidgetContainer {
    /// The struct field this widget shows, if it's bound to one (Elastic's
    /// `struct_meta` property with a non-empty `path`).
    ///
    /// ```
    /// let json = r#"{"title": "x", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0,
    ///   "type": "Text Display", "properties": {"topic": "/Drive/pose",
    ///   "struct_meta": {"path": ["translation", "x"], "schema_name": "Pose2d", "type": "double"}}}"#;
    /// let w: raptaboard_layout::WidgetContainer = serde_json::from_str(json).unwrap();
    /// let meta = w.struct_meta().unwrap();
    /// assert_eq!(meta.path, ["translation", "x"]);
    /// assert_eq!(meta.schema_name, "Pose2d");
    /// ```
    pub fn struct_meta(&self) -> Option<StructMeta> {
        let meta = self.properties.get("struct_meta")?;
        let path: Vec<String> = meta
            .get("path")?
            .as_array()?
            .iter()
            .map(|p| p.as_str().map(str::to_owned))
            .collect::<Option<_>>()?;
        let schema_name = meta.get("schema_name")?.as_str()?.to_owned();
        (!path.is_empty() && !schema_name.is_empty()).then_some(StructMeta { path, schema_name })
    }

    /// The NT topic or table this widget is bound to.
    pub fn topic(&self) -> Option<&str> {
        self.properties.get("topic").and_then(Value::as_str)
    }

    /// Update period in seconds, if set.
    pub fn period(&self) -> Option<f64> {
        self.properties.get("period").and_then(Value::as_f64)
    }

    /// Size in whole grid cells (rounded), for a given grid size in pixels.
    pub fn grid_cells(&self, grid_size: u32) -> (u32, u32) {
        let g = f64::from(grid_size.max(1));
        (
            (self.rect.width / g).round().max(1.0) as u32,
            (self.rect.height / g).round().max(1.0) as u32,
        )
    }
}
