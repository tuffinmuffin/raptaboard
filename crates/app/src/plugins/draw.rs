//! Drawing a plugin's picture with egui: its shapes, in pixels from the top
//! left of the widget's body.

use eframe::egui::{self, Pos2, Rect, Stroke, StrokeKind, Vec2, epaint};
use raptaboard_plugin_host::runtime::{Align, Pixels, Point, Shape, Stroke as PluginStroke};

use super::convert::egui_color;

/// How many straight pieces an arc is drawn with, per radian.
const ARC_STEPS_PER_RADIAN: f32 = 12.0;

/// The largest pixel image a plugin may send, a side, in pixels.
pub const MAX_PIXELS_SIDE: u32 = 4096;

/// Draw `shapes` with `painter`, `origin` being the body's top-left corner.
/// `pixels` names the widget's pixel images on the GPU and says which
/// picture these shapes are from (they're sent again only for a new one).
/// Images from files aren't drawn yet.
pub fn shapes(painter: &egui::Painter, origin: Pos2, shapes: &[Shape], pixels: (egui::Id, u64)) {
    let mut pixels_seen = 0;
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
            Shape::Pixels(p) => {
                let at = pixels.0.with(pixels_seen);
                pixels_seen += 1;
                if let Some(texture) = pixels_texture(painter.ctx(), at, pixels.1, p) {
                    let rect = Rect::from_two_pos(origin + v(&p.min), origin + v(&p.max));
                    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                    painter.image(texture, rect, uv, egui::Color32::WHITE);
                }
            }
        }
    }
}

fn v(p: &Point) -> Vec2 {
    Vec2::new(p.x, p.y)
}

/// Whether a pixel image is one the dashboard draws: a size up to
/// [`MAX_PIXELS_SIDE`] each way, with 4 bytes for every pixel.
///
/// ```
/// use raptaboard::plugins::draw::pixels_fit;
/// assert!(pixels_fit(2, 1, 8));
/// assert!(!pixels_fit(2, 1, 7));
/// assert!(!pixels_fit(0, 1, 0));
/// ```
pub fn pixels_fit(width: u32, height: u32, bytes: usize) -> bool {
    (1..=MAX_PIXELS_SIDE).contains(&width)
        && (1..=MAX_PIXELS_SIDE).contains(&height)
        && bytes == width as usize * height as usize * 4
}

/// How a pixel image is sampled. Smooth: blended both ways. Sharp (pixel
/// art, a game): each pixel a crisp square when enlarged, but blended when
/// shrunk, with mipmaps, since nearest-pixel shrinking drops whole rows and
/// columns and small text breaks up.
///
/// ```
/// use eframe::egui::TextureFilter;
/// use raptaboard::plugins::draw::texture_options;
/// let sharp = texture_options(false);
/// assert_eq!(sharp.magnification, TextureFilter::Nearest);
/// assert_eq!(sharp.minification, TextureFilter::Linear);
/// ```
pub fn texture_options(smooth: bool) -> egui::TextureOptions {
    let filter = if smooth {
        egui::TextureFilter::Linear
    } else {
        egui::TextureFilter::Nearest
    };
    egui::TextureOptions {
        magnification: filter,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: Some(egui::TextureFilter::Linear),
    }
}

/// The GPU texture for pixel image `p`, kept under `at`: sent again only when
/// `picture` (the picture's number) is new. `None` for an image that's the
/// wrong size for its bytes.
fn pixels_texture(
    ctx: &egui::Context,
    at: egui::Id,
    picture: u64,
    p: &Pixels,
) -> Option<egui::TextureId> {
    if !pixels_fit(p.width, p.height, p.rgba.len()) {
        return None;
    }
    let kept: Option<(u64, egui::TextureHandle)> = ctx.data(|d| d.get_temp(at));
    if let Some((number, handle)) = &kept
        && *number == picture
    {
        return Some(handle.id());
    }
    let image =
        egui::ColorImage::from_rgba_unmultiplied([p.width as usize, p.height as usize], &p.rgba);
    let options = texture_options(p.smooth);
    let handle = match kept {
        Some((_, mut handle)) => {
            handle.set(image, options);
            handle
        }
        None => ctx.load_texture("plugin pixels", image, options),
    };
    let id = handle.id();
    ctx.data_mut(|d| d.insert_temp(at, (picture, handle)));
    Some(id)
}

/// A region's outline and label, for the plugin sandbox's overlay: a
/// magenta box no plugin is likely to draw, its label inside its top left.
pub fn outline(painter: &egui::Painter, area: Rect, label: &str) {
    let color = egui::Color32::from_rgb(255, 0, 200);
    painter.rect_stroke(area, 0, Stroke::new(1.0, color), StrokeKind::Inside);
    painter.text(
        area.left_top() + Vec2::new(2.0, 1.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::monospace(9.0),
        color,
    );
}

/// A plugin region's rectangle, in the window.
pub fn region_rect(origin: Pos2, min: &Point, max: &Point) -> Rect {
    Rect::from_two_pos(
        origin + Vec2::new(min.x, min.y),
        origin + Vec2::new(max.x, max.y),
    )
}
