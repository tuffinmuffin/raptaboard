//! The Mechanism2d widget: a table as WPILib publishes it, moving and
//! growing a ligament, and the table dragged from the tree.

// Tests written by Claude.

use super::*;

/// The topics of a WPILib Mechanism2d at `/Test/Lift`, as RobotPy's
/// `Mechanism2d(3, 2)` with an elevator and an arm on it publishes them
/// (kept alive by the caller).
struct Lift {
    _fixed: Vec<Publisher<String>>,
    _dims: Publisher<Vec<f64>>,
    numbers: Vec<(String, Publisher<f64>)>,
}

impl Lift {
    fn publish(robot: &Robot) -> Lift {
        let s = &robot._server;
        let text = |name: &str, value: &str| {
            let p = s.publish::<String>(&format!("/Test/Lift/{name}"));
            p.set(value.into());
            p
        };
        let fixed = vec![
            text(".type", "Mechanism2d"),
            text("backgroundColor", "#000020"),
            text("base/elevator/.type", "line"),
            text("base/elevator/color", "#FF0000"),
            text("base/elevator/arm/.type", "line"),
            text("base/elevator/arm/color", "#00FF00"),
        ];
        let dims = s.publish::<Vec<f64>>("/Test/Lift/dims");
        dims.set(vec![3.0, 2.0]);
        let numbers = [
            ("base/x", 1.5),
            ("base/y", 0.2),
            ("base/elevator/angle", 90.0),
            ("base/elevator/length", 0.8),
            ("base/elevator/weight", 6.0),
            ("base/elevator/arm/angle", -45.0),
            ("base/elevator/arm/length", 0.5),
            ("base/elevator/arm/weight", 4.0),
        ]
        .into_iter()
        .map(|(name, v)| {
            let p = s.publish::<f64>(&format!("/Test/Lift/{name}"));
            p.set(v);
            (name.to_owned(), p)
        })
        .collect();
        Lift {
            _fixed: fixed,
            _dims: dims,
            numbers,
        }
    }

    /// Set the number at `name` (inside the table) to `v`.
    fn set(&self, name: &str, v: f64) {
        let (_, p) = self.numbers.iter().find(|(n, _)| n == name).unwrap();
        p.set(v);
    }
}

/// A harness with one tab holding a Mechanism2d on `/Test/Lift`, titled
/// "Lift".
fn lift_harness(robot: &Robot) -> Harness {
    let mut view = widget(
        "Mechanism2d",
        "/Test/Lift",
        "",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 384.0,
            height: 256.0,
        },
    );
    view.properties.remove("data_type");
    view.title = "Lift".into();
    let mut tab = Tab::new("Lift");
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

#[test]
fn a_mechanism_shows_its_ligaments_and_follows_them() {
    let robot = Robot::start();
    let lift = Lift::publish(&robot);
    let mut harness = lift_harness(&robot);
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
    // A ligament added later shows too.
    let s = &robot._server;
    let ty = s.publish::<String>("/Test/Lift/base/elevator/arm/claw/.type");
    ty.set("line".into());
    let length = s.publish::<f64>("/Test/Lift/base/elevator/arm/claw/length");
    length.set(0.2);
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 1.25 at 90°; base/elevator/arm: 0.50 at 30°; \
         base/elevator/arm/claw: 0.20 at 0°",
    );
}

#[test]
fn a_mechanism2d_table_drags_from_the_tree() {
    let robot = Robot::start();
    let _lift = Lift::publish(&robot);
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
