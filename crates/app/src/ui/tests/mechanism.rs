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

/// The area the Lift card says it shows.
fn showing(harness: &Harness) -> String {
    harness
        .query_by_label_contains("Lift: ")
        .and_then(|n| n.value())
        .unwrap_or_default()
}

#[test]
fn the_view_zooms_to_what_the_mechanism_has_used_and_only_grows() {
    let robot = Robot::start();
    let lift = Lift::publish(&robot);
    let mut harness = lift_harness(&robot);
    harness.state_mut().locked = true;
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 0.80 at 90°; base/elevator/arm: 0.50 at -45°",
    );
    // The base up the elevator and the arm, plus a margin: not the whole
    // 3 by 2 canvas.
    let start = "showing 1.38 to 1.97 across, 0.08 to 1.47 up";
    assert_eq!(showing(&harness), start);
    // The elevator comes down: the view stays (it only grows), so it
    // doesn't jump as the mechanism moves.
    lift.set("base/elevator/length", 0.4);
    labelled_soon(
        &mut harness,
        "Lift: base/elevator: 0.40 at 90°; base/elevator/arm: 0.50 at -45°",
    );
    assert_eq!(showing(&harness), start);
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
        showing(&harness),
        "showing 1.42 to 1.93 across, 0.12 to 1.03 up"
    );
    // Fit set to the canvas: the whole of it.
    harness.state_mut().layout.tabs[0].grid_layout.containers[0]
        .properties
        .insert("fit".into(), json!("canvas"));
    settle(&mut harness);
    assert_eq!(
        showing(&harness),
        "showing 0.00 to 3.00 across, 0.00 to 2.00 up"
    );
}
