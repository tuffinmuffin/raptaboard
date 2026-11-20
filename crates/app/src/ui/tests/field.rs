//! The Field widget: the robot's pose from a struct topic or a Field2d
//! table, in either origin, and a Pose2d dragged from the tree.

// Tests written by Claude.

use super::*;

/// A harness with one tab: a Field widget made from `widget`, filling
/// most of the grid.
fn field_harness(robot: &Robot, mut field: WidgetContainer) -> Harness {
    field.rect = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 1024.0,
        height: 512.0,
    };
    let mut tab = Tab::new("Field");
    tab.grid_layout.containers.push(field);
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![tab],
        ..Default::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    harness
}

/// A widget's place before [`field_harness`] sets it.
const ZERO_RECT: LayoutRect = LayoutRect {
    x: 0.0,
    y: 0.0,
    width: 256.0,
    height: 256.0,
};

fn pose_field() -> WidgetContainer {
    widget("Field", "/Drive/pose", "struct:Pose2d", ZERO_RECT)
}

#[test]
fn a_pose2d_topic_places_the_robot_in_the_2027_frame() {
    let robot = Robot::start();
    // The test server's pose: (1.5, 2.5), 0.75 rad.
    let mut harness = field_harness(&robot, pose_field());
    labelled_soon(
        &mut harness,
        "Field: robot at x 1.50 m, y 2.50 m, facing 43°",
    );
    // From the pre-2027 frame (origin at the blue corner), on the 2026
    // field (16.541 by 8.069 m): turned half round about the center.
    let mut old = pose_field();
    old.properties.insert(
        "origin".into(),
        json!(crate::types::field::Origin::BlueCorner.key()),
    );
    let mut harness = field_harness(&robot, old);
    labelled_soon(
        &mut harness,
        "Field: robot at x 6.77 m, y 1.53 m, facing -137°",
    );
}

#[test]
fn a_field2d_table_places_the_robot_from_its_robot_array() {
    let robot = Robot::start();
    let ty = robot._server.publish::<String>("/Sim/Field/.type");
    ty.set("Field2d".into());
    let pose = robot._server.publish::<Vec<f64>>("/Sim/Field/Robot");
    pose.set(vec![2.0, -1.0, 90.0]);
    let mut table = widget("Field", "/Sim/Field", "", ZERO_RECT);
    table.properties.remove("data_type");
    let mut harness = field_harness(&robot, table);
    labelled_soon(
        &mut harness,
        "Field: robot at x 2.00 m, y -1.00 m, facing 90°",
    );
    // It follows the robot.
    pose.set(vec![-3.0, 0.5, -45.0]);
    labelled_soon(
        &mut harness,
        "Field: robot at x -3.00 m, y 0.50 m, facing -45°",
    );
}

#[test]
fn a_pose2d_drags_from_the_tree_as_a_field_and_still_opens() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    // Clicking the struct's row still opens it into its fields.
    open_drive(&mut harness);
    row_at(&harness, "x", 0);
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    drag_row(&mut harness, "pose  Pose2d", 0, Pos2::new(1700.0, 1150.0));
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Field");
    assert_eq!(made.topic(), Some("/Drive/pose"));
    assert_eq!(made.properties["data_type"], "struct:Pose2d");
}

/// Bytes of a `Pose2d[]` for `poses` (x, y, radians each).
fn pose_array(poses: &[[f64; 3]]) -> nt4::Value {
    nt4::Value::Raw(
        poses
            .iter()
            .flatten()
            .flat_map(|d| d.to_le_bytes())
            .collect(),
    )
}

#[test]
fn layers_draw_other_robots_paths_and_the_tags_seen() {
    let robot = Robot::start();
    let fleet = robot._server.publish_generic(
        "/Sim/Fleet",
        "struct:Pose2d[]".parse().unwrap(),
        nt4::Properties::new(),
    );
    fleet
        .set(pose_array(&[[1.0, 1.0, 0.0], [-1.0, -1.0, 3.0]]))
        .unwrap();
    let path = robot._server.publish_generic(
        "/Sim/Trajectory",
        "struct:Pose2d[]".parse().unwrap(),
        nt4::Properties::new(),
    );
    path.set(pose_array(&[
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [4.0, 1.0, 0.0],
    ]))
    .unwrap();
    let tags = robot._server.publish::<Vec<i64>>("/Sim/seenTags");
    // 99 isn't on the 2026 field, so it isn't lit.
    tags.set(vec![7, 3, 99]);
    let mut field = pose_field();
    crate::widgets::builtin::field::layers::add_layers(
        &mut field.properties,
        &[
            ("/Sim/Fleet".into(), "struct:Pose2d[]".into()),
            ("/Sim/Trajectory".into(), "struct:Pose2d[]".into()),
            ("/Sim/seenTags".into(), "int[]".into()),
        ],
    );
    let mut harness = field_harness(&robot, field);
    labelled_soon(
        &mut harness,
        "Field: robot at x 1.50 m, y 2.50 m, facing 43°; 2 more robots; 1 path; tags seen 3, 7",
    );
    // The tags a camera sees change as it looks round.
    tags.set(vec![12]);
    labelled_soon(
        &mut harness,
        "Field: robot at x 1.50 m, y 2.50 m, facing 43°; 2 more robots; 1 path; tags seen 12",
    );
}

#[test]
fn a_field2d_tables_other_objects_are_robots_or_trajectories() {
    let robot = Robot::start();
    let ty = robot._server.publish::<String>("/Sim/Field/.type");
    ty.set("Field2d".into());
    let me = robot._server.publish::<Vec<f64>>("/Sim/Field/Robot");
    me.set(vec![0.0, 0.0, 0.0]);
    let ghost = robot._server.publish::<Vec<f64>>("/Sim/Field/Ghost");
    ghost.set(vec![1.0, 1.0, 0.0, 2.0, 2.0, 90.0]);
    // More than 8 poses: a trajectory.
    let traj = robot._server.publish::<Vec<f64>>("/Sim/Field/Auto");
    traj.set(
        (0..10)
            .flat_map(|i| [f64::from(i) * 0.5, 0.0, 0.0])
            .collect(),
    );
    let mut table = widget("Field", "/Sim/Field", "", ZERO_RECT);
    table.properties.remove("data_type");
    let mut harness = field_harness(&robot, table.clone());
    labelled_soon(
        &mut harness,
        "Field: robot at x 0.00 m, y 0.00 m, facing 0°; 2 more robots; 1 path",
    );
    // Elastic's switches turn them off.
    table
        .properties
        .insert("show_trajectories".into(), json!(false));
    table
        .properties
        .insert("show_other_objects".into(), json!(false));
    let mut harness = field_harness(&robot, table);
    labelled_soon(
        &mut harness,
        "Field: robot at x 0.00 m, y 0.00 m, facing 0°",
    );
}

#[test]
fn a_pose_array_dropped_on_a_field_becomes_a_layer() {
    let robot = Robot::start();
    let fleet = robot._server.publish_generic(
        "/Drive/fleet",
        "struct:Pose2d[]".parse().unwrap(),
        nt4::Properties::new(),
    );
    fleet
        .set(pose_array(&[[1.0, 1.0, 0.0], [-1.0, -1.0, 3.0]]))
        .unwrap();
    let mut harness = field_harness(&robot, pose_field());
    let on_field = harness.state().grid.area.min + egui::vec2(512.0, 256.0);
    open(&mut harness, "Drive  (");
    drag_row(&mut harness, "fleet  Pose2d[2]", 0, on_field);
    let field = harness.state().layout.tabs[0]
        .grid_layout
        .containers
        .clone();
    assert_eq!(field.len(), 1, "a layer, not a new widget");
    let layers = crate::widgets::builtin::field::layers::layers(&field[0].properties);
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].topic, "/Drive/fleet");
    assert!(harness.state().status.starts_with("Added a layer"));
    labelled_soon(
        &mut harness,
        "Field: robot at x 1.50 m, y 2.50 m, facing 43°; 2 more robots",
    );
}

#[test]
fn objects_draw_on_the_field_or_from_the_robot() {
    let robot = Robot::start();
    let points = |xy: &[[f64; 2]]| {
        nt4::Value::Raw(xy.iter().flatten().flat_map(|d| d.to_le_bytes()).collect())
    };
    let fuel = robot._server.publish_generic(
        "/Sim/fuel",
        "struct:Translation2d[]".parse().unwrap(),
        nt4::Properties::new(),
    );
    fuel.set(points(&[[0.0, 0.0], [3.0, -2.0]])).unwrap();
    let seen = robot._server.publish_generic(
        "/Sim/seenBalls",
        "struct:Translation2d[]".parse().unwrap(),
        nt4::Properties::new(),
    );
    seen.set(points(&[[1.5, 0.0]])).unwrap();
    let mut field = pose_field();
    use crate::widgets::builtin::field::layers::{self, Layer, LayerKind};
    let mut ahead = Layer::new(
        LayerKind::Objects,
        "/Sim/seenBalls",
        "struct:Translation2d[]",
    );
    ahead.robot_relative = true;
    layers::set_layers(
        &mut field.properties,
        &[
            Layer::new(LayerKind::Objects, "/Sim/fuel", "struct:Translation2d[]"),
            ahead,
        ],
    );
    let mut harness = field_harness(&robot, field);
    labelled_soon(
        &mut harness,
        "Field: robot at x 1.50 m, y 2.50 m, facing 43°; 3 objects",
    );
    // Relative to a robot with no pose: not drawn.
    let mut lost = pose_field();
    lost.properties
        .insert("topic".into(), json!("/Sim/nowhere"));
    let mut ahead = Layer::new(
        LayerKind::Objects,
        "/Sim/seenBalls",
        "struct:Translation2d[]",
    );
    ahead.robot_relative = true;
    layers::set_layers(&mut lost.properties, &[ahead]);
    let mut harness = field_harness(&robot, lost);
    labelled_soon(&mut harness, "Field: no robot pose");
}

#[test]
fn cameras_and_a_part_ride_on_the_robot() {
    let robot = Robot::start();
    let out = robot._server.publish::<bool>("/Sim/intakeOut");
    out.set(true);
    let mut field = pose_field();
    field.properties.insert(
        "attachments".into(),
        json!([
            {"kind": "camera", "name": "Side", "x": -0.15, "y": 0.3, "yaw": 90},
            {"kind": "camera", "name": "Back", "x": -0.23, "y": 0.19, "yaw": 180},
            {"kind": "part", "name": "Intake", "x": 0.34, "shown_by": "/Sim/intakeOut"}
        ]),
    );
    let mut harness = field_harness(&robot, field);
    labelled_soon(
        &mut harness,
        "Field: robot at x 1.50 m, y 2.50 m, facing 43°; 2 cameras; 1 part",
    );
    // Stowed: the intake isn't drawn, the cameras still are.
    out.set(false);
    labelled_soon(
        &mut harness,
        "Field: robot at x 1.50 m, y 2.50 m, facing 43°; 2 cameras",
    );
}
