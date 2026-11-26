//! Gyro: which way the robot faces, on a compass dial. 0° is straight up;
//! a robot outline in the middle and a needle point the way it faces, with
//! the angle (0 to 360°) below.
//!
//! Bound to a `Rotation2d` struct topic (or a `Rotation3d`, its yaw), a
//! number of degrees, or a WPILib `Gyro` table (`.type` "Gyro", its angle
//! in `Value`), as Elastic reads it. Rotations count counterclockwise, as
//! WPILib defines them. Numbers count the way Elastic's
//! `counter_clockwise_positive` says: clockwise unless it's on, as most
//! gyros' `getAngle` counts. A `Rotation2d` dragged from the tree makes a
//! Gyro (a `Rotation3d` makes a 3D Pose; a Gyro can still show its yaw).

use std::f32::consts::PI;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::types::angle::{self, GYRO_TYPE, Heading};
use crate::types::structs;
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// Numbers count counterclockwise (Elastic; off: clockwise).
pub const COUNTER_CLOCKWISE_POSITIVE: &str = "counter_clockwise_positive";

const KEYS: [&str; 1] = [COUNTER_CLOCKWISE_POSITIVE];

/// Room left outside the dial for its labels, in points.
const LABEL_ROOM: f32 = 14.0;
/// Size of the labels, in points.
const LABEL_SIZE: f32 = 10.0;
/// Size of the angle's text, in points.
const VALUE_SIZE: f32 = 16.0;
/// The needle's color (Material blue 500, as the Radial Gauge's arc).
const NEEDLE: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);

/// The Gyro widget (Elastic's `"Gyro"`).
pub struct Gyro;

/// Whether `p`'s widget is bound to a `Gyro` table (no data type), not a
/// topic.
fn is_table(p: &Map<String, Json>) -> bool {
    props::text(p, props::DATA_TYPE, "").is_empty()
}

/// The heading `widget` shows now, from its value (or its table's `Value`).
fn heading(widget: &WidgetContainer, live: &Live<'_>) -> Option<Heading> {
    let p = &widget.properties;
    let ccw = props::flag(p, COUNTER_CLOCKWISE_POSITIVE, false);
    if is_table(p) {
        let topic = angle::gyro_value_topic(widget.topic()?);
        let value = live.frame.others.get(topic.as_str())?;
        return angle::heading_of(value, "double", None, ccw);
    }
    let ty = props::text(p, props::DATA_TYPE, "");
    angle::heading_of(live.value?, ty, live.frame.structs, ccw)
}

/// The point `angle` degrees clockwise from straight up, `radius` from
/// `center`, on screen (y down).
fn point(center: Pos2, angle: f64, radius: f32) -> Pos2 {
    let a = angle as f32 * PI / 180.0;
    center + egui::vec2(a.sin() * radius, -a.cos() * radius)
}

/// The angle's text: `90.00°`, wrapped to 0 up to 360 (never `360.00°` or
/// `-0.00°`).
fn angle_text(h: Heading) -> String {
    let text = format!("{:.2}", h.wrapped());
    match text.as_str() {
        "360.00" | "-0.00" => "0.00°".to_owned(),
        _ => format!("{text}°"),
    }
}

/// Draw the dial in `rect` for `heading` (`None`: just the dial, with
/// `why` in the middle).
fn draw_dial(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    heading: Option<Heading>,
    ccw: bool,
) {
    let center = rect.center();
    let radius = (rect.width().min(rect.height()) / 2.0 - LABEL_ROOM).max(6.0);
    let weak = visuals.weak_text_color();
    painter.circle(
        center,
        radius,
        visuals.extreme_bg_color,
        Stroke::new(2.0, visuals.widgets.inactive.bg_fill),
    );
    // Ticks every 15°, longer every 45°; labels every 45° when there's room
    // (every 90° when small), placed the way the angles count.
    let every = if radius > 60.0 { 45 } else { 90 };
    let font = egui::FontId::proportional(LABEL_SIZE);
    for n in (0..360).step_by(15) {
        let at = Heading {
            degrees: f64::from(n),
            ccw,
        }
        .clockwise();
        let long = n % 45 == 0;
        let inner = radius - if long { 9.0 } else { 5.0 };
        painter.line_segment(
            [point(center, at, radius), point(center, at, inner)],
            Stroke::new(if long { 1.5 } else { 1.0 }, weak),
        );
        if n % every == 0 {
            painter.text(
                point(center, at, radius + LABEL_ROOM / 2.0 + 1.0),
                egui::Align2::CENTER_CENTER,
                n.to_string(),
                font.clone(),
                weak,
            );
        }
    }
    let Some(h) = heading else {
        return;
    };
    let turn = h.clockwise();
    // The robot: a square facing the heading, with a nose.
    let half = radius * 0.22;
    let (sin, cos) = (turn as f32 * PI / 180.0).sin_cos();
    let at = |forward: f32, right: f32| {
        center + egui::vec2(sin * forward + cos * right, -cos * forward + sin * right)
    };
    let corners = vec![
        at(half, -half),
        at(half, half),
        at(-half, half),
        at(-half, -half),
    ];
    painter.add(egui::Shape::convex_polygon(
        corners,
        visuals.widgets.inactive.bg_fill,
        Stroke::new(1.5, visuals.text_color()),
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            at(half * 0.8, 0.0),
            at(0.0, half * 0.5),
            at(0.0, -half * 0.5),
        ],
        visuals.text_color(),
        Stroke::NONE,
    ));
    // The needle, from the robot's front to the dial.
    let tip = point(center, turn, radius - 10.0);
    painter.line_segment(
        [point(center, turn, half * 1.2), tip],
        Stroke::new(3.0, NEEDLE),
    );
    painter.circle_filled(tip, 4.0, NEEDLE);
    painter.text(
        center + egui::vec2(0.0, radius * 0.55),
        egui::Align2::CENTER_CENTER,
        angle_text(h),
        egui::FontId::monospace(VALUE_SIZE),
        visuals.text_color(),
    );
}

impl Gyro {
    /// Draw the dial in `rect` and name it for screen readers (and the
    /// tests) with the angle.
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let p = &widget.properties;
        let h = heading(widget, live);
        // A rotation always counts counterclockwise; a number as set.
        let ccw = h.map_or_else(
            || props::flag(p, COUNTER_CLOCKWISE_POSITIVE, false),
            |h| h.ccw,
        );
        draw_dial(&painter, &visuals, rect, h, ccw);
        let summary = match h {
            Some(h) => format!("{}: {}", widget.title, angle_text(h)),
            None => {
                let why = if live.value.is_some() {
                    "not an angle"
                } else {
                    "no value"
                };
                card::draw_message(&painter, &visuals, rect, why);
                format!("{}: {why}", widget.title)
            }
        };
        ui.interact(rect, ui.scope_id().with("gyro"), Sense::hover())
            .widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &summary)
            });
    }
}

impl WidgetType for Gyro {
    fn names(&self) -> &'static [&'static str] {
        &["Gyro"]
    }

    /// Not a default: a Rotation2d dragged from the tree makes a Struct;
    /// "Add as" in the tree's menu makes a Gyro. (A Gyro table still makes
    /// one.)
    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        // A Rotation3d shows its yaw.
        matches!(
            nt_type,
            "struct:Rotation2d" | "struct:Rotation3d" | "double" | "float" | "int"
        )
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[GYRO_TYPE]
    }

    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        match props::text(p, "topic", "") {
            topic if is_table(p) && !topic.is_empty() => vec![angle::gyro_value_topic(topic)],
            _ => Vec::new(),
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

    /// An empty dial, for previews while dragging.
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        p: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        let rotation = structs::struct_type(props::text(p, props::DATA_TYPE, "")).is_some();
        let ccw = rotation || props::flag(p, COUNTER_CLOCKWISE_POSITIVE, false);
        draw_dial(painter, visuals, rect, None, ccw);
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, value: ValueInfo<'_>) {
        if value.nt_type.is_some_and(|t| t.starts_with("struct:")) {
            ui.label("Counts");
            ui.weak("counterclockwise (a WPILib rotation)");
            ui.end_row();
            return;
        }
        props::flag_row(
            ui,
            p,
            "Counterclockwise",
            "Bigger angles turn counterclockwise (WPILib's way; most gyros count clockwise)",
            COUNTER_CLOCKWISE_POSITIVE,
            false,
        );
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
    fn the_angle_reads_from_0_to_360_and_never_minus_zero() {
        let h = |degrees, ccw| Heading { degrees, ccw };
        assert_eq!(angle_text(h(-90.0, true)), "270.00°");
        assert_eq!(angle_text(h(359.999, false)), "0.00°");
        assert_eq!(angle_text(h(-1e-9, true)), "0.00°");
        assert_eq!(angle_text(h(45.5, false)), "45.50°");
    }

    #[test]
    fn counterclockwise_turns_the_needle_left() {
        let c = egui::pos2(0.0, 0.0);
        let left = point(
            c,
            Heading {
                degrees: 90.0,
                ccw: true,
            }
            .clockwise(),
            10.0,
        );
        assert!(
            (left.x + 10.0).abs() < 1e-4 && left.y.abs() < 1e-4,
            "{left:?}"
        );
        let right = point(
            c,
            Heading {
                degrees: 90.0,
                ccw: false,
            }
            .clockwise(),
            10.0,
        );
        assert!((right.x - 10.0).abs() < 1e-4, "{right:?}");
    }

    #[test]
    fn a_gyro_table_needs_its_value_and_a_topic_nothing_more() {
        let table = serde_json::json!({"topic": "/SmartDashboard/Gyro"});
        assert_eq!(
            Gyro.extra_topics(table.as_object().unwrap(), Server::default()),
            ["/SmartDashboard/Gyro/Value"]
        );
        let topic =
            serde_json::json!({"topic": "/Drive/heading", "data_type": "struct:Rotation2d"});
        assert!(
            Gyro.extra_topics(topic.as_object().unwrap(), Server::default())
                .is_empty()
        );
        // It shows them but isn't their default (a Struct is, from "Add as").
        assert!(Gyro.shows("struct:Rotation2d") && !Gyro.accepts("struct:Rotation2d"));
        assert!(Gyro.shows("double"));
    }
}
