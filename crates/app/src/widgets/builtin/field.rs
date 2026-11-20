//! Field: the robot on a top-down view of the field.
//!
//! The field is one of [`crate::fields`] (`field`: its id, or the path of a
//! field JSON; 2026 REBUILT if not set, or the first whose game matches
//! Elastic's `field_game`), drawn with the blue alliance wall on the right
//! unless `field_rotation` turns it (0, 90, 180 or 270 degrees clockwise;
//! 180 is the view from the red side).
//!
//! The robot's pose comes from the widget's topic: a `Pose2d` (or `Pose3d`,
//! drawn flat) struct topic, or a WPILib `Field2d` table (`.type`
//! `"Field2d"`; its `Robot` is a `double[]` of x, y and degrees), as Elastic
//! reads it. Poses are in WPILib 2027's frame (origin at the field's
//! center) unless `origin` is `blue_corner`, the frame used before 2027.
//! The robot is drawn `robot_length` by `robot_width` meters, in
//! `robot_color`, with an arrow the way it faces.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::fields::{self, FieldEntry};
use crate::types::field::{FieldInfo, FieldPose, Origin};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// The field shown: an id from [`fields::available`] or a field JSON's path
/// (raptaboard).
pub const FIELD: &str = "field";
/// Elastic's choice of field, by game name (`"Rebuilt"`); used when
/// [`FIELD`] isn't set.
pub const FIELD_GAME: &str = "field_game";
/// Where the poses' origin is: `center` (WPILib 2027) or `blue_corner`
/// (raptaboard).
pub const ORIGIN: &str = "origin";
/// The robot's length (front to back) and width, in meters (Elastic).
pub const ROBOT_LENGTH: &str = "robot_length";
/// See [`ROBOT_LENGTH`].
pub const ROBOT_WIDTH: &str = "robot_width";
/// The robot's color (Elastic).
pub const ROBOT_COLOR: &str = "robot_color";
/// How far the field is turned clockwise, in degrees: 0, 90, 180 or 270
/// (Elastic).
pub const FIELD_ROTATION: &str = "field_rotation";

const KEYS: [&str; 7] = [
    FIELD,
    FIELD_GAME,
    ORIGIN,
    ROBOT_LENGTH,
    ROBOT_WIDTH,
    ROBOT_COLOR,
    FIELD_ROTATION,
];

/// A WPILib `Field2d` table's `.type`.
pub const FIELD2D_TYPE: &str = "Field2d";
/// The robot's pose in a `Field2d` table.
const FIELD2D_ROBOT: &str = "Robot";
/// The robot's default length and width, in meters (Elastic's).
const DEFAULT_ROBOT_SIZE_M: f64 = 0.85;
const DEFAULT_ROBOT_COLOR: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);
/// The widest a field image's texture is made, in pixels.
const MAX_TEXTURE_WIDTH: u32 = 2048;
/// The field's color while there's no image (or it's still loading).
const PLAIN_FIELD: Color32 = Color32::from_rgb(0x3A, 0x4A, 0x3A);

/// Which way the field is turned on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// Blue alliance wall on the right.
    None,
    /// A quarter turn clockwise: blue at the bottom.
    Quarter,
    /// A half turn: blue on the left, the view from the red side.
    Half,
    /// Three quarters: blue at the top.
    ThreeQuarter,
}

impl Turn {
    const ALL: [Turn; 4] = [Turn::None, Turn::Quarter, Turn::Half, Turn::ThreeQuarter];

    /// The turn in `p`'s `field_rotation` (degrees, the nearest quarter).
    fn of(p: &Map<String, Json>) -> Turn {
        let quarters = (props::number(p, FIELD_ROTATION, 0.0) / 90.0).round() as i64;
        Turn::ALL[quarters.rem_euclid(4) as usize]
    }

    fn degrees(self) -> f64 {
        f64::from(self as u8) * 90.0
    }

    fn label(self) -> &'static str {
        match self {
            Turn::None => "Blue on the right",
            Turn::Quarter => "Blue at the bottom (90°)",
            Turn::Half => "Blue on the left, the red side's view (180°)",
            Turn::ThreeQuarter => "Blue at the top (270°)",
        }
    }

    /// Where `(u, v)` on the field (0 to 1 from the red wall to the blue,
    /// and from the field's top edge to its bottom, as the image has them)
    /// goes on the turned field (0 to 1 across and down).
    fn place(self, (u, v): (f32, f32)) -> (f32, f32) {
        match self {
            Turn::None => (u, v),
            Turn::Quarter => (1.0 - v, u),
            Turn::Half => (1.0 - u, 1.0 - v),
            Turn::ThreeQuarter => (v, 1.0 - u),
        }
    }

    /// Whether the field's length runs up and down on screen.
    fn upright(self) -> bool {
        matches!(self, Turn::Quarter | Turn::ThreeQuarter)
    }
}

/// Where the field is drawn: its rect on screen (as big as fits in the
/// widget, keeping its shape), its size, and how it's turned.
#[derive(Debug, Clone, Copy)]
pub struct FieldView {
    /// The field's area on screen.
    pub rect: Rect,
    /// The field's length and width, in meters.
    pub size: (f64, f64),
    turn: Turn,
}

impl FieldView {
    /// The view of a `length` by `width` meter field fitted into `area`,
    /// turned `turn`.
    fn fit(area: Rect, (length, width): (f64, f64), turn: Turn) -> FieldView {
        let (across, down) = if turn.upright() {
            (width, length)
        } else {
            (length, width)
        };
        let scale = (f64::from(area.width()) / across).min(f64::from(area.height()) / down);
        let size = egui::vec2((across * scale) as f32, (down * scale) as f32);
        FieldView {
            rect: Rect::from_center_size(area.center(), size),
            size: (length, width),
            turn,
        }
    }

    /// Where `(u, v)` on the unturned field (see [`Turn::place`]) is on
    /// screen.
    fn at_uv(&self, uv: (f32, f32)) -> Pos2 {
        let (x, y) = self.turn.place(uv);
        self.rect.min + egui::vec2(x * self.rect.width(), y * self.rect.height())
    }

    /// Where the field point `(x, y)` (meters, 2027 frame) is on screen.
    pub fn to_screen(&self, x: f64, y: f64) -> Pos2 {
        let (length, width) = self.size;
        let u = (x + length / 2.0) / length;
        let v = (width / 2.0 - y) / width;
        self.at_uv((u as f32, v as f32))
    }
}

/// The field image as a texture, per field: being decoded, ready, or
/// without one.
#[derive(Clone)]
enum Image {
    /// Decoding on another thread; the result lands here.
    Loading(Arc<Mutex<Option<Option<fields::Decoded>>>>),
    /// On the GPU, with the image's size before scaling.
    Ready(egui::TextureHandle, (u32, u32)),
    /// No image, or it couldn't be read.
    None,
}

/// What the Field widgets in a window remember: each field found by its
/// key, and each image's texture.
#[derive(Clone, Default)]
struct Cache(Arc<Mutex<CacheInner>>);

#[derive(Default)]
struct CacheInner {
    entries: HashMap<String, Option<FieldEntry>>,
    images: HashMap<String, Image>,
}

/// The key naming the field a widget with settings `p` shows: its
/// `field`, else Elastic's `field_game` as `game:Rebuilt`, else the
/// default field.
fn field_key(p: &Map<String, Json>) -> String {
    if let Some(key) = p.get(FIELD).and_then(Json::as_str) {
        return key.to_owned();
    }
    match props::text(p, FIELD_GAME, "") {
        "" => fields::DEFAULT_ID.to_owned(),
        game => format!("game:{game}"),
    }
}

/// The field `key` names (see [`field_key`]): a game's first field, or one
/// by id or path.
fn find_field(key: &str) -> Option<FieldEntry> {
    let folders = fields::default_folders();
    match key.strip_prefix("game:") {
        Some(game) => fields::available(&folders)
            .into_iter()
            .find(|f| f.info.game.eq_ignore_ascii_case(game)),
        None => fields::find(key, &folders),
    }
}

/// The window's [`Cache`].
fn cache(ctx: &egui::Context) -> Cache {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Cache>(egui::Id::unique("fields"))
            .clone()
    })
}

/// The field named `key`, found once per window.
fn entry(ctx: &egui::Context, key: &str) -> Option<FieldEntry> {
    let cache = cache(ctx);
    let mut inner = cache.0.lock().unwrap_or_else(|e| e.into_inner());
    inner
        .entries
        .entry(key.to_owned())
        .or_insert_with(|| find_field(key))
        .clone()
}

/// The texture of `field`'s image, once it's decoded (on another thread the
/// first time, so a big image doesn't stall the window).
fn texture(ctx: &egui::Context, field: &FieldEntry) -> Option<(egui::TextureHandle, (u32, u32))> {
    let cache = cache(ctx);
    let mut inner = cache.0.lock().unwrap_or_else(|e| e.into_inner());
    let image = inner.images.entry(field.id.clone()).or_insert_with(|| {
        let Some(bytes) = field.image_bytes() else {
            return Image::None;
        };
        let slot = Arc::new(Mutex::new(None));
        let (done, repaint) = (slot.clone(), ctx.clone());
        let bytes = bytes.into_owned();
        std::thread::spawn(move || {
            let decoded = fields::decode_image(&bytes, MAX_TEXTURE_WIDTH);
            *done.lock().unwrap_or_else(|e| e.into_inner()) = Some(decoded);
            repaint.request_repaint();
        });
        Image::Loading(slot)
    });
    if let Image::Loading(slot) = image {
        let finished = slot.lock().unwrap_or_else(|e| e.into_inner()).take();
        match finished {
            None => return None,
            Some(None) => *image = Image::None,
            Some(Some(d)) => {
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [d.width as usize, d.height as usize],
                    &d.pixels,
                );
                let handle = ctx.load_texture(
                    format!("field {}", field.id),
                    color,
                    egui::TextureOptions::LINEAR,
                );
                *image = Image::Ready(handle, d.source_size);
            }
        }
    }
    match image {
        Image::Ready(handle, size) => Some((handle.clone(), *size)),
        _ => None,
    }
}

/// Draw the field: its image (the part inside the field's bounds), or a
/// plain field until there is one, and its outline and center line.
fn draw_field(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    view: &FieldView,
    info: &FieldInfo,
    image: Option<(egui::TextureHandle, (u32, u32))>,
) {
    let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    match (image, &info.image) {
        (Some((texture, (w, h))), Some(b)) => {
            let (w, h) = (w as f32, h as f32);
            let uv = [
                (b.left / w, b.top / h),
                (b.right / w, b.top / h),
                (b.right / w, b.bottom / h),
                (b.left / w, b.bottom / h),
            ];
            let mut mesh = egui::Mesh::with_texture(texture.id());
            for (corner, (u, v)) in corners.iter().zip(uv) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: view.at_uv(*corner),
                    uv: egui::pos2(u, v),
                    color: Color32::WHITE,
                });
            }
            mesh.indices.extend([0, 1, 2, 0, 2, 3]);
            painter.add(Shape::mesh(mesh));
        }
        _ => {
            painter.rect_filled(view.rect, 0, PLAIN_FIELD);
            let line = Stroke::new(1.0, visuals.weak_text_color());
            painter.line_segment([view.at_uv((0.5, 0.0)), view.at_uv((0.5, 1.0))], line);
            painter.rect_stroke(view.rect, 0, line, egui::StrokeKind::Inside);
        }
    }
}

/// Draw a robot `size` (length, width) meters at `pose`: a filled outline
/// and an arrow from its center to its front.
fn draw_robot(
    painter: &egui::Painter,
    view: &FieldView,
    pose: FieldPose,
    size: (f64, f64),
    color: Color32,
) {
    let (sin, cos) = pose.heading.sin_cos();
    let at = |fx: f64, fy: f64| {
        view.to_screen(pose.x + fx * cos - fy * sin, pose.y + fx * sin + fy * cos)
    };
    let (hl, hw) = (size.0 / 2.0, size.1 / 2.0);
    let outline = vec![at(hl, hw), at(hl, -hw), at(-hl, -hw), at(-hl, hw)];
    painter.add(Shape::convex_polygon(
        outline,
        color.gamma_multiply(0.35),
        Stroke::new(2.0, color),
    ));
    let center = at(0.0, 0.0);
    let front = at(hl, 0.0);
    painter.line_segment([center, front], Stroke::new(2.5, Color32::WHITE));
    painter.add(Shape::convex_polygon(
        vec![
            at(hl, 0.0),
            at(hl * 0.55, hw * 0.45),
            at(hl * 0.55, -hw * 0.45),
        ],
        Color32::WHITE,
        Stroke::NONE,
    ));
}

/// Whether the widget's topic is a `Field2d` table (else a pose topic).
fn is_table(widget: &WidgetContainer) -> bool {
    !props::text(&widget.properties, props::DATA_TYPE, "").starts_with("struct:")
}

/// The robot's pose for `widget` now, in the 2027 frame, on a field `size`
/// meters.
fn robot_pose(widget: &WidgetContainer, live: &Live<'_>, size: (f64, f64)) -> Option<FieldPose> {
    let p = &widget.properties;
    let pose = if is_table(widget) {
        let table = widget.topic()?.trim_end_matches('/');
        let robot = live
            .frame
            .others
            .get(format!("{table}/{FIELD2D_ROBOT}").as_str())?;
        crate::types::field::pose_of(robot, "double[]", None)?
    } else {
        let ty = props::text(p, props::DATA_TYPE, "");
        crate::types::field::pose_of(live.value?, ty, live.frame.structs)?
    };
    let origin = Origin::from_key(props::text(p, ORIGIN, Origin::Center.key()));
    Some(origin.to_center(pose, size.0, size.1))
}

/// The robot's length and width in `p`, in meters.
fn robot_size(p: &Map<String, Json>) -> (f64, f64) {
    (
        props::number(p, ROBOT_LENGTH, DEFAULT_ROBOT_SIZE_M).max(0.05),
        props::number(p, ROBOT_WIDTH, DEFAULT_ROBOT_SIZE_M).max(0.05),
    )
}

/// The Field widget (Elastic's `"Field"`).
pub struct FieldWidget;

impl FieldWidget {
    /// Draw the field and robot in `rect`, named for screen readers (and the
    /// tests) with where the robot is.
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let p = &widget.properties;
        let key = field_key(p);
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let Some(field) = entry(ui.ctx(), &key) else {
            card::draw_message(
                &painter,
                &visuals,
                rect,
                "field not found: pick one in its settings",
            );
            return;
        };
        let size = (field.info.length_m, field.info.width_m);
        let view = FieldView::fit(rect, size, Turn::of(p));
        let image = texture(ui.ctx(), &field);
        draw_field(&painter, &visuals, &view, &field.info, image);
        let pose = robot_pose(widget, live, size);
        if let Some(pose) = pose {
            let color = props::color_property(p, ROBOT_COLOR).unwrap_or(DEFAULT_ROBOT_COLOR);
            draw_robot(&painter, &view, pose, robot_size(p), color);
        }
        let summary = match pose {
            Some(pose) => format!(
                "{}: robot at x {:.2} m, y {:.2} m, facing {:.0}°",
                widget.title,
                pose.x,
                pose.y,
                pose.heading.to_degrees()
            ),
            None => format!("{}: no robot pose", widget.title),
        };
        ui.interact(rect, ui.scope_id().with("field"), Sense::hover())
            .widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &summary)
            });
    }
}

impl WidgetType for FieldWidget {
    fn names(&self) -> &'static [&'static str] {
        &["Field", "Field2d"]
    }
    fn accepts(&self, nt_type: &str) -> bool {
        matches!(nt_type, "struct:Pose2d" | "struct:Pose3d")
    }
    fn table_types(&self) -> &'static [&'static str] {
        &[FIELD2D_TYPE]
    }
    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        let topic = props::text(p, "topic", "").trim_end_matches('/');
        if topic.is_empty() || props::text(p, props::DATA_TYPE, "").starts_with("struct:") {
            Vec::new()
        } else {
            vec![format!("{topic}/{FIELD2D_ROBOT}"), format!("{topic}/.type")]
        }
    }
    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        self.show_in(ui, body.shrink(card::BODY_MARGIN), widget, live);
        None
    }
    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.show_in(ui, rect, widget, live);
        None
    }
    /// A plain field, for previews while dragging.
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        p: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        let Some(field) = entry(painter.ctx(), &field_key(p)) else {
            return;
        };
        let size = (field.info.length_m, field.info.width_m);
        let view = FieldView::fit(rect, size, Turn::of(p));
        draw_field(painter, visuals, &view, &field.info, None);
    }
    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        let found = fields::available(&fields::default_folders());
        // The field shown now, by id (Elastic's game name finds one).
        let key = entry(ui.ctx(), &field_key(p)).map_or_else(|| field_key(p), |f| f.id);
        ui.label("Field");
        ui.horizontal(|ui| {
            let shown = found
                .iter()
                .find(|f| f.id == key)
                .map(|f| f.info.name.clone())
                .unwrap_or_else(|| key.clone());
            let mut picked = key.clone();
            egui::ComboBox::from_id_salt(FIELD)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for f in &found {
                        ui.selectable_value(&mut picked, f.id.clone(), &f.info.name);
                    }
                });
            if ui
                .button("Open field file…")
                .on_hover_text("A WPILib fields JSON, with its image beside it")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Field", &["json"])
                    .pick_file()
            {
                picked = path.display().to_string();
            }
            if picked != key {
                props::set_property(
                    p,
                    FIELD,
                    (picked != fields::DEFAULT_ID).then(|| picked.into()),
                );
                props::set_property(p, FIELD_GAME, None);
            }
        });
        ui.end_row();
        ui.label("Poses' origin");
        let origin = Origin::from_key(props::text(p, ORIGIN, Origin::Center.key()));
        let mut picked = origin;
        egui::ComboBox::from_id_salt(ORIGIN)
            .selected_text(origin_label(origin))
            .show_ui(ui, |ui| {
                for o in Origin::ALL {
                    ui.selectable_value(&mut picked, o, origin_label(o));
                }
            })
            .response
            .on_hover_text(
                "Where (0, 0) is in the poses robot code sends. WPILib 2027 puts it at the \
                 center of the field; before 2027 it was the blue alliance wall's right corner.",
            );
        if picked != origin {
            props::set_property(
                p,
                ORIGIN,
                (picked != Origin::Center).then(|| picked.key().into()),
            );
        }
        ui.end_row();
        ui.label("View");
        let turn = Turn::of(p);
        let mut picked = turn;
        egui::ComboBox::from_id_salt(FIELD_ROTATION)
            .selected_text(turn.label())
            .show_ui(ui, |ui| {
                for t in Turn::ALL {
                    ui.selectable_value(&mut picked, t, t.label());
                }
            });
        if picked != turn {
            props::set_property(
                p,
                FIELD_ROTATION,
                (picked != Turn::None).then(|| picked.degrees().into()),
            );
        }
        ui.end_row();
        props::number_row(
            ui,
            p,
            "Robot length (m)",
            ROBOT_LENGTH,
            DEFAULT_ROBOT_SIZE_M,
            0.05..=5.0,
        );
        props::number_row(
            ui,
            p,
            "Robot width (m)",
            ROBOT_WIDTH,
            DEFAULT_ROBOT_SIZE_M,
            0.05..=5.0,
        );
        props::color_row(ui, p, "Robot color", ROBOT_COLOR, DEFAULT_ROBOT_COLOR);
    }
    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

/// How the settings name `origin`.
fn origin_label(origin: Origin) -> &'static str {
    match origin {
        Origin::Center => "Field center (WPILib 2027)",
        Origin::BlueCorner => "Blue alliance corner (before 2027)",
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn view(turn: Turn) -> FieldView {
        // A 16 by 8 meter field in a 400 by 400 area: 400 by 200 on screen,
        // or 200 by 400 turned a quarter.
        let area = Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 400.0));
        FieldView::fit(area, (16.0, 8.0), turn)
    }

    #[test]
    fn field_points_land_where_the_turn_puts_them() {
        let v = view(Turn::None);
        assert_eq!(v.rect.size(), egui::vec2(400.0, 200.0));
        // The center, the blue wall's left corner (+X, +Y: top right), the
        // red wall's right corner (bottom left).
        assert_eq!(v.to_screen(0.0, 0.0), egui::pos2(200.0, 200.0));
        assert_eq!(v.to_screen(8.0, 4.0), egui::pos2(400.0, 100.0));
        assert_eq!(v.to_screen(-8.0, -4.0), egui::pos2(0.0, 300.0));
        // From the red side, blue is on the left.
        let red = view(Turn::Half);
        assert_eq!(red.to_screen(8.0, 4.0), egui::pos2(0.0, 300.0));
        // A quarter turn clockwise: blue at the bottom, the field upright.
        let q = view(Turn::Quarter);
        assert_eq!(q.rect.size(), egui::vec2(200.0, 400.0));
        assert_eq!(q.to_screen(8.0, 0.0), egui::pos2(200.0, 400.0));
        let tq = view(Turn::ThreeQuarter);
        assert_eq!(tq.to_screen(8.0, 0.0), egui::pos2(200.0, 0.0));
        // Saved as degrees; anything near a quarter rounds to it.
        let p = serde_json::json!({"field_rotation": 175.0});
        assert_eq!(Turn::of(p.as_object().unwrap()), Turn::Half);
        let p = serde_json::json!({"field_rotation": -90});
        assert_eq!(Turn::of(p.as_object().unwrap()), Turn::ThreeQuarter);
    }

    #[test]
    fn elastics_game_name_finds_a_field_and_raptaboards_setting_wins() {
        let elastic = serde_json::json!({"field_game": "Rebuilt"});
        let key = field_key(elastic.as_object().unwrap());
        assert_eq!(key, "game:Rebuilt");
        assert_eq!(find_field(&key).unwrap().info.game, "Rebuilt");
        assert!(find_field("game:Nothing").is_none());
        let both = serde_json::json!({"field_game": "Rebuilt", "field": "2026-rebuilt-andymark"});
        let key = field_key(both.as_object().unwrap());
        assert_eq!(
            find_field(&key).unwrap().info.name,
            "2026 FRC Rebuilt AndyMark"
        );
        assert_eq!(field_key(&Map::new()), fields::DEFAULT_ID);
    }

    #[test]
    fn a_field2d_table_needs_its_robot_and_a_pose_topic_nothing_more() {
        let table = serde_json::json!({"topic": "/SmartDashboard/Field"});
        assert_eq!(
            FieldWidget.extra_topics(table.as_object().unwrap(), Server::default()),
            ["/SmartDashboard/Field/Robot", "/SmartDashboard/Field/.type"]
        );
        let pose = serde_json::json!({"topic": "/Drive/pose", "data_type": "struct:Pose2d"});
        assert!(
            FieldWidget
                .extra_topics(pose.as_object().unwrap(), Server::default())
                .is_empty()
        );
        assert!(FieldWidget.accepts("struct:Pose3d") && !FieldWidget.accepts("struct:Pose2d[]"));
    }
}
