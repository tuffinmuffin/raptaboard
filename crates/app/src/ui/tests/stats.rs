//! Min, max and average on number widgets: from every value received,
//! since reset (and Reset min/max in the right-click menu) or over the last
//! few seconds.

// Tests written by Claude.

use super::*;

/// A Text Display with its min, max and average since reset, and a Number
/// Bar with them over the last half second, both on `/Test/number`.
fn harness_with(robot: &Robot) -> Harness {
    let at = |x: f64| LayoutRect {
        x,
        y: 0.0,
        width: 256.0,
        height: 128.0,
    };
    let mut text = widget("Text Display", "/Test/number", "double", at(0.0));
    text.title = "Text".into();
    text.properties.insert("show_stats".into(), json!(true));
    let mut bar = widget("Number Bar", "/Test/number", "double", at(256.0));
    bar.title = "Bar".into();
    bar.properties.insert("show_stats".into(), json!(true));
    bar.properties.insert("stats_window".into(), json!(0.5));
    let mut tab = Tab::new("Stats");
    tab.grid_layout.containers = vec![text, bar];
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![tab],
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
fn min_max_and_average_count_every_value_and_reset_from_the_menu() {
    let robot = Robot::start();
    let mut harness = harness_with(&robot);
    labelled_soon(&mut harness, "Text: min 0.25, avg 0.25, max 0.25");
    // A peak and a dip between frames count, each value once.
    robot.number.set(5.0);
    robot.number.set(-1.0);
    robot.number.set(0.75);
    labelled_soon(&mut harness, "Text: min -1, avg 1.25, max 5");

    // Over the last half second: once it's past, only the value now.
    std::thread::sleep(Duration::from_millis(700));
    labelled_soon(&mut harness, "Bar: min 0.75, avg 0.75, max 0.75");
    // Since reset is unchanged by time passing.
    assert!(
        harness
            .query_by_label("Text: min -1, avg 1.25, max 5")
            .is_some()
    );

    // Reset min/max: from the value now.
    menu_of(&mut harness, 0);
    harness.get_by_label("Reset min/max").click();
    labelled_soon(&mut harness, "Text: min 0.75, avg 0.75, max 0.75");

    // A window has nothing to reset.
    harness.key_press(Key::Escape);
    settle(&mut harness);
    menu_of(&mut harness, 1);
    assert!(
        harness.query_by_label("Put in a new list").is_some(),
        "its menu"
    );
    assert!(harness.query_by_label("Reset min/max").is_none());
}
