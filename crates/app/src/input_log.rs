//! A log of pointer and touch input, for working out odd clicks and taps
//! (a touchscreen tap that lands where the mouse was). Off unless the
//! `RAPTABOARD_LOG_INPUT` environment variable is set (to anything but
//! `0`); then each line goes to the terminal (stderr):
//!
//! ```sh
//! RAPTABOARD_LOG_INPUT=1 cargo run -p raptaboard -- --address sim layouts/robot2027_sim.json
//! ```
//!
//! Lines start with egui's time in seconds. `event` lines are egui's raw
//! pointer and touch events each frame; other lines are what widgets made
//! of them (the Field's target: where a press began, click or drag, what
//! was sent).

use std::sync::OnceLock;

use eframe::egui;

/// The environment variable that turns the log on.
pub const ENV_VAR: &str = "RAPTABOARD_LOG_INPUT";

/// Whether the log is on (read once).
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var(ENV_VAR).is_ok_and(|v| v != "0" && !v.is_empty()))
}

/// Log a line made by `line`, at egui time `time` (seconds), if the log is
/// on. `line` is only called then, so building it costs nothing otherwise.
pub fn log(time: f64, line: impl FnOnce() -> String) {
    if enabled() {
        eprintln!("[input {time:9.3}] {}", line());
    }
}

/// One of egui's input events as a log line, if it's about the pointer or
/// touch (keys and text aren't logged).
pub fn describe(event: &egui::Event) -> Option<String> {
    let pos = |p: egui::Pos2| format!("({:.0}, {:.0})", p.x, p.y);
    Some(match event {
        egui::Event::PointerMoved(p) => format!("event moved to {}", pos(*p)),
        egui::Event::PointerButton {
            pos: p,
            button,
            pressed,
            modifiers,
        } => format!(
            "event {:?} {} at {}{}",
            button,
            if *pressed { "pressed" } else { "released" },
            pos(*p),
            if modifiers.is_none() {
                String::new()
            } else {
                format!(" with {modifiers:?}")
            }
        ),
        egui::Event::PointerGone => "event pointer gone".to_owned(),
        egui::Event::Touch {
            device_id,
            id,
            phase,
            pos: p,
            force,
        } => format!(
            "event touch {phase:?} id {:?} device {:?} at {}{}",
            id.0,
            device_id.0,
            pos(*p),
            force.map(|f| format!(" force {f:.2}")).unwrap_or_default()
        ),
        egui::Event::MouseWheel { delta, .. } => {
            format!("event wheel {:.1}, {:.1}", delta.x, delta.y)
        }
        _ => return None,
    })
}

/// Log this frame's pointer and touch events, if the log is on.
pub fn log_frame(ctx: &egui::Context) {
    if !enabled() {
        return;
    }
    ctx.input(|i| {
        for event in &i.raw.events {
            if let Some(line) = describe(event) {
                log(i.time, || line);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn pointer_and_touch_events_are_described_and_keys_are_not() {
        let press = egui::Event::PointerButton {
            pos: egui::pos2(10.4, 20.6),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(
            describe(&press).unwrap(),
            "event Primary pressed at (10, 21)"
        );
        let moved = egui::Event::PointerMoved(egui::pos2(1.0, 2.0));
        assert_eq!(describe(&moved).unwrap(), "event moved to (1, 2)");
        let touch = egui::Event::Touch {
            device_id: egui::TouchDeviceId(3),
            id: egui::TouchId(7),
            phase: egui::TouchPhase::Start,
            pos: egui::pos2(5.0, 6.0),
            force: None,
        };
        assert_eq!(
            describe(&touch).unwrap(),
            "event touch Start id 7 device 3 at (5, 6)"
        );
        let key = egui::Event::Text("a".into());
        assert_eq!(describe(&key), None);
    }
}
