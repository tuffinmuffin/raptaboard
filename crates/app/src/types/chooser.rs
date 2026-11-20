//! A WPILib SendableChooser's table, as robot code publishes it: what a
//! chooser widget (and an enum widget bound to a chooser) reads.
//!
//! The table's `.type` is `"String Chooser"`. Robot code publishes
//! `options` (every choice), `default` and `active` (the choice it's
//! using); the dashboard publishes `selected`, which robot code reads. A
//! table made by hand with the same topics works the same, at any path.

use std::collections::HashMap;

use nt4::Value;

/// The `.type` of a SendableChooser's table.
pub const CHOOSER_TYPE: &str = "String Chooser";
/// The chooser table's topics, under the table.
pub const OPTIONS: &str = "options";
/// The option the robot starts with.
pub const DEFAULT: &str = "default";
/// The option the robot is using now.
pub const ACTIVE: &str = "active";
/// The option the dashboard picked, which robot code reads.
pub const SELECTED: &str = "selected";

/// What a chooser table holds now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chooser {
    /// Every choice, in the robot's order.
    pub options: Vec<String>,
    /// The robot's starting choice.
    pub default: Option<String>,
    /// The choice the robot is using.
    pub active: Option<String>,
    /// The choice a dashboard picked.
    pub selected: Option<String>,
}

/// The full name of `key` in the table `table`: `/Auto` and `options` is
/// `/Auto/options`.
fn under(table: &str, key: &str) -> String {
    format!("{}/{key}", table.trim_end_matches('/'))
}

impl Chooser {
    /// The chooser table `table`, from the values in `values` (full topic
    /// names).
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use nt4::Value;
    /// use raptaboard::types::chooser::Chooser;
    /// let options = Value::StringArray(vec!["Left".into(), "Right".into()]);
    /// let active = Value::String("Left".into());
    /// let values = HashMap::from([("/Auto/options", &options), ("/Auto/active", &active)]);
    /// let c = Chooser::read("/Auto", &values);
    /// assert_eq!(c.options, ["Left", "Right"]);
    /// assert_eq!(c.pick(), Some("Left"));
    /// ```
    pub fn read(table: &str, values: &HashMap<&str, &Value>) -> Chooser {
        let text = |key: &str| match values.get(under(table, key).as_str()) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        let options = match values.get(under(table, OPTIONS).as_str()) {
            Some(Value::StringArray(list)) => list.clone(),
            _ => Vec::new(),
        };
        Chooser {
            options,
            default: text(DEFAULT),
            active: text(ACTIVE),
            selected: text(SELECTED),
        }
    }

    /// Whether `choice` is one of the options.
    fn has(&self, choice: &str) -> bool {
        self.options.iter().any(|o| o == choice)
    }

    /// The choice shown as picked: `selected` if it's an option, else the
    /// robot's `active`, else `default`.
    pub fn pick(&self) -> Option<&str> {
        [&self.selected, &self.active, &self.default]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .find(|c| self.has(c))
    }

    /// The robot's own choice, ignoring any pick: `active` if it's an
    /// option, else `default`. What a chooser that isn't editable shows.
    pub fn robot_pick(&self) -> Option<&str> {
        [&self.active, &self.default]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .find(|c| self.has(c))
    }

    /// Whether the robot hasn't taken the pick yet: one was sent, and the
    /// robot's `active` is something else.
    ///
    /// ```
    /// use raptaboard::types::chooser::Chooser;
    /// let c = Chooser {
    ///     options: vec!["A".into(), "B".into()],
    ///     active: Some("A".into()),
    ///     selected: Some("B".into()),
    ///     ..Chooser::default()
    /// };
    /// assert!(c.waiting());
    /// ```
    pub fn waiting(&self) -> bool {
        match (self.pick(), &self.selected) {
            (Some(pick), Some(selected)) if pick == selected => {
                self.active.as_deref() != Some(pick)
            }
            _ => false,
        }
    }

    /// What to send when the robot's list has changed from `before`: the
    /// robot's own choice, so a pick from the old list doesn't linger.
    /// `None` if the list is the same, there's no pick to drop, or the
    /// robot has no choice to send. An empty list (the robot restarting)
    /// isn't a change.
    ///
    /// ```
    /// use raptaboard::types::chooser::Chooser;
    /// let c = Chooser {
    ///     options: vec!["A".into(), "C".into()],
    ///     active: Some("A".into()),
    ///     selected: Some("C".into()),
    ///     ..Chooser::default()
    /// };
    /// let old = vec!["A".to_string(), "B".to_string()];
    /// assert_eq!(c.dropped_pick(&old), Some("A".to_string()));
    /// assert_eq!(c.dropped_pick(&c.options), None);
    /// ```
    pub fn dropped_pick(&self, before: &[String]) -> Option<String> {
        if before.is_empty() || self.options.is_empty() || self.options == before {
            return None;
        }
        let selected = self.selected.as_ref()?;
        let robots = [&self.active, &self.default]
            .into_iter()
            .flatten()
            .find(|c| self.has(c))?;
        (robots != selected).then(|| robots.clone())
    }
}

/// The chooser topics a widget bound to `table` needs.
pub fn chooser_topics(table: &str) -> Vec<String> {
    [OPTIONS, DEFAULT, ACTIVE, SELECTED]
        .into_iter()
        .map(|k| under(table, k))
        .collect()
}

/// Whether `table` looks like a chooser table among the server's topics,
/// given as (name, NT type): it has a string[] `options` and a string
/// `active` or `default` under it (whether or not it has a `.type`).
///
/// ```
/// use raptaboard::types::chooser::is_chooser_table;
/// let topics = vec![
///     ("/Auto/options".to_string(), "string[]".to_string()),
///     ("/Auto/default".to_string(), "string".to_string()),
/// ];
/// assert!(is_chooser_table("/Auto/", &topics));
/// assert!(!is_chooser_table("/Arm", &topics));
/// ```
pub fn is_chooser_table(table: &str, topics: &[(String, String)]) -> bool {
    let has = |key: &str, ty: &str| {
        let name = under(table, key);
        topics.iter().any(|(n, t)| *n == name && t == ty)
    };
    has(OPTIONS, "string[]") && (has(ACTIVE, "string") || has(DEFAULT, "string"))
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn chooser(options: &[&str], active: Option<&str>, selected: Option<&str>) -> Chooser {
        Chooser {
            options: options.iter().map(|s| s.to_string()).collect(),
            default: Some("B".into()),
            active: active.map(Into::into),
            selected: selected.map(Into::into),
        }
    }

    #[test]
    fn the_pick_is_selected_then_active_then_default() {
        assert_eq!(chooser(&["A", "B"], Some("A"), Some("B")).pick(), Some("B"));
        assert_eq!(chooser(&["A", "B"], Some("A"), None).pick(), Some("A"));
        assert_eq!(chooser(&["A", "B"], None, None).pick(), Some("B"));
        // A pick that isn't an option (from an old list) doesn't count.
        assert_eq!(chooser(&["A", "B"], Some("A"), Some("Z")).pick(), Some("A"));
        assert_eq!(chooser(&[], Some("A"), Some("A")).pick(), None);
    }

    #[test]
    fn waiting_means_the_robot_has_something_else() {
        assert!(chooser(&["A", "B"], Some("A"), Some("B")).waiting());
        assert!(chooser(&["A", "B"], None, Some("B")).waiting());
        assert!(!chooser(&["A", "B"], Some("B"), Some("B")).waiting());
        // Nothing picked: the robot's own choice is shown, nothing waits.
        assert!(!chooser(&["A", "B"], Some("A"), None).waiting());
    }

    #[test]
    fn a_new_list_drops_the_pick() {
        let old = vec!["A".to_string(), "B".to_string()];
        // Changed list: send the robot's choice instead of ours.
        let c = chooser(&["A", "B", "C"], Some("A"), Some("C"));
        assert_eq!(c.dropped_pick(&old), Some("A".into()));
        // Ours is already the robot's: nothing to send.
        assert_eq!(
            chooser(&["A", "C"], Some("A"), Some("A")).dropped_pick(&old),
            None
        );
        // No pick, same list, or a robot restarting (empty list): nothing.
        assert_eq!(
            chooser(&["A", "C"], Some("A"), None).dropped_pick(&old),
            None
        );
        assert_eq!(
            chooser(&["A", "B"], Some("A"), Some("B")).dropped_pick(&old),
            None
        );
        assert_eq!(chooser(&[], Some("A"), Some("B")).dropped_pick(&old), None);
        assert_eq!(
            chooser(&["A", "C"], Some("A"), Some("C")).dropped_pick(&[]),
            None
        );
        // The robot's active isn't in the new list: its default is sent.
        let c = chooser(&["B", "C"], Some("A"), Some("C"));
        assert_eq!(c.dropped_pick(&old), Some("B".into()));
    }

    #[test]
    fn values_are_read_from_under_the_table() {
        let options = Value::StringArray(vec!["X".into()]);
        let selected = Value::String("X".into());
        let wrong = Value::Double(1.0);
        let values = HashMap::from([
            ("/SmartDashboard/Auto/options", &options),
            ("/SmartDashboard/Auto/selected", &selected),
            ("/SmartDashboard/Auto/active", &wrong),
        ]);
        let c = Chooser::read("/SmartDashboard/Auto/", &values);
        assert_eq!(c.options, ["X"]);
        assert_eq!(c.selected.as_deref(), Some("X"));
        assert_eq!(c.active, None, "not a string");
        assert_eq!(chooser_topics("/A").len(), 4);
    }
}
