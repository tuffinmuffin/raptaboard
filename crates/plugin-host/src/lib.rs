//! Plugin manifests and table matching for raptaboard.
//!
//! A plugin is a folder holding `plugin.toml` and `widget.wasm`. This crate
//! reads the manifest and decides which NT tables a plugin can draw. Running
//! the WASM itself (wasmtime, time and memory limits) comes next (`docs/spec.md`,
//! "Plugin system"); the interface it implements is `wit/dashboard.wit` at the
//! repo root.
//!
//! ```
//! use raptaboard_plugin_host::{Manifest, TableInfo};
//! let manifest = Manifest::from_toml(r#"
//!     name = "arm"
//!     version = "0.1.0"
//!     api = "0.2"
//!     [match]
//!     type = "Arm"
//! "#).unwrap();
//! let table = TableInfo { path: "/Telemetry/Arm", type_tag: Some("Arm"), keys: &["angle_rad"] };
//! assert!(manifest.matches(&table).is_some());
//! ```

pub mod path;

use std::collections::BTreeMap;

use serde::Deserialize;

/// Host API version this build of raptaboard implements.
pub const HOST_API: &str = "0.2";

/// Errors reading a plugin manifest.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("plugin.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("bad path pattern {0:?}: {1}")]
    Pattern(String, glob::PatternError),
    #[error("plugin needs host API {needed}, this dashboard has {HOST_API}")]
    Api { needed: String },
    #[error("[match] needs at least one of type, path or has")]
    EmptyMatch,
}

/// Contents of a plugin's `plugin.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    /// Host API version the plugin was built against, like `"0.2"`.
    pub api: String,
    #[serde(rename = "match")]
    pub match_rules: MatchRules,
    #[serde(default)]
    pub size: Size,
    /// Settings shown in the widget's edit dialog, by name.
    #[serde(default)]
    pub properties: BTreeMap<String, PropertySpec>,
    #[serde(default)]
    pub permissions: Permissions,
}

/// Which NT tables a plugin can draw. Every rule given must pass.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchRules {
    /// The table's `.type` string must equal this.
    #[serde(rename = "type")]
    pub type_tag: Option<String>,
    /// The table path must match this glob, like `/Drive/Module/*`.
    pub path: Option<String>,
    /// The table must have all of these child keys.
    #[serde(default)]
    pub has: Vec<String>,
}

/// Default and minimum size in grid cells, as `[width, height]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Size {
    #[serde(default = "one_by_one")]
    pub default: [u32; 2],
    #[serde(default = "one_by_one")]
    pub min: [u32; 2],
}

fn one_by_one() -> [u32; 2] {
    [1, 1]
}

impl Default for Size {
    fn default() -> Self {
        Size {
            default: one_by_one(),
            min: one_by_one(),
        }
    }
}

/// One user-editable setting of a plugin widget.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum PropertySpec {
    Number {
        default: f64,
    },
    Bool {
        default: bool,
    },
    String {
        default: String,
    },
    /// A color as `#RRGGBB` or `#RRGGBBAA`.
    Color {
        default: String,
    },
    Enum {
        default: String,
        options: Vec<String>,
    },
}

/// What a plugin is allowed to do beyond drawing.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// May write NT values inside its bound table.
    #[serde(default)]
    pub publish: bool,
}

/// What the dashboard knows about one NT table when matching plugins.
#[derive(Debug, Clone, Copy)]
pub struct TableInfo<'a> {
    /// Full table path, like `/Telemetry/Arm`.
    pub path: &'a str,
    /// Value of the table's `.type` topic, if it has one.
    pub type_tag: Option<&'a str>,
    /// Names of the table's direct children.
    pub keys: &'a [&'a str],
}

/// How well a plugin matched; higher wins when several plugins fit a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity(pub u32);

impl Manifest {
    /// Parse and check a `plugin.toml`.
    pub fn from_toml(text: &str) -> Result<Self, ManifestError> {
        let manifest: Manifest = toml::from_str(text)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), ManifestError> {
        let m = &self.match_rules;
        if m.type_tag.is_none() && m.path.is_none() && m.has.is_empty() {
            return Err(ManifestError::EmptyMatch);
        }
        if let Some(p) = &m.path {
            glob::Pattern::new(p).map_err(|e| ManifestError::Pattern(p.clone(), e))?;
        }
        if !api_compatible(&self.api) {
            return Err(ManifestError::Api {
                needed: self.api.clone(),
            });
        }
        Ok(())
    }

    /// Check a table against the match rules. Returns how specific the match
    /// is, or `None` if any rule fails. A `.type` match counts most, then a
    /// path pattern, then each required key.
    pub fn matches(&self, table: &TableInfo<'_>) -> Option<Specificity> {
        let m = &self.match_rules;
        let mut score = 0;
        if let Some(t) = &m.type_tag {
            if table.type_tag != Some(t.as_str()) {
                return None;
            }
            score += 100;
        }
        if let Some(p) = &m.path {
            let pattern = glob::Pattern::new(p).ok()?;
            if !pattern.matches(table.path.trim_end_matches('/')) {
                return None;
            }
            score += 10;
        }
        for key in &m.has {
            if !table.keys.contains(&key.as_str()) {
                return None;
            }
            score += 1;
        }
        Some(Specificity(score))
    }
}

/// Same major version, and a minor version no newer than ours.
fn api_compatible(needed: &str) -> bool {
    let parse = |s: &str| -> Option<(u32, u32)> {
        let (a, b) = s.split_once('.')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    match (parse(needed), parse(HOST_API)) {
        (Some((nmaj, nmin)), Some((hmaj, hmin))) => nmaj == hmaj && nmin <= hmin,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    const ARM: &str = include_str!("../../../plugins/arm/plugin.toml");

    #[test]
    fn example_arm_manifest_loads() {
        let m = Manifest::from_toml(ARM).unwrap();
        assert_eq!(m.name, "arm");
        assert_eq!(m.size.default, [2, 2]);
        assert!(
            matches!(m.properties["length_m"], PropertySpec::Number { default } if default == 0.8)
        );
        assert!(!m.permissions.publish);
    }

    #[test]
    fn type_match_beats_key_match() {
        let by_type = Manifest::from_toml(ARM).unwrap();
        let by_keys =
            Manifest::from_toml("name='k'\nversion='0'\napi='0.1'\n[match]\nhas=['angle_rad']\n")
                .unwrap();
        let table = TableInfo {
            path: "/Arm",
            type_tag: Some("Arm"),
            keys: &["angle_rad"],
        };
        assert!(by_type.matches(&table) > by_keys.matches(&table));
    }

    #[test]
    fn path_glob_and_keys_must_all_pass() {
        let m = Manifest::from_toml(
            "name='m'\nversion='0'\napi='0.1'\n[match]\npath='/Drive/Module/*'\nhas=['drive_velocity_mps']\n",
        )
        .unwrap();
        let ok = TableInfo {
            path: "/Drive/Module/frontLeft",
            type_tag: None,
            keys: &["drive_velocity_mps"],
        };
        let wrong_path = TableInfo {
            path: "/Drive/Gyro",
            ..ok
        };
        let missing_key = TableInfo { keys: &[], ..ok };
        assert_eq!(m.matches(&ok), Some(Specificity(11)));
        assert_eq!(m.matches(&wrong_path), None);
        assert_eq!(m.matches(&missing_key), None);
    }

    #[test]
    fn rejects_empty_match_and_newer_api() {
        let empty = Manifest::from_toml("name='e'\nversion='0'\napi='0.1'\n[match]\n");
        assert!(matches!(empty, Err(ManifestError::EmptyMatch)));
        let newer = Manifest::from_toml("name='n'\nversion='0'\napi='0.9'\n[match]\ntype='X'\n");
        assert!(matches!(newer, Err(ManifestError::Api { .. })));
    }
}
