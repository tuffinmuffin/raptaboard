//! Swerve Drive (Elastic's `"SwerveDrive"`): the robot from above with its
//! four modules (front left, front right, back left, back right), each
//! wheel turned the way it points and an arrow for its speed. The front of
//! the robot is up, or the robot turns with its heading.
//!
//! Bound to a `SwerveModuleState[]` struct topic (2027:
//! `SwerveModuleVelocity[]`), the measured states, or Elastic's
//! `SwerveDrive` table (read by [`crate::types::swerve`]). raptaboard adds
//! the requested states (`setpoint_topic`, drawn as dashed arrows against
//! the measured ones), a topic for the robot's heading (`heading_topic`)
//! for a struct-bound widget, and the speed of a full-length arrow
//! (`max_speed`). Dropping a states topic from the tree on the widget sets
//! the requested states; a rotation or pose sets the heading.

use std::f32::consts::PI;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::tree::DraggedTopic;
use crate::types::angle;
use crate::types::swerve::{self, CORNER_SHORT, Module, RotationUnit, SWERVE_TYPE};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// Turn the robot with its heading (Elastic).
pub const SHOW_ROBOT_ROTATION: &str = "show_robot_rotation";
/// The unit of an Elastic table's angles: `Radians`, `Degrees` or
/// `Rotations` (Elastic).
pub const ROTATION_UNIT: &str = "rotation_unit";
/// The requested module states, a `SwerveModuleState[]` topic
/// (raptaboard).
pub const SETPOINT_TOPIC: &str = "setpoint_topic";
/// The robot's heading for a struct-bound widget: a `Rotation2d`,
/// `Rotation3d`, `Pose2d` or `Pose3d` topic, or a number of degrees
/// counterclockwise (raptaboard).
pub const HEADING_TOPIC: &str = "heading_topic";
/// The speed drawn as a full-length arrow, meters per second (raptaboard).
pub const MAX_SPEED: &str = "max_speed";

const KEYS: [&str; 5] = [
    SHOW_ROBOT_ROTATION,
    ROTATION_UNIT,
    SETPOINT_TOPIC,
    HEADING_TOPIC,
    MAX_SPEED,
];

/// A full-length arrow's speed when `max_speed` isn't set, m/s (about a
/// competition swerve's top speed).
const DEFAULT_MAX_SPEED: f64 = 4.5;
/// Slower than this (m/s), a module draws no arrow: it's standing still.
const STILL: f64 = 0.05;
/// The measured arrows' color (Material blue 500, as the Gyro's needle).
const MEASURED: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);
/// The requested arrows' color (Material orange 500).
const REQUESTED: Color32 = Color32::from_rgb(0xFF, 0x98, 0x00);

/// The Swerve Drive widget.
pub struct SwerveDrive;

/// Whether `p`'s widget is bound to Elastic's table (no data type).
fn is_table(p: &Map<String, Json>) -> bool {
    props::text(p, props::DATA_TYPE, "").is_empty()
}

/// The newest value of `topic`, if the frame has it.
fn newest<'a>(live: &Live<'a>, topic: &str) -> Option<&'a Value> {
    live.frame.others.get(topic).copied()
}

/// The topic `key` names in `p`, if set.
fn topic_in<'a>(p: &'a Map<String, Json>, key: &str) -> Option<&'a str> {
    Some(props::text(p, key, "").trim()).filter(|t| !t.is_empty())
}

/// What a swerve widget shows now: measured modules, requested ones, and
/// the robot's heading (radians counterclockwise) if it's turned with it.
struct Shown {
    measured: Vec<Module>,
    requested: Vec<Module>,
    heading: Option<f64>,
}

/// What `widget` shows now. The newest values are used for all of it, so
/// the measured and requested arrows are from the same moment.
fn shown(widget: &WidgetContainer, live: &Live<'_>) -> Shown {
    let p = &widget.properties;
    let structs = live.frame.structs;
    let others = live.frame.others;
    let topic = widget.topic().unwrap_or("");
    let (measured, mut heading) = if is_table(p) {
        let unit = RotationUnit::from_key(props::text(p, ROTATION_UNIT, ""));
        (
            swerve::table_modules(topic, others, unit),
            swerve::table_robot_angle(topic, others, unit),
        )
    } else {
        let ty = props::text(p, props::DATA_TYPE, "");
        let value = newest(live, topic).or(live.value);
        (
            value
                .map(|v| swerve::modules_of(v, ty, structs))
                .unwrap_or_default(),
            None,
        )
    };
    if let Some(value) = topic_in(p, HEADING_TOPIC).and_then(|t| newest(live, t)) {
        heading = heading_in(value, structs);
    }
    let requested = topic_in(p, SETPOINT_TOPIC)
        .and_then(|t| newest(live, t))
        .map(|v| {
            // Either name: their bytes are laid out the same.
            [
                "struct:SwerveModuleState[]",
                "struct:SwerveModuleVelocity[]",
            ]
            .iter()
            .map(|ty| swerve::modules_of(v, ty, structs))
            .find(|m| !m.is_empty())
            .unwrap_or_default()
        })
        .unwrap_or_default();
    Shown {
        measured,
        requested,
        heading: heading.filter(|_| props::flag(p, SHOW_ROBOT_ROTATION, true)),
    }
}

/// The heading (radians counterclockwise) in `value`, a heading topic's,
/// whose type the widget doesn't know: a number is degrees
/// counterclockwise; a struct is told by its size (a `Rotation2d` is 8
/// bytes, a `Pose2d` 24, a `Rotation3d` 32, a `Pose3d` 56).
fn heading_in(value: &Value, structs: Option<&nt4::proto::structs::StructDatabase>) -> Option<f64> {
    let ty = match value {
        Value::Raw(bytes) => match bytes.len() {
            8 => "struct:Rotation2d",
            24 => "struct:Pose2d",
            32 => "struct:Rotation3d",
            56 => "struct:Pose3d",
            _ => return None,
        },
        _ => "double",
    };
    angle::heading_of(value, ty, structs, true).map(|h| h.degrees.to_radians())
}

/// Where the robot is drawn in a rect: its center, its size, and which way
/// it's turned.
struct Robot {
    center: Pos2,
    /// Half the distance between modules, points.
    half: f32,
    /// The robot's turn, radians counterclockwise.
    heading: f32,
}

impl Robot {
    fn fit(rect: Rect, heading: Option<f64>) -> Robot {
        let side = rect.width().min(rect.height());
        Robot {
            center: rect.center(),
            // Small enough that a robot turned 45° (modules half·√2 from
            // the center, plus their rings) still fits.
            half: side * 0.2,
            heading: heading.unwrap_or(0.0) as f32,
        }
    }

    /// The on-screen direction of `angle` (radians counterclockwise from
    /// the robot's front): the front is up when the robot isn't turned.
    fn dir(&self, angle: f32) -> Vec2 {
        let a = angle + self.heading;
        egui::vec2(-a.sin(), -a.cos())
    }

    /// The point `forward` and `left` points from the robot's center.
    fn at(&self, forward: f32, left: f32) -> Pos2 {
        self.center + self.dir(0.0) * forward + self.dir(PI / 2.0) * left
    }

    /// Module `i`'s center (front left, front right, back left, back right).
    fn module(&self, i: usize) -> Pos2 {
        let forward = if i < 2 { self.half } else { -self.half };
        let left = if i.is_multiple_of(2) {
            self.half
        } else {
            -self.half
        };
        self.at(forward, left)
    }
}

/// An arrow from `from` along `dir` (a unit vector) `length` points long.
fn arrow(
    painter: &egui::Painter,
    from: Pos2,
    dir: Vec2,
    length: f32,
    stroke: Stroke,
    dashed: bool,
) {
    let tip = from + dir * length;
    if dashed {
        painter.extend(egui::Shape::dashed_line(&[from, tip], stroke, 5.0, 3.0));
    } else {
        painter.line_segment([from, tip], stroke);
    }
    let head = (length * 0.3).clamp(5.0, 10.0);
    let side = egui::vec2(-dir.y, dir.x);
    for s in [1.0, -1.0] {
        painter.line_segment([tip, tip - dir * head + side * s * head * 0.6], stroke);
    }
}

/// Draw the robot frame in `rect`, then each module's wheel and arrows.
fn draw_robot(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    shown: &Shown,
    max_speed: f64,
) {
    let robot = Robot::fit(rect, shown.heading);
    let frame = visuals.widgets.inactive.bg_fill;
    let wheel = visuals.weak_text_color();
    // The frame between the modules, with its front edge picked out.
    let h = robot.half;
    let corners = [
        robot.at(h, h),
        robot.at(h, -h),
        robot.at(-h, -h),
        robot.at(-h, h),
    ];
    painter.add(egui::Shape::closed_line(
        corners.to_vec(),
        Stroke::new(2.0, frame),
    ));
    painter.line_segment(
        [corners[0], corners[1]],
        Stroke::new(4.0, visuals.weak_text_color()),
    );
    // Which way is front: an arrowhead in the middle.
    let nose = h * 0.35;
    painter.add(egui::Shape::convex_polygon(
        vec![
            robot.at(nose, 0.0),
            robot.at(-nose * 0.6, nose * 0.4),
            robot.at(-nose * 0.6, -nose * 0.4),
        ],
        visuals.weak_text_color(),
        Stroke::NONE,
    ));
    let ring = h * 0.4;
    let reach = 2.0 * h;
    for i in 0..4 {
        let c = robot.module(i);
        painter.circle(c, ring, visuals.extreme_bg_color, Stroke::new(1.5, frame));
        let speed_dir = |m: &Module| {
            // A wheel driving backwards moves the other way.
            let flip = if m.speed < 0.0 { PI } else { 0.0 };
            robot.dir(m.angle as f32 + flip)
        };
        let length = |m: &Module| (m.speed.abs() / max_speed).min(1.5) as f32 * reach;
        if let Some(m) = shown.measured.get(i) {
            // The wheel: a bar along the way it points.
            let along = robot.dir(m.angle as f32) * ring * 0.75;
            painter.line_segment([c - along, c + along], Stroke::new(ring * 0.3, wheel));
            if m.speed.abs() >= STILL {
                arrow(
                    painter,
                    c,
                    speed_dir(m),
                    length(m),
                    Stroke::new(3.0, MEASURED),
                    false,
                );
            }
        }
        if let Some(m) = shown.requested.get(i)
            && m.speed.abs() >= STILL
        {
            arrow(
                painter,
                c,
                speed_dir(m),
                length(m),
                Stroke::new(2.0, REQUESTED),
                true,
            );
        }
    }
}

/// A module for the summary: `FL 2.00 m/s at 90°` (the angle from -180 to
/// 180).
fn module_text(name: &str, m: &Module) -> String {
    let degrees = (m.angle.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
    let degrees = if degrees.round() == -180.0 {
        180.0
    } else {
        degrees
    };
    let speed = format!("{:.2}", m.speed);
    let speed = if speed == "-0.00" {
        "0.00".into()
    } else {
        speed
    };
    format!("{name} {speed} m/s at {:.0}°", degrees + 0.0)
}

/// The widget's summary, for screen readers and tests: each module, then
/// the requested ones, then the heading.
fn summary(title: &str, shown: &Shown) -> String {
    if shown.measured.is_empty() {
        return format!("{title}: no module states");
    }
    let list = |modules: &[Module]| -> String {
        modules
            .iter()
            .zip(CORNER_SHORT)
            .map(|(m, n)| module_text(n, m))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = format!("{title}: {}", list(&shown.measured));
    if !shown.requested.is_empty() {
        text += &format!("; requested {}", list(&shown.requested));
    }
    if let Some(h) = shown.heading {
        text += &format!("; heading {:.0}°", h.to_degrees() + 0.0);
    }
    text
}

impl SwerveDrive {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let p = &widget.properties;
        let now = shown(widget, live);
        let max_speed = props::number(p, MAX_SPEED, DEFAULT_MAX_SPEED).max(0.1);
        draw_robot(&painter, &visuals, rect, &now, max_speed);
        if now.measured.is_empty() {
            card::draw_message(&painter, &visuals, rect, "no module states");
        }
        let text = summary(&widget.title, &now);
        ui.interact(rect, ui.scope_id().with("swerve"), Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text));
    }
}

impl WidgetType for SwerveDrive {
    fn names(&self) -> &'static [&'static str] {
        &["SwerveDrive"]
    }

    fn accepts(&self, nt_type: &str) -> bool {
        swerve::is_module_array(nt_type)
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[SWERVE_TYPE]
    }

    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        let topic = props::text(p, "topic", "");
        let mut wanted = if is_table(p) && !topic.is_empty() {
            swerve::table_topics(topic)
        } else {
            vec![topic.to_owned()]
        };
        wanted.extend(
            [SETPOINT_TOPIC, HEADING_TOPIC]
                .into_iter()
                .filter_map(|k| topic_in(p, k).map(str::to_owned)),
        );
        wanted.retain(|t| !t.is_empty());
        wanted
    }

    fn takes_topics(&self) -> bool {
        true
    }

    /// Module states dropped on the widget become its requested states; a
    /// rotation, pose or number becomes its heading.
    fn add_topics(&self, widget: &mut WidgetContainer, topics: &[DraggedTopic]) -> String {
        let own = widget.topic().unwrap_or("").to_owned();
        let mut said = Vec::new();
        for t in topics.iter().filter(|t| t.field.is_none() && t.name != own) {
            let key = if swerve::is_module_array(&t.ty) {
                SETPOINT_TOPIC
            } else if matches!(
                crate::types::structs::struct_type(&t.ty),
                Some(("Rotation2d" | "Rotation3d" | "Pose2d" | "Pose3d", false))
            ) || matches!(t.ty.as_str(), "double" | "float" | "int")
            {
                HEADING_TOPIC
            } else {
                continue;
            };
            props::set_property(&mut widget.properties, key, Some(t.name.clone().into()));
            said.push(if key == SETPOINT_TOPIC {
                format!("requested states from {}", t.name)
            } else {
                format!("heading from {}", t.name)
            });
        }
        if said.is_empty() {
            "A swerve view takes requested module states, or a rotation or pose for its heading"
                .to_owned()
        } else {
            format!("\"{}\" now shows {}", widget.title, said.join(" and "))
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

    /// The robot with no states, for previews while dragging.
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        let none = Shown {
            measured: Vec::new(),
            requested: Vec::new(),
            heading: None,
        };
        draw_robot(painter, visuals, rect, &none, DEFAULT_MAX_SPEED);
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, value: ValueInfo<'_>) {
        props::number_row(
            ui,
            p,
            "Full arrow (m/s)",
            MAX_SPEED,
            DEFAULT_MAX_SPEED,
            0.1..=20.0,
        );
        props::flag_row(
            ui,
            p,
            "Turn with heading",
            "Turn the robot the way it faces (front up when off)",
            SHOW_ROBOT_ROTATION,
            true,
        );
        if is_table(p) {
            let units: Vec<&str> = RotationUnit::ALL.iter().map(|u| u.key()).collect();
            props::choice_row(
                ui,
                p,
                "Angles in",
                ROTATION_UNIT,
                RotationUnit::Degrees.key(),
                &units,
            );
            return;
        }
        for (label, key, fits, hover) in [
            (
                "Requested states",
                SETPOINT_TOPIC,
                swerve::is_module_array as fn(&str) -> bool,
                "The states asked for, drawn as dashed arrows against the measured ones",
            ),
            (
                "Heading",
                HEADING_TOPIC,
                heading_type as fn(&str) -> bool,
                "Turns the robot the way it faces: a rotation, a pose, or degrees counterclockwise",
            ),
        ] {
            ui.label(label).on_hover_text(hover);
            let mut topic = props::text(p, key, "").to_owned();
            if props::topic_picker(ui, key, &mut topic, value.topics, fits) {
                props::set_property(p, key, (!topic.is_empty()).then(|| topic.into()));
            }
            ui.end_row();
        }
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

/// Whether a topic of NT type `ty` can be a swerve view's heading.
fn heading_type(ty: &str) -> bool {
    matches!(ty, "double" | "float" | "int")
        || matches!(
            crate::types::structs::struct_type(ty),
            Some(("Rotation2d" | "Rotation3d" | "Pose2d" | "Pose3d", false))
        )
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn modules_sit_at_the_corners_and_turn_with_the_robot() {
        let rect = Rect::from_center_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let upright = Robot::fit(rect, None);
        let fl = upright.module(0);
        assert!(
            fl.x < 0.0 && fl.y < 0.0,
            "front left is up and left: {fl:?}"
        );
        let br = upright.module(3);
        assert!(
            br.x > 0.0 && br.y > 0.0,
            "back right is down and right: {br:?}"
        );
        // A wheel at 90° points left.
        let left = upright.dir(PI / 2.0);
        assert!((left.x + 1.0).abs() < 1e-6 && left.y.abs() < 1e-6);
        // Turned 90° counterclockwise, the front points left, so front left
        // is down and left.
        let turned = Robot::fit(rect, Some(std::f64::consts::FRAC_PI_2));
        let fl = turned.module(0);
        assert!(fl.x < 0.0 && fl.y > 0.0, "{fl:?}");
    }

    #[test]
    fn the_summary_lists_modules_requested_and_heading() {
        let m = |speed, degrees: f64| Module {
            speed,
            angle: degrees.to_radians(),
        };
        let shown = Shown {
            measured: vec![m(2.0, 90.0), m(-0.001, 270.0), m(0.0, -180.0), m(1.0, 0.0)],
            requested: vec![m(2.5, 90.0)],
            heading: Some(std::f64::consts::PI),
        };
        assert_eq!(
            summary("Swerve", &shown),
            "Swerve: FL 2.00 m/s at 90°, FR 0.00 m/s at -90°, BL 0.00 m/s at 180°, \
             BR 1.00 m/s at 0°; requested FL 2.50 m/s at 90°; heading 180°"
        );
        let empty = Shown {
            measured: Vec::new(),
            requested: Vec::new(),
            heading: None,
        };
        assert_eq!(summary("S", &empty), "S: no module states");
    }

    #[test]
    fn dropped_topics_set_the_requested_states_and_heading() {
        let mut w = WidgetContainer {
            title: "Swerve".into(),
            rect: raptaboard_layout::Rect {
                x: 0.0,
                y: 0.0,
                width: 256.0,
                height: 256.0,
            },
            widget_type: "SwerveDrive".into(),
            properties: serde_json::json!({"topic": "/Drive/MeasuredStates", "data_type": "struct:SwerveModuleState[]"})
                .as_object()
                .unwrap()
                .clone(),
            extra: Map::new(),
        };
        let dragged = |name: &str, ty: &str| DraggedTopic {
            name: name.into(),
            ty: ty.into(),
            field: None,
        };
        let said = SwerveDrive.add_topics(
            &mut w,
            &[
                dragged("/Drive/SetpointStates", "struct:SwerveModuleState[]"),
                dragged("/Drive/Pose", "struct:Pose2d"),
                dragged("/Drive/flag", "boolean"),
            ],
        );
        assert_eq!(w.properties[SETPOINT_TOPIC], "/Drive/SetpointStates");
        assert_eq!(w.properties[HEADING_TOPIC], "/Drive/Pose");
        assert!(
            said.contains("requested states") && said.contains("heading"),
            "{said}"
        );
        assert_eq!(
            SwerveDrive.extra_topics(&w.properties, Server::default()),
            [
                "/Drive/MeasuredStates",
                "/Drive/SetpointStates",
                "/Drive/Pose"
            ]
        );
    }
}
