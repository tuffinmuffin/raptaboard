//! Recent values of the topics graphs show, for drawing them over time.
//!
//! The NT client keeps only each topic's newest value, so the app records
//! every value graphs want here, taken from the client's event stream so none
//! are lost between frames. Each series is a ring buffer (`VecDeque`) that
//! grows as needed up to [`MAX_POINTS`]. Only the series some graph on
//! the open tab wants are kept, each for the longest time window a graph
//! wants it for; a series no graph wants any more is forgotten.
//!
//! Each series also keeps the lowest and highest values it has had since
//! it was first recorded or last [reset](Traces::reset), for autoscaled
//! bars and gauges, and its lowest, highest and average since the last
//! reset or reconnect ([`Traces::since_reset`]), for number widgets'
//! min/max/average. Both outlive the points: they stay while the widget's
//! tab is closed (nothing is recorded then), and are never saved, so each
//! run starts fresh. [`Traces::window`] gives the same over the last few
//! seconds, from the points kept.

use std::collections::{HashMap, VecDeque};

/// One line on a graph: a topic, or one number field of a struct topic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SeriesKey {
    /// Full topic name.
    pub topic: String,
    /// The field's path, for one field of a struct topic.
    pub field: Option<Vec<String>>,
}

/// The most points kept for one series, so a fast topic with a long window
/// can't use up memory: 100 a second for 10 minutes.
pub const MAX_POINTS: usize = 60_000;

/// Recorded values for every series a graph wants.
///
/// ```
/// use raptaboard::traces::{SeriesKey, Traces};
/// let key = SeriesKey { topic: "/Sine".into(), field: None };
/// let mut traces = Traces::default();
/// traces.want(&key, 5.0);
/// traces.record(&key, 1_000_000, 0.5);
/// traces.record(&key, 1_000_000, 0.5); // the same value again: ignored
/// traces.record(&key, 1_020_000, 0.6);
/// assert_eq!(traces.points(&key).count(), 2);
/// ```
#[derive(Debug, Default)]
pub struct Traces {
    series: HashMap<SeriesKey, Trace>,
    /// (lowest, highest) since first recorded or reset, by series.
    ranges: HashMap<SeriesKey, (f64, f64)>,
    /// Lowest, highest and average since the last reset or reconnect.
    since: HashMap<SeriesKey, Stats>,
    /// Whether the server was connected at the last [`Traces::connected`].
    was_connected: bool,
}

/// The lowest, highest and average of a run of values, each value counting
/// once however long it lasted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// The lowest value.
    pub min: f64,
    /// The highest value.
    pub max: f64,
    /// The sum of the values, for the average.
    sum: f64,
    /// How many values.
    pub count: usize,
}

impl Stats {
    /// The statistics of one value.
    fn of(value: f64) -> Stats {
        Stats {
            min: value,
            max: value,
            sum: value,
            count: 1,
        }
    }

    /// Count one more value.
    fn add(&mut self, value: f64) {
        self.min = self.min.min(value);
        self.max = self.max.max(value);
        self.sum += value;
        self.count += 1;
    }

    /// The statistics of `values`, if there are any (not-a-number ones
    /// skipped).
    ///
    /// ```
    /// use raptaboard::traces::Stats;
    /// let s = Stats::from_values([1.0, 4.0, 2.5, 4.5]).unwrap();
    /// assert_eq!((s.min, s.max, s.mean(), s.count), (1.0, 4.5, 3.0, 4));
    /// assert!(Stats::from_values([]).is_none());
    /// ```
    pub fn from_values(values: impl IntoIterator<Item = f64>) -> Option<Stats> {
        let mut out: Option<Stats> = None;
        for v in values.into_iter().filter(|v| v.is_finite()) {
            match &mut out {
                Some(s) => s.add(v),
                None => out = Some(Stats::of(v)),
            }
        }
        out
    }

    /// The average (mean) of the values.
    pub fn mean(&self) -> f64 {
        self.sum / self.count as f64
    }
}

impl SeriesKey {
    /// What `widget` shows: its topic, and the struct field if it's bound
    /// to one. `None` for a widget with no topic.
    ///
    /// ```
    /// use raptaboard::traces::SeriesKey;
    /// let w: raptaboard_layout::WidgetContainer = serde_json::from_value(serde_json::json!({
    ///     "title": "Arm", "x": 0.0, "y": 0.0, "width": 128.0, "height": 128.0,
    ///     "type": "Number Bar", "properties": {"topic": "/Arm/angle"},
    /// })).unwrap();
    /// assert_eq!(SeriesKey::of(&w).unwrap().topic, "/Arm/angle");
    /// ```
    pub fn of(widget: &raptaboard_layout::WidgetContainer) -> Option<SeriesKey> {
        Some(SeriesKey {
            topic: widget.topic()?.to_owned(),
            field: widget.struct_meta().map(|m| m.path),
        })
    }
}

#[derive(Debug, Default)]
struct Trace {
    /// Times (local clock, microseconds) and values, oldest first.
    points: VecDeque<(i64, f64)>,
    /// How far back to keep, in seconds: the longest window wanted.
    keep_s: f64,
    /// Wanted since the last [`Traces::finish_frame`].
    used: bool,
}

impl Traces {
    /// Keep recording `key` for at least `seconds` back. Call each frame for
    /// every series a graph on the open tab shows. Returns `true` if the
    /// series is new (nothing recorded yet), so the caller can start it off
    /// with the value now.
    pub fn want(&mut self, key: &SeriesKey, seconds: f64) -> bool {
        let trace = self.series.entry(key.clone()).or_default();
        if !trace.used {
            trace.keep_s = 0.0;
        }
        trace.keep_s = trace.keep_s.max(seconds);
        trace.used = true;
        trace.points.is_empty()
    }

    /// Record `value` for `key`, received at `time_us` (local clock,
    /// microseconds). Ignored if `key` isn't wanted, or the time isn't newer
    /// than the last point (the same value seen again).
    pub fn record(&mut self, key: &SeriesKey, time_us: i64, value: f64) {
        let Some(trace) = self.series.get_mut(key) else {
            return;
        };
        if trace.points.back().is_some_and(|&(t, _)| t >= time_us) {
            return;
        }
        trace.points.push_back((time_us, value));
        if trace.points.len() > MAX_POINTS {
            trace.points.pop_front();
        }
        if value.is_finite() {
            let range = self.ranges.entry(key.clone()).or_insert((value, value));
            range.0 = range.0.min(value);
            range.1 = range.1.max(value);
            match self.since.get_mut(key) {
                Some(stats) => stats.add(value),
                None => {
                    self.since.insert(key.clone(), Stats::of(value));
                }
            }
        }
    }

    /// The lowest, highest and average of every value recorded for `key`
    /// since it was first recorded, last [reset](Self::reset), or the
    /// server reconnected ([`connected`](Self::connected)).
    ///
    /// ```
    /// use raptaboard::traces::{SeriesKey, Traces};
    /// let key = SeriesKey { topic: "/Shooter/rpm".into(), field: None };
    /// let mut traces = Traces::default();
    /// traces.want(&key, 0.0);
    /// for (t, v) in [(1, 3000.0), (2, 3400.0), (3, 3200.0)] {
    ///     traces.record(&key, t, v);
    /// }
    /// let stats = traces.since_reset(&key).unwrap();
    /// assert_eq!((stats.min, stats.max, stats.mean()), (3000.0, 3400.0, 3200.0));
    /// ```
    pub fn since_reset(&self, key: &SeriesKey) -> Option<Stats> {
        self.since.get(key).copied()
    }

    /// The lowest, highest and average of `key`'s values over the last
    /// `seconds` before `now_us`: those recorded in that time, and the one
    /// before it (the value when the time began). Only as far back as the
    /// series has been kept ([`want`](Self::want) it for `seconds`).
    ///
    /// ```
    /// use raptaboard::traces::{SeriesKey, Traces};
    /// let key = SeriesKey { topic: "/Arm/angle".into(), field: None };
    /// let mut traces = Traces::default();
    /// traces.want(&key, 10.0);
    /// for s in 0..20 {
    ///     traces.record(&key, s * 1_000_000, s as f64);
    /// }
    /// // The last 2 s at 19 s: 17 (the value at 17 s), 18 and 19.
    /// let stats = traces.window(&key, 19_000_000, 2.0).unwrap();
    /// assert_eq!((stats.min, stats.max, stats.count), (17.0, 19.0, 3));
    /// ```
    pub fn window(&self, key: &SeriesKey, now_us: i64, seconds: f64) -> Option<Stats> {
        let trace = self.series.get(key)?;
        let oldest = now_us - (seconds * 1e6) as i64;
        let first = trace.points.partition_point(|&(t, _)| t < oldest);
        // The value when the window began, if it was set before it.
        let starts_on_one = trace.points.get(first).is_some_and(|&(t, _)| t == oldest);
        let from = if starts_on_one {
            first
        } else {
            first.saturating_sub(1)
        };
        Stats::from_values(trace.points.range(from..).map(|&(_, v)| v))
    }

    /// Tell the traces whether the server is connected, once a frame: when
    /// it connects again after being lost, every series' statistics since
    /// reset start again (a restarted robot's values are new). Autoscaled
    /// ranges are kept.
    pub fn connected(&mut self, connected: bool) {
        if connected && !self.was_connected {
            self.since.clear();
        }
        self.was_connected = connected;
    }

    /// The (lowest, highest) values recorded for `key` since it was first
    /// recorded or last reset, if any.
    ///
    /// ```
    /// use raptaboard::traces::{SeriesKey, Traces};
    /// let key = SeriesKey { topic: "/Arm/angle".into(), field: None };
    /// let mut traces = Traces::default();
    /// traces.want(&key, 0.0);
    /// traces.record(&key, 1, 30.0);
    /// traces.record(&key, 2, -10.0);
    /// traces.record(&key, 3, 5.0);
    /// assert_eq!(traces.range(&key), Some((-10.0, 30.0)));
    /// traces.reset(&key);
    /// assert_eq!(traces.range(&key), Some((5.0, 5.0)));
    /// ```
    pub fn range(&self, key: &SeriesKey) -> Option<(f64, f64)> {
        self.ranges.get(key).copied()
    }

    /// Forget the range and statistics seen for `key`: they start again
    /// from the newest value recorded, if it's still being recorded, or
    /// from the next.
    pub fn reset(&mut self, key: &SeriesKey) {
        let newest = self
            .series
            .get(key)
            .and_then(|t| t.points.back())
            .map(|&(_, v)| v)
            .filter(|v| v.is_finite());
        match newest {
            Some(v) => {
                self.ranges.insert(key.clone(), (v, v));
                self.since.insert(key.clone(), Stats::of(v));
            }
            None => {
                self.ranges.remove(key);
                self.since.remove(key);
            }
        }
    }

    /// The recorded points for `key`, oldest first, as (time in local
    /// microseconds, value).
    pub fn points(&self, key: &SeriesKey) -> impl Iterator<Item = (i64, f64)> + '_ {
        self.series
            .get(key)
            .into_iter()
            .flat_map(|t| t.points.iter().copied())
    }

    /// Call once a frame after recording: forgets series no graph wanted
    /// this frame, and points older than each series' window at `now_us`.
    /// One point from before the window is kept, so the line reaches the
    /// left edge.
    pub fn finish_frame(&mut self, now_us: i64) {
        self.series
            .retain(|_, trace| std::mem::take(&mut trace.used));
        for trace in self.series.values_mut() {
            let oldest = now_us - (trace.keep_s * 1e6) as i64;
            while trace.points.len() > 1 && trace.points[1].0 <= oldest {
                trace.points.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn key(topic: &str) -> SeriesKey {
        SeriesKey {
            topic: topic.into(),
            field: None,
        }
    }

    #[test]
    fn only_wanted_series_are_recorded_and_kept() {
        let mut traces = Traces::default();
        traces.record(&key("/a"), 1, 1.0);
        assert_eq!(traces.points(&key("/a")).count(), 0);
        traces.want(&key("/a"), 1.0);
        traces.record(&key("/a"), 1, 1.0);
        traces.finish_frame(1);
        assert_eq!(traces.points(&key("/a")).count(), 1);
        // Not wanted next frame: forgotten.
        traces.finish_frame(2);
        assert_eq!(traces.points(&key("/a")).count(), 0);
    }

    #[test]
    fn old_points_are_trimmed_to_the_longest_window() {
        let mut traces = Traces::default();
        let a = key("/a");
        // Two graphs want it, for 1 s and 3 s: keep 3 s.
        traces.want(&a, 1.0);
        traces.want(&a, 3.0);
        for s in 0..10 {
            traces.record(&a, s * 1_000_000, s as f64);
        }
        traces.finish_frame(9_000_000);
        let times: Vec<i64> = traces.points(&a).map(|(t, _)| t / 1_000_000).collect();
        // The window starts at 6: 6 to 9 are kept, 6 being on its edge.
        assert_eq!(times, [6, 7, 8, 9]);
        // Next frame only the 1 s graph is left.
        traces.want(&a, 1.0);
        traces.finish_frame(9_000_000);
        let times: Vec<i64> = traces.points(&a).map(|(t, _)| t / 1_000_000).collect();
        assert_eq!(times, [8, 9]);
        // Between points, the one before the edge stays to reach it.
        traces.want(&a, 0.5);
        traces.finish_frame(9_400_000);
        let times: Vec<i64> = traces.points(&a).map(|(t, _)| t / 1_000_000).collect();
        assert_eq!(times, [8, 9]);
    }

    #[test]
    fn struct_fields_are_their_own_series() {
        let mut traces = Traces::default();
        let x = SeriesKey {
            topic: "/pose".into(),
            field: Some(vec!["x".into()]),
        };
        traces.want(&x, 5.0);
        traces.record(&x, 1, 2.0);
        assert_eq!(traces.points(&x).collect::<Vec<_>>(), [(1, 2.0)]);
        assert_eq!(traces.points(&key("/pose")).count(), 0);
    }

    #[test]
    fn a_runaway_series_is_capped() {
        let mut traces = Traces::default();
        let a = key("/fast");
        traces.want(&a, 1e9);
        for t in 0..(MAX_POINTS as i64 + 10) {
            traces.record(&a, t, 0.0);
        }
        assert_eq!(traces.points(&a).count(), MAX_POINTS);
        assert_eq!(traces.points(&a).next().unwrap().0, 10);
    }

    #[test]
    fn statistics_since_reset_start_again_on_reconnect_but_ranges_stay() {
        let mut traces = Traces::default();
        let a = key("/a");
        traces.want(&a, 0.0);
        traces.connected(true);
        traces.record(&a, 1, 2.0);
        traces.record(&a, 2, 8.0);
        // Lost: nothing changes yet.
        traces.connected(false);
        assert_eq!(traces.since_reset(&a).map(|s| s.max), Some(8.0));
        // Back: the statistics start again, the autoscaled range doesn't.
        traces.connected(true);
        assert_eq!(traces.since_reset(&a), None);
        assert_eq!(traces.range(&a), Some((2.0, 8.0)));
        traces.record(&a, 3, 5.0);
        let s = traces.since_reset(&a).unwrap();
        assert_eq!((s.min, s.max, s.count), (5.0, 5.0, 1));
        // Staying connected changes nothing.
        traces.connected(true);
        assert_eq!(traces.since_reset(&a).map(|s| s.count), Some(1));
    }

    #[test]
    fn a_window_starts_with_the_value_held_into_it() {
        let mut traces = Traces::default();
        let a = key("/a");
        traces.want(&a, 10.0);
        traces.record(&a, 0, 100.0);
        traces.record(&a, 5_000_000, 1.0);
        // The last 2 s at 6 s: 1 (set at 5 s), and 100, the value when the
        // window began (at 4 s).
        let s = traces.window(&a, 6_000_000, 2.0).unwrap();
        assert_eq!((s.min, s.max, s.count), (1.0, 100.0, 2));
        // A window starting exactly on a point starts with it.
        let s = traces.window(&a, 7_000_000, 2.0).unwrap();
        assert_eq!((s.min, s.max, s.count), (1.0, 1.0, 1));
        // Nothing new for a long time: the value held.
        let s = traces.window(&a, 60_000_000, 2.0).unwrap();
        assert_eq!((s.min, s.max), (1.0, 1.0));
        // A series not recorded: nothing.
        assert!(traces.window(&key("/b"), 6_000_000, 2.0).is_none());
    }
}
