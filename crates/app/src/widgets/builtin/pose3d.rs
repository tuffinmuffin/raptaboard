//! 3D Pose (raptaboard): which way something is turned in 3D, as X, Y and Z
//! axis arrows (red, green, blue) that turn with it, over faint fixed world
//! axes, with a box turning with them so the turn reads at a glance. The
//! numbers are below: where it is (for a pose) and its roll, pitch and yaw.
//!
//! Bound to a `Pose3d`, `Transform3d`, `Rotation3d` or `Quaternion` struct
//! topic (read by [`crate::types::pose3d`]), or a `Pose2d`, flat on the
//! floor. A Rotation3d, Transform3d or
//! Quaternion dragged from the tree makes one (a Pose3d makes a Field). Drag
//! the card's body to look from another side; double-click to look from
//! the usual corner again.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::types::pose3d::{self, Pose3, Quaternion};
use crate::widgets::shared::{card, props};
use crate::widgets::{Live, ValueInfo, WidgetType};

/// The X axis's color (red), as most 3D tools draw it.
const X_COLOR: Color32 = Color32::from_rgb(0xF4, 0x43, 0x36);
/// The Y axis's color (green).
const Y_COLOR: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50);
/// The Z axis's color (blue).
const Z_COLOR: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);
/// Room below the view for the numbers, points.
const TEXT_ROOM: f32 = 34.0;
/// Size of the numbers, points.
const TEXT_SIZE: f32 = 12.0;
/// The smallest body that shows the numbers, points: about a 2 by 2 card.
const MIN_TEXT_WIDTH: f32 = 200.0;
const MIN_TEXT_HEIGHT: f32 = 160.0;
/// The usual view: from 45° round from +X toward +Y, 25° above.
const DEFAULT_VIEW: View = View {
    azimuth: 45.0,
    elevation: 25.0,
};
/// How far a drag of one point turns the view, degrees.
const DEGREES_PER_POINT: f32 = 0.6;
/// The box's half-size along X, Y and Z, as parts of an axis's length: a
/// robot-ish slab.
const BOX: [f64; 3] = [0.55, 0.45, 0.18];

/// The 3D Pose widget.
pub struct Pose3dView;

/// Where the view looks from: `azimuth` degrees round from +X toward +Y,
/// `elevation` degrees above the floor. Kept per widget while the program
/// runs.
#[derive(Debug, Clone, Copy, PartialEq)]
struct View {
    azimuth: f32,
    elevation: f32,
}

impl View {
    /// The screen's right and up, and the way toward the viewer, as world
    /// directions.
    fn axes(self) -> ([f64; 3], [f64; 3], [f64; 3]) {
        let (sa, ca) = f64::from(self.azimuth).to_radians().sin_cos();
        let (se, ce) = f64::from(self.elevation).to_radians().sin_cos();
        let right = [-sa, ca, 0.0];
        let up = [-se * ca, -se * sa, ce];
        let toward = [ce * ca, ce * sa, se];
        (right, up, toward)
    }

    /// World point `p` on screen, `scale` points per unit round `center`,
    /// and how near the viewer it is (bigger is nearer).
    fn project(self, p: [f64; 3], center: Pos2, scale: f32) -> (Pos2, f64) {
        let (right, up, toward) = self.axes();
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let at = center + egui::vec2(dot(p, right) as f32, -dot(p, up) as f32) * scale;
        (at, dot(p, toward))
    }
}

/// The pose `widget` shows now, from its newest value.
fn pose(widget: &WidgetContainer, live: &Live<'_>) -> Option<Pose3> {
    let ty = props::text(&widget.properties, props::DATA_TYPE, "");
    let value = live.newest(widget)?;
    pose3d::pose3_of(value, ty, live.frame.structs)
}

/// `n` to `places` decimals, never `-0`.
fn rounded(n: f64, places: usize) -> String {
    let text = format!("{n:.places$}");
    match text.strip_prefix('-') {
        Some(rest) if rest.chars().all(|c| c == '0' || c == '.') => rest.to_owned(),
        _ => text,
    }
}

/// The turn as text: `roll 0°, pitch 5°, yaw 43°`.
fn turn_text(q: Quaternion) -> String {
    let (r, p, y) = q.roll_pitch_yaw();
    format!(
        "roll {}°, pitch {}°, yaw {}°",
        rounded(r.to_degrees(), 0),
        rounded(p.to_degrees(), 0),
        rounded(y.to_degrees(), 0)
    )
}

/// The place as text: `x 1.20 m, y 0.50 m, z 0.30 m`.
fn place_text(t: [f64; 3]) -> String {
    format!(
        "x {} m, y {} m, z {} m",
        rounded(t[0], 2),
        rounded(t[1], 2),
        rounded(t[2], 2)
    )
}

/// The widget's summary, for screen readers and tests.
fn summary(title: &str, pose: Option<&Pose3>) -> String {
    match pose {
        None => format!("{title}: no pose"),
        Some(p) => match p.translation {
            Some(t) => format!("{title}: {}; {}", place_text(t), turn_text(p.rotation)),
            None => format!("{title}: {}", turn_text(p.rotation)),
        },
    }
}

/// An arrow on screen from `from` to `to`.
fn arrow(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    painter.line_segment([from, to], stroke);
    let along = to - from;
    let length = along.length();
    if length < 1.0 {
        return;
    }
    let dir = along / length;
    let side = Vec2::new(-dir.y, dir.x);
    let head = (length * 0.2).clamp(4.0, 10.0);
    painter.add(egui::Shape::convex_polygon(
        vec![
            to,
            to - dir * head + side * head * 0.45,
            to - dir * head - side * head * 0.45,
        ],
        stroke.color,
        Stroke::NONE,
    ));
}

/// Draw the view in `rect`: world axes, the box and the turned axes, as
/// seen from `view`.
fn draw_view(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    rotation: Option<Quaternion>,
    view: View,
) {
    let center = rect.center();
    let scale = rect.width().min(rect.height()) * 0.36;
    let at = |p: [f64; 3]| view.project(p, center, scale);
    let faint = visuals.weak_text_color().gamma_multiply(0.5);
    // The world's axes, fixed, faint and dashed.
    for (end, name) in [
        ([1.0, 0.0, 0.0], "x"),
        ([0.0, 1.0, 0.0], "y"),
        ([0.0, 0.0, 1.0], "z"),
    ] {
        let tip = at(end.map(|c| c * 1.15)).0;
        painter.extend(egui::Shape::dashed_line(
            &[center, tip],
            Stroke::new(1.0, faint),
            4.0,
            3.0,
        ));
        painter.text(
            tip,
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(10.0),
            faint,
        );
    }
    let Some(q) = rotation else {
        return;
    };
    // The box, its edges turned with the pose.
    let corner = |i: usize| {
        let s = |bit: usize, half: f64| if i & bit == 0 { -half } else { half };
        q.rotate([s(1, BOX[0]), s(2, BOX[1]), s(4, BOX[2])])
    };
    let edge = Stroke::new(1.0, visuals.text_color().gamma_multiply(0.45));
    for i in 0..8usize {
        for bit in [1, 2, 4] {
            if i & bit == 0 {
                painter.line_segment([at(corner(i)).0, at(corner(i | bit)).0], edge);
            }
        }
    }
    // The turned axes, the farthest drawn first so the nearest is on top.
    let mut axes: Vec<(Pos2, f64, Color32, &str)> = [
        ([1.0, 0.0, 0.0], X_COLOR, "X"),
        ([0.0, 1.0, 0.0], Y_COLOR, "Y"),
        ([0.0, 0.0, 1.0], Z_COLOR, "Z"),
    ]
    .into_iter()
    .map(|(end, color, name)| {
        let (tip, depth) = at(q.rotate(end));
        (tip, depth, color, name)
    })
    .collect();
    axes.sort_by(|a, b| a.1.total_cmp(&b.1));
    for (tip, depth, color, name) in axes {
        // Pointing away from the viewer, a little dimmer.
        let color = if depth < 0.0 {
            color.gamma_multiply(0.6)
        } else {
            color
        };
        arrow(painter, center, tip, Stroke::new(3.0, color));
        let label = center + (tip - center) * 1.12;
        painter.text(
            label,
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(12.0),
            color,
        );
    }
    painter.circle_filled(center, 3.0, visuals.text_color());
}

impl Pose3dView {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let visuals = ui.visuals().clone();
        let now = pose(widget, live);
        // On a small card the numbers don't fit: the view takes it all (the
        // summary still has them).
        let words = rect.width() >= MIN_TEXT_WIDTH && rect.height() >= MIN_TEXT_HEIGHT;
        let (view_rect, text_rect) = if words {
            rect.split_top_bottom_at_y(rect.max.y - TEXT_ROOM)
        } else {
            (rect, Rect::NOTHING)
        };
        // Drag to look from another side; double-click for the usual view.
        let id = ui.scope_id().with("pose3d view");
        let response = ui.interact(view_rect, id, Sense::click_and_drag());
        let mut view: View = ui.data(|d| d.get_temp(id)).unwrap_or(DEFAULT_VIEW);
        // The view turns by how far the pointer is from where the press
        // began (egui calls it a drag only a few points on, and those
        // points count too), from the view when the drag began.
        let start_id = id.with("drag start");
        if response.drag_started() {
            ui.data_mut(|data| data.insert_temp(start_id, view));
        }
        if response.dragged()
            && let (Some(from), Some(now)) = (
                ui.input(|i| i.pointer.press_origin()),
                ui.input(|i| i.pointer.latest_pos()),
            )
        {
            let start: View = ui.data(|d| d.get_temp(start_id)).unwrap_or(view);
            let d = (now - from) * DEGREES_PER_POINT;
            view.azimuth = (start.azimuth - d.x).rem_euclid(360.0);
            view.elevation = (start.elevation + d.y).clamp(-89.0, 89.0);
            ui.data_mut(|data| data.insert_temp(id, view));
        }
        if response.double_clicked() {
            view = DEFAULT_VIEW;
            ui.data_mut(|data| data.remove::<View>(id));
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        draw_view(&painter, &visuals, view_rect, now.map(|p| p.rotation), view);
        match &now {
            Some(_) if !words => {}
            Some(p) => {
                let lines: Vec<String> = p
                    .translation
                    .map(place_text)
                    .into_iter()
                    .chain([turn_text(p.rotation)])
                    .collect();
                let line_height = TEXT_SIZE + 3.0;
                let top = text_rect.center().y - line_height * (lines.len() as f32 - 1.0) / 2.0;
                for (i, line) in lines.iter().enumerate() {
                    painter.text(
                        egui::pos2(text_rect.center().x, top + i as f32 * line_height),
                        egui::Align2::CENTER_CENTER,
                        line,
                        egui::FontId::monospace(TEXT_SIZE),
                        visuals.text_color(),
                    );
                }
            }
            None => card::draw_message(&painter, &visuals, rect, "no pose"),
        }
        let text = summary(&widget.title, now.as_ref());
        // The view as the value, for screen readers (and the tests).
        let seen = format!(
            "seen from {:.0}° round, {:.0}° up",
            view.azimuth, view.elevation
        );
        response.widget_info(|| {
            let mut info = egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text);
            info.current_text_value = Some(seen.clone());
            info
        });
    }
}

impl WidgetType for Pose3dView {
    fn names(&self) -> &'static [&'static str] {
        &["3D Pose"]
    }

    /// Not a default: a struct dragged from the tree makes a Struct; "Add
    /// as" in the tree's menu makes a 3D Pose.
    fn accepts(&self, _: &str) -> bool {
        false
    }

    /// A Pose2d too, flat on the floor.
    fn shows(&self, nt_type: &str) -> bool {
        pose3d::is_pose3_type(nt_type) || nt_type == "struct:Pose2d"
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

    /// The world axes alone, for previews while dragging.
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        draw_view(painter, visuals, rect, None, DEFAULT_VIEW);
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
    fn the_usual_view_puts_x_down_left_y_down_right_and_z_up() {
        let c = Pos2::ZERO;
        let (x, _) = DEFAULT_VIEW.project([1.0, 0.0, 0.0], c, 10.0);
        let (y, _) = DEFAULT_VIEW.project([0.0, 1.0, 0.0], c, 10.0);
        let (z, _) = DEFAULT_VIEW.project([0.0, 0.0, 1.0], c, 10.0);
        assert!(
            x.x < 0.0 && x.y > 0.0,
            "X comes toward the viewer, down left: {x:?}"
        );
        assert!(y.x > 0.0 && y.y > 0.0, "Y down right: {y:?}");
        assert!(z.x.abs() < 1e-4 && z.y < 0.0, "Z straight up: {z:?}");
        // From straight above (nearly), Z points at the viewer.
        let above = View {
            azimuth: 0.0,
            elevation: 89.0,
        };
        assert!(above.project([0.0, 0.0, 1.0], c, 10.0).1 > 0.99);
    }

    #[test]
    fn the_summary_reads_place_and_turn() {
        let p = Pose3 {
            translation: Some([1.2, -0.001, 0.3]),
            rotation: Quaternion::from_yaw(std::f64::consts::FRAC_PI_4),
        };
        assert_eq!(
            summary("Pose", Some(&p)),
            "Pose: x 1.20 m, y 0.00 m, z 0.30 m; roll 0°, pitch 0°, yaw 45°"
        );
        let turn = Pose3 {
            translation: None,
            ..p
        };
        assert_eq!(summary("R", Some(&turn)), "R: roll 0°, pitch 0°, yaw 45°");
        assert_eq!(summary("N", None), "N: no pose");
        assert!(Pose3dView.shows("struct:Rotation3d") && Pose3dView.shows("struct:Quaternion"));
        assert!(!Pose3dView.accepts("struct:Rotation3d"));
    }
}
