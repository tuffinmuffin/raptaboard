//! The Number Slider and typed values (an editable Text Display).

// Tests written by Claude.

use super::*;

/// A layout of Number Sliders on `/Test/setpoint`, from 0 to 10: one that
/// sends when let go, one that sends all the way (`publish_all`), each 512
/// by 256 along the top, and a List Layout holding one below them.
fn slider_layout() -> ElasticLayout {
    let at = |i: usize| LayoutRect {
        x: i as f64 * 512.0,
        y: 0.0,
        width: 512.0,
        height: 256.0,
    };
    let slider = |rect| {
        let mut w = widget("Number Slider", "/Test/setpoint", "double", rect);
        w.properties.insert("min_value".into(), json!(0.0));
        w.properties.insert("max_value".into(), json!(10.0));
        w
    };
    let mut tab = Tab::new("Sliders");
    let grid = &mut tab.grid_layout;
    grid.containers = vec![slider(at(0)), slider(at(1))];
    grid.containers[1]
        .properties
        .insert("publish_all".into(), json!(true));
    grid.layouts.push(list_layout::new_list(
        LayoutRect {
            x: 0.0,
            y: 256.0,
            width: 512.0,
            height: 256.0,
        },
        vec![slider(at(0))],
    ));
    // Labels inside, over the top of each child, as the sim layout has.
    grid.layouts[0]
        .properties
        .insert(list_layout::LABEL_INSIDE.into(), json!(true));
    ElasticLayout {
        tabs: vec![tab],
        ..ElasticLayout::default()
    }
}

/// The harness showing [`slider_layout`].
fn slider_harness(robot: &Robot) -> Harness {
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = slider_layout();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    harness
}

/// Where slider widget `index`'s track is on screen.
fn slider_track(harness: &Harness, index: usize) -> Rect {
    let props = &harness.state().layout.tabs[0].grid_layout.containers[index].properties;
    widgets::builtin::number_slider::track_on_card(card_on_screen(harness, index), props)
}

/// The point `fraction` of the way along `track`.
fn along(track: Rect, fraction: f32) -> Pos2 {
    egui::pos2(track.min.x + track.width() * fraction, track.center().y)
}

/// Wait until the server has `want` for `/Test/setpoint`.
fn server_has_setpoint(harness: &mut Harness, robot: &Robot, want: f64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while robot
        .seen_setpoint
        .get()
        .is_none_or(|v| (v - want).abs() > 1e-6)
    {
        assert!(
            Instant::now() < deadline,
            "the server never got {want}, has {:?}",
            robot.seen_setpoint.get()
        );
        settle(harness);
    }
}

/// Press at `from`, move to each of `path` a frame apart, then hold at the
/// last point (without letting go).
fn press_and_move(harness: &mut Harness, from: Pos2, path: &[Pos2]) {
    harness.hover_at(from);
    harness.step();
    harness.drag_at(from);
    harness.step();
    for &to in path {
        harness.hover_at(to);
        harness.step();
    }
}

#[test]
fn a_slider_sends_where_it_is_let_go_even_outside_the_widget() {
    let robot = Robot::start();
    let mut harness = slider_harness(&robot);
    let primary = egui::PointerButton::Primary;
    for locked in [true, false] {
        harness.state_mut().locked = locked;
        settle(&mut harness);
        // A click sets the value under the pointer.
        let track = slider_track(&harness, 0);
        click_at(&mut harness, along(track, 0.3), primary, Modifiers::NONE);
        server_has_setpoint(&mut harness, &robot, 3.0);
        // A drag sends nothing until it's let go...
        let path: Vec<Pos2> = (1..=6)
            .map(|i| along(track, 0.3 + 0.05 * i as f32))
            .collect();
        press_and_move(&mut harness, along(track, 0.3), &path);
        std::thread::sleep(Duration::from_millis(150));
        settle(&mut harness);
        assert_eq!(robot.seen_setpoint.get(), Some(3.0));
        // ...and keeps going when the pointer leaves the widget: past the
        // right end (and below the card) is the maximum.
        let outside = track.right_bottom() + egui::vec2(200.0, 300.0);
        harness.hover_at(outside);
        harness.step();
        harness.drop_at(outside);
        server_has_setpoint(&mut harness, &robot, 10.0);
        // Then the robot's own change shows and wins: a click back sends
        // a new value from there.
        robot.setpoint.set(5.0);
        server_has_setpoint(&mut harness, &robot, 5.0);
        click_at(&mut harness, along(track, 0.0), primary, Modifiers::NONE);
        server_has_setpoint(&mut harness, &robot, 0.0);
    }
    // The drags didn't move or resize the widget, locked or not.
    let r = harness.state().layout.tabs[0].grid_layout.containers[0].rect;
    assert_eq!((r.x, r.y, r.width), (0.0, 0.0, 512.0));
    assert_eq!(harness.state().sound.played(), 0);
}

#[test]
fn publish_all_sends_during_the_drag() {
    let robot = Robot::start();
    let mut harness = slider_harness(&robot);
    harness.state_mut().locked = true;
    settle(&mut harness);
    let track = slider_track(&harness, 1);
    press_and_move(
        &mut harness,
        along(track, 0.1),
        &[along(track, 0.4), along(track, 0.6)],
    );
    // Still held down, the server already has it.
    server_has_setpoint(&mut harness, &robot, 6.0);
    harness.drop_at(along(track, 0.6));
    settle(&mut harness);
    assert_eq!(robot.seen_setpoint.get(), Some(6.0));
}

#[test]
fn a_slider_works_inside_a_list() {
    let robot = Robot::start();
    let mut harness = slider_harness(&robot);
    let primary = egui::PointerButton::Primary;
    harness.state_mut().locked = true;
    settle(&mut harness);
    // The child's row: its slider's track runs across the row less the
    // padding, so the row's middle is the middle of the range.
    let middle = child_middle(&harness, 0, 0);
    click_at(&mut harness, middle, primary, Modifiers::NONE);
    // Waited for: on a slow machine (CI's Ubuntu) the value can still be on
    // its way when the click's frames are done.
    let deadline = Instant::now() + Duration::from_secs(10);
    while robot
        .seen_setpoint
        .get()
        .is_none_or(|v| (v - 5.0).abs() > 1.0)
    {
        assert!(
            Instant::now() < deadline,
            "the list's slider never sent the middle of its range: {:?}",
            robot.seen_setpoint.get()
        );
        settle(&mut harness);
    }
}

/// Type `text` into whatever has the keyboard.
fn type_text(harness: &mut Harness, text: &str) {
    harness.event(Event::Text(text.into()));
    harness.step();
}

#[test]
fn clicking_a_sliders_value_types_one_in() {
    let robot = Robot::start();
    let mut harness = slider_harness(&robot);
    let primary = egui::PointerButton::Primary;
    harness.state_mut().locked = true;
    settle(&mut harness);
    let value = widgets::builtin::number_slider::value_on_card(card_on_screen(&harness, 0));
    let wants_keys = |h: &Harness| h.ctx.egui_wants_keyboard_input();
    // A click on the value opens the box, with the value selected:
    // typing replaces it, and Enter sends it.
    click_at(&mut harness, value, primary, Modifiers::NONE);
    assert!(wants_keys(&harness), "the box didn't open");
    // Opening it sends nothing: the press is the value's, not the
    // slider's (which would jump to the middle, under the value).
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_setpoint.get(), Some(0.0));
    // Clicking inside the open box doesn't move the slider either.
    click_at(
        &mut harness,
        value + egui::vec2(20.0, 0.0),
        primary,
        Modifiers::NONE,
    );
    assert!(wants_keys(&harness), "a click in the box closed it");
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_setpoint.get(), Some(0.0));
    // Select it all again for typing over.
    harness.event(Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.step();
    type_text(&mut harness, "7.5");
    press(&mut harness, Key::Enter);
    server_has_setpoint(&mut harness, &robot, 7.5);
    assert!(!wants_keys(&harness));
    // Not a number: nothing is sent and the box stays open to fix it...
    click_at(&mut harness, value, primary, Modifiers::NONE);
    type_text(&mut harness, "fast");
    press(&mut harness, Key::Enter);
    assert!(wants_keys(&harness), "the box closed on a bad value");
    // ...and Escape closes it, still sending nothing.
    press(&mut harness, Key::Escape);
    assert!(!wants_keys(&harness));
    // A click somewhere else closes it unsent too.
    click_at(&mut harness, value, primary, Modifiers::NONE);
    type_text(&mut harness, "2");
    let empty = harness.state().grid.area.min + egui::vec2(1400.0, 900.0);
    click_at(&mut harness, empty, primary, Modifiers::NONE);
    assert!(!wants_keys(&harness));
    // Past the range goes to its end.
    click_at(&mut harness, value, primary, Modifiers::NONE);
    type_text(&mut harness, "250");
    press(&mut harness, Key::Enter);
    server_has_setpoint(&mut harness, &robot, 10.0);
    // Wait out the widget's hold, then check nothing else was sent.
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_setpoint.get(), Some(10.0));
    // Unlocked the value still opens the box (it's in the body).
    harness.state_mut().locked = false;
    settle(&mut harness);
    click_at(&mut harness, value, primary, Modifiers::NONE);
    assert!(wants_keys(&harness));
    press(&mut harness, Key::Escape);
    assert_eq!(harness.state().selected, None);
}

#[test]
fn escape_cancels_a_slider_drag() {
    let robot = Robot::start();
    let mut harness = slider_harness(&robot);
    harness.state_mut().locked = true;
    settle(&mut harness);
    // Sent on release: Escape mid-drag, and letting go sends nothing.
    let track = slider_track(&harness, 0);
    press_and_move(&mut harness, along(track, 0.2), &[along(track, 0.8)]);
    press(&mut harness, Key::Escape);
    harness.hover_at(along(track, 0.9));
    harness.step();
    harness.drop_at(along(track, 0.9));
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_setpoint.get(), Some(0.0));
    // With publish_all the drag already sent values: Escape sends back
    // the value from before it.
    let track = slider_track(&harness, 1);
    press_and_move(&mut harness, along(track, 0.2), &[along(track, 0.7)]);
    server_has_setpoint(&mut harness, &robot, 7.0);
    press(&mut harness, Key::Escape);
    server_has_setpoint(&mut harness, &robot, 0.0);
    harness.drop_at(along(track, 0.7));
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_setpoint.get(), Some(0.0));
    // After a cancel, the next drag works as usual.
    let track = slider_track(&harness, 0);
    click_at(
        &mut harness,
        along(track, 0.4),
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    server_has_setpoint(&mut harness, &robot, 4.0);
}

#[test]
fn a_list_sliders_whole_value_opens_the_box() {
    let robot = Robot::start();
    let mut harness = slider_harness(&robot);
    let primary = egui::PointerButton::Primary;
    harness.state_mut().locked = false;
    settle(&mut harness);
    // The child's value is written just below its inside label.
    let app = harness.state();
    let l = &app.layout.tabs[0].grid_layout.layouts[0];
    let min = app.grid.area.min + egui::vec2(l.rect.x as f32, l.rect.y as f32);
    let card = Rect::from_min_size(min, egui::vec2(l.rect.width as f32, l.rect.height as f32))
        .shrink(crate::ui::style::CELL_GAP);
    let row = list_layout::rows_on_screen(card, l)[0];
    let label = row.label.expect("the child has a label");
    let top = label.max.y;
    // The top and bottom of the number, a little off its middle.
    for y in [top + 3.0, top + 20.0] {
        let at = egui::pos2(row.content.center().x + 10.0, y);
        click_at(&mut harness, at, primary, Modifiers::NONE);
        assert!(
            harness.ctx.egui_wants_keyboard_input(),
            "a click {:.0} below the label didn't open the box",
            y - top
        );
        assert_eq!(harness.state().selected, None, "the list took the click");
        press(&mut harness, Key::Escape);
    }
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_setpoint.get(), Some(0.0));
}

#[test]
fn an_editable_text_display_takes_typed_values() {
    let robot = Robot::start();
    let mut harness = writing_harness(&robot);

    let primary = egui::PointerButton::Primary;
    let wants_keys = |h: &Harness| h.ctx.egui_wants_keyboard_input();
    harness.state_mut().locked = true;
    settle(&mut harness);
    // A number: the box opens with the value selected; Enter sends.
    let (body, title) = body_and_title(&harness, 4);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    assert!(wants_keys(&harness), "the box didn't open");
    type_text(&mut harness, "2.5");
    press(&mut harness, Key::Enter);
    server_has_setpoint(&mut harness, &robot, 2.5);
    // Not a number: not sent, the box stays open; Escape closes it.
    click_at(&mut harness, body, primary, Modifiers::NONE);
    type_text(&mut harness, "fast");
    press(&mut harness, Key::Enter);
    assert!(wants_keys(&harness), "the box closed on a bad value");
    press(&mut harness, Key::Escape);
    assert!(!wants_keys(&harness));
    assert_eq!(harness.state().sound.played(), 0);
    // A boolean takes true or false.
    let (flag, _) = body_and_title(&harness, 5);
    click_at(&mut harness, flag, primary, Modifiers::NONE);
    type_text(&mut harness, "TRUE");
    press(&mut harness, Key::Enter);
    server_sees(&mut harness, &robot, true);
    // One that isn't editable doesn't open a box.
    let (plain, _) = body_and_title(&harness, 6);
    click_at(&mut harness, plain, primary, Modifiers::NONE);
    assert!(!wants_keys(&harness));
    // Unlocked, the title bar selects it; the body still opens the box.
    harness.state_mut().locked = false;
    settle(&mut harness);
    let grid_at = harness.state().grid.area.min;
    click_at(&mut harness, title, primary, Modifiers::NONE);
    assert!(!wants_keys(&harness));
    assert_eq!(harness.state().selected, Some(Item::Widget(4)));
    // Selecting it opened its folder in the tree, whose rows are wider
    // than the tree: the tree scrolls rather than pushing the grid over.
    assert_eq!(harness.state().grid.area.min, grid_at, "the grid moved");
    click_at(&mut harness, body, primary, Modifiers::NONE);
    assert!(wants_keys(&harness));
    type_text(&mut harness, "-1");
    press(&mut harness, Key::Enter);
    server_has_setpoint(&mut harness, &robot, -1.0);
    assert!(harness.state().sends.failed.is_empty());
}
