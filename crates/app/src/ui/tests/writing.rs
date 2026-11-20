//! Widgets that send values: toggles, failed sends, and writing widgets inside a List Layout.

// Tests written by Claude.

use super::*;

#[test]
fn toggles_send_the_opposite_value_locked_or_not() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);
    let none = Modifiers::NONE;
    let primary = egui::PointerButton::Primary;
    // Locked: anywhere in the body toggles.
    harness.state_mut().locked = true;
    settle(&mut harness);
    let (body, _) = body_and_title(&harness, 0);
    dashboard_has(&mut harness, "/Test/enabled", nt4::Value::Boolean(false));
    click_at(&mut harness, body, primary, none);
    server_sees(&mut harness, &robot, true);
    // The button shows it, and a click on it sends it back.
    let (button, title) = body_and_title(&harness, 1);
    click_at(&mut harness, button, primary, none);
    server_sees(&mut harness, &robot, false);
    // Locked, the title bar toggles too (it's part of the widget).
    click_at(&mut harness, title, primary, none);
    server_sees(&mut harness, &robot, true);

    // Unlocked: the body still toggles...
    harness.state_mut().locked = false;
    settle(&mut harness);
    click_at(&mut harness, button, primary, none);
    server_sees(&mut harness, &robot, false);
    // ...but the title bar selects and moves the widget instead.
    click_at(&mut harness, title, primary, none);
    assert_eq!(harness.state().selected, Some(Item::Widget(1)));
    assert_eq!(robot.seen_enabled.get(), Some(false));
    harness.hover_at(title);
    harness.step();
    harness.drag_at(title);
    harness.step();
    for step in 1..=8 {
        harness.hover_at(title + egui::vec2(0.0, 32.0 * step as f32));
        harness.step();
    }
    harness.drop_at(title + egui::vec2(0.0, 256.0));
    settle(&mut harness);
    let moved = harness.state().layout.tabs[0].grid_layout.containers[1].rect;
    assert_eq!((moved.x, moved.y), (256.0, 256.0));
    assert_eq!(robot.seen_enabled.get(), Some(false));
    // Nothing failed.
    assert_eq!(harness.state().sound.played(), 0);
    assert!(harness.state().sends.failed.is_empty());
}

#[test]
fn the_robots_own_changes_win() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);
    let (body, _) = body_and_title(&harness, 0);
    let primary = egui::PointerButton::Primary;
    click_at(&mut harness, body, primary, Modifiers::NONE);
    server_sees(&mut harness, &robot, true);
    // The robot puts it back: the widget shows that, so the next click
    // sends true again rather than false.
    robot.enabled.set(false);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        settle(&mut harness);
        let shown = harness
            .state()
            .nt
            .topics()
            .into_iter()
            .find(|t| &*t.name == "/Test/enabled")
            .and_then(|t| t.value)
            .map(|v| v.value);
        if shown == Some(nt4::Value::Boolean(false)) {
            break;
        }
        assert!(Instant::now() < deadline, "the robot's value never arrived");
    }
    // Wait past the widget's period so it has taken the new value.
    std::thread::sleep(Duration::from_millis(100));
    settle(&mut harness);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    server_sees(&mut harness, &robot, true);
}

#[test]
fn a_failed_send_flashes_beeps_and_says_why() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);
    let primary = egui::PointerButton::Primary;
    // A topic the robot never published.
    let (body, _) = body_and_title(&harness, 2);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    let app = harness.state();
    assert_eq!(
        app.status,
        "Can't send to /Test/never_sent: the robot hasn't published it"
    );
    assert_eq!(app.sound.played(), 1);
    assert!(app.sends.failed.contains_key(&(0, Sender::Widget(2))));
    // A struct field.
    let (body, _) = body_and_title(&harness, 3);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    let app = harness.state();
    assert_eq!(
        app.status,
        "Can't send to /Drive/pose: struct values can't be changed"
    );
    assert_eq!(app.sound.played(), 2);
    // With the sound off: still flashes and says why, but no tone.
    harness.state_mut().alert_sound = false;
    click_at(&mut harness, body, primary, Modifiers::NONE);
    assert_eq!(harness.state().sound.played(), 2);
    // A later good send clears that widget's flash.
    harness
        .state_mut()
        .sends
        .failed
        .insert((0, Sender::Widget(0)), Instant::now());
    let (good, _) = body_and_title(&harness, 0);
    click_at(&mut harness, good, primary, Modifiers::NONE);
    assert!(
        !harness
            .state()
            .sends
            .failed
            .contains_key(&(0, Sender::Widget(0)))
    );
    // Disconnected: says so.
    harness.state_mut().address.custom = "".into();
    settle(&mut harness);
    click_at(&mut harness, good, primary, Modifiers::NONE);
    assert_eq!(
        harness.state().status,
        "Can't send: not connected to the robot"
    );
}

#[test]
fn toggles_work_inside_a_list() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);
    let primary = egui::PointerButton::Primary;
    let button = child_middle(&harness, 0, 0);
    // Locked and unlocked alike.
    for locked in [true, false] {
        harness.state_mut().locked = locked;
        settle(&mut harness);
        let now = robot.seen_enabled.get().unwrap_or(false);
        dashboard_has(&mut harness, "/Test/enabled", nt4::Value::Boolean(now));
        let want = !now;
        click_at(&mut harness, button, primary, Modifiers::NONE);
        server_sees(&mut harness, &robot, want);
    }
    // The grid toggles on the same topic show what the list's sent.
    let shown = harness
        .state()
        .nt
        .client()
        .and_then(|c| c.topic("/Test/enabled"))
        .and_then(|t| t.value)
        .map(|v| v.value);
    assert_eq!(shown, robot.seen_enabled.get().map(nt4::Value::Boolean));
    // A child that can't send flashes its row and says why.
    let failing = child_middle(&harness, 0, 1);
    click_at(&mut harness, failing, primary, Modifiers::NONE);
    let app = harness.state();
    assert!(
        app.sends
            .failed
            .contains_key(&(0, Sender::Child { list: 0, child: 1 }))
    );
    assert_eq!(
        app.status,
        "Can't send to /Test/never_sent: the robot hasn't published it"
    );
}

#[test]
fn a_send_that_works_clears_the_last_failures_message() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);
    let primary = egui::PointerButton::Primary;
    let (fails, _) = body_and_title(&harness, 2);
    let (works, _) = body_and_title(&harness, 0);
    click_at(&mut harness, fails, primary, Modifiers::NONE);
    assert!(harness.state().status.contains("hasn't published"));
    click_at(&mut harness, works, primary, Modifiers::NONE);
    assert_eq!(harness.state().status, "");
    // A message from something else since isn't the failure's to clear.
    click_at(&mut harness, fails, primary, Modifiers::NONE);
    harness.state_mut().status = "Opened match.json".into();
    click_at(&mut harness, works, primary, Modifiers::NONE);
    assert_eq!(harness.state().status, "Opened match.json");
}

/// List `list`'s child `child`'s inside label, on screen.
fn child_label(harness: &Harness, list: usize, child: usize) -> Rect {
    let app = harness.state();
    let l = &app.layout.tabs[0].grid_layout.layouts[list];
    let min = app.grid.area.min + egui::vec2(l.rect.x as f32, l.rect.y as f32);
    let card = Rect::from_min_size(min, egui::vec2(l.rect.width as f32, l.rect.height as f32))
        .shrink(crate::ui::style::CELL_GAP);
    list_layout::rows_on_screen(card, l)
        .into_iter()
        .find(|row| row.index == child)
        .and_then(|row| row.label)
        .expect("the child has a label")
}

#[test]
fn unlocked_a_list_childs_label_drags_it_and_the_rest_toggles() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);
    let primary = egui::PointerButton::Primary;
    harness.state_mut().locked = false;
    settle(&mut harness);
    let before = robot.seen_enabled.get();
    dashboard_has(
        &mut harness,
        "/Test/enabled",
        nt4::Value::Boolean(before.unwrap_or(false)),
    );
    // A click on the label selects; it doesn't toggle.
    let label = child_label(&harness, 0, 0).center();
    click_at(&mut harness, label, primary, Modifiers::NONE);
    assert_eq!(harness.state().selected, Some(Item::List(0)));
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_enabled.get(), before);
    // Below the label, it toggles.
    let below = child_label(&harness, 0, 0).center_bottom() + egui::vec2(0.0, 20.0);
    click_at(&mut harness, below, primary, Modifiers::NONE);
    server_sees(&mut harness, &robot, before != Some(true));
    // Locked, the label toggles too.
    harness.state_mut().locked = true;
    settle(&mut harness);
    let now = robot.seen_enabled.get();
    click_at(&mut harness, label, primary, Modifiers::NONE);
    server_sees(&mut harness, &robot, now != Some(true));
    // Unlocked again: dragged by its label, the child leaves the list for
    // the grid.
    harness.state_mut().locked = false;
    settle(&mut harness);
    let grid = |h: &Harness| h.state().layout.tabs[0].grid_layout.clone();
    let (widgets, children) = (
        grid(&harness).containers.len(),
        grid(&harness).layouts[0].children.len(),
    );
    let to = harness.state().grid.area.min + egui::vec2(640.0, 400.0);
    harness.hover_at(label);
    harness.step();
    harness.drag_at(label);
    harness.step();
    for step in 1..=10 {
        harness.hover_at(label + (to - label) * (step as f32 / 10.0));
        harness.step();
    }
    harness.drop_at(to);
    settle(&mut harness);
    assert_eq!(grid(&harness).layouts[0].children.len(), children - 1);
    assert_eq!(grid(&harness).containers.len(), widgets + 1);
}

#[test]
fn double_clicking_a_list_child_opens_only_its_settings() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);
    let primary = egui::PointerButton::Primary;
    harness.state_mut().locked = false;
    settle(&mut harness);
    // Double-click the child's label (its handle while unlocked): two
    // clicks a frame apart, well within egui's 0.3 s.
    let label = child_label(&harness, 0, 0).center();
    harness.hover_at(label);
    harness.step();
    for _ in 0..2 {
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: label,
                button: primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        harness.step();
    }
    settle(&mut harness);
    assert!(matches!(
        harness.state().editing,
        Some(Editing::List {
            child: Some((0, _)),
            child_only: true,
            ..
        })
    ));
    // Closing it closes the dialog: the list's settings weren't asked for.
    harness.key_press(Key::Escape);
    settle(&mut harness);
    assert!(harness.state().editing.is_none());
}
