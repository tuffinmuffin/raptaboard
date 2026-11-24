//! The FMSInfo widget's robot state sounds: heard from any tab, from the
//! first FMSInfo only, and while the window draws nothing.

// Tests written by Claude.

use super::*;
use crate::sound::{Clip, Tone};

/// A layout whose open tab has no FMSInfo: tab 1 has the first one (its
/// default sounds), tab 2 a second on the other year's table whose teleop
/// sound is a file, which must never play: only the first FMSInfo in the
/// layout plays.
fn layout() -> ElasticLayout {
    let at = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 384.0,
        height: 256.0,
    };
    let info = |table: &str, extra: serde_json::Value| {
        let mut w = widget("FMSInfo", table, "", at);
        w.properties.remove("data_type");
        for (k, v) in extra.as_object().unwrap() {
            w.properties.insert(k.clone(), v.clone());
        }
        w
    };
    let mut driving = Tab::new("Driving");
    driving.grid_layout.containers = vec![widget("Text Display", "/Test/number", "double", at)];
    let mut first = Tab::new("Robot");
    first.grid_layout.containers = vec![info("/FMSInfo", json!({}))];
    let mut second = Tab::new("Spare");
    second.grid_layout.containers = vec![info(
        "/DriverStation",
        json!({"sound_teleop": "/second.mp3"}),
    )];
    ElasticLayout {
        tabs: vec![driving, first, second],
        ..ElasticLayout::default()
    }
}

/// Wait (without drawing) until the app has played `n` state sounds.
fn sounds_soon(harness: &Harness, n: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.state().sound.events().len() < n {
        assert!(
            Instant::now() < deadline,
            "{n} sounds, got {:?}",
            harness.state().sound.events()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Step for a while (long enough for a value to arrive and be acted on).
fn wait_a_while(harness: &mut Harness) {
    for _ in 0..20 {
        settle(harness);
    }
}

#[test]
fn state_sounds_play_from_a_closed_tab_without_frames_and_only_for_changes() {
    let robot = Robot::start();
    let table_type = robot._server.publish::<String>("/FMSInfo/.type");
    table_type.set(crate::types::robot_state::TYPE_2026.into());
    let bits = robot._server.publish::<i64>("/FMSInfo/FMSControlData");
    // Disabled, with a DS attached.
    bits.set(0x20);
    let mut harness = harness_for(&robot);
    let app = harness.state_mut();
    app.layout = layout();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    app.alert_sound = true;
    // The tree and the e-stop pop-up ask for values of their own; with them
    // off, only the sounds' count (so the wait below means the sound task
    // has seen the state).
    app.tree.shown = false;
    crate::ui::robot_state_view::Look {
        estop_popup: Some(false),
        ..Default::default()
    }
    .write_to(&mut app.layout);
    wait_a_while(&mut harness);
    // The sound task has seen it disabled (values arrive in order, so it
    // sees the change next); on a slow machine the first state it saw could
    // otherwise be the enabled one, which is quiet, as at connect.
    dashboard_has(
        &mut harness,
        "/FMSInfo/FMSControlData",
        nt4::Value::Int(0x20),
    );
    // Disabled when it connected: no sound for that.
    assert!(harness.state().sound.events().is_empty());
    // Enabled in teleop, with no frames drawn (a window in the background):
    // the first FMSInfo's default, though its tab is closed; never the
    // second one's file.
    bits.set(0x21);
    sounds_soon(&harness, 1);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        harness.state().sound.events(),
        [(Clip::Tone(Tone::GoGoGo), 0.8)]
    );
    // Muted with the bell button: a change plays nothing.
    harness.state_mut().alert_sound = false;
    wait_a_while(&mut harness);
    bits.set(0x20);
    wait_a_while(&mut harness);
    assert_eq!(harness.state().sound.events().len(), 1);
}

/// Give the app's installed sounds `fire 1` and `horn` from the
/// "IT Crowd clips" pack, as installing it would.
fn install_clips(harness: &mut Harness) {
    let files = harness.state().sound.files();
    let mut layers = files.write().unwrap();
    for (name, file) in [
        ("fire 1", "/packs/fan-clips/fire1.mp3"),
        ("horn", "/packs/fan-clips/horn.mp3"),
    ] {
        layers.add(
            name,
            crate::sound::LayeredSound {
                pack: "fan-clips".into(),
                pack_name: "IT Crowd clips".into(),
                path: file.into(),
                installed_at: 1,
            },
        );
    }
}

/// The dropdown named `name` (beside the label of the same text).
fn combo<'h>(harness: &'h Harness, name: &'h str) -> egui_kittest::Node<'h> {
    harness.get_by(|n| {
        n.role() == egui::accesskit::Role::ComboBox && n.label().as_deref() == Some(name)
    })
}

/// Open the dropdown named `name`, and pick `entry` in it.
fn pick_sound(harness: &mut Harness, name: &str, entry: &str) {
    let combo = combo(harness, name).rect().center();
    click_at(
        harness,
        combo,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    // The list scrolls: bring `entry` into view first.
    harness.get_by_label(entry).scroll_to_me();
    settle(harness);
    harness.get_by_label(entry).click();
    settle(harness);
}

/// The FMSInfo's saved sound for test mode.
fn test_sound(harness: &Harness) -> Option<serde_json::Value> {
    harness.state().layout.tabs[1].grid_layout.containers[0]
        .properties
        .get("sound_test")
        .cloned()
}

#[test]
fn the_sound_picker_offers_each_packs_sound_and_the_built_in_one() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    let app = harness.state_mut();
    app.layout = layout();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 1;
    let original = app.layout.tabs[1].grid_layout.containers[0].clone();
    app.editing = Some(Editing::Widget {
        tab: 1,
        index: 0,
        original,
    });
    install_clips(&mut harness);
    settle(&mut harness);
    // Test mode's "danger danger" to the built-in fire 1, ignoring the pack.
    assert_eq!(
        combo(&harness, "Sound: Test").value().as_deref(),
        Some("danger danger")
    );
    pick_sound(&mut harness, "Sound: Test", "    fire 1 (built in)");
    assert_eq!(test_sound(&harness), Some(json!("builtin:fire 1")));
    let plays = |h: &Harness| {
        let w = &h.state().layout.tabs[1].grid_layout.containers[0];
        let clip = crate::widgets::builtin::fms_info::sound_for(
            &w.properties,
            crate::types::robot_state::Event::Test,
        )
        .unwrap();
        clip.playable(&h.state().sound.files().read().unwrap())
    };
    assert_eq!(
        plays(&harness),
        crate::sound::Playable::BuiltIn(Tone::Fire1)
    );
    // Then a sound only the pack has.
    pick_sound(&mut harness, "Sound: Test", "horn");
    assert_eq!(test_sound(&harness), Some(json!("horn")));
    assert_eq!(
        plays(&harness),
        crate::sound::Playable::File("/packs/fan-clips/horn.mp3".into())
    );
    assert!(harness.query_by_label("not installed").is_none());
    // The pack removed: it says so, and plays nothing.
    *harness.state().sound.files().write().unwrap() = Default::default();
    settle(&mut harness);
    assert_eq!(
        combo(&harness, "Sound: Test").value().as_deref(),
        Some("horn (not installed)")
    );
    harness.get_by_label("not installed");
    assert_eq!(plays(&harness), crate::sound::Playable::Nothing);
}
