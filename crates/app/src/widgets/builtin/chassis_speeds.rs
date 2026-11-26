//! Chassis Speeds (raptaboard): how the robot is moving, drawn on the robot
//! from above: an arrow for its velocity (forward and sideways) and a
//! curved arrow round it for its spin, showing the motion more than the
//! numbers. The front is up, or the robot turns with its heading.
//!
//! Bound to a `ChassisSpeeds` struct topic (2027: `ChassisVelocities`),
//! relative to the robot, as robot code's measured speeds are; a
//! `Translation2d` shows as a velocity with no spin. As the swerve view
//! does, it draws requested speeds (`setpoint_topic`) dashed against the
//! measured ones, turns with a heading topic (`heading_topic`), and scales
//! its arrow to `max_speed`; `max_spin` scales the spin. Dropping speeds
//! from the tree on the widget sets the requested ones; a rotation or pose
//! sets the heading. The measured and requested numbers are written below
//! the robot in their colors when the card has room.

use eframe::egui::{self, Color32, Rect, Sense};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::swerve::{HEADING_TOPIC, MAX_SPEED, SETPOINT_TOPIC};
use crate::tree::DraggedTopic;
use crate::types::angle;
use crate::types::motion::{self, Motion};
use crate::widgets::shared::robot_view::{MEASURED, REQUESTED, RobotView, motion_arrows};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// The spin drawn as a three-quarter turn, degrees per second (raptaboard).
pub const MAX_SPIN: &str = "max_spin";

const KEYS: [&str; 4] = [SETPOINT_TOPIC, HEADING_TOPIC, MAX_SPEED, MAX_SPIN];

/// A full-length arrow's speed when `max_speed` isn't set, m/s.
const DEFAULT_MAX_SPEED: f64 = 4.5;
/// A three-quarter turn's spin when `max_spin` isn't set, degrees per
/// second (a full turn a second).
pub(crate) const DEFAULT_MAX_SPIN: f64 = 360.0;

/// Size of the numbers, points.
const TEXT_SIZE: f32 = 11.0;
/// The smallest body that shows the numbers, points: about a 2 by 2 card.
const MIN_TEXT_WIDTH: f32 = 200.0;
const MIN_TEXT_HEIGHT: f32 = 160.0;

/// The Chassis Speeds widget.
pub struct ChassisSpeeds;

/// What the widget shows now.
struct Shown {
    measured: Option<Motion>,
    requested: Option<Motion>,
    /// The robot's heading, radians counterclockwise, if it's turned.
    heading: Option<f64>,
}

/// The topic `key` names in `p`, if set.
fn topic_in<'a>(p: &'a Map<String, Json>, key: &str) -> Option<&'a str> {
    Some(props::text(p, key, "").trim()).filter(|t| !t.is_empty())
}

/// What `widget` shows now, all from the newest values so the measured and
/// requested speeds are from the same moment.
fn shown(widget: &WidgetContainer, live: &Live<'_>) -> Shown {
    let p = &widget.properties;
    let structs = live.frame.structs;
    let newest = |t: &str| live.frame.others.get(t).copied();
    let ty = props::text(p, props::DATA_TYPE, "");
    let measured = live
        .newest(widget)
        .and_then(|v| motion::motion_of(v, ty, structs));
    let requested = topic_in(p, SETPOINT_TOPIC)
        .and_then(newest)
        .and_then(|v| motion::untyped_motion(v, structs));
    let heading = topic_in(p, HEADING_TOPIC)
        .and_then(newest)
        .and_then(|v| angle::untyped_heading(v, structs));
    Shown {
        measured,
        requested,
        heading,
    }
}

/// Draw the robot in `rect` with its measured and requested motion.
fn draw(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    shown: &Shown,
    max_speed: f64,
    max_spin: f64,
) {
    let robot = RobotView::fit(rect, 0.36, shown.heading);
    robot.draw_frame(painter, visuals);
    let reach = rect.width().min(rect.height()) * 0.42;
    if let Some(m) = &shown.requested {
        motion_arrows(
            painter,
            &robot,
            m,
            true,
            reach,
            robot.half * 1.85,
            max_speed,
            max_spin,
        );
    }
    if let Some(m) = &shown.measured {
        motion_arrows(
            painter,
            &robot,
            m,
            false,
            reach,
            robot.half * 1.6,
            max_speed,
            max_spin,
        );
    }
}

/// `n` to `places` decimals, never `-0`.
fn rounded(n: f64, places: usize) -> String {
    let text = format!("{n:.places$}");
    match text.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_owned(),
        _ => text,
    }
}

/// A motion for the summary: `forward 1.00 m/s, left 0.50 m/s, spin 30°/s`.
pub(crate) fn motion_text(m: &Motion) -> String {
    let mut text = format!(
        "forward {} m/s, left {} m/s",
        rounded(m.vx, 2),
        rounded(m.vy, 2)
    );
    if let Some(omega) = m.omega {
        text += &format!(", spin {}°/s", rounded(omega.to_degrees(), 0));
    }
    text
}

/// A motion in a few characters, for the card: `1.50 fwd  -0.50 left m/s  29°/s`.
fn short_text(m: &Motion) -> String {
    let mut text = format!("{} fwd  {} left m/s", rounded(m.vx, 2), rounded(m.vy, 2));
    if let Some(omega) = m.omega {
        text += &format!("  {}°/s", rounded(omega.to_degrees(), 0));
    }
    text
}

/// Draw `text` centered in `row`, at [`TEXT_SIZE`] or smaller if it
/// doesn't fit across.
fn draw_line(painter: &egui::Painter, row: Rect, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::monospace(TEXT_SIZE), color);
    let width = galley.size().x;
    let size = if width > row.width() && width > 0.0 {
        (TEXT_SIZE * row.width() / width).max(6.0)
    } else {
        TEXT_SIZE
    };
    painter.text(
        row.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(size),
        color,
    );
}

/// The widget's summary, for screen readers and tests.
fn summary(title: &str, shown: &Shown) -> String {
    let Some(m) = &shown.measured else {
        return format!("{title}: no speeds");
    };
    let mut text = format!("{title}: {}", motion_text(m));
    if let Some(r) = &shown.requested {
        text += &format!("; requested {}", motion_text(r));
    }
    if let Some(h) = shown.heading {
        text += &format!("; heading {}°", rounded(h.to_degrees(), 0));
    }
    text
}

impl ChassisSpeeds {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let p = &widget.properties;
        let now = shown(widget, live);
        let max_speed = props::number(p, MAX_SPEED, DEFAULT_MAX_SPEED).max(0.1);
        let max_spin = props::number(p, MAX_SPIN, DEFAULT_MAX_SPIN).max(1.0);
        // The numbers below the robot, when there's room: measured in its
        // color, requested in its.
        let lines: Vec<(String, Color32)> = [
            now.measured
                .map(|m| (format!("measured  {}", short_text(&m)), MEASURED)),
            now.requested
                .map(|m| (format!("requested  {}", short_text(&m)), REQUESTED)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let room = rect.width() >= MIN_TEXT_WIDTH && rect.height() >= MIN_TEXT_HEIGHT;
        let robot_rect = if room && !lines.is_empty() {
            let line_height = TEXT_SIZE + 4.0;
            let (robot_rect, text_rect) =
                rect.split_top_bottom_at_y(rect.max.y - line_height * lines.len() as f32);
            for (i, (line, color)) in lines.iter().enumerate() {
                let row = Rect::from_min_size(
                    text_rect.min + egui::vec2(0.0, i as f32 * line_height),
                    egui::vec2(text_rect.width(), line_height),
                );
                draw_line(&painter, row, line, *color);
            }
            robot_rect
        } else {
            rect
        };
        draw(&painter, &visuals, robot_rect, &now, max_speed, max_spin);
        if now.measured.is_none() {
            card::draw_message(&painter, &visuals, rect, "no speeds");
        }
        let text = summary(&widget.title, &now);
        ui.interact(rect, ui.scope_id().with("chassis speeds"), Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text));
    }
}

impl WidgetType for ChassisSpeeds {
    fn names(&self) -> &'static [&'static str] {
        &["Chassis Speeds"]
    }

    /// Not a default: speeds dragged from the tree make a Struct; "Add as"
    /// in the tree's menu makes Chassis Speeds.
    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        motion::is_motion_type(nt_type)
    }

    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        [props::text(p, "topic", "").trim()]
            .into_iter()
            .chain(topic_in(p, SETPOINT_TOPIC))
            .chain(topic_in(p, HEADING_TOPIC))
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn takes_topics(&self) -> bool {
        true
    }

    /// Speeds dropped on the widget become its requested speeds; a
    /// rotation, pose or number becomes its heading.
    fn add_topics(&self, widget: &mut WidgetContainer, topics: &[DraggedTopic]) -> String {
        let own = widget.topic().unwrap_or("").to_owned();
        let mut said = Vec::new();
        for t in topics.iter().filter(|t| t.field.is_none() && t.name != own) {
            let (key, what) = if motion::is_motion_type(&t.ty) {
                (SETPOINT_TOPIC, "requested speeds")
            } else if angle::is_heading_type(&t.ty) {
                (HEADING_TOPIC, "heading")
            } else {
                continue;
            };
            props::set_property(&mut widget.properties, key, Some(t.name.clone().into()));
            said.push(format!("{what} from {}", t.name));
        }
        if said.is_empty() {
            "Chassis Speeds takes requested speeds, or a rotation or pose for its heading"
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
        let body = card::draw_card(
            ui.painter(),
            ui.visuals(),
            rect,
            &widget.title,
            &widget.properties,
        );
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

    /// The robot standing still, for previews while dragging.
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        let still = Shown {
            measured: None,
            requested: None,
            heading: None,
        };
        draw(
            painter,
            visuals,
            rect,
            &still,
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
        props::number_row(
            ui,
            p,
            "¾ turn (°/s)",
            MAX_SPIN,
            DEFAULT_MAX_SPIN,
            1.0..=3600.0,
        );
        for (label, key, fits, hover) in [
            (
                "Requested speeds",
                SETPOINT_TOPIC,
                motion::is_motion_type as fn(&str) -> bool,
                "The speeds asked for, drawn dashed against the measured ones",
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
    fn the_summary_reads_the_motion_requested_and_heading() {
        let shown = Shown {
            measured: Some(Motion {
                vx: 1.0,
                vy: -0.004,
                omega: Some(std::f64::consts::FRAC_PI_6),
            }),
            requested: Some(Motion {
                vx: 0.5,
                vy: 0.25,
                omega: None,
            }),
            heading: Some(-std::f64::consts::FRAC_PI_2),
        };
        assert_eq!(
            summary("Speeds", &shown),
            "Speeds: forward 1.00 m/s, left 0.00 m/s, spin 30°/s; \
             requested forward 0.50 m/s, left 0.25 m/s; heading -90°"
        );
        let none = Shown {
            measured: None,
            requested: None,
            heading: None,
        };
        assert_eq!(summary("S", &none), "S: no speeds");
    }

    #[test]
    fn the_card_line_is_short() {
        let m = Motion {
            vx: 1.5,
            vy: -0.5,
            omega: Some(0.5),
        };
        assert_eq!(short_text(&m), "1.50 fwd  -0.50 left m/s  29°/s");
        let v = Motion { omega: None, ..m };
        assert_eq!(short_text(&v), "1.50 fwd  -0.50 left m/s");
    }

    #[test]
    fn speeds_and_headings_dropped_on_it_are_taken() {
        let mut w = WidgetContainer {
            title: "Speeds".into(),
            rect: raptaboard_layout::Rect {
                x: 0.0,
                y: 0.0,
                width: 256.0,
                height: 256.0,
            },
            widget_type: "Chassis Speeds".into(),
            properties:
                serde_json::json!({"topic": "/Drive/Measured", "data_type": "struct:ChassisSpeeds"})
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
        ChassisSpeeds.add_topics(
            &mut w,
            &[
                dragged("/Drive/Commanded", "struct:ChassisSpeeds"),
                dragged("/Drive/Pose", "struct:Pose2d"),
                dragged("/Drive/name", "string"),
            ],
        );
        assert_eq!(w.properties[SETPOINT_TOPIC], "/Drive/Commanded");
        assert_eq!(w.properties[HEADING_TOPIC], "/Drive/Pose");
        assert_eq!(
            ChassisSpeeds.extra_topics(&w.properties, Server::default()),
            ["/Drive/Measured", "/Drive/Commanded", "/Drive/Pose"]
        );
        assert!(ChassisSpeeds.shows("struct:ChassisVelocities"));
        assert!(!ChassisSpeeds.accepts("struct:ChassisSpeeds"));
        assert!(ChassisSpeeds.shows("struct:Translation2d"));
    }
}
