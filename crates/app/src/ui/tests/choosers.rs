//! The Dropdown and Split Button Choosers on a SendableChooser table.

// Tests written by Claude.

use super::*;

#[test]
fn a_dropdown_chooser_sends_the_pick_and_drops_it_for_a_new_list() {
    let robot = Robot::start();
    let mut harness = chooser_harness(&robot);
    harness.state_mut().locked = true;
    settle(&mut harness);
    let card = card_on_screen(&harness, 0);
    let combo = harness.get_by_label("Auto dropdown").rect().center();
    let primary = egui::PointerButton::Primary;
    let listed = |h: &Harness, label: &str| {
        h.get_all_by_label(label)
            .any(|n| card.contains(n.rect().center()))
    };
    // Sized to the widget (256 tall): bigger than egui's usual 20-point
    // button, and the list that opens uses the same bigger text.
    let combo_height = harness.get_by_label("Auto dropdown").rect().height();
    assert!(combo_height > 40.0, "drop-down only {combo_height} tall");
    // Escape closes the open drop-down without picking anything.
    click_at(&mut harness, combo, primary, Modifiers::NONE);
    assert!(listed(&harness, "Right"), "the drop-down didn't open");
    let item = harness
        .get_all_by_label("Right")
        .map(|n| n.rect())
        .find(|r| card.contains(r.center()))
        .unwrap();
    assert!(
        item.height() > 30.0,
        "list item only {} tall",
        item.height()
    );
    press(&mut harness, Key::Escape);
    assert!(
        !listed(&harness, "Right"),
        "Escape didn't close the drop-down"
    );
    assert_eq!(robot.seen_selected.get(), None);
    // Open it again and pick Right.
    click_at(&mut harness, combo, primary, Modifiers::NONE);
    click_label_in(&mut harness, "Right", card);
    server_has_pick(&mut harness, &robot, "Right");
    // The robot takes it.
    robot.take_choice();
    settle(&mut harness);
    // The robot's list changes and Right isn't in it: the pick is dropped
    // and the robot's own choice (its default, as its active isn't an
    // option now) is sent instead.
    robot
        .auto_options
        .set(vec!["Left".into(), "Middle".into(), "Far".into()]);
    server_has_pick(&mut harness, &robot, "Middle");
    assert_eq!(harness.state().sound.played(), 0);
}

#[test]
fn split_button_choosers_send_on_the_grid_and_in_a_list() {
    let robot = Robot::start();
    let mut harness = chooser_harness(&robot);
    for locked in [true, false] {
        harness.state_mut().locked = locked;
        settle(&mut harness);
        let card = card_on_screen(&harness, 1);
        click_label_in(&mut harness, "Left", card);
        server_has_pick(&mut harness, &robot, "Left");
        click_label_in(&mut harness, "Right", card);
        server_has_pick(&mut harness, &robot, "Right");
        // The list's child, below its label.
        let app = harness.state();
        let l = &app.layout.tabs[0].grid_layout.layouts[0];
        let min = app.grid.area.min + egui::vec2(l.rect.x as f32, l.rect.y as f32);
        let list = Rect::from_min_size(min, egui::vec2(l.rect.width as f32, l.rect.height as f32));
        click_label_in(&mut harness, "Middle", list);
        server_has_pick(&mut harness, &robot, "Middle");
    }
    // Unlocked, the title bar still selects the widget.
    let card = card_on_screen(&harness, 1);
    let title = card.center_top() + egui::vec2(0.0, 12.0);
    click_at(
        &mut harness,
        title,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(harness.state().selected, Some(Item::Widget(1)));
}

#[test]
fn a_chooser_table_drags_from_the_tree_as_a_dropdown() {
    let robot = Robot::start();
    let mut harness = chooser_harness(&robot);
    harness.state_mut().locked = false;
    settle(&mut harness);
    open(&mut harness, "Test  (");
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    let to = harness.state().grid.area.min + egui::vec2(1200.0, 700.0);
    drag_row(&mut harness, "Auto", 0, to);
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.widget_type, "Dropdown Chooser");
    assert_eq!(made.topic(), Some("/Test/Auto"));
    assert!(!made.properties.contains_key("data_type"));
    // Its arrow (left of the row) opens it to show its parts and their
    // values, without dragging or picking it.
    assert!(harness.query_by_label("options").is_none());
    let arrow = row_at(&harness, "Auto", 0).left_center() - egui::vec2(10.0, 0.0);
    click_at(
        &mut harness,
        arrow,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert!(
        harness.query_by_label("options").is_some(),
        "the arrow didn't open it"
    );
    assert!(harness.query_by_label("active").is_some());
    assert_eq!(widgets(&harness).len(), before + 1);
    // With metadata shown it's still the chooser's row, and its arrow
    // lists the hidden parts too.
    harness.state_mut().tree.show_metadata = true;
    settle(&mut harness);
    assert!(
        harness.query_by_label(".type").is_some(),
        "no .type under it"
    );
    assert!(
        harness.query_by_label("Auto  (5)").is_none(),
        "shown as a folder"
    );
}

#[test]
fn clicking_a_chooser_finds_its_table_in_the_tree() {
    let robot = Robot::start();
    let mut harness = chooser_harness(&robot);
    harness.state_mut().locked = false;
    settle(&mut harness);
    // A click on its title bar selects it and picks its row in the tree.
    let card = card_on_screen(&harness, 1);
    let title = card.center_top() + egui::vec2(0.0, 12.0);
    click_at(
        &mut harness,
        title,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    assert_eq!(harness.state().selected, Some(Item::Widget(1)));
    settle(&mut harness);
    assert_eq!(
        picked(&harness),
        ["/Test/Auto"],
        "status: {}",
        harness.state().status
    );
}

/// Open widget `index`'s settings and its Widget list, and say which of
/// `names` the list offers (a name already on screen, a widget's title,
/// counts only if the list adds another).
fn offered(harness: &mut Harness, index: usize, names: &[&str]) -> Vec<String> {
    let original = harness.state().layout.tabs[0].grid_layout.containers[index].clone();
    harness.state_mut().editing = Some(Editing::Widget {
        tab: 0,
        index,
        original,
    });
    settle(harness);
    let before: Vec<usize> = names
        .iter()
        .map(|n| harness.query_all_by_label(n).count())
        .collect();
    let list = harness
        .get_by_label("How the value is shown. Settings for other types are kept.")
        .rect();
    click_at(
        harness,
        list.center(),
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    let found = names
        .iter()
        .zip(before)
        .filter(|(n, b)| harness.query_all_by_label(n).count() > *b)
        .map(|(n, _)| (*n).to_owned())
        .collect();
    harness.key_press(Key::Escape);
    settle(harness);
    found
}

#[test]
fn the_widget_list_offers_only_types_that_fit() {
    let robot = Robot::start();
    let mut harness = chooser_harness(&robot);
    harness.state_mut().locked = false;
    settle(&mut harness);
    let names = [
        "Dropdown Chooser",
        "Split Button Chooser",
        "Enum Dropdown",
        "Enum Toggle",
        "Graph",
        "Text Display",
        "Field",
    ];
    // A chooser's table: the choosers and the enum widgets, nothing for
    // single values.
    assert_eq!(
        offered(&mut harness, 1, &names),
        [
            "Dropdown Chooser",
            "Split Button Chooser",
            "Enum Dropdown",
            "Enum Toggle"
        ]
    );
    // A number: numbers' widgets, no choosers.
    harness.state_mut().layout.tabs[0]
        .grid_layout
        .containers
        .push(widget(
            "Text Display",
            "/Test/number",
            "double",
            LayoutRect {
                x: 1024.0,
                y: 0.0,
                width: 256.0,
                height: 128.0,
            },
        ));
    settle(&mut harness);
    let number = offered(&mut harness, 2, &names);
    assert!(number.contains(&"Graph".to_owned()), "{number:?}");
    assert!(
        !number.contains(&"Dropdown Chooser".to_owned()),
        "{number:?}"
    );
    assert!(!number.contains(&"Field".to_owned()), "{number:?}");
}
