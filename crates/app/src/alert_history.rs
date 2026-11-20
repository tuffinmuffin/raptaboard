//! The alert history: every alert seen since raptaboard connected, from
//! every alert group, when it was seen and when it cleared, whether or not
//! an Alerts widget shows it.
//!
//! It keeps the last [`DEFAULT_CAPACITY`] events (the `alert_history`
//! setting), dropping the oldest when full; nothing goes by age, so after a
//! match the alerts from it are still there to look at. It's in memory
//! only: quitting raptaboard forgets it. Alerts carry only their text, so
//! an alert is known by its group, level and text (and which copy, if the
//! same text is active twice).

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Local};

use crate::types::alerts::{ActiveAlert, Level};

/// How many events are kept when the `alert_history` setting isn't set.
pub const DEFAULT_CAPACITY: usize = 1000;
/// The fewest and most events the setting can keep.
pub const MIN_CAPACITY: usize = 10;
pub const MAX_CAPACITY: usize = 100_000;

/// How an alert stopped being active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// The robot cleared it.
    Cleared,
    /// The connection to the robot went down while it was active.
    Disconnected,
    /// Its group stopped being published while it was active.
    Unpublished,
}

impl Ending {
    /// How the history names it.
    pub fn label(self) -> &'static str {
        match self {
            Ending::Cleared => "cleared",
            Ending::Disconnected => "robot disconnected",
            Ending::Unpublished => "group not published",
        }
    }
}

/// One alert, from when it was seen to when it stopped (if it has).
#[derive(Debug, Clone, PartialEq)]
pub struct AlertEvent {
    /// Its group (the table's last name part).
    pub group: String,
    /// How urgent.
    pub level: Level,
    /// What it said.
    pub text: String,
    /// When raptaboard first saw it.
    pub seen: DateTime<Local>,
    /// When and how it stopped; `None` while it's active.
    pub ended: Option<(DateTime<Local>, Ending)>,
    /// Its place in the order events arrived, for finding it again.
    id: u64,
}

impl AlertEvent {
    /// How long it was (or has been, by `now`) active.
    pub fn duration(&self, now: DateTime<Local>) -> chrono::TimeDelta {
        self.ended.map_or(now, |(at, _)| at) - self.seen
    }
}

/// What identifies an active alert: group, level, text, and which copy of
/// that text (0 for the first).
type Key = (String, Level, String, usize);

/// The history: events in the order they were seen, oldest first.
#[derive(Debug, Clone)]
pub struct AlertHistory {
    events: VecDeque<AlertEvent>,
    capacity: usize,
    /// The events still active, by key.
    active: HashMap<Key, u64>,
    next_id: u64,
}

impl Default for AlertHistory {
    fn default() -> Self {
        AlertHistory::new(DEFAULT_CAPACITY)
    }
}

impl AlertHistory {
    /// An empty history keeping up to `capacity` events.
    pub fn new(capacity: usize) -> AlertHistory {
        AlertHistory {
            events: VecDeque::new(),
            capacity: capacity.clamp(MIN_CAPACITY, MAX_CAPACITY),
            active: HashMap::new(),
            next_id: 0,
        }
    }

    /// The events, oldest first.
    pub fn events(&self) -> impl DoubleEndedIterator<Item = &AlertEvent> + ExactSizeIterator {
        self.events.iter()
    }

    /// How many events it keeps.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Keep up to `capacity` events from now on, dropping the oldest if
    /// there are more.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.clamp(MIN_CAPACITY, MAX_CAPACITY);
        while self.events.len() > self.capacity {
            self.drop_oldest();
        }
    }

    /// The most urgent alert active now, if any.
    pub fn highest_active(&self) -> Option<Level> {
        self.active.keys().map(|(_, level, _, _)| *level).min()
    }

    /// How many alerts are active now.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Forget every event that has ended. Active ones stay, so they aren't
    /// seen again as new.
    pub fn clear(&mut self) {
        self.events.retain(|e| e.ended.is_none());
    }

    /// Take in what's active at `now`: the alerts of each group published
    /// now, as (group, alerts), or `None` while not connected. New alerts
    /// start events; ones that went away end theirs: cleared if their group
    /// is still published, else "not published", and every one when the
    /// connection is down.
    pub fn update(&mut self, now: DateTime<Local>, groups: Option<&[(String, Vec<ActiveAlert>)]>) {
        let Some(groups) = groups else {
            let ids: Vec<u64> = self.active.drain().map(|(_, id)| id).collect();
            for id in ids {
                self.end(id, now, Ending::Disconnected);
            }
            return;
        };
        // What's active now, each text counted so a second copy is its own.
        let mut current: HashMap<Key, ()> = HashMap::new();
        let mut order: Vec<Key> = Vec::new();
        for (group, alerts) in groups {
            let mut copies: HashMap<(Level, &str), usize> = HashMap::new();
            for a in alerts {
                let n = copies.entry((a.level, a.text.as_str())).or_insert(0);
                let key = (group.clone(), a.level, a.text.clone(), *n);
                *n += 1;
                if current.insert(key.clone(), ()).is_none() {
                    order.push(key);
                }
            }
        }
        let gone: Vec<(Key, u64)> = self
            .active
            .iter()
            .filter(|(k, _)| !current.contains_key(*k))
            .map(|(k, id)| (k.clone(), *id))
            .collect();
        for (key, id) in gone {
            self.active.remove(&key);
            let still_published = groups.iter().any(|(g, _)| *g == key.0);
            let how = if still_published {
                Ending::Cleared
            } else {
                Ending::Unpublished
            };
            self.end(id, now, how);
        }
        // Newest last, in the robot's order within a group (newest first),
        // so the history reads newest first when shown backwards.
        for key in order.into_iter().rev() {
            if self.active.contains_key(&key) {
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            if self.events.len() >= self.capacity {
                self.drop_oldest();
            }
            self.events.push_back(AlertEvent {
                group: key.0.clone(),
                level: key.1,
                text: key.2.clone(),
                seen: now,
                ended: None,
                id,
            });
            self.active.insert(key, id);
        }
    }

    /// End the event `id` at `now`, if it's still kept.
    fn end(&mut self, id: u64, now: DateTime<Local>, how: Ending) {
        if let Ok(i) = self.events.binary_search_by_key(&id, |e| e.id)
            && self.events[i].ended.is_none()
        {
            self.events[i].ended = Some((now, how));
        }
    }

    /// Drop the oldest event (forgetting it's active, if it is).
    fn drop_oldest(&mut self) {
        if let Some(old) = self.events.pop_front() {
            self.active.retain(|_, id| *id != old.id);
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use chrono::TimeDelta;

    fn alert(level: Level, text: &str) -> ActiveAlert {
        ActiveAlert {
            group: "G".into(),
            level,
            text: text.into(),
        }
    }

    fn group(name: &str, alerts: Vec<ActiveAlert>) -> (String, Vec<ActiveAlert>) {
        (name.into(), alerts)
    }

    fn texts(h: &AlertHistory) -> Vec<(String, Option<Ending>)> {
        h.events()
            .map(|e| (e.text.clone(), e.ended.map(|(_, how)| how)))
            .collect()
    }

    #[test]
    fn alerts_start_and_end_events() {
        let t0 = Local::now();
        let t1 = t0 + TimeDelta::seconds(2);
        let mut h = AlertHistory::default();
        h.update(
            t0,
            Some(&[group(
                "A",
                vec![alert(Level::Error, "fault"), alert(Level::Info, "hi")],
            )]),
        );
        assert_eq!(h.active_count(), 2);
        assert_eq!(h.highest_active(), Some(Level::Error));
        // Seeing the same again changes nothing.
        h.update(
            t0,
            Some(&[group(
                "A",
                vec![alert(Level::Error, "fault"), alert(Level::Info, "hi")],
            )]),
        );
        assert_eq!(h.events().len(), 2);
        // The fault clears.
        h.update(t1, Some(&[group("A", vec![alert(Level::Info, "hi")])]));
        let fault = h.events().find(|e| e.text == "fault").unwrap();
        assert_eq!(fault.ended, Some((t1, Ending::Cleared)));
        assert_eq!(fault.duration(t1), TimeDelta::seconds(2));
        assert_eq!(h.highest_active(), Some(Level::Info));
        // It comes back: a new event.
        h.update(
            t1,
            Some(&[group(
                "A",
                vec![alert(Level::Error, "fault"), alert(Level::Info, "hi")],
            )]),
        );
        assert_eq!(h.events().filter(|e| e.text == "fault").count(), 2);
    }

    #[test]
    fn a_lost_connection_or_group_ends_what_was_active() {
        let now = Local::now();
        let mut h = AlertHistory::default();
        h.update(
            now,
            Some(&[
                group("A", vec![alert(Level::Error, "a")]),
                group("B", vec![alert(Level::Warning, "b")]),
            ]),
        );
        let ending = |h: &AlertHistory, text: &str| {
            h.events()
                .find(|e| e.text == text)
                .and_then(|e| e.ended.map(|(_, how)| how))
        };
        // Group B stops being published.
        h.update(now, Some(&[group("A", vec![alert(Level::Error, "a")])]));
        assert_eq!(ending(&h, "b"), Some(Ending::Unpublished));
        assert_eq!(ending(&h, "a"), None);
        // The connection goes.
        h.update(now, None);
        assert_eq!(ending(&h, "a"), Some(Ending::Disconnected));
        assert_eq!(h.active_count(), 0);
    }

    #[test]
    fn it_keeps_the_newest_up_to_its_capacity() {
        let now = Local::now();
        let mut h = AlertHistory::new(MIN_CAPACITY);
        for i in 0..15 {
            h.update(
                now,
                Some(&[group("A", vec![alert(Level::Error, &i.to_string())])]),
            );
        }
        assert_eq!(h.events().len(), MIN_CAPACITY);
        assert_eq!(h.events().next().unwrap().text, "5");
        // Smaller drops the oldest; capacity is kept within its limits.
        h.set_capacity(1);
        assert_eq!(h.capacity(), MIN_CAPACITY);
        assert_eq!(AlertHistory::new(usize::MAX).capacity(), MAX_CAPACITY);
    }

    #[test]
    fn clear_keeps_what_is_active_and_copies_count() {
        let now = Local::now();
        let mut h = AlertHistory::default();
        h.update(
            now,
            Some(&[group(
                "A",
                vec![alert(Level::Error, "x"), alert(Level::Error, "x")],
            )]),
        );
        assert_eq!(h.active_count(), 2, "two copies of the same text");
        h.update(now, Some(&[group("A", vec![alert(Level::Error, "x")])]));
        h.clear();
        assert_eq!(texts(&h), [("x".to_string(), None)]);
        // The same text in another group is its own alert.
        h.update(
            now,
            Some(&[
                group("A", vec![alert(Level::Error, "x")]),
                group("B", vec![alert(Level::Error, "x")]),
            ]),
        );
        assert_eq!(h.active_count(), 2);
    }
}
