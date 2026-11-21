//! Things fixed to the robot, set in a Field's settings (`attachments`)
//! and drawn with it: cameras, as their field of view with range bands,
//! and parts (an intake, a climber) as rectangles, shown while a boolean
//! topic is true or lengthened by a number topic.
//!
//! Each is placed in the robot's frame: `x` meters forward of its center,
//! `y` meters to its left, facing `yaw` degrees counterclockwise from its
//! front.

use eframe::egui::{self, Color32, Shape, Stroke};
use nt4::Value;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

use super::FieldView;
use crate::types::field::{FieldPose, from_robot};
use crate::widgets::shared::props;

/// The things fixed to the robot: a list of [`Attachment`]s (raptaboard).
pub const ATTACHMENTS: &str = "attachments";

/// A camera's range bands when not set, meters: reliable to 4, marginal
/// to 6, unlikely to 8 (Robot-2026's camera visualizer's).
pub const DEFAULT_BANDS_M: [f64; 3] = [4.0, 6.0, 8.0];
/// A camera's field of view when not set, degrees.
pub const DEFAULT_FOV_DEG: f64 = 70.0;
/// The bands' colors, nearest first.
const BAND_COLORS: [Color32; 3] = [
    Color32::from_rgba_unmultiplied_const(0x4C, 0xAF, 0x50, 0x48),
    Color32::from_rgba_unmultiplied_const(0xFF, 0xC1, 0x07, 0x38),
    Color32::from_rgba_unmultiplied_const(0xF4, 0x43, 0x36, 0x28),
];
/// A part's color when not set.
const PART_COLOR: Color32 = Color32::from_rgb(0x00, 0xBC, 0xD4);
/// How many steps a band's arc is drawn in, per 90 degrees.
const ARC_STEPS_PER_QUARTER: f64 = 12.0;

/// One thing fixed to the robot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Attachment {
    /// A camera: where it is, which way it looks, and how far it sees.
    Camera(Camera),
    /// A part: a rectangle out from a point on the robot.
    Part(Part),
}

/// A camera on the robot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    /// Its name, for the settings.
    #[serde(default)]
    pub name: String,
    /// Meters forward of the robot's center.
    #[serde(default)]
    pub x: f64,
    /// Meters left of the robot's center.
    #[serde(default)]
    pub y: f64,
    /// Which way it looks, degrees counterclockwise from the robot's front.
    #[serde(default)]
    pub yaw: f64,
    /// Its horizontal field of view, degrees.
    #[serde(default = "default_fov")]
    pub fov: f64,
    /// Where its range bands end, meters, nearest first.
    #[serde(default = "default_bands")]
    pub bands: [f64; 3],
}

fn default_fov() -> f64 {
    DEFAULT_FOV_DEG
}

fn default_bands() -> [f64; 3] {
    DEFAULT_BANDS_M
}

/// A part of the robot, drawn as a rectangle from its base point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    /// Its name, for the settings.
    #[serde(default)]
    pub name: String,
    /// Its base, meters forward of the robot's center.
    #[serde(default)]
    pub x: f64,
    /// Its base, meters left of the robot's center.
    #[serde(default)]
    pub y: f64,
    /// Which way it reaches out, degrees counterclockwise from the robot's
    /// front.
    #[serde(default)]
    pub yaw: f64,
    /// How far it reaches, meters (unless `length_topic` says).
    #[serde(default = "default_part_size")]
    pub length: f64,
    /// How wide it is, meters.
    #[serde(default = "default_part_size")]
    pub width: f64,
    /// A boolean topic: drawn only while it's true. Always drawn if empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shown_by: String,
    /// A number topic for its length: the value times `scale`, meters.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub length_topic: String,
    /// What `length_topic`'s value is multiplied by to get meters.
    #[serde(default = "default_scale")]
    pub scale: f64,
    /// Its color, as Elastic saves colors (ARGB); not set is cyan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
}

fn default_part_size() -> f64 {
    0.3
}

fn default_scale() -> f64 {
    1.0
}

impl Part {
    fn color(&self) -> Color32 {
        match self.color {
            Some(argb) => {
                let [a, r, g, b] = argb.to_be_bytes();
                Color32::from_rgba_unmultiplied(r, g, b, a)
            }
            None => PART_COLOR,
        }
    }
}

/// The attachments saved in `p` (any that don't read are skipped).
pub fn attachments(p: &Map<String, Json>) -> Vec<Attachment> {
    p.get(ATTACHMENTS)
        .and_then(Json::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|a| serde_json::from_value(a.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Save `list` as `p`'s attachments (removing the setting when empty).
pub fn set_attachments(p: &mut Map<String, Json>, list: &[Attachment]) {
    let saved: Vec<Json> = list
        .iter()
        .filter_map(|a| serde_json::to_value(a).ok())
        .collect();
    props::set_property(p, ATTACHMENTS, (!saved.is_empty()).then(|| saved.into()));
}

/// The topics the attachments in `p` need.
pub fn attachment_topics(p: &Map<String, Json>) -> Vec<String> {
    attachments(p)
        .into_iter()
        .filter_map(|a| match a {
            Attachment::Part(part) => Some([part.shown_by, part.length_topic]),
            Attachment::Camera(_) => None,
        })
        .flatten()
        .filter(|t| !t.is_empty())
        .collect()
}

/// A number from a topic's value, if it's one.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Double(d) => Some(*d),
        Value::Float(f) => Some(f64::from(*f)),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

/// Where `part` reaches now, from the server's values in `others`: its
/// length in meters, or `None` while it's hidden (its `shown_by` topic
/// isn't true).
pub fn part_length(part: &Part, others: &std::collections::HashMap<&str, &Value>) -> Option<f64> {
    if !part.shown_by.is_empty()
        && others.get(part.shown_by.as_str()) != Some(&&Value::Boolean(true))
    {
        return None;
    }
    if part.length_topic.is_empty() {
        return Some(part.length);
    }
    let value = others.get(part.length_topic.as_str())?;
    Some((number(value)? * part.scale).clamp(0.0, 10.0))
}

/// The corners of a band from `near` to `far` meters, of a camera at
/// `at` (field frame) looking `heading` radians with `fov` radians of view,
/// as rings of points: the near arc and the far arc, each from one edge
/// of the view to the other.
fn band_arcs(at: FieldPose, heading: f64, fov: f64, near: f64, far: f64) -> (ArcPoints, ArcPoints) {
    let steps = ((fov / std::f64::consts::FRAC_PI_2) * ARC_STEPS_PER_QUARTER)
        .ceil()
        .max(2.0) as usize;
    let arc = |r: f64| {
        (0..=steps)
            .map(|k| {
                let a = heading - fov / 2.0 + fov * k as f64 / steps as f64;
                (at.x + r * a.cos(), at.y + r * a.sin())
            })
            .collect()
    };
    (arc(near), arc(far))
}

/// An arc of points on the field, (x, y) meters.
type ArcPoints = Vec<(f64, f64)>;

/// What the attachments drew, for the widget's summary.
#[derive(Debug, Default, PartialEq)]
pub struct Drawn {
    /// Cameras drawn.
    pub cameras: usize,
    /// Parts drawn (shown now).
    pub parts: usize,
}

/// Draw the attachments in `p` on the robot at `robot` (2027 frame).
pub fn draw(
    painter: &egui::Painter,
    view: &FieldView,
    p: &Map<String, Json>,
    robot: FieldPose,
    others: &std::collections::HashMap<&str, &Value>,
) -> Drawn {
    let mut drawn = Drawn::default();
    let to = |x: f64, y: f64| view.to_screen(x, y);
    for attachment in attachments(p) {
        match attachment {
            Attachment::Camera(cam) => {
                let at = from_robot(
                    robot,
                    FieldPose {
                        x: cam.x,
                        y: cam.y,
                        heading: 0.0,
                    },
                );
                let heading = robot.heading + cam.yaw.to_radians();
                let fov = cam.fov.clamp(1.0, 360.0).to_radians();
                let mut near = 0.0;
                for (far, color) in cam.bands.iter().zip(BAND_COLORS) {
                    let far = far.max(near);
                    let (inner, outer) = band_arcs(at, heading, fov, near, far);
                    let mut mesh = egui::Mesh::default();
                    for (a, b) in inner.iter().zip(&outer) {
                        mesh.colored_vertex(to(a.0, a.1), color);
                        mesh.colored_vertex(to(b.0, b.1), color);
                    }
                    for k in 0..inner.len().saturating_sub(1) as u32 {
                        let i = k * 2;
                        mesh.add_triangle(i, i + 1, i + 3);
                        mesh.add_triangle(i, i + 3, i + 2);
                    }
                    painter.add(Shape::mesh(mesh));
                    near = far;
                }
                // The view's edges, and the camera as a dot.
                let edge = Stroke::new(1.0, Color32::from_white_alpha(160));
                let reach = cam.bands.iter().copied().fold(0.0, f64::max);
                let c = to(at.x, at.y);
                for side in [-0.5, 0.5] {
                    let a = heading + side * fov;
                    painter.line_segment(
                        [c, to(at.x + reach * a.cos(), at.y + reach * a.sin())],
                        edge,
                    );
                }
                painter.circle(c, 3.0, Color32::WHITE, Stroke::new(1.0, Color32::BLACK));
                drawn.cameras += 1;
            }
            Attachment::Part(part) => {
                let Some(length) = part_length(&part, others) else {
                    continue;
                };
                let yaw = part.yaw.to_radians();
                let (dx, dy) = (yaw.cos(), yaw.sin());
                let half = part.width / 2.0;
                // Its corners in the robot's frame, then on the field.
                let corners = [
                    (part.x - dy * half, part.y + dx * half),
                    (part.x + dy * half, part.y - dx * half),
                    (
                        part.x + dy * half + dx * length,
                        part.y - dx * half + dy * length,
                    ),
                    (
                        part.x - dy * half + dx * length,
                        part.y + dx * half + dy * length,
                    ),
                ];
                let points = corners
                    .iter()
                    .map(|&(x, y)| {
                        let q = from_robot(robot, FieldPose { x, y, heading: 0.0 });
                        to(q.x, q.y)
                    })
                    .collect();
                let color = part.color();
                painter.add(Shape::convex_polygon(
                    points,
                    color.gamma_multiply(0.45),
                    Stroke::new(1.5, color),
                ));
                drawn.parts += 1;
            }
        }
    }
    drawn
}

/// A settings dialog row editing a number in meters (or degrees), saved
/// when it changes.
fn number_edit(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
) -> bool {
    ui.label(label);
    ui.add(
        egui::DragValue::new(value)
            .range(range)
            .speed(speed)
            .max_decimals(3),
    )
    .changed()
}

/// The settings dialog rows for the attachments: each camera's and part's
/// place and size (parts also their topics and color), a remove button,
/// then "Add camera" and "Add part".
pub fn settings_rows(ui: &mut egui::Ui, p: &mut Map<String, Json>, topics: &[(String, String)]) {
    let mut list = attachments(p);
    let mut changed = false;
    let mut remove = None;
    for (i, item) in list.iter_mut().enumerate() {
        let (kind, name) = match item {
            Attachment::Camera(c) => ("Camera", &mut c.name),
            Attachment::Part(part) => ("Part", &mut part.name),
        };
        ui.label(format!("{kind} {}", i + 1));
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::TextEdit::singleline(name)
                        .hint_text("name")
                        .desired_width(120.0),
                )
                .changed();
            if ui.button("✕").on_hover_text("Remove it").clicked() {
                remove = Some(i);
            }
        });
        ui.end_row();
        ui.label("");
        ui.horizontal(|ui| match item {
            Attachment::Camera(c) => {
                changed |= number_edit(ui, "x (m)", &mut c.x, -5.0..=5.0, 0.01);
                changed |= number_edit(ui, "y (m)", &mut c.y, -5.0..=5.0, 0.01);
                changed |= number_edit(ui, "facing (°)", &mut c.yaw, -360.0..=360.0, 1.0);
                changed |= number_edit(ui, "FOV (°)", &mut c.fov, 1.0..=360.0, 1.0);
            }
            Attachment::Part(part) => {
                changed |= number_edit(ui, "x (m)", &mut part.x, -5.0..=5.0, 0.01);
                changed |= number_edit(ui, "y (m)", &mut part.y, -5.0..=5.0, 0.01);
                changed |= number_edit(ui, "facing (°)", &mut part.yaw, -360.0..=360.0, 1.0);
                changed |= number_edit(ui, "width (m)", &mut part.width, 0.01..=5.0, 0.01);
            }
        });
        ui.end_row();
        ui.label("");
        ui.horizontal(|ui| match item {
            Attachment::Camera(c) => {
                ui.label("Bands to (m)");
                for band in &mut c.bands {
                    changed |= ui
                        .add(egui::DragValue::new(band).range(0.1..=30.0).speed(0.1))
                        .changed();
                }
            }
            Attachment::Part(part) => {
                ui.label("Shown while");
                changed |=
                    props::topic_picker(ui, ("part shown", i), &mut part.shown_by, topics, |t| {
                        t == "boolean"
                    });
                let mut color = part.color();
                if egui::color_picker::color_edit_button_srgba(
                    ui,
                    &mut color,
                    egui::color_picker::Alpha::Opaque,
                )
                .changed()
                {
                    let [r, g, b, a] = color.to_srgba_unmultiplied();
                    part.color = Some(u32::from_be_bytes([a, r, g, b]));
                    changed = true;
                }
            }
        });
        ui.end_row();
        if let Attachment::Part(part) = item {
            ui.label("");
            ui.horizontal(|ui| {
                if part.length_topic.is_empty() {
                    changed |= number_edit(ui, "length (m)", &mut part.length, 0.0..=10.0, 0.01);
                }
                ui.label("length from");
                changed |= props::topic_picker(
                    ui,
                    ("part length", i),
                    &mut part.length_topic,
                    topics,
                    |t| matches!(t, "double" | "float" | "int"),
                );
                if !part.length_topic.is_empty() {
                    changed |=
                        number_edit(ui, "× (to m)", &mut part.scale, -1000.0..=1000.0, 0.001);
                }
            });
            ui.end_row();
        }
    }
    if let Some(i) = remove {
        list.remove(i);
        changed = true;
    }
    ui.label("");
    ui.horizontal(|ui| {
        if ui
            .button("Add camera")
            .on_hover_text("A camera's field of view, drawn from the robot with its range bands")
            .clicked()
        {
            list.push(Attachment::Camera(Camera {
                name: String::new(),
                x: 0.0,
                y: 0.0,
                yaw: 0.0,
                fov: DEFAULT_FOV_DEG,
                bands: DEFAULT_BANDS_M,
            }));
            changed = true;
        }
        if ui
            .button("Add part")
            .on_hover_text(
                "A part of the robot (an intake, a climber), drawn as a rectangle reaching out \
                 from a point on it: shown while a boolean topic is true, or as long as a \
                 number topic says",
            )
            .clicked()
        {
            list.push(Attachment::Part(Part {
                name: String::new(),
                x: 0.0,
                y: 0.0,
                yaw: 0.0,
                length: default_part_size(),
                width: default_part_size(),
                shown_by: String::new(),
                length_topic: String::new(),
                scale: 1.0,
                color: None,
            }));
            changed = true;
        }
    });
    ui.end_row();
    if changed {
        set_attachments(p, &list);
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use serde_json::json;

    #[test]
    fn attachments_read_with_defaults_and_save_back() {
        let p = json!({"attachments": [
            {"kind": "camera", "name": "Side", "x": -0.15, "y": 0.3, "yaw": 90},
            {"kind": "part", "name": "Intake", "x": 0.35, "shown_by": "/Intake/out"},
            {"kind": "unknown"}
        ]});
        let mut p = p.as_object().unwrap().clone();
        let list = attachments(&p);
        assert_eq!(list.len(), 2, "what doesn't read is skipped");
        let Attachment::Camera(cam) = &list[0] else {
            panic!("a camera")
        };
        assert_eq!((cam.fov, cam.bands), (DEFAULT_FOV_DEG, DEFAULT_BANDS_M));
        let Attachment::Part(part) = &list[1] else {
            panic!("a part")
        };
        assert_eq!((part.length, part.width, part.scale), (0.3, 0.3, 1.0));
        assert_eq!(attachment_topics(&p), ["/Intake/out"]);
        set_attachments(&mut p, &list);
        assert_eq!(attachments(&p), list);
        // Empty topics aren't saved.
        assert!(p[ATTACHMENTS][1].get("length_topic").is_none());
        set_attachments(&mut p, &[]);
        assert!(!p.contains_key(ATTACHMENTS));
    }

    #[test]
    fn a_part_shows_while_its_topic_is_true_and_takes_its_length() {
        let mut part: Part = serde_json::from_value(
            json!({"shown_by": "/out", "length_topic": "/ext", "scale": 0.0254}),
        )
        .unwrap();
        let on = Value::Boolean(true);
        let off = Value::Boolean(false);
        let inches = Value::Double(10.0);
        fn values<'a>(
            shown: &'a Value,
            inches: &'a Value,
        ) -> std::collections::HashMap<&'static str, &'a Value> {
            std::collections::HashMap::from([("/out", shown), ("/ext", inches)])
        }
        let length = part_length(&part, &values(&on, &inches)).unwrap();
        assert!((length - 0.254).abs() < 1e-9);
        assert_eq!(part_length(&part, &values(&off, &inches)), None);
        // No length yet: hidden; no topics at all: its own length.
        assert_eq!(
            part_length(&part, &std::collections::HashMap::from([("/out", &on)])),
            None
        );
        part.shown_by.clear();
        part.length_topic.clear();
        assert_eq!(part_length(&part, &Default::default()), Some(0.3));
    }

    #[test]
    fn a_cameras_bands_reach_out_along_its_view() {
        // A camera at the origin looking along +Y, a 90 degree view.
        let at = FieldPose {
            x: 0.0,
            y: 0.0,
            heading: 0.0,
        };
        let (near, far) = band_arcs(
            at,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::FRAC_PI_2,
            1.0,
            2.0,
        );
        assert_eq!(near.len(), far.len());
        // The arcs run from one edge of the view (45 degrees right of +Y)
        // to the other, at 1 and 2 meters.
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let first = near[0];
        assert!((first.0 - h).abs() < 1e-9 && (first.1 - h).abs() < 1e-9);
        let last = far[far.len() - 1];
        assert!((last.0 + 2.0 * h).abs() < 1e-9 && (last.1 - 2.0 * h).abs() < 1e-9);
    }
}
