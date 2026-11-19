//! The card most widgets are drawn in: a rounded box with the title in a
//! strip along the top. [`draw`] draws a whole widget through its
//! [`WidgetType`](crate::widgets::WidgetType), whose default `draw` uses this card;
//! a widget with its own frame uses the pieces here as it needs.

use eframe::egui::{self, CornerRadius, Rect, Stroke, StrokeKind};
use nt4::Value;
use raptaboard_layout::WidgetContainer;

// How cards look. All sizes are screen points, not grid pixels: they stay
// the same at any grid size, so text stays readable.

/// Height of a card's title strip, in points.
pub const TITLE_HEIGHT: f32 = 24.0;
/// Space between the card's edge and its body, in points.
pub const BODY_MARGIN: f32 = 6.0;
/// Corner radius of the card, in points.
pub const CORNER_RADIUS: u8 = 10;
/// Corner radius for shapes filling the body, such as a Boolean Box's
/// color, in points. Smaller than the card's, as the body sits inside it.
pub const BODY_CORNER_RADIUS: u8 = 6;
/// Width of the card's border, in points.
pub const BORDER_WIDTH: f32 = 1.0;
/// Size of the title text, in points.
pub const TITLE_FONT_SIZE: f32 = 14.0;
/// Size of grey messages ("no value") and placeholder text, in points.
pub const MESSAGE_FONT_SIZE: f32 = 12.0;
/// Distance of each of the placeholder's two lines from the body's middle,
/// in points.
const PLACEHOLDER_LINE_OFFSET: f32 = 8.0;

/// Draw an empty card with `title` in a strip along the top, and return
/// the space left below the title for the body. Also used for List Layouts.
pub fn draw_card(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    title: &str,
) -> Rect {
    painter.rect(
        rect,
        CornerRadius::same(CORNER_RADIUS),
        visuals.extreme_bg_color,
        Stroke::new(BORDER_WIDTH, visuals.widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );
    let (title_rect, body_rect) = rect.split_top_bottom_at_y(rect.min.y + TITLE_HEIGHT);
    painter.rect_filled(
        // Inside the border, with corners that follow the card's.
        title_rect.shrink(BORDER_WIDTH),
        CornerRadius {
            nw: CORNER_RADIUS - BORDER_WIDTH as u8,
            ne: CORNER_RADIUS - BORDER_WIDTH as u8,
            sw: 0,
            se: 0,
        },
        visuals.faint_bg_color,
    );
    painter
        .with_clip_rect(title_rect.intersect(painter.clip_rect()))
        .text(
            title_rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(TITLE_FONT_SIZE),
            visuals.text_color(),
        );
    body_rect
}

/// Run `draw` with a painter that can't draw outside `body`, and the body
/// less a small margin, so long text stays inside the card.
pub fn draw_clipped(painter: &egui::Painter, body: Rect, draw: impl FnOnce(&egui::Painter, Rect)) {
    let inside = painter.with_clip_rect(body.intersect(painter.clip_rect()));
    draw(&inside, body.shrink(BODY_MARGIN));
}

/// Draw a widget in `rect` with its type's [`draw`](crate::widgets::WidgetType::draw)
/// (the standard card unless the type has its own). `value` is the newest
/// value of the widget's topic, if any. Types that aren't built yet get a
/// card saying what they are and what they're bound to.
pub fn draw(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    widget: &WidgetContainer,
    value: Option<&Value>,
) {
    if let Some(ty) = crate::widgets::find(&widget.widget_type) {
        ty.draw(painter, visuals, rect, widget, value);
        return;
    }
    let body = draw_card(painter, visuals, rect, &widget.title);
    draw_clipped(painter, body, |painter, body| {
        let font = egui::FontId::proportional(MESSAGE_FONT_SIZE);
        for (line, dy) in [
            (widget.widget_type.as_str(), -PLACEHOLDER_LINE_OFFSET),
            (widget.topic().unwrap_or(""), PLACEHOLDER_LINE_OFFSET),
        ] {
            painter.text(
                body.center() + egui::vec2(0.0, dy),
                egui::Align2::CENTER_CENTER,
                line,
                font.clone(),
                visuals.weak_text_color(),
            );
        }
    });
}

/// Draw `text` as large as fits in `rect`, centered, in `color`. For
/// widgets whose whole body is one value (Large Text Display, Match Time).
pub fn draw_fitted(painter: &egui::Painter, rect: Rect, text: &str, color: egui::Color32) {
    // Measure at a known size, then scale to fit both ways.
    const MEASURE: f32 = 100.0;
    let galley =
        painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(MEASURE), color);
    let size = galley.size();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let font = (MEASURE * scale).clamp(4.0, 400.0);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(font),
        color,
    );
}

/// Say in grey why a widget has nothing to show: `"no value"` yet, or a
/// value of the wrong type.
pub fn draw_message(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect, why: &str) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        why,
        egui::FontId::proportional(MESSAGE_FONT_SIZE),
        visuals.weak_text_color(),
    );
}
