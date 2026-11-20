//! Robot alerts: what WPILib's `Alert` class publishes, read from the
//! server's values.
//!
//! Each alert group is a table whose `.type` is `"Alerts"`
//! (`/SmartDashboard/Alerts` for the default group). It holds three string[]
//! topics, `errors`, `warnings` and `infos`, each listing the text of the
//! alerts of that level that are active now, newest first. That's all an
//! alert carries: its text, and its level from which list it's in. 2027
//! names the levels HIGH, MEDIUM and LOW; they're the same three lists.

use std::collections::{HashMap, HashSet};

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

    /// The level named `name` (`"error"`, `"warning"` or `"info"`, as
    /// [`name`](Self::name) gives for one), if it is one.
    ///
    /// ```
    /// use raptaboard::types::alerts::Level;
    /// assert_eq!(Level::from_name("warning"), Some(Level::Warning));
    /// assert_eq!(Level::from_name("loud"), None);
    /// ```
    pub fn from_name(name: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|l| l.name(1) == name)
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
/// use raptaboard::types::alerts::{read_group, Level};
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

/// The active alerts in every table of `tables`, errors first across
/// groups (each level keeps the robot's order); `None` if none of them is
/// published.
pub fn read_groups(tables: &[String], values: &HashMap<&str, &Value>) -> Option<Vec<ActiveAlert>> {
    let mut found = false;
    let mut all = Vec::new();
    for table in tables {
        if let Some(group) = read_group(table, values) {
            found = true;
            all.extend(group);
        }
    }
    all.sort_by_key(|a| a.level);
    found.then_some(all)
}

/// Every alert group's table among `values`: those whose `.type` is
/// `"Alerts"`, in name order.
///
/// ```
/// use std::collections::HashMap;
/// use nt4::Value;
/// use raptaboard::types::alerts::alert_tables;
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

/// Whether the alerts a [`AlertWatch`] gives are the robot's now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// What the server has now.
    Current,
    /// Nothing published now (yet, or any more): the last alerts seen.
    NotPublishedNow,
    /// Disconnected: the last alerts seen.
    Disconnected,
    /// Connected, and nothing has been published since watching began.
    NeverPublished,
}

/// What a [`AlertWatch`] says to show this time.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    /// The alerts to show: the current ones, or the last seen.
    pub alerts: Vec<ActiveAlert>,
    /// Whether they're current.
    pub freshness: Freshness,
    /// An error is active that wasn't the last time (never on the first
    /// look, so opening a tab with errors already up isn't "new").
    pub new_error: bool,
}

/// Watches the alerts one view shows, look after look: which errors are
/// new, and the last alerts seen, kept while they're not published or the
/// connection is down, so what was wrong just before isn't lost.
///
/// ```
/// use raptaboard::types::alerts::{ActiveAlert, AlertWatch, Freshness, Level};
/// let error = |text: &str| ActiveAlert { group: "Alerts".into(), level: Level::Error, text: text.into() };
/// let mut watch = AlertWatch::default();
/// assert!(!watch.update(Some(vec![error("a")]), true).new_error); // first look
/// assert!(watch.update(Some(vec![error("a"), error("b")]), true).new_error);
/// let down = watch.update(None, false);
/// assert_eq!((down.freshness, down.alerts.len()), (Freshness::Disconnected, 2));
/// ```
#[derive(Debug, Clone, Default)]
pub struct AlertWatch {
    /// The alerts seen last while connected.
    last: Vec<ActiveAlert>,
    /// The errors active last look, by group and text.
    errors: HashSet<(String, String)>,
    /// Seen anything yet.
    started: bool,
}

impl AlertWatch {
    /// Take a look: `current` is what's published now (from
    /// [`read_groups`]), `connected` whether the connection is up.
    pub fn update(&mut self, current: Option<Vec<ActiveAlert>>, connected: bool) -> Seen {
        let stale = |last: &[ActiveAlert], freshness| Seen {
            alerts: last.to_vec(),
            freshness,
            new_error: false,
        };
        match (current, connected) {
            (Some(now), true) => {
                let errors: HashSet<(String, String)> = now
                    .iter()
                    .filter(|a| a.level == Level::Error)
                    .map(|a| (a.group.clone(), a.text.clone()))
                    .collect();
                let new_error = self.started && errors.iter().any(|e| !self.errors.contains(e));
                self.errors = errors;
                self.started = true;
                self.last.clone_from(&now);
                Seen {
                    alerts: now,
                    freshness: Freshness::Current,
                    new_error,
                }
            }
            // A robot dropping out can lose its topics before the
            // connection says so. The errors remembered stay too, so they
            // aren't "new" when they come back.
            (None, true) if !self.last.is_empty() => stale(&self.last, Freshness::NotPublishedNow),
            (None, true) => stale(&[], Freshness::NeverPublished),
            (_, false) => stale(&self.last, Freshness::Disconnected),
        }
    }
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

    #[test]
    fn a_watch_sees_new_errors_only_and_keeps_the_last_alerts() {
        let alert = |level, text: &str| ActiveAlert {
            group: "A".into(),
            level,
            text: text.into(),
        };
        let mut w = AlertWatch::default();
        let first = w.update(Some(vec![alert(Level::Error, "x")]), true);
        assert_eq!(
            (first.freshness, first.new_error),
            (Freshness::Current, false)
        );
        // A new warning or info isn't asking for attention; a new error is.
        assert!(
            !w.update(
                Some(vec![alert(Level::Error, "x"), alert(Level::Warning, "w")]),
                true
            )
            .new_error
        );
        assert!(
            w.update(Some(vec![alert(Level::Error, "y")]), true)
                .new_error
        );
        // Gone from the server: the last ones, then back isn't "new".
        let gone = w.update(None, true);
        assert_eq!(
            (gone.freshness, gone.alerts.len()),
            (Freshness::NotPublishedNow, 1)
        );
        assert!(
            !w.update(Some(vec![alert(Level::Error, "y")]), true)
                .new_error
        );
        // Never published at all.
        let mut fresh = AlertWatch::default();
        assert_eq!(
            fresh.update(None, true).freshness,
            Freshness::NeverPublished
        );
        // Errors sort first across groups.
        let e = Value::StringArray(vec!["e".into()]);
        let i = Value::StringArray(vec!["i".into()]);
        let values = HashMap::from([("/A/infos", &i), ("/B/errors", &e)]);
        let all = read_groups(&["/A".into(), "/B".into()], &values).unwrap();
        assert_eq!(all[0].text, "e");
        assert_eq!(read_groups(&["/C".into()], &values), None);
    }
}
