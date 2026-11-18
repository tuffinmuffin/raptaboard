//! The card every widget is drawn in: a rounded box with the title in a
//! strip along the top. [`draw`] draws a whole widget, asking its
//! [`WidgetType`](super::WidgetType) to fill the body.

use eframe::egui::{self, CornerRadius, Rect, Stroke, StrokeKind};
use nt4::Value;
use raptaboard_layout::WidgetContainer;

/// Height of a card's title strip, in points.
pub const TITLE_HEIGHT: f32 = 24.0;
/// Space between the card's edge and its body, in points.
const BODY_MARGIN: f32 = 6.0;

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
        CornerRadius::same(10),
        visuals.extreme_bg_color,
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );
    let (title_rect, body_rect) = rect.split_top_bottom_at_y(rect.min.y + TITLE_HEIGHT);
    painter.rect_filled(
        title_rect.shrink(1.0),
        CornerRadius {
            nw: 9,
            ne: 9,
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
            egui::FontId::proportional(14.0),
            visuals.text_color(),
        );
    body_rect
}

/// Draw a widget card: a rounded box, its title on top, and its body.
/// `value` is the newest value of the widget's topic, if any. Types that
/// aren't built yet say what they are and what they're bound to.
pub fn draw(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    widget: &WidgetContainer,
    value: Option<&Value>,
) {
    let body_rect = draw_card(painter, visuals, rect, &widget.title);
    // Keep long text inside the card.
    let painter = painter.with_clip_rect(body_rect.intersect(painter.clip_rect()));
    let body_rect = body_rect.shrink(BODY_MARGIN);
    match super::find(&widget.widget_type) {
        Some(ty) => ty.draw_body(&painter, visuals, body_rect, &widget.properties, value),
        None => {
            let font = egui::FontId::proportional(12.0);
            let center = body_rect.center();
            for (line, dy) in [
                (widget.widget_type.as_str(), -8.0),
                (widget.topic().unwrap_or(""), 8.0),
            ] {
                painter.text(
                    center + egui::vec2(0.0, dy),
                    egui::Align2::CENTER_CENTER,
                    line,
                    font.clone(),
                    visuals.weak_text_color(),
                );
            }
        }
    }
}

/// Say in grey why a widget has nothing to show: `"no value"` yet, or a
/// value of the wrong type.
pub fn draw_message(painter: &egui::Painter, visuals: &egui::Visuals, rect: Rect, why: &str) {
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        why,
        egui::FontId::proportional(12.0),
        visuals.weak_text_color(),
    );
}
