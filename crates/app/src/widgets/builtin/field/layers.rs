//! A Field's overlays: layers of other robots, paths and the AprilTags
//! cameras see, each fed by a topic of its own (saved in `layers`), the
//! other objects of a `Field2d` table, and the field's AprilTags.

use std::collections::HashSet;

use eframe::egui::{self, Color32, Shape, Stroke};
use nt4::Value;
use nt4::proto::structs::StructDatabase;
use serde_json::{Map, Value as Json, json};

use super::FieldView;
use crate::types::field::{self, AprilTag, FieldPose, Origin};
use crate::types::structs;
use crate::widgets::shared::props;

/// The layers drawn over the field: a list of `{"kind", "topic",
/// "data_type", "color"}` objects (raptaboard).
pub const LAYERS: &str = "layers";
/// Draw the field's AprilTags (raptaboard).
pub const SHOW_TAGS: &str = "show_tags";
/// Draw a `Field2d` table's other objects as robots (Elastic).
pub const SHOW_OTHER_OBJECTS: &str = "show_other_objects";
/// Draw a `Field2d` table's trajectories as paths (Elastic).
pub const SHOW_TRAJECTORIES: &str = "show_trajectories";

/// A `Field2d` object with more poses than this is a path, not robots.
const MAX_ROBOTS_IN_OBJECT: usize = 8;
/// An AprilTag's size on the field, meters (6.5 inches is 0.165; drawn a
/// little bigger to be seen).
const TAG_SIZE_M: f64 = 0.3;
/// A seen tag's estimated pose this close to a field tag is that tag
/// (meters).
const SAME_TAG_M: f64 = 0.5;
/// Colors for layers without one, in order.
const PALETTE: [Color32; 5] = [
    Color32::from_rgb(0x21, 0x96, 0xF3),
    Color32::from_rgb(0xFF, 0x98, 0x00),
    Color32::from_rgb(0x9C, 0x27, 0xB0),
    Color32::from_rgb(0x00, 0xBC, 0xD4),
    Color32::from_rgb(0xCD, 0xDC, 0x39),
];
/// A seen tag's color.
const SEEN: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);

/// What a layer draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    /// Each pose as a robot outline (another robot, a vision estimate).
    Robots,
    /// The poses joined by a line (a trajectory).
    Path,
    /// The AprilTags seen: their IDs (`int[]`), or their estimated poses
    /// (`Pose3d[]`), lit on the field.
    SeenTags,
}

impl LayerKind {
    /// All of them, in the order a menu lists them.
    pub const ALL: [LayerKind; 3] = [LayerKind::Robots, LayerKind::Path, LayerKind::SeenTags];

    /// How it's saved.
    pub fn key(self) -> &'static str {
        match self {
            LayerKind::Robots => "robots",
            LayerKind::Path => "path",
            LayerKind::SeenTags => "seen_tags",
        }
    }

    fn from_key(key: &str) -> Option<LayerKind> {
        LayerKind::ALL.into_iter().find(|k| k.key() == key)
    }

    fn label(self) -> &'static str {
        match self {
            LayerKind::Robots => "Robots",
            LayerKind::Path => "Path",
            LayerKind::SeenTags => "AprilTags seen",
        }
    }

    /// Whether a topic of NT type `nt_type` can feed this kind.
    pub fn takes(self, nt_type: &str) -> bool {
        let poses = matches!(
            structs::struct_type(nt_type),
            Some(("Pose2d" | "Pose3d", _))
        );
        match self {
            LayerKind::Robots => poses || nt_type == "double[]",
            LayerKind::Path => {
                poses
                    || nt_type == "double[]"
                    || structs::struct_type(nt_type) == Some(("Translation2d", true))
            }
            LayerKind::SeenTags => {
                matches!(nt_type, "int[]" | "double[]")
                    || structs::struct_type(nt_type) == Some(("Pose3d", true))
            }
        }
    }

    /// The kind for a topic named `name` of type `nt_type` dropped on a
    /// Field: tag IDs for `int[]` (or a `double[]` named for tags), a path
    /// for a name with "traj" or "path" in it (or a `Translation2d[]`),
    /// else robots. `None` if it's nothing a Field can draw.
    ///
    /// ```
    /// use raptaboard::widgets::builtin::field::layers::LayerKind;
    /// assert_eq!(LayerKind::for_topic("/Drive/Fleet", "struct:Pose2d[]"), Some(LayerKind::Robots));
    /// assert_eq!(LayerKind::for_topic("/Auto/Trajectory", "struct:Pose2d[]"), Some(LayerKind::Path));
    /// assert_eq!(LayerKind::for_topic("/Vision/tagIds", "int[]"), Some(LayerKind::SeenTags));
    /// assert_eq!(LayerKind::for_topic("/Arm/angle", "double"), None);
    /// ```
    pub fn for_topic(name: &str, nt_type: &str) -> Option<LayerKind> {
        let last = name.rsplit('/').next().unwrap_or(name).to_lowercase();
        let kind = if nt_type == "int[]" || (nt_type == "double[]" && last.contains("tag")) {
            LayerKind::SeenTags
        } else if last.contains("traj")
            || last.contains("path")
            || structs::struct_type(nt_type) == Some(("Translation2d", true))
        {
            LayerKind::Path
        } else {
            LayerKind::Robots
        };
        kind.takes(nt_type).then_some(kind)
    }
}

/// One layer drawn over the field.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// What it draws.
    pub kind: LayerKind,
    /// The topic it's fed by.
    pub topic: String,
    /// The topic's NT type, to decode its value.
    pub data_type: String,
    /// Its color, if set (else one from the palette by its place).
    pub color: Option<Color32>,
}

/// The layers saved in `p` (any that don't read are skipped).
pub fn layers(p: &Map<String, Json>) -> Vec<Layer> {
    p.get(LAYERS)
        .and_then(Json::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|l| {
                    let o = l.as_object()?;
                    Some(Layer {
                        kind: LayerKind::from_key(o.get("kind")?.as_str()?)?,
                        topic: o.get("topic")?.as_str()?.to_owned(),
                        data_type: o
                            .get("data_type")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        color: props::color_property(o, "color"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Save `list` as `p`'s layers (removing the setting when it's empty).
pub fn set_layers(p: &mut Map<String, Json>, list: &[Layer]) {
    let saved: Vec<Json> = list
        .iter()
        .map(|l| {
            let mut o = json!({"kind": l.kind.key(), "topic": l.topic, "data_type": l.data_type});
            if let Some(c) = l.color {
                o["color"] = props::color_json(c);
            }
            o
        })
        .collect();
    props::set_property(p, LAYERS, (!saved.is_empty()).then(|| saved.into()));
}

/// The color layer `i` is drawn in.
fn color_of(layer: &Layer, i: usize) -> Color32 {
    layer.color.unwrap_or(PALETTE[i % PALETTE.len()])
}

/// A `Field2d` table's other objects in `others` (every `double[]` under
/// `table` but `Robot`), by name.
fn table_objects<'a>(
    table: &str,
    others: &'a std::collections::HashMap<&str, &Value>,
) -> Vec<(&'a str, &'a Value)> {
    let prefix = format!("{}/", table.trim_end_matches('/'));
    let mut found: Vec<(&str, &Value)> = others
        .iter()
        .filter_map(|(name, value)| {
            let object = name.strip_prefix(prefix.as_str())?;
            (object != "Robot" && !object.contains('/') && matches!(value, Value::DoubleArray(_)))
                .then_some((*name, *value))
        })
        .collect();
    found.sort_by_key(|(name, _)| *name);
    found
}

/// What the overlays drew, for the widget's summary.
#[derive(Debug, Default, PartialEq)]
pub struct Drawn {
    /// Robots drawn besides the widget's own.
    pub robots: usize,
    /// Paths drawn.
    pub paths: usize,
    /// The AprilTags lit as seen, by ID, in order.
    pub seen: Vec<u32>,
}

/// Everything the overlays need to draw.
pub struct Overlay<'a> {
    pub view: &'a FieldView,
    pub origin: Origin,
    pub robot_size: (f64, f64),
    pub others: &'a std::collections::HashMap<&'a str, &'a Value>,
    pub structs: Option<&'a StructDatabase>,
    pub tags: &'a [AprilTag],
}

impl Overlay<'_> {
    /// `pose` (in the widget's origin) in the 2027 frame.
    fn center(&self, pose: FieldPose) -> FieldPose {
        let (length, width) = self.view.size;
        self.origin.to_center(pose, length, width)
    }

    /// Draw the layers in `p`, a `Field2d` table's other objects if the
    /// widget is bound to `table`, and the AprilTags (lit if seen).
    pub fn draw(
        &self,
        painter: &egui::Painter,
        p: &Map<String, Json>,
        table: Option<&str>,
    ) -> Drawn {
        let mut drawn = Drawn::default();
        let mut seen: HashSet<u32> = HashSet::new();
        let mut shapes: Vec<(LayerKind, Vec<FieldPose>, Color32)> = Vec::new();
        for (i, layer) in layers(p).iter().enumerate() {
            let Some(value) = self.others.get(layer.topic.as_str()) else {
                continue;
            };
            let color = color_of(layer, i);
            match layer.kind {
                LayerKind::SeenTags => {
                    seen.extend(field::tag_ids(value));
                    for pose in field::poses_of(value, &layer.data_type, self.structs) {
                        let pose = self.center(pose);
                        if let Some(tag) = self
                            .tags
                            .iter()
                            .find(|t| (t.x - pose.x).hypot(t.y - pose.y) < SAME_TAG_M)
                        {
                            seen.insert(tag.id);
                        }
                    }
                }
                kind => {
                    let poses = field::poses_of(value, &layer.data_type, self.structs);
                    shapes.push((kind, poses, color));
                }
            }
        }
        if let Some(table) = table {
            let robots = props::flag(p, SHOW_OTHER_OBJECTS, true);
            let paths = props::flag(p, SHOW_TRAJECTORIES, true);
            for (i, (name, value)) in table_objects(table, self.others).into_iter().enumerate() {
                let poses = field::poses_of(value, "double[]", None);
                let last = name.rsplit('/').next().unwrap_or(name).to_lowercase();
                let is_path = poses.len() > MAX_ROBOTS_IN_OBJECT
                    || last.contains("traj")
                    || last.contains("path");
                let color = PALETTE[i % PALETTE.len()];
                match is_path {
                    true if paths => shapes.push((LayerKind::Path, poses, color)),
                    false if robots => shapes.push((LayerKind::Robots, poses, color)),
                    _ => {}
                }
            }
        }
        if props::flag(p, SHOW_TAGS, true) {
            self.draw_tags(painter, &seen);
        }
        for (kind, poses, color) in shapes {
            let poses: Vec<FieldPose> = poses.into_iter().map(|q| self.center(q)).collect();
            match kind {
                LayerKind::Path if poses.len() >= 2 => {
                    let points = poses
                        .iter()
                        .map(|q| self.view.to_screen(q.x, q.y))
                        .collect();
                    painter.add(Shape::line(points, Stroke::new(2.5, color)));
                    drawn.paths += 1;
                }
                LayerKind::Robots => {
                    for pose in &poses {
                        super::draw_robot(
                            painter,
                            self.view,
                            *pose,
                            self.robot_size,
                            color.gamma_multiply(0.8),
                        );
                    }
                    drawn.robots += poses.len();
                }
                _ => {}
            }
        }
        let mut seen: Vec<u32> = seen
            .into_iter()
            .filter(|id| self.tags.iter().any(|t| t.id == *id))
            .collect();
        seen.sort_unstable();
        drawn.seen = seen;
        drawn
    }

    /// The field's AprilTags: a square each, turned the way it faces with
    /// a tick on its face, and its ID; filled green when it's `seen`.
    fn draw_tags(&self, painter: &egui::Painter, seen: &HashSet<u32>) {
        let meter = self.view.rect.width().max(self.view.rect.height())
            / self.view.size.0.max(self.view.size.1) as f32;
        let text = egui::FontId::proportional((meter * 0.22).clamp(7.0, 14.0));
        for tag in self.tags {
            let (sin, cos) = tag.yaw.sin_cos();
            let h = TAG_SIZE_M / 2.0;
            let at = |fx: f64, fy: f64| {
                self.view
                    .to_screen(tag.x + fx * cos - fy * sin, tag.y + fx * sin + fy * cos)
            };
            let square = vec![at(h, h), at(h, -h), at(-h, -h), at(-h, h)];
            let lit = seen.contains(&tag.id);
            let (fill, line) = if lit {
                (SEEN, Color32::WHITE)
            } else {
                (
                    Color32::from_black_alpha(140),
                    Color32::from_white_alpha(200),
                )
            };
            painter.add(Shape::convex_polygon(square, fill, Stroke::new(1.0, line)));
            // A tick out of its face, so it shows which way it looks.
            painter.line_segment([at(h, 0.0), at(h * 2.2, 0.0)], Stroke::new(1.5, line));
            painter.text(
                at(0.0, 0.0),
                egui::Align2::CENTER_CENTER,
                tag.id.to_string(),
                text.clone(),
                Color32::WHITE,
            );
        }
    }
}

/// The topics a Field's layers need.
pub fn layer_topics(p: &Map<String, Json>) -> Vec<String> {
    layers(p).into_iter().map(|l| l.topic).collect()
}

/// A `Field2d` table's topics on the server (its objects and `.type`).
pub fn table_topics(table: &str, server: &[nt4::Topic]) -> Vec<String> {
    let prefix = format!("{}/", table.trim_end_matches('/'));
    let mut names: Vec<String> = server
        .iter()
        .filter(|t| t.name.starts_with(&prefix) && !t.name[prefix.len()..].contains('/'))
        .map(|t| t.name.to_string())
        .collect();
    for always in ["Robot", ".type"] {
        let name = format!("{prefix}{always}");
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Add a layer for each of `topics` (name, NT type) that a Field can draw
/// and doesn't already, to `p`. Returns how many were added.
pub fn add_layers(p: &mut Map<String, Json>, topics: &[(String, String)]) -> usize {
    let mut list = layers(p);
    let before = list.len();
    for (name, ty) in topics {
        if list.iter().any(|l| l.topic == *name) {
            continue;
        }
        if let Some(kind) = LayerKind::for_topic(name, ty) {
            list.push(Layer {
                kind,
                topic: name.clone(),
                data_type: ty.clone(),
                color: None,
            });
        }
    }
    let added = list.len() - before;
    set_layers(p, &list);
    added
}

/// The settings dialog rows for the layers: each one's kind, topic (from
/// the server's `topics`, as (name, NT type), that fit the kind), color
/// and a remove button, then "Add layer".
pub fn settings_rows(ui: &mut egui::Ui, p: &mut Map<String, Json>, topics: &[(String, String)]) {
    let mut list = layers(p);
    let mut changed = false;
    let mut remove = None;
    for (i, layer) in list.iter_mut().enumerate() {
        ui.label(format!("Layer {}", i + 1));
        ui.horizontal(|ui| {
            let mut kind = layer.kind;
            egui::ComboBox::from_id_salt(("layer kind", i))
                .selected_text(kind.label())
                .show_ui(ui, |ui| {
                    for k in LayerKind::ALL {
                        ui.selectable_value(&mut kind, k, k.label());
                    }
                });
            if kind != layer.kind {
                layer.kind = kind;
                changed = true;
            }
            let shown = if layer.topic.is_empty() {
                "(pick a topic)"
            } else {
                layer.topic.as_str()
            };
            egui::ComboBox::from_id_salt(("layer topic", i))
                .selected_text(shown)
                .width(260.0)
                .show_ui(ui, |ui| {
                    for (name, ty) in topics.iter().filter(|(_, ty)| layer.kind.takes(ty)) {
                        if ui
                            .selectable_label(layer.topic == *name, format!("{name}  ({ty})"))
                            .clicked()
                        {
                            layer.topic = name.clone();
                            layer.data_type = ty.clone();
                            changed = true;
                        }
                    }
                });
            let mut color = color_of(layer, i);
            if egui::color_picker::color_edit_button_srgba(
                ui,
                &mut color,
                egui::color_picker::Alpha::Opaque,
            )
            .changed()
            {
                layer.color = Some(color);
                changed = true;
            }
            if ui.button("✕").on_hover_text("Remove this layer").clicked() {
                remove = Some(i);
            }
        });
        ui.end_row();
    }
    if let Some(i) = remove {
        list.remove(i);
        changed = true;
    }
    ui.label("");
    if ui
        .button("Add layer")
        .on_hover_text(
            "Draw more on the field from a topic: other robots or a vision estimate (poses), \
             a path (a trajectory's poses), or the AprilTags a camera sees (their IDs). \
             Dropping a topic from the tree onto the field adds one too.",
        )
        .clicked()
    {
        list.push(Layer {
            kind: LayerKind::Robots,
            topic: String::new(),
            data_type: String::new(),
            color: None,
        });
        changed = true;
    }
    ui.end_row();
    if changed {
        set_layers(p, &list);
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn layers_save_and_read_back_and_drops_add_new_ones() {
        let mut p = Map::new();
        let topics = vec![
            ("/DashTest/Fleet".to_string(), "struct:Pose2d[]".to_string()),
            (
                "/Auto/trajectory".to_string(),
                "struct:Pose2d[]".to_string(),
            ),
            ("/Vision/seenTags".to_string(), "int[]".to_string()),
            ("/Arm/angle".to_string(), "double".to_string()),
        ];
        assert_eq!(add_layers(&mut p, &topics), 3);
        let list = layers(&p);
        let kinds: Vec<LayerKind> = list.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [LayerKind::Robots, LayerKind::Path, LayerKind::SeenTags]
        );
        assert_eq!(list[0].data_type, "struct:Pose2d[]");
        // Dropped again: nothing new.
        assert_eq!(add_layers(&mut p, &topics), 0);
        // A color set is kept; none removes the setting.
        let mut list = list;
        list[1].color = Some(Color32::RED);
        set_layers(&mut p, &list);
        assert_eq!(layers(&p)[1].color, Some(Color32::RED));
        set_layers(&mut p, &[]);
        assert!(!p.contains_key(LAYERS));
        assert_eq!(layer_topics(&p), Vec::<String>::new());
    }

    #[test]
    fn a_field2d_tables_objects_are_its_arrays_but_the_robot() {
        let robot = Value::DoubleArray(vec![0.0; 3]);
        let ghost = Value::DoubleArray(vec![1.0, 1.0, 0.0]);
        let name = Value::String("x".into());
        let others = std::collections::HashMap::from([
            ("/SmartDashboard/Field/Robot", &robot),
            ("/SmartDashboard/Field/Ghost", &ghost),
            ("/SmartDashboard/Field/.name", &name),
            ("/SmartDashboard/Field/Deep/Thing", &ghost),
        ]);
        let found = table_objects("/SmartDashboard/Field", &others);
        assert_eq!(
            found.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            ["/SmartDashboard/Field/Ghost"]
        );
    }
}
