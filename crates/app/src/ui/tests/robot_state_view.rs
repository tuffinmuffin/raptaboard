//! The robot's state for the whole window: the border and the e-stop
//! pop-up, from the server's Driver Station table with no FMSInfo widget in
//! the layout.

// Tests written by Claude.

use super::*;
use crate::types::robot_state::Mode;

#[test]
fn the_border_and_estop_popup_follow_the_robot_from_any_tab() {
    let robot = Robot::start();
    let table_type = robot._server.publish::<String>("/FMSInfo/.type");
    table_type.set(crate::types::robot_state::TYPE_2026.into());
    let bits = robot._server.publish::<i64>("/FMSInfo/FMSControlData");
    // Disabled, with a DS attached.
    bits.set(0x20);
    let mut harness = harness_for(&robot);
    let app = harness.state_mut();
    let mut tab = Tab::new("Driving");
    tab.grid_layout.containers = vec![widget(
        "Text Display",
        "/Test/number",
        "double",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 128.0,
        },
    )];
    app.layout = ElasticLayout {
        tabs: vec![tab],
        ..ElasticLayout::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    // The border is on in the layout.
    crate::ui::robot_state_view::Look {
        border: true,
        ..Default::default()
    }
    .write_to(&mut app.layout);
    // The tree asks for values of its own; hidden, only the border's count.
    app.tree.shown = false;
    let mode_soon = |harness: &mut Harness, want: Mode| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while harness.state().robot.mode != Some(want) {
            assert!(
                Instant::now() < deadline,
                "{want:?}, have {:?}",
                harness.state().robot.mode
            );
            settle(harness);
        }
    };
    mode_soon(&mut harness, Mode::Disabled);
    bits.set(0x21);
    mode_soon(&mut harness, Mode::Teleop);
    let now = Instant::now();
    assert_eq!(
        harness.state().robot.border_color(now),
        crate::ui::robot_state_view::default_color(Mode::Teleop)
    );
    assert!(harness.query_by_label("ROBOT E-STOPPED").is_none());
    // E-stopped: the pop-up, over everything.
    bits.set(0x28);
    labelled_soon(&mut harness, "ROBOT E-STOPPED");
    // A click anywhere takes it down.
    let somewhere = harness.state().grid.visible.center();
    click_at(
        &mut harness,
        somewhere,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    settle(&mut harness);
    assert!(harness.query_by_label("ROBOT E-STOPPED").is_none());
}

#[test]
fn the_border_is_set_in_the_layout_from_the_settings_window_and_undo_leaves_it() {
    use crate::ui::robot_state_view::Look;
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout::default();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.locked = false;
    settle(&mut harness);
    // An edit first: a tab taken away.
    harness.state_mut().layout.tabs.pop();
    settle(&mut harness);
    harness.get_by_label("⚙").click();
    settle(&mut harness);
    // Clear of the bar, so its buttons still work while it's open.
    let window = harness.get_by_label("Settings").rect();
    let bar = harness.get_by_label("Undo").rect();
    assert!(window.min.y > bar.max.y, "{window:?} under {bar:?}");
    harness
        .get_by_label("Color the window's border by the robot's state")
        .click();
    settle(&mut harness);
    // In the layout, so it's an unsaved change to it...
    assert!(Look::of(&harness.state().layout).border);
    assert!(harness.state().unsaved());
    assert!(harness.state().robot.look.border);
    // ...but a setting, not an edit: undo brings the tab back, and the
    // border stays.
    harness.get_by_label("Undo").click();
    settle(&mut harness);
    assert_eq!(harness.state().layout.tabs.len(), 2, "the tab is back");
    assert!(Look::of(&harness.state().layout).border, "the border stays");
    assert!(!harness.state().history.can_undo());
    // The gear closes it again.
    harness.get_by_label("⚙").click();
    settle(&mut harness);
    assert!(!harness.state().gear.settings);
}
