//! Drawing a plugin's picture with egui: its shapes, in pixels from the top
//! left of the widget's body.

use eframe::egui::{self, Pos2, Rect, Stroke, StrokeKind, Vec2, epaint};
use raptaboard_plugin_host::runtime::{Align, Point, Shape, Stroke as PluginStroke};

use super::convert::egui_color;

/// How many straight pieces an arc is drawn with, per radian.
const ARC_STEPS_PER_RADIAN: f32 = 12.0;

/// Draw `shapes` with `painter`, `origin` being the body's top-left corner.
/// Images aren't drawn yet.
pub fn shapes(painter: &egui::Painter, origin: Pos2, shapes: &[Shape]) {
    let at = |p: &Point| origin + Vec2::new(p.x, p.y);
    let stroke = |s: &PluginStroke| Stroke::new(s.width.max(0.0), egui_color(&s.color));
    for shape in shapes {
        match shape {
            Shape::Line(l) => {
                painter.line_segment([at(&l.start), at(&l.end)], stroke(&l.stroke));
            }
            Shape::Polyline(p) => {
                let points: Vec<Pos2> = p.points.iter().map(at).collect();
                let fill = p
                    .fill
                    .as_ref()
                    .map_or(egui::Color32::TRANSPARENT, egui_color);
                let path = if p.closed {
                    epaint::PathShape::convex_polygon(points, fill, stroke(&p.stroke))
                } else {
                    epaint::PathShape::line(points, stroke(&p.stroke))
                };
                painter.add(path);
            }
            Shape::Arc(a) => {
                let sweep = a.end_rad - a.start_rad;
                let steps = ((sweep.abs() * ARC_STEPS_PER_RADIAN).ceil() as usize).clamp(2, 512);
                let center = at(&a.center);
                // Counter-clockwise on screen: y grows down, so it's negated.
                let points = (0..=steps)
                    .map(|i| {
                        let t = a.start_rad + sweep * i as f32 / steps as f32;
                        center + Vec2::new(t.cos(), -t.sin()) * a.radius
                    })
                    .collect();
                painter.add(epaint::PathShape::line(points, stroke(&a.stroke)));
            }
            Shape::Rect(r) => {
                let rect = Rect::from_two_pos(at(&r.min), at(&r.max));
                let fill = r
                    .fill
                    .as_ref()
                    .map_or(egui::Color32::TRANSPARENT, egui_color);
                let line = r.stroke.as_ref().map_or(Stroke::NONE, stroke);
                let radius = r.corner_radius.clamp(0.0, 255.0) as u8;
                painter.rect(rect, radius, fill, line, StrokeKind::Inside);
            }
            Shape::Circle(c) => {
                let fill = c
                    .fill
                    .as_ref()
                    .map_or(egui::Color32::TRANSPARENT, egui_color);
                let line = c.stroke.as_ref().map_or(Stroke::NONE, stroke);
                painter.circle(at(&c.center), c.radius.max(0.0), fill, line);
            }
            Shape::Text(t) => {
                let size = t.size.clamp(1.0, 1000.0);
                let font = if t.monospace {
                    egui::FontId::monospace(size)
                } else {
                    egui::FontId::proportional(size)
                };
                let wrap = t.max_width.unwrap_or(f32::INFINITY);
                let galley = painter.layout(t.text.clone(), font, egui_color(&t.color), wrap);
                let width = galley.size().x;
                let pos = at(&t.pos);
                let left = match t.align {
                    Align::Left => pos.x,
                    Align::Center => pos.x - width / 2.0,
                    Align::Right => pos.x - width,
                };
                painter.galley(Pos2::new(left, pos.y), galley, egui_color(&t.color));
            }
            Shape::Image(_) => {}
        }
    }
}

/// A plugin region's rectangle, in the window.
pub fn region_rect(origin: Pos2, min: &Point, max: &Point) -> Rect {
    Rect::from_two_pos(
        origin + Vec2::new(min.x, min.y),
        origin + Vec2::new(max.x, max.y),
    )
}
