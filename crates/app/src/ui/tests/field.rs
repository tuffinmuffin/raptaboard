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
