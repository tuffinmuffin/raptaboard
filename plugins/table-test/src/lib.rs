//! A table plugin for raptaboard's own tests: counts a WPILib Alerts group's
//! alerts (its `errors`, `warnings` and `infos`), or every group's with
//! `all_groups` (`tables-of-type`), says so as its summary ("1 error, 2
//! warnings, 0 infos"; while the robot is gone, the last ones seen and
//! "(disconnected)"), plays
//! `tone` for a new error with `sound` (never for those up on the first
//! look), and marks its tab for errors and warnings, also while the tab is
//! closed (`background`). The published Alerts plugin, which draws them, is
//! in raptaboard-examples.

use std::collections::BTreeSet;

use raptaboard_plugin::prelude::*;

const KEYS: [&str; 3] = ["errors", "warnings", "infos"];
const NAMES: [(&str, &str); 3] = [
    ("error", "errors"),
    ("warning", "warnings"),
    ("info", "infos"),
];

#[derive(Default)]
struct TableTest {
    /// With `all_groups`: the groups' tables seen.
    tables: BTreeSet<String>,
    /// The errors up at the last look; `None` before the first.
    errors: Option<BTreeSet<String>>,
    /// The alerts seen while connected, kept while the robot is gone.
    last: [Vec<String>; 3],
}

impl TableTest {
    /// The tables read: the widget's own (""), or every group's.
    fn prefixes(&self) -> Vec<String> {
        if property_bool("all_groups", false) {
            self.tables.iter().map(|t| format!("{t}/")).collect()
        } else {
            vec![String::new()]
        }
    }

    /// Each level's alerts, across the tables read.
    fn alerts(&self) -> [Vec<String>; 3] {
        let mut out: [Vec<String>; 3] = Default::default();
        for prefix in self.prefixes() {
            for (i, key) in KEYS.iter().enumerate() {
                if let Some(NtValue::StrArray(a)) = host::get(&format!("{prefix}{key}")) {
                    out[i].extend(a);
                }
            }
        }
        out
    }
}

impl Widget for TableTest {
    fn subscriptions(&self) -> Vec<Subscription> {
        if property_bool("all_groups", false) {
            vec![Subscription::TablesOfType("Alerts".into())]
        } else {
            KEYS.iter()
                .map(|k| Subscription::Key((*k).into()))
                .collect()
        }
    }

    fn update(&mut self, samples: Vec<Sample>) {
        for s in samples {
            if let Some(table) = s.topic.strip_suffix("/.type") {
                match s.value {
                    Some(NtValue::Str(t)) if t == "Alerts" => self.tables.insert(table.into()),
                    _ => self.tables.remove(table),
                };
            }
        }
    }

    fn render(&mut self, frame: FrameInfo) -> Picture {
        if frame.connected {
            self.last = self.alerts();
        }
        let alerts = self.last.clone();
        let now: BTreeSet<String> = alerts[0].iter().cloned().collect();
        let new_error = self
            .errors
            .as_ref()
            .is_some_and(|before| !now.is_subset(before));
        self.errors = Some(now);
        if new_error
            && property_bool("sound", false)
            && let Err(e) = host::play_sound(&property_string("tone", "beep"))
        {
            host::log(&format!("no sound: {e}"));
        }
        let counts: Vec<String> = alerts
            .iter()
            .zip(NAMES)
            .map(|(a, (one, many))| {
                format!("{} {}", a.len(), if a.len() == 1 { one } else { many })
            })
            .collect();
        let mut text = counts.join(", ");
        if !frame.connected {
            text.push_str(" (disconnected)");
        }
        let mut picture = Picture::empty();
        picture.text(centered_text(&frame, &text, 14.0));
        picture.summary = Some(text);
        picture
    }

    fn tab_mark(&self) -> Option<Level> {
        let alerts = self.alerts();
        if !alerts[0].is_empty() {
            Some(Level::Error)
        } else if !alerts[1].is_empty() {
            Some(Level::Warning)
        } else {
            None
        }
    }
}

raptaboard_plugin::widget!(TableTest);
