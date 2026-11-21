//! The 3D Pose widget: a Pose3d's place and turn, a Pose2d flat, dragging
//! the view round and double-clicking it back, and a Rotation3d dragged
//! from the tree.

// Tests written by Claude.

use super::*;

/// A harness with one tab holding `view`, titled "Pose", locked so the
/// whole card takes input.
fn pose_harness(robot: &Robot, mut view: WidgetContainer) -> Harness {
    view.title = "Pose".into();
    let mut tab = Tab::new("Pose");
    tab.grid_layout.containers.push(view);
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![tab],
        ..Default::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    app.locked = true;
    settle(&mut harness);
    harness
}

const CARD: LayoutRect = LayoutRect {
    x: 0.0,
    y: 0.0,
    width: 384.0,
    height: 384.0,
};

/// Publish the 3D schemas and a struct topic `name` of `ty` holding
/// `values` (doubles).
fn struct_topic(robot: &Robot, name: &str, ty: &str, values: &[f64]) -> nt4::GenericPublisher {
    let s = &robot._server;
    s.add_schema(
        "struct:Translation3d",
        "structschema",
        b"double x;double y;double z",
    );
    s.add_schema(
        "struct:Quaternion",
        "structschema",
        b"double w;double x;double y;double z",
    );
    s.add_schema("struct:Rotation3d", "structschema", b"Quaternion q");
    s.add_schema(
        "struct:Pose3d",
        "structschema",
        b"Translation3d translation;Rotation3d rotation",
    );
    let p = s.publish_generic(name, ty.parse().unwrap(), nt4::Properties::new());
    p.set(nt4::Value::Raw(
        values.iter().flat_map(|v| v.to_le_bytes()).collect(),
    ))
    .unwrap();
    p
}

/// A quaternion `[w, x, y, z]` for a turn of `degrees` about `axis`.
fn turn(axis: [f64; 3], degrees: f64) -> [f64; 4] {
    let (s, c) = (degrees.to_radians() / 2.0).sin_cos();
    [c, axis[0] * s, axis[1] * s, axis[2] * s]
}

#[test]
fn a_pose3d_shows_its_place_and_turn_and_follows_changes() {
    let robot = Robot::start();
    let q = turn([0.0, 0.0, 1.0], 90.0);
    let pose = struct_topic(
        &robot,
        "/Arm/pose",
        "struct:Pose3d",
        &[1.0, 2.0, 0.5, q[0], q[1], q[2], q[3]],
    );
    let view = widget("3D Pose", "/Arm/pose", "struct:Pose3d", CARD);
    let mut harness = pose_harness(&robot, view);
    labelled_soon(
        &mut harness,
        "Pose: x 1.00 m, y 2.00 m, z 0.50 m; roll 0°, pitch 0°, yaw 90°",
    );
    let q = turn([1.0, 0.0, 0.0], -30.0);
    pose.set(nt4::Value::Raw(
        [0.0, 0.0, 1.0, q[0], q[1], q[2], q[3]]
            .iter()
            .flat_map(|v: &f64| v.to_le_bytes())
            .collect(),
    ))
    .unwrap();
    labelled_soon(
        &mut harness,
        "Pose: x 0.00 m, y 0.00 m, z 1.00 m; roll -30°, pitch 0°, yaw 0°",
    );
}

#[test]
fn a_pose2d_shows_flat() {
    let robot = Robot::start();
    // The test server's pose: (1.5, 2.5), 0.75 rad.
    let view = widget("3D Pose", "/Drive/pose", "struct:Pose2d", CARD);
    let mut harness = pose_harness(&robot, view);
    labelled_soon(
        &mut harness,
        "Pose: x 1.50 m, y 2.50 m, z 0.00 m; roll 0°, pitch 0°, yaw 43°",
    );
}

/// The view the widget says it's seen from.
fn seen(harness: &Harness) -> String {
    harness
        .query_by_label_contains("Pose: ")
        .and_then(|n| n.value())
        .unwrap_or_default()
}

#[test]
fn dragging_turns_the_view_and_a_double_click_puts_it_back() {
    let robot = Robot::start();
    let q = turn([0.0, 0.0, 1.0], 0.0);
    let _r = struct_topic(&robot, "/Arm/turn", "struct:Rotation3d", &q);
    let view = widget("3D Pose", "/Arm/turn", "struct:Rotation3d", CARD);
    let mut harness = pose_harness(&robot, view);
    labelled_soon(&mut harness, "Pose: roll 0°, pitch 0°, yaw 0°");
    assert_eq!(seen(&harness), "seen from 45° round, 25° up");
    // Drag 50 points left and 25 down: 30° round, 15° higher.
    let middle = card_on_screen(&harness, 0).center();
    harness.hover_at(middle);
    harness.step();
    harness.drag_at(middle);
    harness.step();
    let to = middle + egui::vec2(-50.0, 25.0);
    for step in 1..=10 {
        harness.hover_at(middle + (to - middle) * (step as f32 / 10.0));
        harness.step();
    }
    harness.drop_at(to);
    settle(&mut harness);
    assert_eq!(seen(&harness), "seen from 75° round, 40° up");
    // A double-click looks from the usual corner again.
    for _ in 0..2 {
        click_at(
            &mut harness,
            middle,
            egui::PointerButton::Primary,
            Modifiers::NONE,
        );
    }
    assert_eq!(seen(&harness), "seen from 45° round, 25° up");
}

#[test]
fn a_rotation3d_drags_from_the_tree_as_a_3d_pose() {
    let robot = Robot::start();
    let _r = struct_topic(
        &robot,
        "/Drive/tilt",
        "struct:Rotation3d",
        &turn([0.0, 1.0, 0.0], 10.0),
    );
    let mut harness = harness_for(&robot);
    open(&mut harness, "Drive  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    drag_row(
        &mut harness,
        "tilt  Rotation3d",
        0,
        Pos2::new(1700.0, 1150.0),
    );
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "3D Pose");
    assert_eq!(made.properties["data_type"], "struct:Rotation3d");
}
