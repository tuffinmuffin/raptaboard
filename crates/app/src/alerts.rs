//! Robot alerts: what WPILib's `Alert` class publishes, read from the
//! server's values.
//!
//! Each alert group is a table whose `.type` is `"Alerts"`
//! (`/SmartDashboard/Alerts` for the default group). It holds three string[]
//! topics, `errors`, `warnings` and `infos`, each listing the text of the
//! alerts of that level that are active now, newest first. That's all an
//! alert carries: its text, and its level from which list it's in. 2027
//! names the levels HIGH, MEDIUM and LOW; they're the same three lists.

use std::collections::HashMap;

use eframe::egui::Color32;
use nt4::Value;

/// The `.type` of an alert group's table.
pub const ALERTS_TYPE: &str = "Alerts";

/// How urgent an alert is. Ordered most urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// An error (2027: HIGH): red ✕.
    Error,
    /// A warning (2027: MEDIUM): yellow !.
    Warning,
    /// Information (2027: LOW): blue i.
    Info,
}

impl Level {
    /// All three, most urgent first.
    pub const ALL: [Level; 3] = [Level::Error, Level::Warning, Level::Info];

    /// The topic under the group's table that lists this level's alerts.
    pub fn key(self) -> &'static str {
        match self {
            Level::Error => "errors",
            Level::Warning => "warnings",
            Level::Info => "infos",
        }
    }

    /// Its color: red, yellow, blue (WPILib 2027's).
    pub fn color(self) -> Color32 {
        match self {
            Level::Error => Color32::from_rgb(0xF4, 0x43, 0x36),
            Level::Warning => Color32::from_rgb(0xFF, 0xC1, 0x07),
            Level::Info => Color32::from_rgb(0x21, 0x96, 0xF3),
        }
    }

    /// Its name for people, plural when `n` isn't 1.
    pub fn name(self, n: usize) -> &'static str {
        match (self, n == 1) {
            (Level::Error, true) => "error",
            (Level::Error, false) => "errors",
            (Level::Warning, true) => "warning",
            (Level::Warning, false) => "warnings",
            (Level::Info, true) => "info",
            (Level::Info, false) => "infos",
        }
    }
}

/// One active alert.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActiveAlert {
    /// The group: its table's last name part (`Alerts`).
    pub group: String,
    /// How urgent.
    pub level: Level,
    /// What it says.
    pub text: String,
}

/// The topics a group's table `table` has: `.type` and the three lists.
pub fn table_topics(table: &str) -> Vec<String> {
    let table = table.trim_end_matches('/');
    std::iter::once(".type")
        .chain(Level::ALL.iter().map(|l| l.key()))
        .map(|k| format!("{table}/{k}"))
        .collect()
}

/// The active alerts in the group's table `table`, errors first, each
/// level newest first; `None` if none of its lists has a value (not
/// published, or not yet).
///
/// ```
/// use std::collections::HashMap;
/// use nt4::Value;
/// use raptaboard::alerts::{read_group, Level};
/// let errors = Value::StringArray(vec!["Encoder lost".into()]);
/// let values = HashMap::from([("/SmartDashboard/Alerts/errors", &errors)]);
/// let alerts = read_group("/SmartDashboard/Alerts", &values).unwrap();
/// assert_eq!((alerts[0].level, alerts[0].text.as_str()), (Level::Error, "Encoder lost"));
/// assert!(read_group("/Other", &values).is_none());
/// ```
pub fn read_group(table: &str, values: &HashMap<&str, &Value>) -> Option<Vec<ActiveAlert>> {
    let table = table.trim_end_matches('/');
    let group = table.rsplit('/').next().unwrap_or(table).to_owned();
    let mut found = false;
    let mut alerts = Vec::new();
    for level in Level::ALL {
        if let Some(Value::StringArray(texts)) =
            values.get(format!("{table}/{}", level.key()).as_str())
        {
            found = true;
            alerts.extend(texts.iter().map(|text| ActiveAlert {
                group: group.clone(),
                level,
                text: text.clone(),
            }));
        }
    }
    found.then_some(alerts)
}

/// Every alert group's table among `values`: those whose `.type` is
/// `"Alerts"`, in name order.
///
/// ```
/// use std::collections::HashMap;
/// use nt4::Value;
/// use raptaboard::alerts::alert_tables;
/// let alerts = Value::String("Alerts".into());
/// let chooser = Value::String("String Chooser".into());
/// let values = HashMap::from([
///     ("/SmartDashboard/Drive/.type", &alerts),
///     ("/SmartDashboard/Auto/.type", &chooser),
/// ]);
/// assert_eq!(alert_tables(&values), ["/SmartDashboard/Drive"]);
/// ```
pub fn alert_tables(values: &HashMap<&str, &Value>) -> Vec<String> {
    let mut tables: Vec<String> = values
        .iter()
        .filter(|(_, v)| matches!(v, Value::String(t) if t == ALERTS_TYPE))
        .filter_map(|(name, _)| name.strip_suffix("/.type").map(str::to_owned))
        .collect();
    tables.sort();
    tables
}

/// The most urgent level among `alerts`, if any.
pub fn highest(alerts: &[ActiveAlert]) -> Option<Level> {
    alerts.iter().map(|a| a.level).min()
}

/// How many of `alerts` there are of each level, errors first.
pub fn counts(alerts: &[ActiveAlert]) -> [usize; 3] {
    let mut n = [0; 3];
    for a in alerts {
        n[a.level as usize] += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn groups_read_in_level_order_and_keep_the_robots_order() {
        let errors = Value::StringArray(vec!["new".into(), "old".into()]);
        let infos = Value::StringArray(vec!["hi".into()]);
        let empty = Value::StringArray(vec![]);
        let values = HashMap::from([
            ("/A/infos", &infos),
            ("/A/errors", &errors),
            ("/A/warnings", &empty),
        ]);
        let a = read_group("/A/", &values).unwrap();
        let texts: Vec<&str> = a.iter().map(|a| a.text.as_str()).collect();
        assert_eq!(texts, ["new", "old", "hi"]);
        assert_eq!(a[0].group, "A");
        assert_eq!(highest(&a), Some(Level::Error));
        assert_eq!(counts(&a), [2, 0, 1]);
        // Published but empty is "no alerts", not "no group".
        let values = HashMap::from([("/B/warnings", &empty)]);
        assert_eq!(read_group("/B", &values), Some(vec![]));
        assert_eq!(highest(&[]), None);
        assert_eq!(table_topics("/A").len(), 4);
    }
}
