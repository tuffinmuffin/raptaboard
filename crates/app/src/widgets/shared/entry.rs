//! The box a widget's value is typed into (Number Slider, and a Text
//! Display with `editable` on).
//!
//! A widget [`open`]s it when its value is clicked, then calls [`show`]
//! every frame while it [`is_open`]. It opens with the value selected, so
//! typing replaces it. Enter gives back the text if the widget accepts it;
//! text it doesn't accept turns red and the box stays open to be fixed.
//! Escape, or a click anywhere else, closes it with nothing sent. What's in
//! the box is kept in egui's memory under the id the widget gives.

use eframe::egui::{self, Color32, Id, Rect};

/// The color of the dot that marks a widget whose value can be typed in
/// (and of the slider's fill): Material blue.
pub const EDITABLE_COLOR: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);
/// The dot's radius and its distance from the corner, in points.
const DOT_RADIUS: f32 = 3.5;
const DOT_INSET: f32 = 7.0;

/// The box's state while it's open.
#[derive(Clone, Debug, Default)]
struct Entry {
    text: String,
    /// The box has been given the keyboard (the frame it opens).
    focused: bool,
    /// Enter was pressed on text the widget didn't accept.
    bad: bool,
}

/// Whether the box with `id` is open.
pub fn is_open(ui: &egui::Ui, id: Id) -> bool {
    ui.data(|d| d.get_temp::<Entry>(id).is_some())
}

/// Open the box with `id`, holding `text` (all selected).
pub fn open(ui: &egui::Ui, id: Id, text: String) {
    ui.data_mut(|d| {
        d.insert_temp(
            id,
            Entry {
                text,
                ..Entry::default()
            },
        );
    });
}

/// Show the box with `id` in `rect` if it's open. Returns the text when
/// Enter is pressed on text `accept` takes (and closes the box); `None`
/// otherwise.
pub fn show(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    font: egui::FontId,
    accept: impl Fn(&str) -> bool,
) -> Option<String> {
    let mut entry: Entry = ui.data(|d| d.get_temp(id))?;
    let box_id = id.with("box");
    let mut edit = egui::TextEdit::singleline(&mut entry.text)
        .id(box_id)
        .font(font)
        .align(egui::Align2::CENTER_CENTER);
    if entry.bad {
        edit = edit.text_color(ui.visuals().error_fg_color);
    }
    let r = ui.put(rect, edit);
    let mut done = None;
    let mut close = false;
    if !entry.focused {
        r.request_focus();
        entry.focused = true;
        // The text starts selected, so typing replaces it.
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), box_id) {
            let all = egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(entry.text.chars().count()),
            );
            state.cursor.set_char_range(Some(all));
            state.store(ui.ctx(), box_id);
        }
    } else if r.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if accept(&entry.text) {
                done = Some(entry.text.clone());
                close = true;
            } else {
                // Not taken: stay open, in red, to be fixed.
                entry.bad = true;
                r.request_focus();
            }
        } else {
            // Escape, or a click somewhere else: nothing sent.
            close = true;
        }
    } else if r.changed() {
        entry.bad = false;
    }
    ui.data_mut(|d| {
        if close {
            d.remove::<Entry>(id);
        } else {
            d.insert_temp(id, entry);
        }
    });
    done
}

/// Draw the dot in the top right corner of `rect` that says the widget's
/// value can be typed in.
pub fn draw_editable_dot(painter: &egui::Painter, rect: Rect) {
    let at = rect.right_top() + egui::vec2(-DOT_INSET, DOT_INSET);
    painter.circle_filled(at, DOT_RADIUS, EDITABLE_COLOR);
}
