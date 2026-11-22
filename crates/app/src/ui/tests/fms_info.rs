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
    // The tree asks for values of its own; hidden, only the sounds' count.
    app.tree.shown = false;
    wait_a_while(&mut harness);
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
