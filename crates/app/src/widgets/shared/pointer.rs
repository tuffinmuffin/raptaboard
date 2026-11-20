//! Where the pointer really was, for taps on a touchscreen.
//!
//! A touchscreen on macOS sends a whole tap in one frame (a move to the tap,
//! the press and release there) and then puts the cursor back where the
//! mouse was, so by the end of the frame "where the pointer is" is somewhere
//! else. The button events carry where they happened.

use eframe::egui;

/// Where the primary button was let go this frame, if it was.
pub fn released_at(ui: &egui::Ui) -> Option<egui::Pos2> {
    released_in(ui.ctx())
}

/// [`released_at`], from the context (for code without a `Ui`).
pub fn released_in(ctx: &egui::Context) -> Option<egui::Pos2> {
    ctx.input(|i| {
        i.raw.events.iter().rev().find_map(|e| match e {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            } => Some(*pos),
            _ => None,
        })
    })
}
