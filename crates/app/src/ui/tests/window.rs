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
    // Clicked through accesskit, so it doesn't depend on where the button
    // lands (macOS CI missed a pointer click on it).
    harness.get_by_label("⚙").click();
    settle(&mut harness);
    assert!(
        harness.state().gear.settings,
        "the gear didn't open Settings"
    );
    labelled_soon(&mut harness, "Robot address");
    assert!(harness.query_by_label("Server address").is_some());
    // Still connected while the window is open.
    settle(&mut harness);
    assert!(connected(harness.state()));
    // About and licenses: each page draws, with the credits and crates.
    harness.get_by_label("About and licenses…").click();
    settle(&mut harness);
    labelled_soon(&mut harness, "Sounds, images, fields");
    for (page, shows) in [
        ("Sounds, images, fields", "Sarah-Christin Freytag"),
        ("Privacy", "We collect no data"),
        ("Rust crates", "Rust crates. Their licenses:"),
        ("License texts", "Copy all"),
    ] {
        harness.get_by_label(page).click();
        settle(&mut harness);
        assert!(
            harness.query_by_label_contains(shows).is_some(),
            "{page} doesn't show {shows:?}"
        );
    }
    assert!(harness.state().gear.about && harness.state().gear.settings);
}

#[test]
fn the_driver_station_choice_connects_where_the_driver_station_says() {
    use std::io::Write;
    let robot = Robot::start();
    let ctx = egui::Context::default();
    let mut app = app_for(&robot);
    // An NI Driver Station on this computer that knows the robot is at
    // 127.0.0.1 (2130706433) once asked a second time.
    let ni = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let ni_at = ni.local_addr().unwrap();
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let (tell, told) = std::sync::mpsc::channel::<()>();
    let ds = std::thread::spawn(move || {
        let (mut stream, _) = ni.accept().unwrap();
        stream.write_all(b"{\"robotIP\":0}\n").unwrap();
        told.recv().unwrap();
        stream.write_all(b"{\"robotIP\":2130706433}\n").unwrap();
        let _ = told.recv();
    });
    app.ds_at = Some((format!("ws://{closed}/ipws"), ni_at));
    app.address.mode = AddressMode::DriverStation;
    for _ in 0..30 {
        frame(&ctx, &mut app, vec![]);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app.ds.is_some());
    assert_eq!(app.nt.status(), ConnectionStatus::NoTarget);
    tell.send(()).unwrap();
    run_until(&ctx, &mut app, &robot, connected);
    // Another choice stops listening to the Driver Station.
    app.address.mode = AddressMode::Custom;
    frame(&ctx, &mut app, vec![]);
    assert!(app.ds.is_none());
    drop(tell);
    ds.join().unwrap();
}

#[test]
fn the_right_click_menus_pin_fill_and_pack() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    let area = harness.state().grid.area.min;
    let grid = |h: &Harness| h.state().layout.tabs[0].grid_layout.clone();
    // Pin the first widget from its menu.
    let first = area + egui::vec2(128.0, 160.0);
    click_at(
        &mut harness,
        first,
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    harness.get_by_label("Pin in place").click();
    settle(&mut harness);
    assert!(crate::grid::Item::Widget(0).pinned(&grid(&harness)));
    // A checkbox leaves the menu open; a click on empty grid closes it.
    // (Past the fifth column of 256-point cells, inside the window.)
    let empty = area + egui::vec2(1350.0, 100.0);
    click_at(
        &mut harness,
        empty,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    // Take the second widget away, and pack: the third slides left into
    // the gap; the pinned first stays.
    let third_before = grid(&harness).containers[2].rect;
    harness.state_mut().layout.tabs[0]
        .grid_layout
        .containers
        .remove(1);
    settle(&mut harness);
    // Right-click empty grid, past the last column of widgets.
    click_at(
        &mut harness,
        empty,
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    harness.get_by_label("Pack widgets").click();
    settle(&mut harness);
    let after = grid(&harness);
    assert_eq!(after.containers[0].rect.x, 0.0, "pinned stays");
    assert!(
        after.containers[1].rect.x < third_before.x,
        "slid into the gap"
    );
    assert!(
        harness.state().status.starts_with("Packed"),
        "{}",
        harness.state().status
    );
    // Fill space from the first widget's menu: nothing beside it is free
    // after packing, so it says so.
    click_at(
        &mut harness,
        first,
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    harness.get_by_label("Fill space").click();
    settle(&mut harness);
    assert!(!harness.state().status.is_empty());
}

#[test]
fn new_group_from_the_empty_grids_menu() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    // The empty tab: room for a group.
    harness.state_mut().current_tab = 1;
    settle(&mut harness);
    let area = harness.state().grid.area.min;
    click_at(
        &mut harness,
        area + egui::vec2(300.0, 200.0),
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
    harness.get_by_label("New Group").click();
    settle(&mut harness);
    let layouts = &harness.state().layout.tabs[1].grid_layout.layouts;
    assert_eq!(layouts.len(), 1);
    assert!(crate::widgets::group::is_group(&layouts[0]));
    // Near where it was asked for: the cell under the click.
    assert_eq!((layouts[0].rect.x, layouts[0].rect.y), (256.0, 128.0));
    assert_eq!(harness.state().selected, Some(crate::grid::Item::List(0)));
}

#[test]
fn a_groups_shared_period_reaches_the_widgets_inside_unless_they_opt_out() {
    use crate::widgets::{for_topic, group};
    let robot = Robot::start();
    let mut app = app_for(&robot);
    let cell = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: group::INNER_CELL,
        height: group::INNER_CELL,
    };
    let mut g = group::new_group(LayoutRect {
        x: 0.0,
        y: 2048.0,
        width: 512.0,
        height: 256.0,
    });
    // One at its type's usual period and one with its own: the group's
    // applies to both. One not using the shared settings: its own.
    let usual = for_topic("/Test/count", "int", None, cell).unwrap();
    let mut own = for_topic(
        "/Test/number",
        "double",
        None,
        LayoutRect { x: 128.0, ..cell },
    )
    .unwrap();
    own.properties.insert("period".into(), 0.2.into());
    let mut apart = for_topic(
        "/Test/flag",
        "boolean",
        None,
        LayoutRect { x: 256.0, ..cell },
    )
    .unwrap();
    apart.properties.insert("period".into(), 0.2.into());
    apart
        .properties
        .insert(group::USE_SHARED.into(), false.into());
    g.children.extend([usual, own, apart]);
    g.properties
        .insert(group::SHARED.into(), serde_json::json!({"period": 0.75}));
    app.layout.tabs[0].grid_layout.layouts.push(g);
    let wanted = app.tab_topics(&[]);
    let at = |period: f64| {
        wanted
            .iter()
            .find(|(p, _)| *p == period)
            .map(|(_, names)| names.clone())
            .unwrap_or_default()
    };
    assert!(at(0.75).contains(&"/Test/count".to_owned()), "{wanted:?}");
    assert!(at(0.75).contains(&"/Test/number".to_owned()), "{wanted:?}");
    assert!(!at(0.2).contains(&"/Test/number".to_owned()), "{wanted:?}");
    assert!(at(0.2).contains(&"/Test/flag".to_owned()), "{wanted:?}");
}

#[test]
fn a_widget_in_a_group_shows_what_the_group_sets_greyed_until_it_opts_out() {
    use crate::widgets::{for_topic, group};
    use egui_kittest::kittest::NodeT;
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    let cell = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: group::INNER_CELL,
        height: group::INNER_CELL,
    };
    let mut g = group::new_group(LayoutRect {
        x: 0.0,
        y: 2048.0,
        width: 512.0,
        height: 256.0,
    });
    // Its own text size, which the group's replaces.
    let mut child = for_topic("/Test/number", "double", None, cell).unwrap();
    child.properties.insert("font_size".into(), 20.0.into());
    g.children.push(child.clone());
    g.properties
        .insert(group::SHARED.into(), serde_json::json!({"font_size": 30.0}));
    let app = harness.state_mut();
    let tab = &mut app.layout.tabs[0].grid_layout.layouts;
    tab.push(g.clone());
    let index = tab.len() - 1;
    app.editing = Some(Editing::List {
        tab: 0,
        index,
        original: Box::new(g),
        child: Some((0, child)),
        child_only: true,
    });
    settle(&mut harness);
    // Resets: the title's (it has the topic's name) and the text size's
    // (its own, but the group sets it): both off.
    let any_reset_on = |h: &Harness| {
        h.query_all_by_label("Reset")
            .any(|r| !r.accesskit_node().is_disabled())
    };
    assert!(
        harness
            .query_by_label_contains("Set by the group: text size 30 pt")
            .is_some()
    );
    assert!(!any_reset_on(&harness));
    harness.get_by_label("Use the shared settings").click();
    settle(&mut harness);
    assert!(any_reset_on(&harness), "its own text size can be reset");
    let saved = &harness.state().layout.tabs[0].grid_layout.layouts[index].children[0];
    assert_eq!(saved.properties[group::USE_SHARED], false);
}
