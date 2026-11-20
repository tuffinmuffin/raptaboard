//! The Enum Dropdown and Enum Toggle.

// Tests written by Claude.

use super::*;

/// A layout of enum widgets, each 512 by 256 (the last 256 wide):
/// 0. an Enum Dropdown on `/Test/mode`, names from `/Test/modeOptions`;
/// 1. an Enum Toggle on the int `/Test/level`, listed as Low and High;
/// 2. an Enum Toggle on `/Test/mode` that isn't editable;
/// 3. a Split Button Chooser on `/Test/Auto` that isn't editable;
/// 4. an Enum Toggle on the chooser table `/Test/Auto`;
/// 5. a Text Display on `/Test/mode`, for changing its type;
/// 6. an Enum Toggle on `/Test/mode` switching between Stowed and Shooting.
fn enum_layout() -> ElasticLayout {
    let at = |x: f64, y: f64, w: f64| LayoutRect {
        x,
        y,
        width: w,
        height: 256.0,
    };
    let with = |mut w: WidgetContainer, props: serde_json::Value| {
        for (k, v) in props.as_object().unwrap() {
            w.properties.insert(k.clone(), v.clone());
        }
        w
    };
    // A set text size, so its list fits on the card.
    let names =
        json!({"enum_source": "topic", "options_topic": "/Test/modeOptions", "font_size": 14});
    let mut tab = Tab::new("Enums");
    tab.grid_layout.containers = vec![
        with(
            widget("Enum Dropdown", "/Test/mode", "string", at(0.0, 0.0, 512.0)),
            names.clone(),
        ),
        with(
            widget("Enum Toggle", "/Test/level", "int", at(512.0, 0.0, 512.0)),
            json!({"enum_values": ["Low", "High"]}),
        ),
        with(
            widget("Enum Toggle", "/Test/mode", "string", at(0.0, 256.0, 512.0)),
            json!({"enum_source": "topic", "options_topic": "/Test/modeOptions", "editable": false}),
        ),
        with(
            widget(
                "Split Button Chooser",
                "/Test/Auto",
                "",
                at(512.0, 256.0, 512.0),
            ),
            json!({"editable": false}),
        ),
        with(
            widget("Enum Toggle", "/Test/Auto", "", at(1024.0, 0.0, 256.0)),
            json!({"enum_source": "chooser"}),
        ),
        widget(
            "Text Display",
            "/Test/mode",
            "string",
            at(1024.0, 256.0, 256.0),
        ),
        with(
            widget("Enum Toggle", "/Test/mode", "string", at(0.0, 512.0, 512.0)),
            json!({"enum_source": "topic", "options_topic": "/Test/modeOptions",
                   "switches_between": ["Stowed", "Shooting"]}),
        ),
    ];
    for i in [3, 4] {
        tab.grid_layout.containers[i].properties.remove("data_type");
    }
    for (i, title) in [
        "Mode",
        "Level",
        "Mode (watch)",
        "Auto (watch)",
        "Auto toggle",
        "Mode text",
        "Mode switch",
    ]
    .into_iter()
    .enumerate()
    {
        tab.grid_layout.containers[i].title = title.into();
    }
    ElasticLayout {
        tabs: vec![tab],
        ..ElasticLayout::default()
    }
}

/// The harness showing [`enum_layout`], locked.
fn enum_harness(robot: &Robot) -> Harness {
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = enum_layout();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    app.locked = true;
    settle(&mut harness);
    harness
}

/// Wait until `get` gives `want`.
fn wait_for<T: PartialEq + std::fmt::Debug>(
    harness: &mut Harness,
    what: &str,
    get: impl Fn() -> Option<T>,
    want: T,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while get().as_ref() != Some(&want) {
        assert!(
            Instant::now() < deadline,
            "{what} never became {want:?}, is {:?}",
            get()
        );
        settle(harness);
    }
}

/// Wait until the drop-down titled `title` shows `want`.
fn shows_soon(harness: &mut Harness, title: &str, want: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let shown = harness.get_by_label(title).value();
        if shown.as_deref() == Some(want) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{title} never showed {want:?}, shows {shown:?}"
        );
        settle(harness);
    }
}

#[test]
fn an_enum_dropdown_sends_and_follows_the_robot() {
    let robot = Robot::start();
    let mut harness = enum_harness(&robot);
    let card = card_on_screen(&harness, 0);
    shows_soon(&mut harness, "Mode", "Stowed");
    let combo = harness.get_by_label("Mode").rect().center();
    click_at(
        &mut harness,
        combo,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    click_label_in(&mut harness, "Shooting", card);
    wait_for(
        &mut harness,
        "the server's mode",
        || robot.seen_mode.get(),
        "Shooting".into(),
    );
    // The robot changes it: the drop-down follows.
    robot.mode.set("Intaking".into());
    shows_soon(&mut harness, "Mode", "Intaking");
    // A value that isn't a state says so.
    robot.mode.set("Climbing".into());
    shows_soon(&mut harness, "Mode", "Climbing (not listed)");
    assert_eq!(harness.state().sound.played(), 0);
}

#[test]
fn an_enum_toggle_steps_through_its_states_and_follows_the_robot() {
    let robot = Robot::start();
    let mut harness = enum_harness(&robot);
    let primary = egui::PointerButton::Primary;
    let (body, _) = body_and_title(&harness, 1);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    wait_for(&mut harness, "level", || robot.seen_level.get(), 1);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    wait_for(&mut harness, "level", || robot.seen_level.get(), 0);
    // The robot moves it to High: the next click goes back to Low (0).
    robot.level.set(1);
    std::thread::sleep(Duration::from_millis(150));
    settle(&mut harness);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    wait_for(&mut harness, "level", || robot.seen_level.get(), 0);
    // On a chooser table it sends the next option to `selected`.
    let (chooser, _) = body_and_title(&harness, 4);
    click_at(&mut harness, chooser, primary, Modifiers::NONE);
    wait_for(
        &mut harness,
        "the pick",
        || robot.seen_selected.get(),
        "Right".into(),
    );
}

#[test]
fn widgets_that_arent_editable_only_show() {
    let robot = Robot::start();
    let mut harness = enum_harness(&robot);
    let primary = egui::PointerButton::Primary;
    let (toggle, _) = body_and_title(&harness, 2);
    click_at(&mut harness, toggle, primary, Modifiers::NONE);
    let card = card_on_screen(&harness, 3);
    click_label_in(&mut harness, "Left", card);
    std::thread::sleep(Duration::from_millis(200));
    settle(&mut harness);
    assert_eq!(robot.seen_mode.get(), Some("Stowed".into()));
    assert_eq!(robot.seen_selected.get(), None);
}

#[test]
fn changing_a_widget_to_an_enum_guesses_its_names() {
    let robot = Robot::start();
    let mut harness = enum_harness(&robot);
    let original = harness.state().layout.tabs[0].grid_layout.containers[5].clone();
    harness.state_mut().editing = Some(Editing::Widget {
        tab: 0,
        index: 5,
        original,
    });
    settle(&mut harness);
    // The settings' widget type, then Enum Dropdown in its list.
    let ty = harness
        .get_by_label("How the value is shown. Settings for other types are kept.")
        .rect();
    click_at(
        &mut harness,
        ty.center(),
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    harness.get_by_label("Enum Dropdown").click();
    settle(&mut harness);
    let w = &harness.state().layout.tabs[0].grid_layout.containers[5];
    assert_eq!(w.widget_type, "Enum Dropdown");
    assert_eq!(w.properties["enum_source"], "topic");
    assert_eq!(w.properties["options_topic"], "/Test/modeOptions");
}

#[test]
fn an_enum_toggle_can_switch_between_two_of_its_states() {
    let robot = Robot::start();
    let mut harness = enum_harness(&robot);
    let primary = egui::PointerButton::Primary;
    let (body, _) = body_and_title(&harness, 6);
    // Stowed to Shooting, skipping Intaking, and back.
    click_at(&mut harness, body, primary, Modifiers::NONE);
    wait_for(
        &mut harness,
        "mode",
        || robot.seen_mode.get(),
        "Shooting".into(),
    );
    click_at(&mut harness, body, primary, Modifiers::NONE);
    wait_for(
        &mut harness,
        "mode",
        || robot.seen_mode.get(),
        "Stowed".into(),
    );
    // From a state it doesn't switch between, a click goes to the first.
    robot.mode.set("Intaking".into());
    std::thread::sleep(Duration::from_millis(150));
    settle(&mut harness);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    wait_for(
        &mut harness,
        "mode",
        || robot.seen_mode.get(),
        "Stowed".into(),
    );
}

#[test]
fn a_long_topic_name_wraps_in_the_settings() {
    let robot = Robot::start();
    let mut harness = enum_harness(&robot);
    let long = "/SmartDashboard/A table with a long name for testing/and a long topic under it";
    let app = harness.state_mut();
    app.layout.tabs[0].grid_layout.containers[5]
        .properties
        .insert("topic".into(), json!(long));
    let original = app.layout.tabs[0].grid_layout.containers[5].clone();
    app.editing = Some(Editing::Widget {
        tab: 0,
        index: 5,
        original,
    });
    settle(&mut harness);
    // All of it, on more than one line, no wider than the row allows.
    let shown = harness.get_by_label(long).rect();
    assert!(shown.width() <= 321.0, "{shown:?}");
    assert!(shown.height() > 20.0, "on one line: {shown:?}");
}
