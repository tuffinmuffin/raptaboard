//! Named states: a plain string or int topic that holds one of a few
//! named states (an intake's mode, an arm's position), as the Enum Dropdown
//! and Enum Toggle show it.
//!
//! The names come from a [`Source`]: listed by hand ([`listed_states`]), a
//! string[] topic robot code publishes ([`named_states`]), or a chooser
//! table ([`crate::types::chooser`]). [`guess`] works out which from the
//! server's topics.

use nt4::Value;

use crate::types::chooser;

/// Where a widget's states come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Listed in the settings.
    List,
    /// A string[] topic the robot publishes.
    Topic,
    /// The widget's topic is a chooser table.
    Chooser,
}

impl Source {
    /// The source saved as `key` (`List` for anything else).
    ///
    /// ```
    /// use raptaboard::types::enums::Source;
    /// assert_eq!(Source::from_key("topic"), Source::Topic);
    /// assert_eq!(Source::from_key("???"), Source::List);
    /// ```
    pub fn from_key(key: &str) -> Source {
        match key {
            "topic" => Source::Topic,
            "chooser" => Source::Chooser,
            _ => Source::List,
        }
    }

    /// How it's saved.
    pub fn key(self) -> &'static str {
        match self {
            Source::List => "list",
            Source::Topic => "topic",
            Source::Chooser => "chooser",
        }
    }
}

/// One named state.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// What it's shown as.
    pub name: String,
    /// What's sent for it.
    pub value: Value,
}

/// The states listed in the settings, from `lines`: for a string topic
/// (`whole` false) each line is a value; for an int topic a line is
/// `3 = Climb`, or a name alone for the number after the one before
/// (starting at 0). Blank lines are skipped.
///
/// ```
/// use nt4::Value;
/// use raptaboard::types::enums::listed_states;
/// let s = listed_states(&["Idle", "Intake", "-1 = Fault", "Shoot"], true);
/// let values: Vec<_> = s.iter().map(|s| (s.name.as_str(), s.value.clone())).collect();
/// assert_eq!(values, [
///     ("Idle", Value::Int(0)), ("Intake", Value::Int(1)),
///     ("Fault", Value::Int(-1)), ("Shoot", Value::Int(0)),
/// ]);
/// let s = listed_states(&["Stowed", " ", "Out"], false);
/// assert_eq!(s[1].value, Value::String("Out".into()));
/// ```
pub fn listed_states(lines: &[&str], whole: bool) -> Vec<State> {
    let mut next = 0_i64;
    let mut states = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !whole {
            states.push(State {
                name: line.to_owned(),
                value: Value::String(line.to_owned()),
            });
            continue;
        }
        let (n, name) = match line.split_once('=') {
            Some((n, name)) if n.trim().parse::<i64>().is_ok() => {
                (n.trim().parse::<i64>().unwrap_or(next), name.trim())
            }
            _ => (next, line),
        };
        states.push(State {
            name: name.to_owned(),
            value: Value::Int(n),
        });
        next = n + 1;
    }
    states
}

/// The states named by a string[] topic's `names`: the names themselves for
/// a string topic, or 0, 1, 2 ... for an int topic.
pub fn named_states(names: &[String], whole: bool) -> Vec<State> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| State {
            name: name.clone(),
            value: if whole {
                Value::Int(i as i64)
            } else {
                Value::String(name.clone())
            },
        })
        .collect()
}

/// Which of `states` `value` is, or what to say when it's none of them:
/// `unnamed (7)` for an int, `Run (not listed)` for text.
///
/// ```
/// use nt4::Value;
/// use raptaboard::types::enums::{listed_states, locate};
/// let states = listed_states(&["Idle", "Run"], true);
/// assert_eq!(locate(&states, Some(&Value::Int(1))), (Some(1), None));
/// assert_eq!(locate(&states, Some(&Value::Int(7))), (None, Some("unnamed (7)".into())));
/// assert_eq!(locate(&states, None), (None, None));
/// ```
pub fn locate(states: &[State], value: Option<&Value>) -> (Option<usize>, Option<String>) {
    let current = value.and_then(|v| states.iter().position(|s| s.value == *v));
    let unnamed = match (value, current) {
        (Some(Value::Int(n)), None) => Some(format!("unnamed ({n})")),
        (Some(Value::String(s)), None) => Some(format!("{s} (not listed)")),
        (Some(_), None) => Some("not a state".to_owned()),
        _ => None,
    };
    (current, unnamed)
}

/// Guess where a widget on `topic` gets its states from, given every topic
/// on the server as (name, NT type): a chooser table if `topic` is one
/// (it has `options` and `active` or `default` under it), else a string[]
/// topic beside it named for it (`modeOptions`, `mode_options`,
/// `modeNames`, `mode_names` or `modes` for `mode`, in any case), else a
/// list. The source, and the options topic for [`Source::Topic`].
///
/// ```
/// use raptaboard::types::enums::{guess, Source};
/// let topics = vec![
///     ("/Intake/mode".to_string(), "string".to_string()),
///     ("/Intake/modeOptions".to_string(), "string[]".to_string()),
/// ];
/// assert_eq!(guess("/Intake/mode", &topics), (Source::Topic, Some("/Intake/modeOptions".into())));
/// assert_eq!(guess("/Arm/state", &topics), (Source::List, None));
/// ```
pub fn guess(topic: &str, topics: &[(String, String)]) -> (Source, Option<String>) {
    let table = topic.trim_end_matches('/');
    if chooser::is_chooser_table(table, topics) {
        return (Source::Chooser, None);
    }
    let (parent, last) = table.rsplit_once('/').unwrap_or(("", table));
    let wanted: Vec<String> = ["options", "_options", "names", "_names", "s"]
        .iter()
        .map(|end| format!("{last}{end}").to_lowercase())
        .collect();
    let found = topics.iter().find(|(name, ty)| {
        ty == "string[]"
            && name
                .rsplit_once('/')
                .is_some_and(|(p, n)| p == parent && wanted.contains(&n.to_lowercase()))
    });
    match found {
        Some((name, _)) => (Source::Topic, Some(name.clone())),
        None => (Source::List, None),
    }
}

/// The next state after `current` (the first if there's none).
///
/// ```
/// use raptaboard::types::enums::next_state;
/// assert_eq!(next_state(Some(0), 2), 1);
/// assert_eq!(next_state(Some(1), 2), 0);
/// assert_eq!(next_state(None, 3), 0);
/// ```
pub fn next_state(current: Option<usize>, n: usize) -> usize {
    current.map_or(0, |i| (i + 1) % n.max(1))
}

/// Where a click on a toggle switching between states `a` and `b` goes:
/// to `b` from `a`, and to `a` from `b` or any other state.
///
/// ```
/// use raptaboard::types::enums::pair_target;
/// assert_eq!(pair_target(Some(0), 0, 2), 2);
/// assert_eq!(pair_target(Some(2), 0, 2), 0);
/// assert_eq!(pair_target(Some(1), 0, 2), 0);
/// assert_eq!(pair_target(None, 0, 2), 0);
/// ```
pub fn pair_target(current: Option<usize>, a: usize, b: usize) -> usize {
    if current == Some(a) { b } else { a }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn topics(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect()
    }

    #[test]
    fn the_guess_prefers_a_chooser_then_a_names_topic() {
        let t = topics(&[
            ("/Auto/options", "string[]"),
            ("/Auto/active", "string"),
            ("/Arm/state", "int"),
            ("/Arm/STATE_NAMES", "string[]"),
            ("/Arm/other", "string[]"),
            ("/Elsewhere/stateNames", "string[]"),
        ]);
        assert_eq!(guess("/Auto", &t), (Source::Chooser, None));
        assert_eq!(guess("/Auto/", &t), (Source::Chooser, None));
        // Named for it, beside it (not elsewhere), in any case.
        assert_eq!(
            guess("/Arm/state", &t),
            (Source::Topic, Some("/Arm/STATE_NAMES".into()))
        );
        assert_eq!(guess("/Arm/mode", &t), (Source::List, None));
        // A string topic named for its plural.
        let t = topics(&[("/Intake/mode", "string"), ("/Intake/modes", "string[]")]);
        assert_eq!(
            guess("/Intake/mode", &t),
            (Source::Topic, Some("/Intake/modes".into()))
        );
    }

    #[test]
    fn listed_ints_number_on_from_the_last() {
        let s = listed_states(&["5 = Five", "Six", "x = Odd"], true);
        assert_eq!(s[0].value, Value::Int(5));
        assert_eq!(s[1].value, Value::Int(6));
        // Not a number before the `=`: the whole line is the name.
        assert_eq!(
            (s[2].name.as_str(), &s[2].value),
            ("x = Odd", &Value::Int(7))
        );
        // Text lines keep any `=`.
        assert_eq!(
            listed_states(&["a = b"], false)[0].value,
            Value::String("a = b".into())
        );
    }

    #[test]
    fn values_not_listed_say_so() {
        let states = listed_states(&["Idle", "Run"], false);
        let text = Value::String("Fly".into());
        assert_eq!(
            locate(&states, Some(&text)),
            (None, Some("Fly (not listed)".into()))
        );
        assert_eq!(
            locate(&states, Some(&Value::Double(1.0))).1.as_deref(),
            Some("not a state")
        );
        // Names from a topic number from 0 for an int topic.
        let named = named_states(&["A".into(), "B".into()], true);
        assert_eq!(named[1].value, Value::Int(1));
        assert_eq!(next_state(Some(1), 2), 0);
    }
}
