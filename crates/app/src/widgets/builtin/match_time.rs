//! Match Time: the time left in the match, as big as fits, colored by how
//! little is left: blue above 60 s, green to 60 s, yellow from
//! `yellow_start_time` (30 s), red from `red_start_time` (20 s).
//!
//! Settings are Elastic's: `time_display_mode` (`Minutes and Seconds`, as
//! `1:05`, or `Seconds Only`, as `65`) and the two color times.

use eframe::egui::{self, Color32, Rect};
use nt4::Value;
use serde_json::{Map, Value as Json};

use crate::widgets::shared::format::number_of;
use crate::widgets::shared::{card, props};
use crate::widgets::{ValueInfo, WidgetType};

/// `Minutes and Seconds` or `Seconds Only` (Elastic).
pub const TIME_DISPLAY_MODE: &str = "time_display_mode";
/// Seconds left at which the time turns red (Elastic).
pub const RED_START_TIME: &str = "red_start_time";
/// Seconds left at which the time turns yellow (Elastic).
pub const YELLOW_START_TIME: &str = "yellow_start_time";

const KEYS: [&str; 3] = [TIME_DISPLAY_MODE, RED_START_TIME, YELLOW_START_TIME];

/// Time shown as minutes and seconds, `1:05` (Elastic's default).
pub const MINUTES_AND_SECONDS: &str = "Minutes and Seconds";
/// Time shown as seconds, `65`.
pub const SECONDS_ONLY: &str = "Seconds Only";

const DEFAULT_RED: i64 = 20;
const DEFAULT_YELLOW: i64 = 30;

/// Elastic's colors (Material red, yellow, green, blue 500).
const RED: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);
const YELLOW: Color32 = Color32::from_rgb(0xFF, 0xEB, 0x3B);
const GREEN: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
const BLUE: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);

/// The Match Time widget (Elastic's `"Match Time"`).
pub struct MatchTime;

/// The time as shown: whole seconds, as `m:ss` unless `seconds_only`.
/// Negative times (no match running) show as plain seconds.
///
/// ```
/// use raptaboard::widgets::builtin::match_time::time_text;
/// assert_eq!(time_text(65.7, false), "1:05");
/// assert_eq!(time_text(65.7, true), "65");
/// assert_eq!(time_text(-1.0, false), "-1");
/// ```
pub fn time_text(seconds: f64, seconds_only: bool) -> String {
    let whole = seconds.floor() as i64;
    if seconds_only || whole < 0 {
        whole.to_string()
    } else {
        format!("{}:{:02}", whole / 60, whole % 60)
    }
}

/// The color for `seconds` left, with the red and yellow start times.
///
/// ```
/// use raptaboard::widgets::builtin::match_time::time_color;
/// use eframe::egui::Color32;
/// assert_eq!(time_color(15.0, 20, 30), Color32::from_rgb(0xF4, 0x43, 0x36));
/// assert_eq!(time_color(90.0, 20, 30), Color32::from_rgb(0x21, 0x96, 0xF3));
/// ```
pub fn time_color(seconds: f64, red: i64, yellow: i64) -> Color32 {
    let t = seconds.floor();
    if t <= red as f64 {
        RED
    } else if t <= yellow as f64 {
        YELLOW
    } else if t <= 60.0 {
        GREEN
    } else {
        BLUE
    }
}

impl WidgetType for MatchTime {
    fn names(&self) -> &'static [&'static str] {
        &["Match Time"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        matches!(nt_type, "double" | "float" | "int")
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        p: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        let Some(seconds) = value.and_then(number_of) else {
            let why = if value.is_some() {
                "not a number"
            } else {
                "no value"
            };
            return card::draw_message(painter, visuals, rect, why);
        };
        let seconds_only = props::text(p, TIME_DISPLAY_MODE, MINUTES_AND_SECONDS)
            .eq_ignore_ascii_case(SECONDS_ONLY);
        let red = props::number(p, RED_START_TIME, DEFAULT_RED as f64) as i64;
        let yellow = props::number(p, YELLOW_START_TIME, DEFAULT_YELLOW as f64) as i64;
        card::draw_fitted(
            painter,
            rect,
            &time_text(seconds, seconds_only),
            time_color(seconds, red, yellow),
        );
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::choice_row(
            ui,
            p,
            "Show",
            TIME_DISPLAY_MODE,
            MINUTES_AND_SECONDS,
            &[MINUTES_AND_SECONDS, SECONDS_ONLY],
        );
        props::whole_row(
            ui,
            p,
            "Yellow from (s)",
            YELLOW_START_TIME,
            DEFAULT_YELLOW,
            0..=300,
        );
        props::whole_row(ui, p, "Red from (s)", RED_START_TIME, DEFAULT_RED, 0..=300);
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn times_and_colors() {
        assert_eq!(time_text(0.0, false), "0:00");
        assert_eq!(time_text(135.0, false), "2:15");
        assert_eq!(time_color(20.0, 20, 30), RED);
        assert_eq!(time_color(25.9, 20, 30), YELLOW);
        assert_eq!(time_color(30.0, 20, 30), YELLOW);
        assert_eq!(time_color(45.0, 20, 30), GREEN);
        assert_eq!(time_color(61.0, 20, 30), BLUE);
        // No match running: Elastic shows -1 in red.
        assert_eq!(time_color(-1.0, 20, 30), RED);
    }
}
