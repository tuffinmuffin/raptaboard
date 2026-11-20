//! Which topics the open tab needs, and recording traced values (graphs')
//! as they arrive.

use std::collections::HashMap;

use nt4::{Topic, Value};

use crate::traces::SeriesKey;
use crate::types::structs;
use crate::widgets::shared::{format, props};
use crate::widgets::{self, list_layout};
use raptaboard_layout::WidgetContainer;

use super::DashboardApp;

/// How often traced topics are sent when no widget tracing them says
/// (seconds): about 60 a second.
const DEFAULT_TRACE_PERIOD_S: f64 = 0.016;

/// How often the topics of widgets on tabs that aren't open are sent, for
/// the tabs' marks (seconds).
const BACKGROUND_PERIOD_S: f64 = 0.25;

/// The widgets on `tab` raptaboard can draw: on the grid, and in lists
/// (unless hidden).
fn drawn(tab: &raptaboard_layout::Tab) -> impl Iterator<Item = &WidgetContainer> {
    let grid = &tab.grid_layout;
    grid.containers
        .iter()
        .chain(
            grid.layouts
                .iter()
                .flat_map(|l| &l.children)
                .filter(|w| !list_layout::is_hidden(w)),
        )
        .filter(|w| widgets::find(&w.widget_type).is_some())
}

impl DashboardApp {
    /// The topics the widgets on the open tab are bound to, grouped by how
    /// often they want updates (each widget's `period`, in seconds), with
    /// what each needs besides (`extra_topics`), from the server's `topics`.
    /// Only widgets raptaboard can draw are counted. Widgets on other tabs
    /// add what they need for their tab's mark (`background_topics`).
    pub(super) fn tab_topics(&self, topics: &[Topic]) -> Vec<(f64, Vec<String>)> {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return Vec::new();
        };
        let tables: Vec<String> = topics
            .iter()
            .filter_map(|t| t.name.strip_suffix("/.type").map(str::to_owned))
            .collect();
        let server = widgets::Server { tables: &tables };
        let mut groups: Vec<(f64, Vec<String>)> = Vec::new();
        let mut add = |period: f64, wanted: Vec<String>| match groups
            .iter_mut()
            .find(|(p, _)| *p == period)
        {
            Some((_, names)) => names.extend(wanted),
            None => groups.push((period, wanted)),
        };
        for w in drawn(tab) {
            let Some(topic) = w.topic() else { continue };
            let mut wanted = vec![topic.to_owned()];
            if let Some(ty) = widgets::find(&w.widget_type) {
                wanted.extend(ty.extra_topics(&w.properties, server));
            }
            add(props::period_s(w), wanted);
        }
        for (i, other) in self.layout.tabs.iter().enumerate() {
            if i == self.current_tab {
                continue;
            }
            for w in drawn(other) {
                if let Some(ty) = widgets::find(&w.widget_type) {
                    let wanted = ty.background_topics(&w.properties, server);
                    if !wanted.is_empty() {
                        add(BACKGROUND_PERIOD_S, wanted);
                    }
                }
            }
        }
        groups
    }

    /// Record the newest values of the series the open tab's widgets trace
    /// (graphs' lines), and forget what none traces any more.
    pub(super) fn record_traces(&mut self, topics: &[Topic]) {
        // Every value received since last frame, for the traced topics.
        let arrived = self.nt.take_traced();
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return;
        };
        let grid = &tab.grid_layout;
        let mut wanted = Vec::new();
        let mut fastest = f64::INFINITY;
        for w in grid
            .containers
            .iter()
            .chain(grid.layouts.iter().flat_map(|l| &l.children))
        {
            let Some(ty) = widgets::find(&w.widget_type) else {
                continue;
            };
            let traced = ty.traced(w);
            if !traced.is_empty() {
                fastest = fastest.min(props::period_s(w));
                wanted.extend(traced);
            }
        }
        // Ask for every change of the traced topics, not only the newest.
        let mut names: Vec<String> = wanted.iter().map(|(k, _)| k.topic.clone()).collect();
        names.sort();
        names.dedup();
        let period = if fastest.is_finite() {
            fastest
        } else {
            DEFAULT_TRACE_PERIOD_S
        };
        self.nt.trace(names, period);

        let by_name: HashMap<&str, &Topic> = topics.iter().map(|t| (&*t.name, t)).collect();
        let structs = self.nt.structs();
        // A sample as a number to plot: a struct field decoded, booleans as
        // 0 and 1.
        let number_of = |key: &SeriesKey, value: &Value| -> Option<f64> {
            let value = match (&key.field, value) {
                (None, v) => Some(v.clone()),
                (Some(path), Value::Raw(bytes)) => {
                    let ty = by_name.get(key.topic.as_str())?.ty.to_string();
                    let ((schema, _), db) = structs::struct_type(&ty).zip(structs)?;
                    structs::field_value(db, schema, bytes, path)
                }
                (Some(_), _) => None,
            }?;
            match value {
                Value::Boolean(b) => Some(f64::from(u8::from(b))),
                v => format::number_of(&v),
            }
        };
        for (key, seconds) in &wanted {
            let new = self.traces.want(key, *seconds);
            // A new line starts from the value now, so a topic that rarely
            // changes still shows.
            if new
                && let Some(sample) = by_name
                    .get(key.topic.as_str())
                    .and_then(|t| t.value.as_ref())
                && let Some(n) = number_of(key, &sample.value)
            {
                self.traces.record(key, sample.local_time_us, n);
            }
        }
        for (name, sample) in &arrived {
            for (key, _) in wanted.iter().filter(|(k, _)| k.topic == **name) {
                if let Some(n) = number_of(key, &sample.value) {
                    self.traces.record(key, sample.local_time_us, n);
                }
            }
        }
        let now_us = self.nt.client().map_or(0, |c| c.now_us());
        self.traces.finish_frame(now_us);
    }
}
