//! Autoscaled bars and gauges: the scale follows every value received
//! (short peaks too), stays while its tab is closed, and Reset scale in the
//! right-click menu starts it again.

// Tests written by Claude.

use super::*;

/// A tab with an autoscaled Number Bar and a Radial Gauge that isn't, both
/// on `/Test/number`, and a second, empty tab.
fn harness_with(robot: &Robot) -> Harness {
    let at = |x: f64| LayoutRect {
        x,
        y: 0.0,
        width: 256.0,
        height: 128.0,
    };
    let mut bar = widget("Number Bar", "/Test/number", "double", at(0.0));
    bar.title = "Bar".into();
    bar.properties.insert("autoscale".into(), json!(true));
    let mut gauge = widget("Radial Gauge", "/Test/number", "double", at(256.0));
    gauge.title = "Gauge".into();
    let mut tab = Tab::new("Scales");
    tab.grid_layout.containers = vec![bar, gauge];
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![tab, Tab::new("Other")],
        ..ElasticLayout::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    app.locked = false;
    settle(&mut harness);
    harness
}

#[test]
fn an_autoscaled_bar_follows_every_value_and_resets_from_its_menu() {
    let robot = Robot::start();
    let mut harness = harness_with(&robot);
    // Seen only 0.25 so far: a scale around it.
    labelled_soon(&mut harness, "Bar: scale -1 to 1.50");
    // The gauge doesn't autoscale: Elastic's 0 to 100.
    labelled_soon(&mut harness, "Gauge: scale 0 to 100");
    // A peak and back between two frames still widens the scale.
    robot.number.set(7.86);
    robot.number.set(0.25);
    labelled_soon(&mut harness, "Bar: scale 0 to 8");
    robot.number.set(-3.0);
    labelled_soon(&mut harness, "Bar: scale -5 to 10");
    assert!(harness.query_by_label("Gauge: scale 0 to 100").is_some());

    // Kept while its tab is closed (where nothing is recorded).
    harness.state_mut().current_tab = 1;
    for _ in 0..5 {
        settle(&mut harness);
    }
    harness.state_mut().current_tab = 0;
    labelled_soon(&mut harness, "Bar: scale -5 to 10");
    robot.number.set(0.25);
    dashboard_has(&mut harness, "/Test/number", nt4::Value::Double(0.25));

    // Reset scale: from the value now.
    let bar = card_on_screen(&harness, 0);
    click_at(
        &mut harness,
        bar.center(),
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    settle(&mut harness);
    assert!(
        harness.query_by_label("Reset scale").is_some(),
        "the autoscaled bar's menu"
    );
    harness.get_by_label("Reset scale").click();
    labelled_soon(&mut harness, "Bar: scale -1 to 1.50");

    // The gauge, not autoscaled, has no Reset scale.
    harness.key_press(Key::Escape);
    settle(&mut harness);
    let gauge = card_on_screen(&harness, 1);
    click_at(
        &mut harness,
        gauge.center(),
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    settle(&mut harness);
    assert!(
        harness.query_by_label("Put in a new list").is_some(),
        "its menu"
    );
    assert!(harness.query_by_label("Reset scale").is_none());
}

#[test]
fn keeping_the_set_range_only_widens_past_it() {
    let robot = Robot::start();
    let mut harness = harness_with(&robot);
    let app = harness.state_mut();
    app.layout.tabs[0].grid_layout.containers[0]
        .properties
        .insert("autoscale_keep_range".into(), json!(true));
    settle(&mut harness);
    // Inside Number Bar's -1 to 1: just that.
    labelled_soon(&mut harness, "Bar: scale -1 to 1");
    robot.number.set(1.7);
    labelled_soon(&mut harness, "Bar: scale -1 to 2");
}
