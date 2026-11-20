//! How an alert level looks: its color and its sign (✕ for errors, ! for
//! warnings, i for infos), drawn by the Alerts widget, the tab marks and the
//! alert history.

use eframe::egui::{self, Color32, Pos2, Stroke};

use crate::types::alerts::Level;

/// The color of `level`: red, yellow, blue (WPILib 2027's).
pub fn color(level: Level) -> Color32 {
    match level {
        Level::Error => Color32::from_rgb(0xF4, 0x43, 0x36),
        Level::Warning => Color32::from_rgb(0xFF, 0xC1, 0x07),
        Level::Info => Color32::from_rgb(0x21, 0x96, 0xF3),
    }
}

/// Draw the ✕ (an error's sign) at `c`, `size` across.
fn draw_cross(painter: &egui::Painter, c: Pos2, size: f32, color: Color32) {
    let h = size / 2.0;
    let stroke = Stroke::new((size * 0.18).max(1.5), color);
    painter.line_segment([c + egui::vec2(-h, -h), c + egui::vec2(h, h)], stroke);
    painter.line_segment([c + egui::vec2(-h, h), c + egui::vec2(h, -h)], stroke);
}

/// Draw a ! (a warning's sign, `upside_down` false) or an i (info, true)
/// at `c`, `size` tall: a bar and a dot.
fn draw_bar_and_dot(
    painter: &egui::Painter,
    c: Pos2,
    size: f32,
    color: Color32,
    upside_down: bool,
) {
    let h = size / 2.0;
    let width = (size * 0.2).max(1.5);
    let dot = width * 0.75;
    let (bar_top, bar_bottom, dot_y) = if upside_down {
        (c.y - h * 0.35, c.y + h, c.y - h + dot)
    } else {
        (c.y - h, c.y + h * 0.35, c.y + h - dot)
    };
    painter.line_segment(
        [egui::pos2(c.x, bar_top), egui::pos2(c.x, bar_bottom)],
        Stroke::new(width, color),
    );
    painter.circle_filled(egui::pos2(c.x, dot_y), dot, color);
}

/// Draw `level`'s sign at `c`, `size` across, in `color`.
pub fn draw_sign(painter: &egui::Painter, level: Level, c: Pos2, size: f32, color: Color32) {
    match level {
        Level::Error => draw_cross(painter, c, size * 0.8, color),
        Level::Warning => draw_bar_and_dot(painter, c, size, color, false),
        Level::Info => draw_bar_and_dot(painter, c, size, color, true),
    }
}

/// Draw a badge at the right end of `rect` (a tab or button), `inset`
/// points in from its edge: a dot in `level`'s color with its sign in black.
/// The text in `rect` should leave room for it.
pub fn draw_badge(painter: &egui::Painter, level: Level, rect: egui::Rect, inset: f32) {
    let r = (rect.height() * 0.32).min(7.0);
    let c = egui::pos2(rect.right() - r - inset, rect.center().y);
    painter.circle_filled(c, r, color(level));
    draw_sign(painter, level, c, r * 1.2, Color32::BLACK);
}
