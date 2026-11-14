//! Helpers for NT topic paths.
//!
//! NT has no real folders; a "table" is just a shared path prefix. These
//! helpers turn flat topic names into the parent/child view the tree and
//! plugin matching need.

/// Name of the topic that tags a table with its widget type.
pub const TYPE_KEY: &str = ".type";

/// Split a topic name into its parts, ignoring empty segments.
///
/// ```
/// use raptaboard_nt4::path::segments;
/// assert_eq!(segments("/Drive/Module/frontLeft/"), vec!["Drive", "Module", "frontLeft"]);
/// ```
pub fn segments(name: &str) -> Vec<&str> {
    name.split('/').filter(|s| !s.is_empty()).collect()
}

/// Join a table path and a relative key with exactly one slash.
///
/// ```
/// use raptaboard_nt4::path::join;
/// assert_eq!(join("/Arm/", "angle_rad"), "/Arm/angle_rad");
/// assert_eq!(join("/Arm", "/angle_rad"), "/Arm/angle_rad");
/// ```
pub fn join(table: &str, key: &str) -> String {
    format!(
        "{}/{}",
        table.trim_end_matches('/'),
        key.trim_start_matches('/')
    )
}

/// The table a topic sits in, or `None` for a topic at the root.
///
/// ```
/// use raptaboard_nt4::path::parent;
/// assert_eq!(parent("/Arm/angle_rad"), Some("/Arm"));
/// assert_eq!(parent("/angle"), None);
/// ```
pub fn parent(name: &str) -> Option<&str> {
    let trimmed = name.trim_end_matches('/');
    let idx = trimmed.rfind('/')?;
    if idx == 0 {
        None
    } else {
        Some(&trimmed[..idx])
    }
}

/// If `name` is a `.type` topic, return the table it tags.
///
/// ```
/// use raptaboard_nt4::path::typed_table;
/// assert_eq!(typed_table("/Telemetry/Arm/.type"), Some("/Telemetry/Arm"));
/// assert_eq!(typed_table("/Telemetry/Arm/angle"), None);
/// ```
pub fn typed_table(name: &str) -> Option<&str> {
    name.strip_suffix(TYPE_KEY)?.strip_suffix('/')
}

/// True if a key is hidden metadata (starts with a dot), like `.type` or `.name`.
pub fn is_metadata(key: &str) -> bool {
    key.starts_with('.')
}
