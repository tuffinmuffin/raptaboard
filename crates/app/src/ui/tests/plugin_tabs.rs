//! A tab a plugin fills (plugin.toml's `tab = "…"`): the repo's tab-test
//! plugin on an empty tab of its name, the keyboard while it has focus, and
//! its sound.

// Tests written by Claude.

use std::path::Path;

use super::*;

fn load_tab_test() {
    raptaboard_plugin_host::dev::repo_plugins();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/tab-test");
    crate::plugins::load(&folder).unwrap();
}

/// Tabs: "Widgets" (a Text Display), "tab test" (empty: the plugin's, in
/// another case) and "TAB TEST" (the plugin's name, but with a widget: the
/// user's).
fn tabs_harness(robot: &Robot) -> Harness {
    let display = |x| {
        widget(
            "Text Display",
            "/Test/number",
            "double",
            LayoutRect {
                x,
                y: 0.0,
                width: 256.0,
                height: 128.0,
            },
        )
    };
    let mut widgets = Tab::new("Widgets");
    widgets.grid_layout.containers = vec![display(0.0)];
    let mut taken = Tab::new("TAB TEST");
    taken.grid_layout.containers = vec![display(256.0)];
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![widgets, Tab::new("tab test"), taken],
        ..Default::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 1;
    app.alert_sound = true;
    settle(&mut harness);
    harness
}

const SHOWN: &str = "tab test: Hello from the tab-test plugin's folder. Keys: ";

/// The plugin's area on the open tab.
fn plugin_area(harness: &Harness) -> Pos2 {
    harness.get_by_label_contains(SHOWN).rect().center()
}

#[test]
fn an_empty_tab_of_its_name_shows_the_plugin_and_gives_it_the_keyboard() {
    load_tab_test();
    let robot = Robot::start();
    let mut harness = tabs_harness(&robot);
    labelled_soon(&mut harness, &format!("{SHOWN}0"));
    // Not clicked yet: keys are the dashboard's.
    assert!(!harness.state().plugin_has_keys(&harness.ctx.clone()));
    let area = plugin_area(&harness);
    click_at(
        &mut harness,
        area,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(harness.state().plugin_has_keys(&harness.ctx.clone()));
    // Space: the plugin counts it, and beeps through the dashboard's sound.
    press(&mut harness, Key::Space);
    labelled_soon(&mut harness, &format!("{SHOWN}1"));
    let audio = harness.state().sound.audio();
    let deadline = Instant::now() + Duration::from_secs(10);
    while audio.heard() == 0 {
        assert!(Instant::now() < deadline, "no sound");
        settle(&mut harness);
    }
    assert_eq!(audio.heard(), 2205, "0.1 s at 22 050 a second");
    // The dashboard's shortcuts are the plugin's keys now: Save saves
    // nothing (it would, from the grid; Open would open a dialog over the
    // game).
    let file =
        std::env::temp_dir().join(format!("raptaboard-plugin-tab-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&file);
    harness.state_mut().file.path = Some(file.clone());
    for event in key(Key::S, Modifiers::COMMAND) {
        harness.event(event);
    }
    settle(&mut harness);
    assert!(!file.exists(), "saved while the plugin had the keyboard");
    labelled_soon(&mut harness, &format!("{SHOWN}2"));
    // And the shortcut to the first tab.
    for event in key(Key::Num1, Modifiers::COMMAND) {
        harness.event(event);
    }
    settle(&mut harness);
    assert_eq!(harness.state().current_tab, 1);
    labelled_soon(&mut harness, &format!("{SHOWN}3"));
    // Shift+Escape gives the keyboard back (and isn't the plugin's key).
    for event in key(Key::Escape, Modifiers::SHIFT) {
        harness.event(event);
    }
    settle(&mut harness);
    assert!(!harness.state().plugin_has_keys(&harness.ctx.clone()));
    labelled_soon(&mut harness, &format!("{SHOWN}3"));
    // So does a click elsewhere (the tab bar), after a click back in.
    click_at(
        &mut harness,
        area,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(harness.state().plugin_has_keys(&harness.ctx.clone()));
    let bar = harness.get_by_label("Widgets").rect().right_center() + egui::vec2(300.0, 0.0);
    click_at(
        &mut harness,
        bar,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(!harness.state().plugin_has_keys(&harness.ctx.clone()));
    for event in key(Key::S, Modifiers::COMMAND) {
        harness.event(event);
    }
    settle(&mut harness);
    assert!(file.exists(), "Save is the dashboard's again");
    let _ = std::fs::remove_file(&file);
    for event in key(Key::Num1, Modifiers::COMMAND) {
        harness.event(event);
    }
    settle(&mut harness);
    assert_eq!(harness.state().current_tab, 0);
}

#[test]
fn a_tab_of_its_name_with_widgets_stays_the_users() {
    load_tab_test();
    let robot = Robot::start();
    let mut harness = tabs_harness(&robot);
    let layout = harness.state().layout.clone();
    assert!(crate::plugins::tab_plugin(&layout.tabs[1]).is_some());
    assert!(crate::plugins::tab_plugin(&layout.tabs[2]).is_none());
    assert!(crate::plugins::tab_plugin(&layout.tabs[0]).is_none());
    harness.state_mut().current_tab = 2;
    settle(&mut harness);
    assert!(harness.query_by_label_contains(SHOWN).is_none());
    // Its worker keeps running while its tab is closed (a game pauses: it
    // isn't drawn), and stops when the tab goes.
    let ctx = harness.ctx.clone();
    let ty = crate::plugins::tab_plugin(&layout.tabs[1]).unwrap();
    let stand_in = crate::plugins::tab_widget(ty, "tab test");
    assert!(crate::plugins::has_worker(&ctx, &stand_in));
    harness.state_mut().layout.tabs.remove(1);
    harness.state_mut().current_tab = 0;
    settle(&mut harness);
    assert!(!crate::plugins::has_worker(&ctx, &stand_in));
}

#[test]
fn a_tab_plugin_holds_the_mouse_while_it_has_the_keyboard() {
    load_tab_test();
    let robot = Robot::start();
    let mut harness = tabs_harness(&robot);
    labelled_soon(&mut harness, &format!("{SHOWN}0"));
    let ctx = harness.ctx.clone();
    let holds = || crate::plugins::widget::holds_mouse(&ctx, "tab test");
    assert!(!holds());
    let area = plugin_area(&harness);
    click_at(
        &mut harness,
        area,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(holds(), "it asked for the mouse (capture_mouse)");
    // Its movement, wherever the cursor would be.
    harness.event(Event::MouseMoved(egui::vec2(12.0, 4.0)));
    settle(&mut harness);
    labelled_soon(&mut harness, &format!("{SHOWN}0, mouse 12, clicks 0"));
    // Another app takes the focus: the mouse is let go (never stuck), and
    // taken again when the window comes back. (eframe sets the raw input's
    // `focused` from the window's own event.)
    harness.input_mut().focused = false;
    settle(&mut harness);
    assert!(!holds());
    harness.input_mut().focused = true;
    settle(&mut harness);
    assert!(holds());
    // Shift+Escape gives both back.
    for event in key(Key::Escape, Modifiers::SHIFT) {
        harness.event(event);
    }
    settle(&mut harness);
    assert!(!holds());
    assert!(!harness.state().plugin_has_keys(&ctx));
}

#[test]
fn a_tab_plugin_can_give_the_keyboard_and_mouse_back() {
    load_tab_test();
    let robot = Robot::start();
    let mut harness = tabs_harness(&robot);
    labelled_soon(&mut harness, &format!("{SHOWN}0"));
    let ctx = harness.ctx.clone();
    let area = plugin_area(&harness);
    click_at(
        &mut harness,
        area,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(harness.state().plugin_has_keys(&ctx));
    assert!(crate::plugins::widget::holds_mouse(&ctx, "tab test"));
    // Q: tab-test asks (release-focus), as a game's Quit would.
    press(&mut harness, Key::Q);
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.state().plugin_has_keys(&ctx) {
        assert!(Instant::now() < deadline, "still has the keyboard");
        settle(&mut harness);
    }
    assert!(!crate::plugins::widget::holds_mouse(&ctx, "tab test"));
}
