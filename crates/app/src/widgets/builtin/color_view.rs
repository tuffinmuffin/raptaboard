//! Single Color View and Multi Color View: colors sent as hex text.
//!
//! Single Color View fills with one color from a string such as
//! `"#FF8800"` (`RRGGBB`) or `"#80FF8800"` (`AARRGGBB`). Multi Color View
//! takes a string array and blends the colors left to right. Both read the
//! colors as Elastic does; neither has settings.

use eframe::egui::{self, Color32, CornerRadius, Rect};
use nt4::Value;
use serde_json::{Map, Value as Json};

use crate::widgets::shared::card;
use crate::widgets::{ValueInfo, WidgetType};

/// The Single Color View widget (Elastic's `"Single Color View"`).
pub struct SingleColorView;
/// The Multi Color View widget (Elastic's `"Multi Color View"`).
pub struct MultiColorView;

/// A color from hex text: `RRGGBB` or `AARRGGBB`, with or without `#`, in
/// any case. `None` if it isn't one.
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::widgets::builtin::color_view::parse_color;
/// assert_eq!(parse_color("#ff8800"), Some(Color32::from_rgb(0xFF, 0x88, 0x00)));
/// assert_eq!(parse_color("80FF0000"), Some(Color32::from_rgba_unmultiplied(0xFF, 0, 0, 0x80)));
/// assert_eq!(parse_color("orange"), None);
/// ```
pub fn parse_color(text: &str) -> Option<Color32> {
    let hex = text.trim().trim_start_matches('#');
    let argb = match hex.len() {
        6 => 0xFF00_0000 | u32::from_str_radix(hex, 16).ok()?,
        8 => u32::from_str_radix(hex, 16).ok()?,
        _ => return None,
    };
    let [a, r, g, b] = argb.to_be_bytes();
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// Fill `rect` with `colors` blended evenly from left to right.
fn draw_gradient(painter: &egui::Painter, rect: Rect, colors: &[Color32]) {
    let mut mesh = egui::Mesh::default();
    let steps = colors.len().max(2) - 1;
    for (i, &color) in colors.iter().enumerate() {
        let x = rect.min.x + rect.width() * i as f32 / steps as f32;
        mesh.colored_vertex(egui::pos2(x, rect.min.y), color);
        mesh.colored_vertex(egui::pos2(x, rect.max.y), color);
        if i > 0 {
            let n = (i * 2) as u32;
            mesh.add_triangle(n - 2, n - 1, n);
            mesh.add_triangle(n - 1, n, n + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

impl WidgetType for SingleColorView {
    fn names(&self) -> &'static [&'static str] {
        &["Single Color View"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        nt_type == "string"
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        match value {
            Some(Value::String(text)) => match parse_color(text) {
                Some(color) => {
                    painter.rect_filled(rect, CornerRadius::same(card::BODY_CORNER_RADIUS), color);
                }
                None => card::draw_message(painter, visuals, rect, "not a color"),
            },
            Some(_) => card::draw_message(painter, visuals, rect, "not text"),
            None => card::draw_message(painter, visuals, rect, "no value"),
        }
    }

    fn settings(&self, _: &mut egui::Ui, _: &mut Map<String, Json>, _: ValueInfo<'_>) {}

    fn setting_keys(&self) -> &'static [&'static str] {
        &[]
    }
}

impl WidgetType for MultiColorView {
    fn names(&self) -> &'static [&'static str] {
        &["Multi Color View"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        nt_type == "string[]"
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        match value {
            Some(Value::StringArray(texts)) => {
                let colors: Vec<Color32> = texts.iter().filter_map(|t| parse_color(t)).collect();
                match colors.as_slice() {
                    [] => card::draw_message(painter, visuals, rect, "no colors"),
                    // One color fills it all, as Elastic does.
                    [one] => draw_gradient(painter, rect, &[*one, *one]),
                    many => draw_gradient(painter, rect, many),
                }
            }
            Some(_) => card::draw_message(painter, visuals, rect, "not a string array"),
            None => card::draw_message(painter, visuals, rect, "no value"),
        }
    }

    fn settings(&self, _: &mut egui::Ui, _: &mut Map<String, Json>, _: ValueInfo<'_>) {}

    fn setting_keys(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn hex_colors_read_as_elastic_reads_them() {
        assert_eq!(parse_color("00FF00"), Some(Color32::from_rgb(0, 255, 0)));
        assert_eq!(
            parse_color("  #0000ff "),
            Some(Color32::from_rgb(0, 0, 255))
        );
        assert_eq!(parse_color("#FFF"), None);
        assert_eq!(parse_color("#GG0000"), None);
        assert_eq!(parse_color(""), None);
    }

    #[test]
    fn gradients_have_a_quad_between_each_pair() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(90.0, 10.0));
            draw_gradient(
                ui.painter(),
                rect,
                &[Color32::RED, Color32::GREEN, Color32::BLUE],
            );
        });
        output.textures_delta.clear();
        let mesh = output
            .shapes
            .into_iter()
            .find_map(|s| match s.shape {
                egui::epaint::Shape::Mesh(m) => Some(m),
                _ => None,
            })
            .unwrap();
        // Three colors: six corners, two quads (four triangles).
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.indices.len(), 12);
        assert_eq!(mesh.vertices[4].pos.x, 90.0);
    }
}
