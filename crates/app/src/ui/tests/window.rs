//! The window as a whole: every widget drawn, settings dialogs, finding widgets in the tree, shortcuts, saving, tab menus.

// Tests written by Claude.

use super::*;

#[test]
fn the_window_draws_every_widget_type_with_live_values() {
    let robot = Robot::start();
    let ctx = egui::Context::default();
    let mut app = app_for(&robot);
    run_until(&ctx, &mut app, &robot, connected);
    // Values reach the widgets, and the graph records both its lines.
    let graph = app.layout.tabs[0]
        .grid_layout
        .containers
        .iter()
        .find(|w| w.widget_type == "Graph")
        .unwrap()
        .clone();
    run_until(&ctx, &mut app, &robot, |app| {
        graph::series(&graph)
            .iter()
            .all(|s| app.traces.points(&s.key).count() >= 3)
    });
    // Every widget's topic is subscribed, at the widgets' period.
    let topics = app.nt.topics();
    let wanted: Vec<String> = app
        .tab_topics(&topics)
        .into_iter()
        .flat_map(|(_, names)| names)
        .collect();
    for name in [
        "/Test/number",
        "/Test/count",
        "/Test/flag",
        "/Test/color",
        "/Test/colors",
    ] {
        assert!(
            wanted.iter().any(|w| w == name),
            "{name} not wanted: {wanted:?}"
        );
    }
    // Locked, the grid draws without resize corners or menus.
    app.locked = true;
    frame(&ctx, &mut app, vec![]);
    // An empty tab: only the Alerts widget's lists on the other tab, for
    // that tab's mark.
    app.current_tab = 1;
    frame(&ctx, &mut app, vec![]);
    let topics = app.nt.topics();
    let wanted: Vec<String> = app
        .tab_topics(&topics)
        .into_iter()
        .flat_map(|(_, n)| n)
        .collect();
    assert!(!wanted.is_empty());
    assert!(
        wanted.iter().all(|n| n.starts_with("/Test/Auto/")),
        "{wanted:?}"
    );
}

#[test]
fn every_widget_settings_dialog_opens_and_escape_cancels() {
    let robot = Robot::start();
    let ctx = egui::Context::default();
    let mut app = app_for(&robot);
    run_until(&ctx, &mut app, &robot, connected);
    let before = app.layout.clone();
    let widgets = app.layout.tabs[0].grid_layout.containers.len();
    for index in 0..widgets {
        app.selected = Some(Item::Widget(index));
        app.run_edit_action(&ctx, EditAction::Edit);
        assert!(matches!(app.editing, Some(Editing::Widget { .. })));
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, key(Key::Escape, Modifiers::NONE));
        assert!(
            app.editing.is_none(),
            "widget {index}'s dialog didn't close"
        );
    }
    // The list's dialog, and a child's settings inside it.
    app.selected = Some(Item::List(0));
    app.run_edit_action(&ctx, EditAction::Edit);
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    if let Some(Editing::List { child, .. }) = &mut app.editing {
        let first = app.layout.tabs[0].grid_layout.layouts[0].children[0].clone();
        *child = Some((0, first));
    }
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, vec![]);
    // Opened from the list's dialog, Escape goes back to it, then closes it.
    frame(&ctx, &mut app, key(Key::Escape, Modifiers::NONE));
    frame(&ctx, &mut app, vec![]);
    assert!(
        matches!(app.editing, Some(Editing::List { child: None, .. })),
        "back to the list's settings"
    );
    frame(&ctx, &mut app, key(Key::Escape, Modifiers::NONE));
    frame(&ctx, &mut app, vec![]);
    assert!(app.editing.is_none());
    // Cancelled every time: nothing changed.
    assert_eq!(app.layout, before);
}

#[test]
fn clicking_a_widget_finds_its_topic_in_the_tree() {
    let robot = Robot::start();
    let ctx = egui::Context::default();
    let mut app = app_for(&robot);
    run_until(&ctx, &mut app, &robot, connected);
    let number = app.layout.tabs[0]
        .grid_layout
        .containers
        .iter()
        .find(|w| w.topic() == Some("/Test/number"))
        .unwrap()
        .clone();
    app.find_in_tree(&number);
    assert!(app.tree.selection.not_found.is_none());
    // The tree opens the way and shows the rows with their values.
    for _ in 0..5 {
        frame(&ctx, &mut app, vec![]);
    }
    assert_eq!(app.tree.selection.selected().len(), 1);
    assert_eq!(app.tree.selection.selected()[0].name, "/Test/number");
    // A topic that's never sent can't be found, and says so.
    let missing = app.layout.tabs[0]
        .grid_layout
        .containers
        .iter()
        .find(|w| w.topic() == Some("/Test/never_sent"))
        .unwrap()
        .clone();
    app.find_in_tree(&missing);
    frame(&ctx, &mut app, vec![]);
    assert!(app.tree.selection.selected().is_empty());
    assert_eq!(app.status, "/Test/never_sent isn't published");
    // A click that finds one clears that message.
    app.find_in_tree(&number);
    assert_eq!(app.status, "");
    // Metadata rows and the filter.
    app.tree.show_metadata = true;
    app.tree.filter = "count".into();
    frame(&ctx, &mut app, vec![]);
    // Hiding the tree.
    app.tree.shown = false;
    frame(&ctx, &mut app, vec![]);
}

#[test]
fn topics_from_the_tree_menu_go_to_the_grid_a_list_or_a_new_list() {
    let robot = Robot::start();
    let ctx = egui::Context::default();
    let mut app = app_for(&robot);
    run_until(&ctx, &mut app, &robot, connected);
    let topic = |name: &str, ty: &str| DraggedTopic {
        name: name.into(),
        ty: ty.into(),
        field: None,
    };
    let picked = vec![
        topic("/Test/number", "double"),
        topic("/Test/flag", "boolean"),
    ];
    let grid = |app: &DashboardApp| app.layout.tabs[0].grid_layout.clone();
    let (widgets, lists) = (grid(&app).containers.len(), grid(&app).layouts.len());

    app.add_topics(&picked, AddTo::Grid, None);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(grid(&app).containers.len(), widgets + 2);

    let children = grid(&app).layouts[0].children.len();
    app.add_topics(&picked, AddTo::List(0), None);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(grid(&app).layouts[0].children.len(), children + 2);

    app.add_topics(&picked, AddTo::NewList, None);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(grid(&app).layouts.len(), lists + 1);
    assert_eq!(grid(&app).layouts[lists].children.len(), 2);

    // All of it undoes, one step at a time.
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![]);
        app.run_edit_action(&ctx, EditAction::Undo);
    }
    assert_eq!(grid(&app), every_widget_layout().tabs[0].grid_layout);
}

#[test]
fn keyboard_shortcuts_edit_and_switch_tabs() {
    let robot = Robot::start();
    let ctx = egui::Context::default();
    let mut app = app_for(&robot);
    run_until(&ctx, &mut app, &robot, connected);
    let widgets = app.layout.tabs[0].grid_layout.containers.len();
    // Duplicate the selected widget, then delete it, then undo the delete.
    app.selected = Some(Item::Widget(0));
    frame(&ctx, &mut app, key(Key::D, Modifiers::COMMAND));
    assert_eq!(app.layout.tabs[0].grid_layout.containers.len(), widgets + 1);
    frame(&ctx, &mut app, key(Key::Delete, Modifiers::NONE));
    assert_eq!(app.layout.tabs[0].grid_layout.containers.len(), widgets);
    frame(&ctx, &mut app, key(Key::Z, Modifiers::COMMAND));
    assert_eq!(app.layout.tabs[0].grid_layout.containers.len(), widgets + 1);
    assert!(app.unsaved());
    // Next tab.
    frame(&ctx, &mut app, key(Key::Tab, Modifiers::COMMAND));
    assert_eq!(app.current_tab, 1);
}

#[test]
fn saving_writes_the_file_and_clears_unsaved() {
    let robot = Robot::start();
    let ctx = egui::Context::default();
    let mut app = app_for(&robot);
    frame(&ctx, &mut app, vec![]);
    let folder = std::env::temp_dir().join(format!("raptaboard_save_{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join("layout.json");
    app.file.path = Some(path.clone());
    app.layout.tabs[0].name = "Renamed".into();
    assert!(app.unsaved());
    assert!(app.save(false));
    assert!(!app.unsaved());
    frame(&ctx, &mut app, vec![]);
    let saved = ElasticLayout::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved.tabs[0].name, "Renamed");
    // Opening it again shows it as it was saved.
    app.show_layout(saved, path);
    assert!(!app.unsaved());
    assert_eq!(app.layout.tabs[0].name, "Renamed");
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn right_clicking_a_tab_opens_its_menu() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    harness.state_mut().locked = false;
    settle(&mut harness);
    let tab = harness.get_by_label("Empty").rect().center();
    click_at(
        &mut harness,
        tab,
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    assert!(
        harness.query_by_label("Rename").is_some(),
        "no menu on the tab"
    );
}

#[test]
fn any_click_on_a_widget_finds_it_in_the_tree_locked_or_not() {
    let robot = Robot::start();
    let mut harness = chooser_harness(&robot);
    for locked in [true, false] {
        harness.state_mut().locked = locked;
        harness.state_mut().tree.selection.clear();
        settle(&mut harness);
        // A click on one of the chooser's buttons picks and finds it.
        let card = card_on_screen(&harness, 1);
        click_label_in(&mut harness, "Left", card);
        settle(&mut harness);
        assert_eq!(picked(&harness), ["/Test/Auto"], "locked {locked}");
        // So does a click on a list's child.
        harness.state_mut().tree.selection.clear();
        let app = harness.state();
        let l = &app.layout.tabs[0].grid_layout.layouts[0];
        let min = app.grid.area.min + egui::vec2(l.rect.x as f32, l.rect.y as f32);
        let list = Rect::from_min_size(min, egui::vec2(l.rect.width as f32, l.rect.height as f32));
        click_label_in(&mut harness, "Right", list);
        settle(&mut harness);
        assert_eq!(
            picked(&harness),
            ["/Test/Auto"],
            "list child, locked {locked}"
        );
    }
    // With hidden fields shown it's still a chooser row, found the same.
    harness.state_mut().tree.show_metadata = true;
    harness.state_mut().tree.selection.clear();
    settle(&mut harness);
    let card = card_on_screen(&harness, 1);
    click_label_in(&mut harness, "Middle", card);
    settle(&mut harness);
    assert_eq!(picked(&harness), ["/Test/Auto"], "with hidden fields shown");
    server_has_pick(&mut harness, &robot, "Middle");
}

#[test]
fn a_touchscreen_tap_finds_the_widget_tapped_not_the_one_under_the_mouse() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    harness.state_mut().locked = true;
    settle(&mut harness);
    let topic_of = |h: &Harness, i: usize| {
        h.state().layout.tabs[0].grid_layout.containers[i]
            .topic()
            .unwrap()
            .to_owned()
    };
    // The first widget is tapped; the mouse was on the fourth.
    let (tapped, mouse) = (0, 3);
    assert_ne!(topic_of(&harness, tapped), topic_of(&harness, mouse));
    let at = card_on_screen(&harness, tapped).center();
    let away = card_on_screen(&harness, mouse).center();
    tap_burst(&mut harness, at, away);
    assert_eq!(picked(&harness), [topic_of(&harness, tapped)]);
}

#[test]
fn a_tab_drags_along_the_tab_bar_to_move_it() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    let names = |h: &Harness| -> Vec<String> {
        h.state()
            .layout
            .tabs
            .iter()
            .map(|t| t.name.clone())
            .collect()
    };
    for name in ["Second", "Third"] {
        harness.state_mut().layout.tabs.push(Tab::new(name));
    }
    harness.state_mut().locked = false;
    settle(&mut harness);
    let before = names(&harness);
    let first = before[0].clone();
    // Drag the first tab to just past the last one's middle.
    let tab_at = |h: &Harness, name: &str| h.get_by_label(name).rect();
    let from = tab_at(&harness, &first).center();
    let last = tab_at(&harness, "Third");
    let to = egui::pos2(last.center().x + 4.0, last.center().y);
    let drag = |harness: &mut Harness, from: Pos2, to: Pos2| {
        harness.hover_at(from);
        harness.step();
        harness.drag_at(from);
        harness.step();
        for step in 1..=10 {
            harness.hover_at(from + (to - from) * (step as f32 / 10.0));
            harness.step();
        }
        harness.drop_at(to);
        settle(harness);
    };
    drag(&mut harness, from, to);
    let mut moved = before.clone();
    let t = moved.remove(0);
    moved.push(t);
    assert_eq!(names(&harness), moved, "the first tab went last");
    let last = harness.state().layout.tabs.len() - 1;
    assert_eq!(harness.state().current_tab, last, "and it's open");
    // One undo puts it back.
    harness.get_by_label("Undo").click();
    settle(&mut harness);
    assert_eq!(names(&harness), before);
    // Locked, a drag moves nothing.
    harness.state_mut().locked = true;
    settle(&mut harness);
    let from = tab_at(&harness, &first).center();
    let last = tab_at(&harness, "Third");
    drag(
        &mut harness,
        from,
        egui::pos2(last.center().x + 4.0, last.center().y),
    );
    assert_eq!(names(&harness), before);
}

#[test]
fn the_gear_opens_connection_settings_and_the_licenses() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    // The address is no longer in the bar, only in the Settings window.
    assert!(harness.query_by_label("Robot address").is_none());
    let gear = harness.get_by_label("⚙").rect().center();
    click_at(
        &mut harness,
        gear,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    labelled_soon(&mut harness, "Robot address");
    assert!(harness.query_by_label("Server address").is_some());
    // Still connected while the window is open.
    settle(&mut harness);
    assert!(connected(harness.state()));
    // About and licenses: each page draws, with the credits and crates.
    let about = harness.get_by_label("About and licenses…").rect().center();
    click_at(
        &mut harness,
        about,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    labelled_soon(&mut harness, "Sounds, images, fields");
    for (page, shows) in [
        ("Sounds, images, fields", "CC BY 4.0"),
        ("Rust crates", "Rust crates. Their licenses:"),
        ("License texts", "Copy all"),
    ] {
        let tab = harness.get_by_label(page).rect().center();
        click_at(
            &mut harness,
            tab,
            egui::PointerButton::Primary,
            Modifiers::NONE,
        );
        settle(&mut harness);
        assert!(
            harness.query_by_label_contains(shows).is_some(),
            "{page} doesn't show {shows:?}"
        );
    }
    assert!(harness.state().gear.about && harness.state().gear.settings);
}
