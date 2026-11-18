//! The card most widgets are drawn in: a rounded box with the title in a
//! strip along the top. [`draw`] draws a whole widget through its
//! [`WidgetType`](crate::widgets::WidgetType), whose default `draw` uses this card;
//! a widget with its own frame uses the pieces here as it needs.

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
        let font = egui::FontId::proportional(12.0);
        for (line, dy) in [
            (widget.widget_type.as_str(), -8.0),
            (widget.topic().unwrap_or(""), 8.0),
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
