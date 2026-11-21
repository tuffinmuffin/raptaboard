//! The Chassis Speeds widget: measured speeds with requested ones and a
//! heading, a Translation2d as a velocity, and speeds dragged from the
//! tree.

// Tests written by Claude.

use super::*;

/// A harness with one tab holding `speeds`, titled "Speeds".
fn speeds_harness(robot: &Robot, mut speeds: WidgetContainer) -> Harness {
    speeds.title = "Speeds".into();
    let mut tab = Tab::new("Speeds");
    tab.grid_layout.containers.push(speeds);
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

const CARD: LayoutRect = LayoutRect {
    x: 0.0,
    y: 0.0,
    width: 256.0,
    height: 256.0,
};

/// A `ChassisSpeeds` topic `name` holding `vx`, `vy` (m/s) and `omega`
/// (rad/s); the test server has its schema.
fn speeds_topic(robot: &Robot, name: &str, values: [f64; 3]) -> nt4::GenericPublisher {
    let p = robot._server.publish_generic(
        name,
        "struct:ChassisSpeeds".parse().unwrap(),
        nt4::Properties::new(),
    );
    p.set(nt4::Value::Raw(
        values.iter().flat_map(|v| v.to_le_bytes()).collect(),
    ))
    .unwrap();
    p
}

#[test]
fn speeds_show_with_the_requested_ones_and_the_heading() {
    let robot = Robot::start();
    // The test server's speeds: 1.5 m/s forward, 0.5 right, 0.5 rad/s.
    let _commanded = speeds_topic(&robot, "/Drive/commanded", [2.0, 0.0, 0.0]);
    let mut speeds = widget(
        "Chassis Speeds",
        "/Drive/speeds",
        "struct:ChassisSpeeds",
        CARD,
    );
    speeds
        .properties
        .insert("setpoint_topic".into(), json!("/Drive/commanded"));
    // The test server's pose faces 0.75 rad.
    speeds
        .properties
        .insert("heading_topic".into(), json!("/Drive/pose"));
    let mut harness = speeds_harness(&robot, speeds);
    labelled_soon(
        &mut harness,
        "Speeds: forward 1.50 m/s, left -0.50 m/s, spin 29°/s; \
         requested forward 2.00 m/s, left 0.00 m/s, spin 0°/s; heading 43°",
    );
}

#[test]
fn a_translation2d_shows_as_a_velocity_with_no_spin() {
    let robot = Robot::start();
    let velocity = robot._server.publish_generic(
        "/Drive/velocity",
        "struct:Translation2d".parse().unwrap(),
        nt4::Properties::new(),
    );
    velocity
        .set(nt4::Value::Raw(
            [0.25f64, 1.0]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        ))
        .unwrap();
    let speeds = widget(
        "Chassis Speeds",
        "/Drive/velocity",
        "struct:Translation2d",
        CARD,
    );
    let mut harness = speeds_harness(&robot, speeds);
    labelled_soon(&mut harness, "Speeds: forward 0.25 m/s, left 1.00 m/s");
}

#[test]
fn speeds_drag_as_a_struct_and_add_as_chassis_speeds() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open(&mut harness, "Drive  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    drag_row(
        &mut harness,
        "speeds  ChassisSpeeds",
        0,
        Pos2::new(1700.0, 1150.0),
    );
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    // A whole struct drags out as a Struct (decided with the user
    // 2026-11-21); "Add as" makes the Chassis Speeds.
    assert_eq!(made.widget_type, "Struct");
    add_as(&mut harness, "speeds  ChassisSpeeds", 0, "Chassis Speeds");
    assert_eq!(widgets(&harness).len(), before + 2);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Chassis Speeds");
    assert_eq!(made.topic(), Some("/Drive/speeds"));
}
