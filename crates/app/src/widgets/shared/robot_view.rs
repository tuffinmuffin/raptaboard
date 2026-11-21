//! A robot seen from above, for widgets that draw how it moves (the swerve
//! view, Chassis Speeds): where it sits in a rect, which way it's turned,
//! its frame with the front picked out, and arrows (straight for speed,
//! curved for spin). Measured values draw in [`MEASURED`], solid;
//! requested ones in [`REQUESTED`], dashed.

use std::f32::consts::PI;

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

/// Measured values' color (Material blue 500).
pub const MEASURED: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);
/// Requested values' color (Material orange 500).
pub const REQUESTED: Color32 = Color32::from_rgb(0xFF, 0x98, 0x00);

/// Where the robot is drawn: its center, its size, and which way it's
/// turned. The front is up when it isn't turned.
#[derive(Debug, Clone, Copy)]
pub struct RobotView {
    /// The robot's center on screen.
    pub center: Pos2,
    /// Half the robot's side, points.
    pub half: f32,
    /// The robot's turn, radians counterclockwise.
    pub heading: f32,
}

impl RobotView {
    /// The robot in the middle of `rect`, its side `fraction` of the
    /// rect's smaller side, turned `heading` radians counterclockwise (none:
    /// front up).
    pub fn fit(rect: Rect, fraction: f32, heading: Option<f64>) -> RobotView {
        RobotView {
            center: rect.center(),
            half: rect.width().min(rect.height()) * fraction / 2.0,
            heading: heading.unwrap_or(0.0) as f32,
        }
    }

    /// The on-screen direction of `angle` (radians counterclockwise from
    /// the robot's front), as a unit vector.
    ///
    /// ```
    /// use eframe::egui::{pos2, Rect};
    /// use raptaboard::widgets::shared::robot_view::RobotView;
    /// let r = RobotView::fit(Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 10.0)), 0.5, None);
    /// let left = r.dir(std::f32::consts::FRAC_PI_2);
    /// assert!((left.x + 1.0).abs() < 1e-6, "90° is the robot's left");
    /// ```
    pub fn dir(&self, angle: f32) -> Vec2 {
        let a = angle + self.heading;
        egui::vec2(-a.sin(), -a.cos())
    }

    /// The point `forward` and `left` points from the robot's center.
    pub fn at(&self, forward: f32, left: f32) -> Pos2 {
        self.center + self.dir(0.0) * forward + self.dir(PI / 2.0) * left
    }

    /// Corner `i` of the robot (front left, front right, back left, back
    /// right: where swerve modules sit).
    pub fn corner(&self, i: usize) -> Pos2 {
        let forward = if i < 2 { self.half } else { -self.half };
        let left = if i.is_multiple_of(2) {
            self.half
        } else {
            -self.half
        };
        self.at(forward, left)
    }

    /// Draw the robot's frame: its outline with the front edge picked out,
    /// and an arrowhead in the middle pointing to the front.
    pub fn draw_frame(&self, painter: &egui::Painter, visuals: &egui::Visuals) {
        let h = self.half;
        let corners = [
            self.at(h, h),
            self.at(h, -h),
            self.at(-h, -h),
            self.at(-h, h),
        ];
        let front = visuals.weak_text_color();
        painter.add(egui::Shape::closed_line(
            corners.to_vec(),
            Stroke::new(2.0, visuals.widgets.inactive.bg_fill),
        ));
        painter.line_segment([corners[0], corners[1]], Stroke::new(4.0, front));
        let nose = h * 0.35;
        painter.add(egui::Shape::convex_polygon(
            vec![
                self.at(nose, 0.0),
                self.at(-nose * 0.6, nose * 0.4),
                self.at(-nose * 0.6, -nose * 0.4),
            ],
            front,
            Stroke::NONE,
        ));
    }
}

/// Lines for an arrowhead at `tip` pointing along `dir` (a unit vector),
/// `size` points long.
fn head(painter: &egui::Painter, tip: Pos2, dir: Vec2, size: f32, stroke: Stroke) {
    let side = egui::vec2(-dir.y, dir.x);
    for s in [1.0, -1.0] {
        painter.line_segment([tip, tip - dir * size + side * s * size * 0.6], stroke);
    }
}

/// An arrow from `from` along `dir` (a unit vector) `length` points long,
/// dashed for a requested value.
pub fn arrow(
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
    head(painter, tip, dir, (length * 0.3).clamp(5.0, 10.0), stroke);
}

/// A curved arrow round `robot` at `radius` points from its center, for
/// spin: from the robot's front, `sweep` radians round (counterclockwise
/// when positive), dashed for a requested value.
pub fn spin_arrow(
    painter: &egui::Painter,
    robot: &RobotView,
    radius: f32,
    sweep: f32,
    stroke: Stroke,
    dashed: bool,
) {
    let steps = ((sweep.abs() / 0.08).ceil() as usize).max(2);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| robot.center + robot.dir(sweep * i as f32 / steps as f32) * radius)
        .collect();
    if dashed {
        painter.extend(egui::Shape::dashed_line(&points, stroke, 5.0, 3.0));
    } else {
        painter.add(egui::Shape::line(points.clone(), stroke));
    }
    // The head points along the circle, the way it turns.
    let along = robot.dir(sweep + sweep.signum() * PI / 2.0);
    head(painter, points[steps], along, 8.0, stroke);
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn corners_sit_where_their_names_say_and_turn_with_the_robot() {
        let rect = Rect::from_center_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
        let upright = RobotView::fit(rect, 0.4, None);
        let fl = upright.corner(0);
        assert!(
            fl.x < 0.0 && fl.y < 0.0,
            "front left is up and left: {fl:?}"
        );
        let br = upright.corner(3);
        assert!(
            br.x > 0.0 && br.y > 0.0,
            "back right is down and right: {br:?}"
        );
        // Turned 90° counterclockwise, the front points left, so front left
        // is down and left.
        let turned = RobotView::fit(rect, 0.4, Some(std::f64::consts::FRAC_PI_2));
        let fl = turned.corner(0);
        assert!(fl.x < 0.0 && fl.y > 0.0, "{fl:?}");
    }
}
