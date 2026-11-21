//! The Swerve Drive widget: module states from a struct array with
//! requested states and a heading, Elastic's `SwerveDrive` table, and a
//! states topic dragged from the tree.

// Tests written by Claude.

use super::*;

/// A harness with one tab holding `swerve`, titled "Swerve".
fn swerve_harness(robot: &Robot, mut swerve: WidgetContainer) -> Harness {
    swerve.title = "Swerve".into();
    let mut tab = Tab::new("Swerve");
    tab.grid_layout.containers.push(swerve);
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

#[test]
fn module_states_show_with_the_requested_ones_and_the_heading() {
    let robot = Robot::start();
    let requested = robot._server.publish_generic(
        "/Drive/requested",
        "struct:SwerveModuleState[]".parse().unwrap(),
        nt4::Properties::new(),
    );
    let doubles = |values: &[f64]| -> nt4::Value {
        nt4::Value::Raw(values.iter().flat_map(|v| v.to_le_bytes()).collect())
    };
    requested.set(doubles(&[3.5, 0.0, -1.0, 0.2])).unwrap();
    // The test server's modules: 3 m/s at 0.1 rad and 4 m/s at 0.2 rad;
    // its pose faces 0.75 rad.
    let mut swerve = widget(
        "SwerveDrive",
        "/Drive/modules",
        "struct:SwerveModuleState[]",
        CARD,
    );
    swerve
        .properties
        .insert("setpoint_topic".into(), json!("/Drive/requested"));
    swerve
        .properties
        .insert("heading_topic".into(), json!("/Drive/pose"));
    let mut harness = swerve_harness(&robot, swerve);
    labelled_soon(
        &mut harness,
        "Swerve: FL 3.00 m/s at 6°, FR 4.00 m/s at 11°; \
         requested FL 3.50 m/s at 0°, FR -1.00 m/s at 11°; heading 43°",
    );
}

#[test]
fn elastics_swerve_table_shows_in_its_unit() {
    let robot = Robot::start();
    let ty = robot
        ._server
        .publish::<String>("/SmartDashboard/Swerve/.type");
    ty.set("SwerveDrive".into());
    let number = |name: &str, v: f64| {
        let p = robot
            ._server
            .publish::<f64>(&format!("/SmartDashboard/Swerve/{name}"));
        p.set(v);
        p
    };
    let _values = [
        number("Front Left Angle", 45.0),
        number("Front Left Velocity", 1.0),
        number("Front Right Angle", -45.0),
        number("Front Right Velocity", 2.0),
        number("Back Left Angle", 135.0),
        number("Back Left Velocity", 0.0),
        number("Back Right Angle", 180.0),
        number("Back Right Velocity", -1.5),
        number("Robot Angle", 90.0),
    ];
    let mut table = widget("SwerveDrive", "/SmartDashboard/Swerve", "", CARD);
    table.properties.remove("data_type");
    // Elastic reads a file without `rotation_unit` as degrees.
    let mut harness = swerve_harness(&robot, table.clone());
    labelled_soon(
        &mut harness,
        "Swerve: FL 1.00 m/s at 45°, FR 2.00 m/s at -45°, BL 0.00 m/s at 135°, \
         BR -1.50 m/s at 180°; heading 90°",
    );
    // Turning with the heading off: no heading.
    table
        .properties
        .insert("show_robot_rotation".into(), json!(false));
    let mut harness = swerve_harness(&robot, table);
    labelled_soon(
        &mut harness,
        "Swerve: FL 1.00 m/s at 45°, FR 2.00 m/s at -45°, BL 0.00 m/s at 135°, \
         BR -1.50 m/s at 180°",
    );
}

#[test]
fn module_states_drag_from_the_tree_as_a_swerve_view() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open(&mut harness, "Drive  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    drag_row(
        &mut harness,
        "modules  SwerveModuleState[2]",
        0,
        Pos2::new(1700.0, 1150.0),
    );
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "SwerveDrive");
    assert_eq!(made.topic(), Some("/Drive/modules"));
    assert_eq!(made.properties["data_type"], "struct:SwerveModuleState[]");
}
