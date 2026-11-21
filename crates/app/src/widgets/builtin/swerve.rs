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
//! (`max_speed`), and the whole robot's motion from `ChassisSpeeds` topics
//! (`speeds_topic`, `speeds_setpoint_topic`): an arrow from its center for
//! its velocity and an arc inside the frame for its spin, as Chassis Speeds
//! draws them (`max_spin` scales the arc). Dropping a states topic from
//! the tree on the widget sets the requested states; chassis speeds the
//! robot's motion (measured first, then requested); a rotation or pose
//! the heading.

use std::f32::consts::PI;

use eframe::egui::{self, Rect, Sense, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::chassis_speeds::{DEFAULT_MAX_SPIN, MAX_SPIN, motion_text};
use crate::tree::DraggedTopic;
use crate::types::angle;
use crate::types::motion::{self, Motion};
use crate::types::swerve::{self, CORNER_SHORT, Module, RotationUnit, SWERVE_TYPE};
use crate::widgets::shared::robot_view::{MEASURED, REQUESTED, RobotView, arrow, motion_arrows};
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
/// The whole robot's measured motion, a `ChassisSpeeds` topic, drawn from
/// its center (raptaboard).
pub const SPEEDS_TOPIC: &str = "speeds_topic";
/// The whole robot's requested motion, a `ChassisSpeeds` topic (raptaboard).
pub const SPEEDS_SETPOINT_TOPIC: &str = "speeds_setpoint_topic";

const KEYS: [&str; 8] = [
    SHOW_ROBOT_ROTATION,
    ROTATION_UNIT,
    SETPOINT_TOPIC,
    HEADING_TOPIC,
    MAX_SPEED,
    SPEEDS_TOPIC,
    SPEEDS_SETPOINT_TOPIC,
    MAX_SPIN,
];

/// A full-length arrow's speed when `max_speed` isn't set, m/s (about a
/// competition swerve's top speed).
const DEFAULT_MAX_SPEED: f64 = 4.5;
/// Slower than this (m/s), a module draws no arrow: it's standing still.
const STILL: f64 = 0.05;

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
    /// The whole robot's measured and requested motion, if set.
    speeds: Option<Motion>,
    speeds_requested: Option<Motion>,
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
        heading = angle::untyped_heading(value, structs);
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
    let motion_in = |key: &str| {
        topic_in(p, key)
            .and_then(|t| newest(live, t))
            .and_then(|v| motion::untyped_motion(v, structs))
    };
    Shown {
        measured,
        requested,
        heading: heading.filter(|_| props::flag(p, SHOW_ROBOT_ROTATION, true)),
        speeds: motion_in(SPEEDS_TOPIC),
        speeds_requested: motion_in(SPEEDS_SETPOINT_TOPIC),
    }
}

/// Draw the robot frame in `rect`, then each module's wheel and arrows.
fn draw_robot(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    shown: &Shown,
    max_speed: f64,
    max_spin: f64,
) {
    // Small enough that a robot turned 45° (modules half·√2 from the
    // center, plus their rings) still fits.
    let robot = RobotView::fit(rect, 0.4, shown.heading);
    let frame = visuals.widgets.inactive.bg_fill;
    let wheel = visuals.weak_text_color();
    robot.draw_frame(painter, visuals);
    let h = robot.half;
    let ring = h * 0.4;
    let reach = 2.0 * h;
    for i in 0..4 {
        let c = robot.corner(i);
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
    // The whole robot's motion, from its center, on the wheels' scale (a
    // robot driving straight has every arrow the same length); the spin
    // arcs inside the frame.
    if let Some(m) = &shown.speeds_requested {
        motion_arrows(
            painter,
            &robot,
            m,
            true,
            reach,
            h * 0.8,
            max_speed,
            max_spin,
        );
    }
    if let Some(m) = &shown.speeds {
        motion_arrows(
            painter,
            &robot,
            m,
            false,
            reach,
            h * 0.65,
            max_speed,
            max_spin,
        );
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
    if let Some(m) = &shown.speeds {
        text += &format!("; robot {}", motion_text(m));
    }
    if let Some(m) = &shown.speeds_requested {
        text += &format!("; robot requested {}", motion_text(m));
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
        let max_spin = props::number(p, MAX_SPIN, DEFAULT_MAX_SPIN).max(1.0);
        draw_robot(&painter, &visuals, rect, &now, max_speed, max_spin);
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
            [
                SETPOINT_TOPIC,
                HEADING_TOPIC,
                SPEEDS_TOPIC,
                SPEEDS_SETPOINT_TOPIC,
            ]
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
    /// rotation, pose or number becomes its heading; chassis speeds become
    /// the robot's measured motion, then its requested motion.
    fn add_topics(&self, widget: &mut WidgetContainer, topics: &[DraggedTopic]) -> String {
        let own = widget.topic().unwrap_or("").to_owned();
        let mut said = Vec::new();
        for t in topics.iter().filter(|t| t.field.is_none() && t.name != own) {
            let speeds = matches!(
                crate::types::structs::struct_type(&t.ty),
                Some(("ChassisSpeeds" | "ChassisVelocities", false))
            );
            let key = if swerve::is_module_array(&t.ty) {
                SETPOINT_TOPIC
            } else if speeds {
                // The first speeds are the measured ones; then requested.
                if topic_in(&widget.properties, SPEEDS_TOPIC).is_none() {
                    SPEEDS_TOPIC
                } else {
                    SPEEDS_SETPOINT_TOPIC
                }
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
            said.push(match key {
                SETPOINT_TOPIC => format!("requested states from {}", t.name),
                SPEEDS_TOPIC => format!("the robot's motion from {}", t.name),
                SPEEDS_SETPOINT_TOPIC => format!("the robot's requested motion from {}", t.name),
                _ => format!("heading from {}", t.name),
            });
        }
        if said.is_empty() {
            "A swerve view takes requested module states, chassis speeds, or a rotation or pose for its heading"
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
            speeds: None,
            speeds_requested: None,
        };
        draw_robot(
            painter,
            visuals,
            rect,
            &none,
            DEFAULT_MAX_SPEED,
            DEFAULT_MAX_SPIN,
        );
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
        props::number_row(
            ui,
            p,
            "¾ turn (°/s)",
            MAX_SPIN,
            DEFAULT_MAX_SPIN,
            1.0..=3600.0,
        );
        for (label, key, hover) in [
            (
                "Robot speeds",
                SPEEDS_TOPIC,
                "The whole robot's measured motion (ChassisSpeeds), drawn from its center",
            ),
            (
                "Requested speeds",
                SPEEDS_SETPOINT_TOPIC,
                "The whole robot's requested motion (ChassisSpeeds), dashed",
            ),
        ] {
            ui.label(label).on_hover_text(hover);
            let mut topic = props::text(p, key, "").to_owned();
            if props::topic_picker(ui, key, &mut topic, value.topics, |t| {
                motion::is_motion_type(t) && !t.contains("Translation2d")
            }) {
                props::set_property(p, key, (!topic.is_empty()).then(|| topic.into()));
            }
            ui.end_row();
        }
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
                angle::is_heading_type as fn(&str) -> bool,
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

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

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
            speeds: None,
            speeds_requested: None,
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
            speeds: None,
            speeds_requested: None,
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
                dragged("/Drive/MeasuredSpeeds", "struct:ChassisSpeeds"),
                dragged("/Drive/CommandedSpeeds", "struct:ChassisSpeeds"),
            ],
        );
        // The first speeds dropped are the measured ones, then requested.
        assert_eq!(w.properties[SPEEDS_TOPIC], "/Drive/MeasuredSpeeds");
        assert_eq!(
            w.properties[SPEEDS_SETPOINT_TOPIC],
            "/Drive/CommandedSpeeds"
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
                "/Drive/Pose",
                "/Drive/MeasuredSpeeds",
                "/Drive/CommandedSpeeds"
            ]
        );
    }
}
