//! The Mechanism2d widget, against mechanisms published the way WPILib
//! publishes them: any table, roots and chain of ligaments, built with
//! [`Mechanism`]. The Lift (an elevator with an arm on it) is one; others
//! have several roots, deeper chains, and ligaments with no color or
//! weight. Tests: following changes and ligaments added later, the table
//! dragged from the tree, the view's zoom, and placing a deep chain.

// Tests written by Claude.

use std::collections::HashMap;

use super::*;

/// One ligament to publish: its path under the table (`root/a/b`), and
/// what robot code set (color and weight left out aren't sent).
struct Ligament<'a> {
    path: &'a str,
    angle: f64,
    length: f64,
    color: Option<&'a str>,
    weight: Option<f64>,
}

/// A WPILib Mechanism2d's topics on the test server, kept alive by this:
/// `.type`, `dims`, an optional `backgroundColor`, each root's `x` and `y`,
/// and each ligament's `.type`, `angle`, `length`, `color` and `weight`.
struct Mechanism<'r> {
    robot: &'r Robot,
    table: String,
    _dims: Publisher<Vec<f64>>,
    strings: Vec<Publisher<String>>,
    numbers: HashMap<String, Publisher<f64>>,
}

impl<'r> Mechanism<'r> {
    /// Publish a mechanism at `table`: its canvas `dims`, `background`,
    /// `roots` (name, x, y) and `ligaments`.
    fn publish(
        robot: &'r Robot,
        table: &str,
        dims: (f64, f64),
        background: Option<&str>,
        roots: &[(&str, f64, f64)],
        ligaments: &[Ligament<'_>],
    ) -> Mechanism<'r> {
        let dims_pub = robot._server.publish::<Vec<f64>>(&format!("{table}/dims"));
        dims_pub.set(vec![dims.0, dims.1]);
        let mut m = Mechanism {
            robot,
            table: table.to_owned(),
            _dims: dims_pub,
            strings: Vec::new(),
            numbers: HashMap::new(),
        };
        m.text(".type", "Mechanism2d");
        if let Some(color) = background {
            m.text("backgroundColor", color);
        }
        for (name, x, y) in roots {
            m.set(&format!("{name}/x"), *x);
            m.set(&format!("{name}/y"), *y);
        }
        for l in ligaments {
            m.add(l);
        }
        m
    }

    fn text(&mut self, name: &str, value: &str) {
        let p = self
            .robot
            ._server
            .publish::<String>(&format!("{}/{name}", self.table));
        p.set(value.into());
        self.strings.push(p);
    }

    /// Set the number at `name` (inside the table), publishing it if it's
    /// new.
    fn set(&mut self, name: &str, v: f64) {
        let (robot, table) = (self.robot, &self.table);
        self.numbers
            .entry(name.to_owned())
            .or_insert_with(|| robot._server.publish::<f64>(&format!("{table}/{name}")))
            .set(v);
    }

    /// Add a ligament, as robot code appending one does.
    fn add(&mut self, l: &Ligament<'_>) {
        self.text(&format!("{}/.type", l.path), "line");
        self.set(&format!("{}/angle", l.path), l.angle);
        self.set(&format!("{}/length", l.path), l.length);
        if let Some(color) = l.color {
            self.text(&format!("{}/color", l.path), color);
        }
        if let Some(weight) = l.weight {
            self.set(&format!("{}/weight", l.path), weight);
        }
    }
}

/// The Lift at `/Test/Lift`, as RobotPy's `Mechanism2d(3, 2)` with an
/// elevator and an arm on it publishes it.
fn lift(robot: &Robot) -> Mechanism<'_> {
    Mechanism::publish(
        robot,
        "/Test/Lift",
        (3.0, 2.0),
        Some("#000020"),
        &[("base", 1.5, 0.2)],
        &[
            Ligament {
                path: "base/elevator",
                angle: 90.0,
                length: 0.8,
                color: Some("#FF0000"),
                weight: Some(6.0),
            },
            Ligament {
                path: "base/elevator/arm",
                angle: -45.0,
                length: 0.5,
                color: Some("#00FF00"),
                weight: Some(4.0),
            },
        ],
    )
}

/// A harness with one tab holding a Mechanism2d on `table`, titled
/// `title`, 384 by 256.
fn mechanism_harness(robot: &Robot, table: &str, title: &str) -> Harness {
    let mut view = widget(
        "Mechanism2d",
        table,
        "",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 384.0,
            height: 256.0,
        },
    );
    view.properties.remove("data_type");
    view.title = title.into();
    let mut tab = Tab::new("Mechanism");
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

/// The area the card titled `title` says it shows.
fn showing(harness: &Harness, title: &str) -> String {
    harness
        .query_by_label_contains(&format!("{title}: "))
        .and_then(|n| n.value())
        .unwrap_or_default()
}

#[test]
fn a_mechanism_shows_its_ligaments_and_follows_them() {
    let robot = Robot::start();
    let mut lift = lift(&robot);
    let mut harness = mechanism_harness(&robot, "/Test/Lift", "Lift");
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 0.80 at 90°; base/elevator/arm: 0.50 at -45°",
    );
    // Robot code moves the arm and raises the elevator.
    lift.set("base/elevator/arm/angle", 30.0);
    lift.set("base/elevator/length", 1.25);
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 1.25 at 90°; base/elevator/arm: 0.50 at 30°",
    );
    // A ligament added later shows too, with no color or weight sent.
    lift.add(&Ligament {
        path: "base/elevator/arm/claw",
        angle: 0.0,
        length: 0.2,
        color: None,
        weight: None,
    });
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 1.25 at 90°; base/elevator/arm: 0.50 at 30°; \
         base/elevator/arm/claw: 0.20 at 0°",
    );
}

#[test]
fn several_roots_and_a_deep_chain_land_where_their_angles_add_up_to() {
    let robot = Robot::start();
    // Elsewhere than SmartDashboard, a 4 by 3 canvas, no background, two
    // roots, a chain three deep, and ligaments with no color or weight.
    let plain = |path, angle, length| Ligament {
        path,
        angle,
        length,
        color: None,
        weight: None,
    };
    let _m = Mechanism::publish(
        &robot,
        "/Arm/Mech",
        (4.0, 3.0),
        None,
        &[("turret", 3.5, 1.0), ("intake", 1.0, 0.2)],
        &[
            // Up 1, then turned back level (right) for 0.5, then down 0.5:
            // b's end is the mechanism's right edge.
            plain("turret/a", 90.0, 1.0),
            plain("turret/a/b", -90.0, 0.5),
            plain("turret/a/b/c", -90.0, 0.5),
            // Pointing back along -X.
            Ligament {
                path: "intake/roller",
                angle: 180.0,
                length: 0.4,
                color: Some("#FFFF00"),
                weight: Some(3.0),
            },
        ],
    );
    let mut harness = mechanism_harness(&robot, "/Arm/Mech", "Arm");
    // Roots in name order, each chain parent first, angles as robot code
    // set them.
    labelled_soon(
        &mut harness,
        "Arm: intake/roller: 0.40 at 180°; turret/a: 1.00 at 90°; \
         turret/a/b: 0.50 at -90°; turret/a/b/c: 0.50 at -90°",
    );
    // Where they landed, from the view's area: x from 0.6 (the roller's
    // end) to 4 (b's end, right of the turret), y from 0.2 (the intake) to
    // 2 (the top of a), with a margin of a tenth of the larger side (3.4)
    // all round. Had the angles not added up, b would have gone down, not
    // right, and the area would end at 3.5 plus the margin.
    assert_eq!(
        showing(&harness, "Arm"),
        "showing 0.26 to 4.34 across, -0.14 to 2.34 up"
    );
}

#[test]
fn a_mechanism2d_table_drags_from_the_tree() {
    let robot = Robot::start();
    let _lift = lift(&robot);
    let mut harness = harness_for(&robot);
    harness.state_mut().locked = false;
    settle(&mut harness);
    open(&mut harness, "Test  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    let to = harness.state().grid.area.min + egui::vec2(1200.0, 700.0);
    drag_row(&mut harness, "Lift", 0, to);
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Mechanism2d");
    assert_eq!(made.topic(), Some("/Test/Lift"));
}

#[test]
fn the_view_zooms_to_what_the_mechanism_has_used_and_only_grows() {
    let robot = Robot::start();
    let mut lift = lift(&robot);
    let mut harness = mechanism_harness(&robot, "/Test/Lift", "Lift");
    harness.state_mut().locked = true;
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 0.80 at 90°; base/elevator/arm: 0.50 at -45°",
    );
    // The base up the elevator and the arm, plus a margin: not the whole
    // 3 by 2 canvas.
    let start = "showing 1.38 to 1.97 across, 0.08 to 1.47 up";
    assert_eq!(showing(&harness, "Lift"), start);
    // The elevator comes down: the view stays (it only grows), so it
    // doesn't jump as the mechanism moves.
    lift.set("base/elevator/length", 0.4);
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 0.40 at 90°; base/elevator/arm: 0.50 at -45°",
    );
    assert_eq!(showing(&harness, "Lift"), start);
    // A double-click starts it over, from where the mechanism is now.
    let middle = card_on_screen(&harness, 0).center();
    for _ in 0..2 {
        click_at(
            &mut harness,
            middle,
            egui::PointerButton::Primary,
            Modifiers::NONE,
        );
    }
    assert_eq!(
        showing(&harness, "Lift"),
        "showing 1.42 to 1.93 across, 0.12 to 1.03 up"
    );
    // Fit set to the canvas: the whole of it.
    harness.state_mut().layout.tabs[0].grid_layout.containers[0]
        .properties
        .insert("fit".into(), json!("canvas"));
    settle(&mut harness);
    assert_eq!(
        showing(&harness, "Lift"),
        "showing 0.00 to 3.00 across, 0.00 to 2.00 up"
    );
}
