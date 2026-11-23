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
    app.robot.border = true;
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
