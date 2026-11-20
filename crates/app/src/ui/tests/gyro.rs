//! The Gyro widget: a Rotation2d struct topic, a number of degrees either
//! way round, a WPILib `Gyro` table, and a Rotation2d dragged from the tree.

// Tests written by Claude.

use super::*;

/// A harness with one tab holding `gyro`.
fn gyro_harness(robot: &Robot, mut gyro: WidgetContainer) -> Harness {
    gyro.title = "Gyro".into();
    let mut tab = Tab::new("Gyro");
    tab.grid_layout.containers.push(gyro);
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

/// A `Rotation2d` topic at `/Drive/heading` holding `radians`.
fn heading_topic(robot: &Robot, radians: f64) -> nt4::GenericPublisher {
    let heading = robot._server.publish_generic(
        "/Drive/heading",
        "struct:Rotation2d".parse().unwrap(),
        nt4::Properties::new(),
    );
    heading
        .set(nt4::Value::Raw(radians.to_le_bytes().to_vec()))
        .unwrap();
    heading
}

#[test]
fn a_rotation2d_shows_in_degrees_from_0_to_360() {
    let robot = Robot::start();
    let heading = heading_topic(&robot, -std::f64::consts::FRAC_PI_2);
    let gyro = widget("Gyro", "/Drive/heading", "struct:Rotation2d", CARD);
    let mut harness = gyro_harness(&robot, gyro);
    labelled_soon(&mut harness, "Gyro: 270.00°");
    heading
        .set(nt4::Value::Raw(0.5f64.to_le_bytes().to_vec()))
        .unwrap();
    labelled_soon(&mut harness, "Gyro: 28.65°");
}

#[test]
fn a_number_and_a_gyro_table_show_their_degrees() {
    let robot = Robot::start();
    // /Test/number is 0.25.
    let number = widget("Gyro", "/Test/number", "double", CARD);
    let mut harness = gyro_harness(&robot, number);
    labelled_soon(&mut harness, "Gyro: 0.25°");
    // A Sendable gyro's table, as WPILib publishes one.
    let ty = robot
        ._server
        .publish::<String>("/SmartDashboard/Gyro/.type");
    ty.set("Gyro".into());
    let value = robot._server.publish::<f64>("/SmartDashboard/Gyro/Value");
    value.set(-450.0);
    let mut table = widget("Gyro", "/SmartDashboard/Gyro", "", CARD);
    table.properties.remove("data_type");
    let mut harness = gyro_harness(&robot, table);
    labelled_soon(&mut harness, "Gyro: 270.00°");
}

#[test]
fn a_rotation2d_drags_from_the_tree_as_a_gyro() {
    let robot = Robot::start();
    let _heading = heading_topic(&robot, 1.0);
    let mut harness = harness_for(&robot);
    open(&mut harness, "Drive  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    drag_row(
        &mut harness,
        "heading  Rotation2d",
        0,
        Pos2::new(1700.0, 1150.0),
    );
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Gyro");
    assert_eq!(made.topic(), Some("/Drive/heading"));
    assert_eq!(made.properties["data_type"], "struct:Rotation2d");
}
