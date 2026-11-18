//! Layout files for raptaboard.
//!
//! Milestone 1 reads and writes Elastic's layout JSON (`"version": 1.0`)
//! without losing anything, so existing Elastic layouts open in raptaboard and
//! a layout saved by raptaboard still opens in Elastic.
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

impl WidgetContainer {
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
