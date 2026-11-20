//! Touchscreen taps, made to land where they were tapped.
//!
//! A touchscreen on macOS sends a whole tap in one frame: a move to the
//! tap, the press and release there, then a move back to where the mouse
//! was and "pointer gone" (seen in the input log). egui gives a click to the
//! widget under the pointer at the end of the frame, so the tap went to the
//! widget under the mouse, or to nothing once the pointer was gone. Held
//! back to the next frame, those last moves let egui see the tap where it
//! was.

use eframe::egui::{self, Event, PointerButton};

/// Holds the pointer moves (and "pointer gone") that come after a release
/// in a frame, for the next frame. Used from `eframe::App::raw_input_hook`,
/// before egui reads the frame's input.
#[derive(Debug, Default)]
pub(crate) struct TapHold {
    /// Events held for the next frame.
    later: Vec<Event>,
}

impl TapHold {
    /// Fix up one frame's `events`: last frame's held events go first, and
    /// moves and "pointer gone" after this frame's last primary release
    /// wait for the next frame. Returns whether any are waiting (the next
    /// frame should come soon).
    pub(crate) fn apply(&mut self, events: &mut Vec<Event>) -> bool {
        let mut all = std::mem::take(&mut self.later);
        all.append(events);
        let release = all.iter().rposition(|e| {
            matches!(
                e,
                Event::PointerButton {
                    button: PointerButton::Primary,
                    pressed: false,
                    ..
                }
            )
        });
        for (i, event) in all.into_iter().enumerate() {
            let after_release = release.is_some_and(|r| i > r);
            if after_release && matches!(event, Event::PointerMoved(_) | Event::PointerGone) {
                self.later.push(event);
            } else {
                events.push(event);
            }
        }
        !self.later.is_empty()
    }
}

/// Apply `hold` to a frame's `raw` input, asking for the next frame soon if
/// it held anything back, and noting it in the input log.
pub(crate) fn hold_after_taps(hold: &mut TapHold, ctx: &egui::Context, raw: &mut egui::RawInput) {
    if hold.apply(&mut raw.events) {
        ctx.request_repaint();
        let held = hold.later.len();
        crate::input_log::log(raw.time.unwrap_or_default(), || {
            format!("held {held} pointer event(s) after a release for the next frame")
        });
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn button(x: f32, pressed: bool) -> Event {
        Event::PointerButton {
            pos: egui::pos2(x, 0.0),
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn moves_after_a_release_wait_a_frame() {
        let mut hold = TapHold::default();
        let tap = egui::pos2(500.0, 0.0);
        let mouse = egui::pos2(1128.0, 0.0);
        let mut frame = vec![
            Event::PointerMoved(tap),
            button(500.0, true),
            button(500.0, false),
            Event::PointerMoved(mouse),
            Event::PointerGone,
        ];
        assert!(hold.apply(&mut frame));
        assert_eq!(
            frame,
            [
                Event::PointerMoved(tap),
                button(500.0, true),
                button(500.0, false)
            ]
        );
        // The next frame gets them, first.
        let mut next = vec![Event::PointerMoved(tap)];
        assert!(!hold.apply(&mut next));
        assert_eq!(
            next,
            [
                Event::PointerMoved(mouse),
                Event::PointerGone,
                Event::PointerMoved(tap)
            ]
        );
        // Without a release nothing waits.
        let mut drag = vec![button(1.0, true), Event::PointerMoved(mouse)];
        assert!(!hold.apply(&mut drag));
        assert_eq!(drag.len(), 2);
    }
}
