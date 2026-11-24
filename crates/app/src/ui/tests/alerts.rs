//! The Alerts widget, tab marks and the alert history.

// Tests written by Claude.

use super::*;

/// A layout with Alerts widgets: on tab 0, one on `/Test/Alerts` that
/// sounds and one showing every group; on tab 1, one on `/Test/Arm`.
fn alerts_layout() -> ElasticLayout {
    let at = |x: f64| LayoutRect {
        x,
        y: 0.0,
        width: 512.0,
        height: 256.0,
    };
    let alerts = |topic: &str, x: f64, extra: serde_json::Value| {
        let mut w = widget("Alerts", topic, "", at(x));
        w.properties.remove("data_type");
        for (k, v) in extra.as_object().unwrap() {
            w.properties.insert(k.clone(), v.clone());
        }
        w
    };
    let mut main = Tab::new("Alerts here");
    main.grid_layout.containers = vec![
        alerts("/Test/Alerts", 0.0, json!({"sound": true})),
        alerts("/Test/Alerts", 512.0, json!({"all_groups": true})),
    ];
    main.grid_layout.containers[1].title = "Everything".into();
    let mut other = Tab::new("Arm tab");
    other.grid_layout.containers = vec![alerts("/Test/Arm", 0.0, json!({}))];
    other.grid_layout.containers[0].title = "Arm alerts".into();
    ElasticLayout {
        tabs: vec![main, other, Tab::new("Nothing")],
        ..ElasticLayout::default()
    }
}

/// The harness showing [`alerts_layout`], locked, with sounds on.
fn alerts_harness(robot: &Robot) -> Harness {
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = alerts_layout();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    app.locked = true;
    app.alert_sound = true;
    settle(&mut harness);
    harness
}

#[test]
fn an_alerts_widget_counts_by_level_and_sounds_once_for_a_new_error() {
    let robot = Robot::start();
    let mut harness = alerts_harness(&robot);
    labelled_soon(&mut harness, "Alerts: 0 errors, 1 warning, 1 info");
    // Already active when it first looked: no tone for those.
    assert!(harness.state().sound.tones().is_empty());
    // A new error: one tone.
    robot.alert_errors.set(vec!["Encoder lost".into()]);
    labelled_soon(&mut harness, "Alerts: 1 error, 1 warning, 1 info");
    assert_eq!(harness.state().sound.tones(), [crate::sound::Tone::Beep]);
    // Another right after: no second tone so soon. A warning never sounds.
    robot
        .alert_errors
        .set(vec!["CAN fault".into(), "Encoder lost".into()]);
    robot
        .alert_warnings
        .set(vec!["Battery low".into(), "Hot motor".into()]);
    labelled_soon(&mut harness, "Alerts: 2 errors, 2 warnings, 1 info");
    assert_eq!(harness.state().sound.tones().len(), 1);
    // The bell button's mute stops it.
    harness.state_mut().alert_sound = false;
    std::thread::sleep(Duration::from_millis(3100));
    robot.alert_errors.set(vec!["Brownout".into()]);
    labelled_soon(&mut harness, "Alerts: 1 error, 2 warnings, 1 info");
    assert_eq!(harness.state().sound.tones().len(), 1);
}

#[test]
fn all_groups_and_the_tab_marks_follow_every_alert_widget() {
    let robot = Robot::start();
    let mut harness = alerts_harness(&robot);
    labelled_soon(&mut harness, "Everything: 0 errors, 1 warning, 1 info");
    use crate::types::alerts::Level;
    // A warning on tab 0; tab 1's group has nothing; tab 2 has no widget.
    let marks = |h: &mut Harness| {
        settle(h);
        let app = h.state();
        let topics = app.nt.topics();
        let values: std::collections::HashMap<&str, &nt4::Value> = topics
            .iter()
            .filter_map(|t| Some((&*t.name, &t.value.as_ref()?.value)))
            .collect();
        app.tab_marks(&values)
    };
    assert_eq!(marks(&mut harness), [Some(Level::Warning), None, None]);
    // An error in the arm group: on tab 1, whose tab isn't open, and in
    // "every group" on tab 0.
    robot.arm_errors.set(vec!["Arm motor fault".into()]);
    labelled_soon(&mut harness, "Everything: 1 error, 1 warning, 1 info");
    assert_eq!(
        marks(&mut harness),
        [Some(Level::Error), Some(Level::Error), None]
    );
    // Infos don't mark a tab.
    robot.arm_errors.set(vec![]);
    robot.alert_warnings.set(vec![]);
    robot.arm_warnings.set(vec![]);
    labelled_soon(&mut harness, "Everything: 0 errors, 0 warnings, 1 info");
    assert_eq!(marks(&mut harness), [None, None, None]);
}

#[test]
fn an_alert_group_drags_from_the_tree_as_an_alerts_widget() {
    let robot = Robot::start();
    let mut harness = alerts_harness(&robot);
    harness.state_mut().locked = false;
    harness.state_mut().current_tab = 2;
    settle(&mut harness);
    open(&mut harness, "Test  (");
    let to = harness.state().grid.area.min + egui::vec2(300.0, 300.0);
    drag_row(&mut harness, "Arm", 0, to);
    let made = harness.state().layout.tabs[2]
        .grid_layout
        .containers
        .clone();
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].widget_type, "Alerts");
    assert_eq!(made[0].topic(), Some("/Test/Arm"));
}

#[test]
fn alerts_stay_shown_greyed_when_the_connection_drops() {
    let robot = Robot::start();
    let mut harness = alerts_harness(&robot);
    robot.alert_errors.set(vec!["Brownout".into()]);
    labelled_soon(&mut harness, "Alerts: 1 error, 1 warning, 1 info");
    // The robot goes away: the last alerts seen stay, marked.
    drop(robot);
    labelled_soon(
        &mut harness,
        "Alerts: 1 error, 1 warning, 1 info (disconnected)",
    );
}

#[test]
fn the_alert_history_records_every_group_and_opens_from_the_top_bar() {
    let robot = Robot::start();
    let mut harness = alerts_harness(&robot);
    use crate::alert_history::Ending;
    // With no Alerts widget anywhere: it records anyway.
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![Tab::new("Nothing")],
        ..ElasticLayout::default()
    };
    app.current_tab = 0;
    settle(&mut harness);
    robot.arm_errors.set(vec!["Arm motor fault".into()]);
    let ended = |h: &Harness, text: &str| {
        h.state()
            .alerts
            .history
            .events()
            .rev()
            .find(|e| e.text == text)
            .map(|e| e.ended.map(|(_, how)| how))
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while ended(&harness, "Arm motor fault") != Some(None) {
        assert!(Instant::now() < deadline, "the fault was never recorded");
        settle(&mut harness);
    }
    // The starting warning and info are in it too.
    assert_eq!(ended(&harness, "Battery low"), Some(None));
    // The top-bar button counts what's active and opens the history.
    let button = harness.get_by_label_contains("Alerts 3").rect().center();
    click_at(
        &mut harness,
        button,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(harness.state().alerts.panel_shown);
    labelled_soon(&mut harness, "Arm: Arm motor fault");
    // Cleared by the robot.
    robot.arm_errors.set(vec![]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while ended(&harness, "Arm motor fault") != Some(Some(Ending::Cleared)) {
        assert!(Instant::now() < deadline, "never cleared");
        settle(&mut harness);
    }
    // Clear forgets ended ones, keeps active ones.
    // The history's Clear (the tree's filter has one too), at the right.
    let clear = harness
        .get_all_by_label("Clear")
        .map(|n| n.rect())
        .max_by(|a, b| a.min.x.total_cmp(&b.min.x))
        .unwrap();
    click_at(
        &mut harness,
        clear.center(),
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(ended(&harness, "Arm motor fault"), None);
    assert_eq!(ended(&harness, "Battery low"), Some(None));
    // The robot goes: what was active ends as disconnected.
    drop(robot);
    let deadline = Instant::now() + Duration::from_secs(10);
    while ended(&harness, "Battery low") != Some(Some(Ending::Disconnected)) {
        assert!(Instant::now() < deadline, "never ended by the disconnect");
        settle(&mut harness);
    }
}

#[test]
fn a_changing_alert_is_one_row_and_can_be_left_out_of_the_history() {
    let robot = Robot::start();
    let mut harness = alerts_harness(&robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![Tab::new("Nothing")],
        ..ElasticLayout::default()
    };
    app.current_tab = 0;
    app.alerts.panel_shown = true;
    settle(&mut harness);
    // Robot code that puts a reading in its alert's text, every loop.
    for pct in [37, 38, 36, 39] {
        robot
            .arm_warnings
            .set(vec![format!("Loop at {pct}% of its budget")]);
        settle(&mut harness);
        std::thread::sleep(Duration::from_millis(30));
        settle(&mut harness);
    }
    let rows = |h: &Harness| -> Vec<(String, u32)> {
        h.state()
            .alerts
            .history
            .events()
            .filter(|e| e.group == "Arm")
            .map(|e| (e.text.clone(), e.changes))
            .collect()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while rows(&harness).first().map(|r| r.0.as_str()) != Some("Loop at 39% of its budget") {
        assert!(Instant::now() < deadline, "{:?}", rows(&harness));
        settle(&mut harness);
    }
    let got = rows(&harness);
    assert_eq!(got.len(), 1, "one alert, not one per reading: {got:?}");
    assert!(got[0].1 >= 1, "its changes are counted");
    // Right-click its row: don't record alerts like this.
    let row = harness
        .get_by_label("Arm: Loop at 39% of its budget")
        .rect()
        .center();
    click_at(
        &mut harness,
        row,
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    harness
        .get_by_label("Don't record alerts like this")
        .click();
    settle(&mut harness);
    assert_eq!(harness.state().alerts.history.ignored(), ["Loop at"]);
    assert!(rows(&harness).is_empty());
    // New readings aren't recorded now.
    robot
        .arm_warnings
        .set(vec!["Loop at 81% of its budget".into()]);
    std::thread::sleep(Duration::from_millis(100));
    settle(&mut harness);
    assert!(rows(&harness).is_empty());
}

#[test]
fn an_alerts_widget_can_sound_a_packs_sound() {
    let robot = Robot::start();
    let mut harness = alerts_harness(&robot);
    harness.state_mut().layout.tabs[0].grid_layout.containers[0]
        .properties
        .insert("tone".into(), "pack:fan-clips/horn".into());
    labelled_soon(&mut harness, "Alerts: 0 errors, 1 warning, 1 info");
    robot.alert_errors.set(vec!["Encoder lost".into()]);
    labelled_soon(&mut harness, "Alerts: 1 error, 1 warning, 1 info");
    let horn = crate::sound::SoundRef::Pack {
        pack: "fan-clips".into(),
        name: "horn".into(),
    };
    assert_eq!(
        harness.state().sound.sounds(),
        [crate::sound::Clip::Sound(horn)]
    );
    assert!(harness.state().sound.tones().is_empty());
}
