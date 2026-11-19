//! Recent values of the topics graphs show, for drawing them over time.
//!
//! The NT client keeps only each topic's newest value, so the app records
//! every value graphs want here, taken from the client's event stream so none
//! are lost between frames. Each series is a ring buffer (`VecDeque`) that
//! grows as needed up to [`MAX_POINTS`]. Only the series some graph on
//! the open tab wants are kept, each for the longest time window a graph
//! wants it for; a series no graph wants any more is forgotten.

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
}
