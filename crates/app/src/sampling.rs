//! Showing each widget's value once per its update period.
//!
//! The window redraws whenever it needs to: on a timer, but also when the
//! mouse moves or a menu opens. And the NT client keeps one newest value per
//! topic, which can arrive faster than a widget asked for (the tree may be
//! watching the same topic). If widgets showed the newest value on every
//! redraw, a widget set to 1 second would jump ahead in bursts whenever the
//! mouse moved. [`Sampler`] takes a new value for each widget only once per
//! its period, so it changes on a steady beat.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use nt4::Value;

/// The values widgets are showing, one per topic and period.
///
/// ```
/// use std::time::{Duration, Instant};
/// use nt4::Value;
/// use raptaboard::sampling::Sampler;
/// let mut sampler = Sampler::default();
/// let start = Instant::now();
/// let at = |ms| start + Duration::from_millis(ms);
/// // The first value shows at once, then nothing new until a second has passed.
/// assert_eq!(sampler.sample("/Sine", 1.0, Some(&Value::Double(0.1)), at(0)), Some(Value::Double(0.1)));
/// assert_eq!(sampler.sample("/Sine", 1.0, Some(&Value::Double(0.2)), at(500)), Some(Value::Double(0.1)));
/// assert_eq!(sampler.sample("/Sine", 1.0, Some(&Value::Double(0.3)), at(1000)), Some(Value::Double(0.3)));
/// ```
#[derive(Debug, Default)]
pub struct Sampler {
    shown: HashMap<(String, u64), Shown>,
}

#[derive(Debug)]
struct Shown {
    value: Option<Value>,
    /// When to take the next value.
    due: Instant,
    /// Asked for since the last [`Sampler::finish_frame`].
    used: bool,
}

impl Sampler {
    /// The value to show for `topic` in a widget updating every `period_s`
    /// seconds, given the newest value received (`latest`) and the time
    /// now. A new value is taken at most once per period, on a steady beat;
    /// until the first value arrives, it's taken as soon as it does.
    pub fn sample(
        &mut self,
        topic: &str,
        period_s: f64,
        latest: Option<&Value>,
        now: Instant,
    ) -> Option<Value> {
        let period = Duration::from_secs_f64(period_s.max(0.001));
        let shown = self
            .shown
            .entry((topic.to_owned(), period_s.to_bits()))
            .or_insert(Shown {
                value: None,
                due: now,
                used: true,
            });
        shown.used = true;
        if now >= shown.due && latest.is_some() {
            shown.value = latest.cloned();
            // Keep to the beat even if this frame came a little late; after
            // a long gap (window hidden), start a new beat from now.
            shown.due += period;
            if shown.due <= now {
                shown.due = now + period;
            }
        }
        shown.value.clone()
    }

    /// Call once a frame after drawing: forgets values no widget asked for
    /// this frame (another tab, a deleted widget), and returns how long until
    /// a widget is due for its next value, if any is waiting for one.
    pub fn finish_frame(&mut self, now: Instant) -> Option<Duration> {
        self.shown.retain(|_, s| std::mem::take(&mut s.used));
        self.shown
            .values()
            .filter(|s| s.value.is_some())
            .map(|s| s.due.saturating_duration_since(now))
            .min()
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn d(v: f64) -> Value {
        Value::Double(v)
    }

    #[test]
    fn fast_redraws_dont_speed_a_slow_widget_up() {
        let mut sampler = Sampler::default();
        let start = Instant::now();
        let mut changes = 0;
        let mut last = None;
        // Redraw every 16 ms for 5 seconds with a new value every frame, as
        // when the mouse is moving: a 1 s widget changes 5 times (at 0, 1,
        // 2, 3 and 4 s), not 300.
        for frame in 0..=312u64 {
            let now = start + Duration::from_millis(frame * 16);
            let shown = sampler.sample("/Sine", 1.0, Some(&d(frame as f64)), now);
            if shown != last {
                changes += 1;
                last = shown;
            }
        }
        assert_eq!(changes, 5);
    }

    #[test]
    fn the_beat_stays_steady_when_frames_are_late() {
        let mut sampler = Sampler::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        sampler.sample("/t", 1.0, Some(&d(0.0)), at(0));
        // A frame 30 ms late takes the value, and the next one is still due at 2 s.
        assert_eq!(
            sampler.sample("/t", 1.0, Some(&d(1.0)), at(1030)),
            Some(d(1.0))
        );
        assert_eq!(
            sampler.finish_frame(at(1030)),
            Some(Duration::from_millis(970))
        );
        assert_eq!(
            sampler.sample("/t", 1.0, Some(&d(2.0)), at(2000)),
            Some(d(2.0))
        );
        // After a long gap the beat starts again from now.
        assert_eq!(
            sampler.sample("/t", 1.0, Some(&d(9.0)), at(9500)),
            Some(d(9.0))
        );
        assert_eq!(sampler.finish_frame(at(9500)), Some(Duration::from_secs(1)));
    }

    #[test]
    fn widgets_without_values_wait_for_the_first_one() {
        let mut sampler = Sampler::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        assert_eq!(sampler.sample("/t", 1.0, None, at(0)), None);
        // Nothing to wait for, so no redraw asked for.
        assert_eq!(sampler.finish_frame(at(0)), None);
        // The first value shows as soon as it arrives.
        assert_eq!(
            sampler.sample("/t", 1.0, Some(&d(1.0)), at(300)),
            Some(d(1.0))
        );
        assert_eq!(
            sampler.sample("/t", 1.0, Some(&d(2.0)), at(800)),
            Some(d(1.0))
        );
    }

    #[test]
    fn each_period_keeps_its_own_beat_and_unused_ones_are_forgotten() {
        let mut sampler = Sampler::default();
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        sampler.sample("/t", 1.0, Some(&d(0.0)), at(0));
        sampler.sample("/t", 0.1, Some(&d(0.0)), at(0));
        assert_eq!(
            sampler.sample("/t", 0.1, Some(&d(1.0)), at(100)),
            Some(d(1.0))
        );
        assert_eq!(
            sampler.sample("/t", 1.0, Some(&d(1.0)), at(100)),
            Some(d(0.0))
        );
        // The soonest due is the 0.1 s widget.
        assert_eq!(
            sampler.finish_frame(at(100)),
            Some(Duration::from_millis(100))
        );
        // Only the 1 s widget asked this frame; the 0.1 s one is forgotten
        // and starts fresh if it comes back.
        sampler.sample("/t", 1.0, Some(&d(2.0)), at(150));
        assert_eq!(
            sampler.finish_frame(at(150)),
            Some(Duration::from_millis(850))
        );
        assert_eq!(
            sampler.sample("/t", 0.1, Some(&d(3.0)), at(160)),
            Some(d(3.0))
        );
    }
}
