//! The Struct widget: any struct a team defines, with enum names and
//! nested structs, struct arrays, a struct with no schema, and a struct
//! dragged from the tree.

// Tests written by Claude.

use super::*;

/// A harness with one tab holding `view`, titled "Arm".
fn struct_harness(robot: &Robot, mut view: WidgetContainer) -> Harness {
    view.title = "Arm".into();
    let mut tab = Tab::new("Struct");
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
    settle(&mut harness);
    harness
}

const CARD: LayoutRect = LayoutRect {
    x: 0.0,
    y: 0.0,
    width: 384.0,
    height: 256.0,
};

/// A team's own struct, as robot code would define it: an enum, a number
/// and a nested Translation2d (the test server has its schema).
const ARM_SCHEMA: &[u8] = b"enum {kStow=0, kIntake=3} uint8 mode;double angle;Translation2d tip";

/// An `ArmState`'s bytes: `mode`, then `angle`, then the tip's x and y.
fn arm_bytes(mode: u8, angle: f64, tip: (f64, f64)) -> Vec<u8> {
    let mut bytes = vec![mode];
    for d in [angle, tip.0, tip.1] {
        bytes.extend(d.to_le_bytes());
    }
    bytes
}

/// Publish `ArmState`'s schema and a topic `name` of `ty` holding `bytes`.
fn arm_topic(robot: &Robot, name: &str, ty: &str, bytes: Vec<u8>) -> nt4::GenericPublisher {
    robot
        ._server
        .add_schema("struct:ArmState", "structschema", ARM_SCHEMA);
    let p = robot
        ._server
        .publish_generic(name, ty.parse().unwrap(), nt4::Properties::new());
    p.set(nt4::Value::Raw(bytes)).unwrap();
    p
}

#[test]
fn a_teams_struct_shows_its_fields_enum_names_and_nested_structs() {
    let robot = Robot::start();
    let arm = arm_topic(
        &robot,
        "/Arm/state",
        "struct:ArmState",
        arm_bytes(3, 1.5, (0.25, -0.5)),
    );
    let view = widget("Struct", "/Arm/state", "struct:ArmState", CARD);
    let mut harness = struct_harness(&robot, view);
    labelled_soon(
        &mut harness,
        "Arm: ArmState; mode = kIntake (3); angle = 1.5; tip.x = 0.25; tip.y = -0.5",
    );
    // A number the enum doesn't name shows as the number.
    arm.set(nt4::Value::Raw(arm_bytes(7, 0.0, (0.0, 0.0))))
        .unwrap();
    labelled_soon(
        &mut harness,
        "Arm: ArmState; mode = 7; angle = 0; tip.x = 0; tip.y = 0",
    );
}

#[test]
fn a_struct_array_shows_each_item() {
    let robot = Robot::start();
    let mut bytes = arm_bytes(0, 0.5, (1.0, 2.0));
    bytes.extend(arm_bytes(3, 1.0, (3.0, 4.0)));
    let _arms = arm_topic(&robot, "/Arm/states", "struct:ArmState[]", bytes);
    let view = widget("Struct", "/Arm/states", "struct:ArmState[]", CARD);
    let mut harness = struct_harness(&robot, view);
    labelled_soon(
        &mut harness,
        "Arm: ArmState[2]; [0].mode = kStow (0); [0].angle = 0.5; [0].tip.x = 1; [0].tip.y = 2; \
         [1].mode = kIntake (3); [1].angle = 1; [1].tip.x = 3; [1].tip.y = 4",
    );
}

#[test]
fn a_struct_with_no_schema_says_it_is_waiting() {
    let robot = Robot::start();
    // Robot code that never sent its schema.
    let lost = robot._server.publish_generic(
        "/Arm/lost",
        "struct:LostState".parse().unwrap(),
        nt4::Properties::new(),
    );
    lost.set(nt4::Value::Raw(vec![1, 2, 3])).unwrap();
    let view = widget("Struct", "/Arm/lost", "struct:LostState", CARD);
    let mut harness = struct_harness(&robot, view);
    labelled_soon(&mut harness, "Arm: waiting for the schema");
}

#[test]
fn any_struct_drags_from_the_tree_as_a_struct() {
    let robot = Robot::start();
    let _arm = arm_topic(
        &robot,
        "/Drive/arm",
        "struct:ArmState",
        arm_bytes(0, 0.0, (0.0, 0.0)),
    );
    let mut harness = harness_for(&robot);
    open(&mut harness, "Drive  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    drag_row(&mut harness, "arm  ArmState", 0, Pos2::new(1700.0, 1150.0));
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Struct");
    assert_eq!(made.properties["data_type"], "struct:ArmState");
}

/// Where the tree row for `path` (`/Drive/pose → translation`) is: its
/// hover text names it, first line exactly.
fn struct_row(harness: &mut Harness, path: &str) -> Pos2 {
    use egui_kittest::kittest::NodeT;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = harness
            .query_all_by_label_contains(path)
            .find(|n| {
                n.accesskit_node()
                    .label()
                    .is_some_and(|l| l.lines().next() == Some(path))
            })
            .map(|n| n.rect().center());
        if let Some(at) = found {
            return at;
        }
        assert!(Instant::now() < deadline, "no tree row for {path:?}");
        settle(harness);
    }
}

#[test]
fn a_nested_struct_drags_out_on_its_own() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    // The test server's pose is (1.5, 2.5), 0.75 rad: its translation.
    let from = struct_row(&mut harness, "/Drive/pose → translation");
    drag_from(&mut harness, from, Pos2::new(1700.0, 1150.0));
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Struct");
    assert_eq!(made.title, "pose.translation");
    assert_eq!(made.properties["data_type"], "struct:Translation2d");
    assert_eq!(made.struct_meta().unwrap().path, ["translation"]);
    labelled_soon(
        &mut harness,
        "pose.translation: Translation2d; x = 1.5; y = 2.5",
    );
    // An item of a struct array, too: the second swerve module.
    let from = struct_row(&mut harness, "/Drive/modules → [1]");
    drag_from(&mut harness, from, Pos2::new(1700.0, 900.0));
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.properties["data_type"], "struct:SwerveModuleState");
    labelled_soon(
        &mut harness,
        "modules[1]: SwerveModuleState; speed = 4; angle.value = 0.2",
    );
}

#[test]
fn an_item_of_a_pose_array_adds_as_a_3d_pose_showing_that_item() {
    let robot = Robot::start();
    let fleet = robot._server.publish_generic(
        "/Drive/fleet",
        "struct:Pose2d[]".parse().unwrap(),
        nt4::Properties::new(),
    );
    fleet
        .set(nt4::Value::Raw(
            [1.0f64, 2.0, 0.0, 3.0, 4.0, std::f64::consts::FRAC_PI_2]
                .iter()
                .flat_map(|d| d.to_le_bytes())
                .collect(),
        ))
        .unwrap();
    let mut harness = harness_for(&robot);
    open(&mut harness, "Drive  (");
    open(&mut harness, "fleet  ");
    let at = struct_row(&mut harness, "/Drive/fleet → [1]");
    add_as_at(&mut harness, at, "3D Pose");
    // The second pose, not the array's first (the widget reads the item,
    // not its topic's whole value).
    labelled_soon(
        &mut harness,
        "fleet[1]: x 3.00 m, y 4.00 m, z 0.00 m; roll 0°, pitch 0°, yaw 90°",
    );
}
