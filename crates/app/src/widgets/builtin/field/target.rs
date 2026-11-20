//! Click to set a target: with `click_target` on, a click on the field
//! publishes that point as a `Pose2d` target, for drivers to aim or send the
//! robot somewhere (robot code is free to ignore it). Dragging sets which
//! way it faces; right-click clears it.
//!
//! The target goes to `target_topic` (`/raptaboard/target` if not set), in the widget's `origin`, with a boolean `<topic>_set`
//! beside it: true when one is set, false once it's cleared (a pose can't
//! be "none"). The green X is drawn from the topic's value, so a target
//! robot code or another dashboard sets shows too.

use eframe::egui::{self, Color32, Sense, Stroke};
use nt4::{NtType, Value};
use serde_json::{Map, Value as Json};

use super::FieldView;
use crate::input_log;
use crate::types::field::{self, FieldPose, Origin};
use crate::widgets::shared::props;
use crate::widgets::{Effect, Live};

/// Turn on clicking to set a target (raptaboard).
pub const CLICK_TARGET: &str = "click_target";
/// The topic a target is published to (raptaboard); see [`target_topic`].
pub const TARGET_TOPIC: &str = "target_topic";

/// A `Pose2d` and the structs in it, as WPILib's schemas write them.
const POSE2D_SCHEMAS: &[(&str, &str)] = &[
    ("Pose2d", "Translation2d translation;Rotation2d rotation"),
    ("Translation2d", "double x;double y"),
    ("Rotation2d", "double value"),
];
/// A drag shorter than this (meters) sets no heading: it's a click.
const MIN_DRAG_M: f64 = 0.15;
/// The X's size on the field, meters, and its smallest on screen, points.
const MARK_M: f64 = 0.5;
const MIN_MARK_POINTS: f32 = 14.0;
/// The target's color.
const TARGET_COLOR: Color32 = Color32::from_rgb(0x00, 0xE6, 0x76);

/// Whether `p` turns clicking to set a target on.
pub fn enabled(p: &Map<String, Json>) -> bool {
    props::flag(p, CLICK_TARGET, false)
}

/// Where a Field's target goes when `target_topic` isn't set.
pub const DEFAULT_TARGET_TOPIC: &str = "/raptaboard/target";

/// The topic a Field with settings `p` publishes its target to.
///
/// ```
/// use raptaboard::widgets::builtin::field::target::target_topic;
/// assert_eq!(target_topic(&serde_json::Map::new()), "/raptaboard/target");
/// let set = serde_json::json!({"target_topic": "/Driver/aim"});
/// assert_eq!(target_topic(set.as_object().unwrap()), "/Driver/aim");
/// ```
pub fn target_topic(p: &Map<String, Json>) -> String {
    match props::text(p, TARGET_TOPIC, "").trim() {
        "" => DEFAULT_TARGET_TOPIC.to_owned(),
        topic => topic.to_owned(),
    }
}

/// The boolean beside `topic` that says whether a target is set.
pub fn set_topic(topic: &str) -> String {
    format!("{topic}_set")
}

/// The topics a Field with settings `p` needs for its target, if it takes
/// one.
pub fn topics(p: &Map<String, Json>) -> Vec<String> {
    if !enabled(p) {
        return Vec::new();
    }
    let topic = target_topic(p);
    vec![set_topic(&topic), topic]
}

/// A `Pose2d`'s bytes: x, y and heading (radians), little-endian doubles.
fn pose_bytes(pose: FieldPose) -> Vec<u8> {
    [pose.x, pose.y, pose.heading]
        .iter()
        .flat_map(|d| d.to_le_bytes())
        .collect()
}

/// The target set now, in the 2027 frame (`None` if there's none, or it's
/// been cleared).
pub fn current(
    p: &Map<String, Json>,
    live: &Live<'_>,
    origin: Origin,
    size: (f64, f64),
) -> Option<FieldPose> {
    let topic = target_topic(p);
    if live.frame.others.get(set_topic(&topic).as_str()) == Some(&&Value::Boolean(false)) {
        return None;
    }
    let value = live.frame.others.get(topic.as_str())?;
    let pose = field::pose_of(value, "struct:Pose2d", live.frame.structs)?;
    Some(origin.to_center(pose, size.0, size.1))
}

/// Ask for `pose` (2027 frame) to be published as the target, in `origin`.
fn publish(live: &Live<'_>, topic: &str, pose: FieldPose, origin: Origin, size: (f64, f64)) {
    // The pre-2027 frame's conversion is its own opposite.
    let sent = origin.to_center(pose, size.0, size.1);
    let mut effects = live.frame.effects.borrow_mut();
    effects.push(Effect::PublishStruct {
        topic: topic.to_owned(),
        struct_name: "Pose2d",
        bytes: pose_bytes(sent),
        schemas: POSE2D_SCHEMAS,
    });
    effects.push(Effect::Write {
        topic: set_topic(topic),
        value: Value::Boolean(true),
        ty: NtType::Boolean,
    });
}

/// Take clicks and drags on the field in `rect` (shown as `view`): a click
/// sets the target there, a drag sets it where it started facing where it
/// went, a right-click clears it. Returns the target being dragged, to draw
/// before it's sent.
pub fn input(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    view: &FieldView,
    p: &Map<String, Json>,
    live: &Live<'_>,
    origin: Origin,
) -> Option<FieldPose> {
    let topic = target_topic(p);
    let id = ui.scope_id().with("field target");
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let size = view.size;
    let start: Option<(f64, f64)> = ui.data(|d| d.get_temp(id));
    // Where the press began, not where the pointer is when egui decides
    // it's a drag (a few points on), so a drag doesn't shift the target.
    if response.drag_started()
        && let Some(at) = ui
            .input(|i| i.pointer.press_origin())
            .or(response.interact_pointer_pos())
    {
        let field_at = view.to_field(at);
        ui.data_mut(|d| d.insert_temp(id, field_at));
        let now = ui.input(|i| (i.time, i.pointer.latest_pos()));
        input_log::log(now.0, || {
            format!(
                "field target: drag began; press at ({:.0}, {:.0}) = field ({:.2}, {:.2}); pointer now {:?}",
                at.x, at.y, field_at.0, field_at.1, now.1
            )
        });
    }
    let aimed = |end: egui::Pos2| -> Option<FieldPose> {
        let (x, y) = start?;
        let (ex, ey) = view.to_field(end);
        let heading = if (ex - x).hypot(ey - y) >= MIN_DRAG_M {
            (ey - y).atan2(ex - x)
        } else {
            0.0
        };
        Some(FieldPose { x, y, heading })
    };
    if response.drag_stopped() {
        ui.data_mut(|d| d.remove::<(f64, f64)>(id));
        let end = response.interact_pointer_pos();
        if let Some(pose) = end.and_then(aimed) {
            input_log::log(ui.input(|i| i.time), || {
                format!(
                    "field target: drag ended at {end:?}; target field ({:.2}, {:.2}) facing {:.0} deg",
                    pose.x,
                    pose.y,
                    pose.heading.to_degrees()
                )
            });
            publish(live, &topic, pose, origin, size);
        }
        return None;
    }
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        let (x, y) = view.to_field(at);
        input_log::log(ui.input(|i| i.time), || {
            format!(
                "field target: click at ({:.0}, {:.0}) = field ({x:.2}, {y:.2})",
                at.x, at.y
            )
        });
        publish(live, &topic, FieldPose { x, y, heading: 0.0 }, origin, size);
    }
    if response.secondary_clicked() {
        input_log::log(ui.input(|i| i.time), || {
            "field target: right-click, cleared".to_owned()
        });
        live.frame.effects.borrow_mut().push(Effect::Write {
            topic: set_topic(&topic),
            value: Value::Boolean(false),
            ty: NtType::Boolean,
        });
    }
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        return response.interact_pointer_pos().and_then(aimed);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    None
}

/// Draw a target at `pose` (2027 frame): a green X and a line the way it
/// faces. `faint` for one being dragged, not sent yet.
pub fn draw(painter: &egui::Painter, view: &FieldView, pose: FieldPose, faint: bool) {
    let meter = view.rect.width().max(view.rect.height()) / view.size.0.max(view.size.1) as f32;
    let r = (MARK_M as f32 * meter).max(MIN_MARK_POINTS) / 2.0;
    let color = if faint {
        TARGET_COLOR.gamma_multiply(0.5)
    } else {
        TARGET_COLOR
    };
    let c = view.to_screen(pose.x, pose.y);
    let shadow = Stroke::new(5.0, Color32::from_black_alpha(160));
    let stroke = Stroke::new(3.0, color);
    for s in [shadow, stroke] {
        painter.line_segment([c + egui::vec2(-r, -r), c + egui::vec2(r, r)], s);
        painter.line_segment([c + egui::vec2(-r, r), c + egui::vec2(r, -r)], s);
    }
    let reach = MARK_M * 1.4;
    let tip = view.to_screen(
        pose.x + reach * pose.heading.cos(),
        pose.y + reach * pose.heading.sin(),
    );
    painter.line_segment([c, tip], Stroke::new(2.0, color));
    painter.circle_filled(tip, 3.0, color);
}

/// The settings dialog rows: turn it on, and the topic.
pub fn settings_rows(ui: &mut egui::Ui, p: &mut Map<String, Json>) {
    props::flag_row(
        ui,
        p,
        "Click to set a target",
        "Clicking the field sends a target",
        CLICK_TARGET,
        false,
    );
    if !enabled(p) {
        return;
    }
    ui.label("Target topic");
    let mut text = props::text(p, TARGET_TOPIC, "").to_owned();
    if ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(DEFAULT_TARGET_TOPIC)
                .desired_width(260.0),
        )
        .on_hover_text(
            "Where the target goes, as a Pose2d (in the poses' origin), with a boolean \
             <topic>_set beside it. A click sets it, a drag also says which way it faces, a \
             right-click clears it. Robot code reads it, or ignores it.",
        )
        .changed()
    {
        let text = text.trim().to_owned();
        props::set_property(p, TARGET_TOPIC, (!text.is_empty()).then(|| text.into()));
    }
    ui.end_row();
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn a_target_is_a_pose2ds_bytes_and_needs_its_topics_only_when_on() {
        let bytes = pose_bytes(FieldPose {
            x: 1.0,
            y: -2.0,
            heading: 0.5,
        });
        assert_eq!(bytes.len(), 24);
        assert_eq!(f64::from_le_bytes(bytes[8..16].try_into().unwrap()), -2.0);
        let off = serde_json::json!({});
        assert!(topics(off.as_object().unwrap()).is_empty());
        let on = serde_json::json!({"click_target": true});
        assert_eq!(
            topics(on.as_object().unwrap()),
            ["/raptaboard/target_set", "/raptaboard/target"]
        );
    }
}
