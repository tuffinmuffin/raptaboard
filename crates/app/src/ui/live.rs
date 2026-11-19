//! Which topics the open tab needs, and recording graph values as they arrive.

use std::collections::HashMap;

use nt4::{Topic, Value};

use crate::struct_view;
use crate::traces::SeriesKey;
use crate::widgets::builtin::graph;
use crate::widgets::shared::{format, props};
use crate::widgets::{self, list_layout};

use super::DashboardApp;

/// How often graphs' topics are sent when no graph says (seconds): about
/// 60 a second, as a graph's default.
pub(super) const DEFAULT_TRACE_PERIOD_S: f64 = graph::DEFAULT_PERIOD_S;

impl DashboardApp {
    /// The topics the widgets on the open tab are bound to, grouped by how
    /// often they want updates (each widget's `period`, in seconds). Only
    /// widgets raptaboard can draw are counted.
    pub(super) fn tab_topics(&self) -> Vec<(f64, Vec<String>)> {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return Vec::new();
        };
        let grid = &tab.grid_layout;
        let drawn = grid
            .containers
            .iter()
            .chain(
                grid.layouts
                    .iter()
                    .flat_map(|l| &l.children)
                    .filter(|w| !list_layout::is_hidden(w)),
            )
            .filter(|w| widgets::find(&w.widget_type).is_some());
        let mut groups: Vec<(f64, Vec<String>)> = Vec::new();
        for w in drawn {
            let Some(topic) = w.topic() else { continue };
            let period = props::period_s(w);
            let mut wanted = vec![topic.to_owned()];
            if let Some(ty) = widgets::find(&w.widget_type) {
                wanted.extend(ty.extra_topics(&w.properties));
            }
            match groups.iter_mut().find(|(p, _)| *p == period) {
                Some((_, names)) => names.extend(wanted),
                None => groups.push((period, wanted)),
            }
        }
        groups
    }

    /// Record the newest values of the series the open tab's graphs show,
    /// and forget what no graph shows any more.
    pub(super) fn record_traces(&mut self, topics: &[Topic]) {
        // Every value received since last frame, for the traced topics.
        let arrived = self.nt.take_traced();
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return;
        };
        let grid = &tab.grid_layout;
        let graphs = grid
            .containers
            .iter()
            .chain(grid.layouts.iter().flat_map(|l| &l.children))
            .filter(|w| w.widget_type == "Graph");
        let mut wanted = Vec::new();
        let mut fastest = f64::INFINITY;
        for w in graphs {
            let seconds = graph::time_displayed(&w.properties);
            fastest = fastest.min(props::period_s(w));
            wanted.extend(graph::series(w).into_iter().map(|s| (s.key, seconds)));
        }
        // Ask for every change of the graphs' topics, not only the newest.
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
                    let ((schema, _), db) = struct_view::struct_type(&ty).zip(structs)?;
                    struct_view::field_value(db, schema, bytes, path)
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
