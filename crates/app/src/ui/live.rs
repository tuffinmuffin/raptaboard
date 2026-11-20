//! Which topics the open tab needs, and recording graph values as they arrive.

use std::collections::HashMap;

use nt4::{Topic, Value};

use crate::traces::SeriesKey;
use crate::types::structs;
use crate::widgets::builtin::alerts as alerts_widget;
use crate::widgets::builtin::graph;
use crate::widgets::shared::{format, props};
use crate::widgets::{self, list_layout};
use raptaboard_layout::WidgetContainer;

use super::DashboardApp;

/// How often graphs' topics are sent when no graph says (seconds): about
/// 60 a second, as a graph's default.
pub(super) const DEFAULT_TRACE_PERIOD_S: f64 = graph::DEFAULT_PERIOD_S;

/// How often the alerts of widgets on tabs that aren't open are sent, for
/// the tabs' marks (seconds).
const OTHER_TAB_ALERTS_PERIOD_S: f64 = 0.25;

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

/// Whether `w` is an Alerts widget.
fn is_alerts(w: &WidgetContainer) -> bool {
    widgets::find(&w.widget_type).is_some_and(|t| t.names()[0] == "Alerts")
}

impl DashboardApp {
    /// The topics the widgets on the open tab are bound to, grouped by how
    /// often they want updates (each widget's `period`, in seconds). Only
    /// widgets raptaboard can draw are counted. Alerts widgets on other tabs
    /// count too, so their tabs can be marked while they're not open.
    pub(super) fn tab_topics(&self) -> Vec<(f64, Vec<String>)> {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return Vec::new();
        };
        // An Alerts widget showing every group needs every table's `.type`.
        let all_groups = |w: &WidgetContainer| {
            is_alerts(w) && props::flag(&w.properties, alerts_widget::ALL_GROUPS, false)
        };
        let server: Vec<String> = if self.layout.tabs.iter().flat_map(drawn).any(all_groups) {
            self.nt
                .topics()
                .into_iter()
                .map(|t| t.name.to_string())
                .filter(|n| n.ends_with("/.type"))
                .collect()
        } else {
            Vec::new()
        };
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
                wanted.extend(ty.extra_topics(&w.properties));
            }
            if is_alerts(w) {
                wanted.extend(alerts_widget::needed_topics(&w.properties, &server));
            }
            add(props::period_s(w), wanted);
        }
        for (i, other) in self.layout.tabs.iter().enumerate() {
            if i == self.current_tab {
                continue;
            }
            for w in drawn(other).filter(|w| is_alerts(w)) {
                add(
                    OTHER_TAB_ALERTS_PERIOD_S,
                    alerts_widget::needed_topics(&w.properties, &server),
                );
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
