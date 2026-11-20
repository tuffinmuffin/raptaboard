//! Whole-window tests: the app, headless, against a real NT server on
//! localhost, with one widget of every type on the grid.

// Tests written by Claude.

use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use eframe::egui::{self, Event, Key, Modifiers, Pos2, RawInput, Rect};
use egui_kittest::kittest::Queryable;
use nt4::{Publisher, Server, ServerConfig};
use raptaboard_layout::{ElasticLayout, Rect as LayoutRect, Tab, WidgetContainer};
use serde_json::{Map, json};

use super::DashboardApp;
use super::dialogs::Editing;
use super::grid_view::Sender;
use crate::cli::StartupOptions;
use crate::connection::AddressMode;
use crate::edit::{EditAction, History};
use crate::grid::Item;
use crate::nt::ConnectionStatus;
use crate::tree::{AddTo, DraggedTopic};
use crate::widgets::{self, builtin::graph, list_layout};

/// The test topics: name and NT type. Each widget type shows the first one
/// it can show.
const TOPICS: &[(&str, &str)] = &[
    ("/Test/number", "double"),
    ("/Test/count", "int"),
    ("/Test/flag", "boolean"),
    ("/Test/color", "string"),
    ("/Test/colors", "string[]"),
    ("/Test/numbers", "double[]"),
];

/// A server publishing [`TOPICS`], on its own runtime as robot code would
/// be its own process.
struct Robot {
    _rt: tokio::runtime::Runtime,
    _server: Server,
    port: u16,
    number: Publisher<f64>,
    count: Publisher<i64>,
    flag: Publisher<bool>,
    /// A boolean only the tests and the dashboard change, and what the
    /// server has of it.
    enabled: Publisher<bool>,
    seen_enabled: nt4::Subscriber<bool>,
    /// A number only the tests and the dashboard change, for sliders, and
    /// what the server has of it.
    setpoint: Publisher<f64>,
    seen_setpoint: nt4::Subscriber<f64>,
    _others: (
        Publisher<String>,
        Publisher<Vec<String>>,
        Publisher<Vec<f64>>,
    ),
    _structs: (nt4::GenericPublisher, nt4::GenericPublisher),
}

impl Robot {
    fn start() -> Robot {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let server = rt
            .block_on(Server::start(ServerConfig {
                listen_address: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 0,
                persist_file: None,
            }))
            .unwrap();
        let port = server.local_addr().port();
        let number = server.publish::<f64>("/Test/number");
        number.set(0.25);
        let count = server.publish::<i64>("/Test/count");
        count.set(3);
        let flag = server.publish::<bool>("/Test/flag");
        flag.set(true);
        let enabled = server.publish::<bool>("/Test/enabled");
        enabled.set(false);
        let seen_enabled = server.subscribe::<bool>("/Test/enabled", Default::default());
        let setpoint = server.publish::<f64>("/Test/setpoint");
        setpoint.set(0.0);
        let seen_setpoint = server.subscribe::<f64>("/Test/setpoint", Default::default());
        let color = server.publish::<String>("/Test/color");
        color.set("#FF8800".into());
        let colors = server.publish::<Vec<String>>("/Test/colors");
        colors.set(vec!["#FF0000".into(), "#00FF00".into()]);
        let numbers = server.publish::<Vec<f64>>("/Test/numbers");
        numbers.set(vec![1.0, 2.0]);
        // Structs as robot code sends them: schemas, then bytes. A Pose2d
        // has nested structs; the swerve states are a struct array.
        server.add_schema("struct:Translation2d", "structschema", b"double x;double y");
        server.add_schema("struct:Rotation2d", "structschema", b"double value");
        server.add_schema(
            "struct:Pose2d",
            "structschema",
            b"Translation2d translation;Rotation2d rotation",
        );
        server.add_schema(
            "struct:SwerveModuleState",
            "structschema",
            b"double speed;Rotation2d angle",
        );
        let doubles = |values: &[f64]| -> nt4::Value {
            nt4::Value::Raw(values.iter().flat_map(|v| v.to_le_bytes()).collect())
        };
        let pose = server.publish_generic(
            "/Drive/pose",
            "struct:Pose2d".parse().unwrap(),
            nt4::Properties::new(),
        );
        pose.set(doubles(&[1.5, 2.5, 0.75])).unwrap();
        let modules = server.publish_generic(
            "/Drive/modules",
            "struct:SwerveModuleState[]".parse().unwrap(),
            nt4::Properties::new(),
        );
        modules.set(doubles(&[3.0, 0.1, 4.0, 0.2])).unwrap();
        Robot {
            _rt: rt,
            _server: server,
            port,
            number,
            count,
            flag,
            enabled,
            seen_enabled,
            setpoint,
            seen_setpoint,
            _others: (color, colors, numbers),
            _structs: (pose, modules),
        }
    }

    /// New values, different each step, so widgets and graphs change.
    fn step(&self, i: u32) {
        self.number.set(f64::from(i % 20) / 20.0);
        self.count.set(i64::from(i));
        self.flag.set(i.is_multiple_of(2));
    }
}

/// A widget of type `type_name` showing `topic`, at `rect`.
fn widget(type_name: &str, topic: &str, nt_type: &str, rect: LayoutRect) -> WidgetContainer {
    let mut properties = Map::new();
    properties.insert("topic".into(), json!(topic));
    properties.insert("period".into(), json!(0.02));
    properties.insert("data_type".into(), json!(nt_type));
    WidgetContainer {
        title: type_name.into(),
        rect,
        widget_type: type_name.into(),
        properties,
        extra: Map::new(),
    }
}

/// One tab with a widget of every type (a graph with two lines), a List
/// Layout holding two more, and a widget for a topic that's never sent.
fn every_widget_layout() -> ElasticLayout {
    let cell = 256.0;
    let at = |i: usize| LayoutRect {
        x: (i % 5) as f64 * cell,
        y: (i / 5) as f64 * cell,
        width: cell,
        height: cell,
    };
    let mut tab = Tab::new("Widgets");
    let grid = &mut tab.grid_layout;
    for (i, ty) in widgets::TYPES.iter().enumerate() {
        let (topic, nt_type) = TOPICS
            .iter()
            .find(|(_, t)| ty.shows(t))
            .expect("every type shows a test topic");
        let mut w = widget(ty.names()[0], topic, nt_type, at(i));
        if w.widget_type == "Graph" {
            graph::add_series(&mut w, "/Test/count", "int", None);
        }
        grid.containers.push(w);
    }
    let n = widgets::TYPES.len();
    grid.containers
        .push(widget("Text Display", "/Test/never_sent", "string", at(n)));
    let list_rect = LayoutRect {
        height: 2.0 * cell,
        ..at(n + 1)
    };
    let one = at(0);
    grid.layouts.push(list_layout::new_list(
        list_rect,
        vec![
            widget("Text Display", "/Test/count", "int", one),
            widget("Boolean Box", "/Test/flag", "boolean", one),
        ],
    ));
    ElasticLayout {
        tabs: vec![tab, Tab::new("Empty")],
        ..ElasticLayout::default()
    }
}

/// The app showing [`every_widget_layout`], unlocked, connected to
/// `robot`, with nowhere to save settings.
fn app_for(robot: &Robot) -> DashboardApp {
    let mut app = DashboardApp::new(StartupOptions::default()).expect("app starts");
    app.layout = every_widget_layout();
    app.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.alert = crate::alert::Alert::silent();
    app.current_tab = 0;
    app.locked = false;
    app.mode = AddressMode::Custom;
    app.custom_address = "127.0.0.1".into();
    app.port = robot.port;
    app
}

/// One frame of the whole window, with `events`.
fn frame(ctx: &egui::Context, app: &mut DashboardApp, events: Vec<Event>) {
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1800.0, 1300.0))),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(raw, |ui| app.show(ui));
    output.textures_delta.clear();
}

/// Run frames, stepping the robot's values, until `done` or 10 s.
fn run_until(
    ctx: &egui::Context,
    app: &mut DashboardApp,
    robot: &Robot,
    done: impl Fn(&DashboardApp) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut i = 0;
    while !done(app) {
        assert!(
            Instant::now() < deadline,
            "timed out; status {:?}",
            app.nt.status()
        );
        robot.step(i);
        i += 1;
        frame(ctx, app, vec![]);
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn connected(app: &DashboardApp) -> bool {
    matches!(app.nt.status(), ConnectionStatus::Connected { topics, .. } if topics >= TOPICS.len())
}

fn key(key: Key, modifiers: Modifiers) -> Vec<Event> {
    [true, false]
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .into()
}

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
    let wanted: Vec<String> = app
        .tab_topics()
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
    // An empty tab, then back.
    app.current_tab = 1;
    frame(&ctx, &mut app, vec![]);
    assert!(app.tab_topics().is_empty());
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
    assert!(app.tree_sel.not_found.is_none());
    // The tree opens the way and shows the rows with their values.
    for _ in 0..5 {
        frame(&ctx, &mut app, vec![]);
    }
    assert_eq!(app.tree_sel.selected().len(), 1);
    assert_eq!(app.tree_sel.selected()[0].name, "/Test/number");
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
    assert!(app.tree_sel.selected().is_empty());
    assert_eq!(app.status, "/Test/never_sent isn't published");
    // A click that finds one clears that message.
    app.find_in_tree(&number);
    assert_eq!(app.status, "");
    // Metadata rows and the filter.
    app.show_metadata = true;
    app.tree_filter = "count".into();
    frame(&ctx, &mut app, vec![]);
    // Hiding the tree.
    app.show_tree = false;
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

    app.add_topics(&picked, AddTo::Grid);
    frame(&ctx, &mut app, vec![]);
    assert_eq!(grid(&app).containers.len(), widgets + 2);

    let children = grid(&app).layouts[0].children.len();
    app.add_topics(&picked, AddTo::List(0));
    frame(&ctx, &mut app, vec![]);
    assert_eq!(grid(&app).layouts[0].children.len(), children + 2);

    app.add_topics(&picked, AddTo::NewList);
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
    app.layout_path = Some(path.clone());
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

/// The whole window in an egui_kittest harness, so tree rows can be found
/// by their labels and clicked or dragged where they're drawn.
type Harness = egui_kittest::Harness<'static, DashboardApp>;

fn harness_for(robot: &Robot) -> Harness {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1800.0, 1300.0))
        // Symbols such as ◀ come from the computer's own fonts in the app
        // (eframe's system_fonts); the harness has only egui's built-in ones.
        .allow_missing_glyphs()
        // Frames as the app draws them (about 60 a second), so egui's
        // timing (double-clicks within 0.3 s) works as it does for real.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut DashboardApp| app.show(ui), app_for(robot));
    let deadline = Instant::now() + Duration::from_secs(10);
    // Connected, with the struct schemas in.
    while !(connected(harness.state())
        && harness
            .state()
            .nt
            .structs()
            .is_some_and(|db| db.find("Pose2d").is_some()))
    {
        assert!(Instant::now() < deadline, "never connected");
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    harness
}

/// The tree panel's right edge: it starts 300 points wide.
const TREE_RIGHT: f32 = 300.0;

/// Where the `nth` tree row labelled exactly `label` is, top to bottom.
/// Rows in a folder are in name order, so `/Drive/modules` comes before
/// `/Drive/pose`.
fn row_at(harness: &Harness, label: &str, nth: usize) -> Rect {
    let mut rows: Vec<Rect> = harness
        .get_all_by_label(label)
        .map(|n| n.rect())
        .filter(|r| r.min.x < TREE_RIGHT)
        .collect();
    rows.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    *rows
        .get(nth)
        .unwrap_or_else(|| panic!("no tree row {nth} labelled {label:?}: {rows:?}"))
}

/// Click the `nth` tree row labelled `label`, holding `modifiers`.
fn click_row(harness: &mut Harness, label: &str, nth: usize, modifiers: Modifiers) {
    let pos = row_at(harness, label, nth).center();
    click_at(harness, pos, egui::PointerButton::Primary, modifiers);
}

/// Right-click the `nth` tree row labelled `label`.
fn right_click_row(harness: &mut Harness, label: &str, nth: usize) {
    let pos = row_at(harness, label, nth).center();
    click_at(
        harness,
        pos,
        egui::PointerButton::Secondary,
        Modifiers::NONE,
    );
}

/// Click at `pos` with `button`, holding `modifiers`.
fn click_at(harness: &mut Harness, pos: Pos2, button: egui::PointerButton, modifiers: Modifiers) {
    harness.hover_at(pos);
    harness.step();
    for pressed in [true, false] {
        harness.event_modifiers(
            Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
            },
            modifiers,
        );
    }
    settle(harness);
}

/// A few frames, with time for values asked for to arrive.
fn settle(harness: &mut Harness) {
    for _ in 0..6 {
        harness.step();
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Open the folder or struct whose header starts with `start`.
fn open(harness: &mut Harness, start: &str) {
    harness.get_by_label_contains(start).click();
    settle(harness);
}

/// What's selected in the tree, as topic names with their field paths.
fn picked(harness: &Harness) -> Vec<String> {
    harness
        .state()
        .tree_sel
        .selected()
        .iter()
        .map(|t| match &t.field {
            Some(f) => format!("{} {}", t.name, f.path.join(".")),
            None => t.name.clone(),
        })
        .collect()
}

/// Open /Drive, its pose (nested structs) and its modules (a struct array).
fn open_drive(harness: &mut Harness) {
    open(harness, "Drive  (");
    open(harness, "pose  ");
    open(harness, "modules  ");
}

#[test]
fn tree_rows_select_with_click_ctrl_and_shift() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open(&mut harness, "Test  (");
    let none = Modifiers::NONE;
    click_row(&mut harness, "count", 0, none);
    assert_eq!(picked(&harness), ["/Test/count"]);
    // Ctrl (Cmd) adds one; clicking it again takes it out.
    click_row(&mut harness, "number", 0, Modifiers::COMMAND);
    assert_eq!(picked(&harness), ["/Test/count", "/Test/number"]);
    click_row(&mut harness, "number", 0, Modifiers::COMMAND);
    assert_eq!(picked(&harness), ["/Test/count"]);
    // Shift takes the rows from the last one clicked (Ctrl clicks count
    // too), in name order: count, enabled, flag, number.
    click_row(&mut harness, "count", 0, none);
    click_row(&mut harness, "number", 0, Modifiers::SHIFT);
    assert_eq!(
        picked(&harness),
        ["/Test/count", "/Test/enabled", "/Test/flag", "/Test/number"]
    );
    // And upwards, from the last click: number back to color.
    click_row(&mut harness, "number", 0, none);
    click_row(&mut harness, "color", 0, Modifiers::SHIFT);
    assert_eq!(
        picked(&harness),
        [
            "/Test/color",
            "/Test/colors",
            "/Test/count",
            "/Test/enabled",
            "/Test/flag",
            "/Test/number"
        ]
    );
    // A plain click picks just that row.
    click_row(&mut harness, "flag", 0, none);
    assert_eq!(picked(&harness), ["/Test/flag"]);
    // A click on empty grid clears it.
    click_at(
        &mut harness,
        Pos2::new(1700.0, 1250.0),
        egui::PointerButton::Primary,
        none,
    );
    assert!(picked(&harness).is_empty());
}

#[test]
fn nested_struct_fields_and_struct_array_items_select() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let none = Modifiers::NONE;
    // A field inside a nested struct: pose → translation → x.
    click_row(&mut harness, "x", 0, none);
    assert_eq!(picked(&harness), ["/Drive/pose translation.x"]);
    // Another branch of the same struct, added: pose → rotation → value
    // (the third "value": both modules come first).
    click_row(&mut harness, "value", 2, Modifiers::COMMAND);
    assert_eq!(
        picked(&harness),
        ["/Drive/pose translation.x", "/Drive/pose rotation.value"]
    );
    // A field of one item of a struct array: modules → [1] → speed.
    click_row(&mut harness, "speed", 1, none);
    assert_eq!(picked(&harness), ["/Drive/modules [1].speed"]);
    // A struct nested in an array item: modules → [0] → angle → value.
    click_row(&mut harness, "value", 0, none);
    assert_eq!(picked(&harness), ["/Drive/modules [0].angle.value"]);
    // Ctrl across topics: the same field of the other item.
    click_row(&mut harness, "value", 1, Modifiers::COMMAND);
    assert_eq!(
        picked(&harness),
        [
            "/Drive/modules [0].angle.value",
            "/Drive/modules [1].angle.value"
        ]
    );
}

/// Drag the `nth` tree row labelled `label` to `to`.
fn drag_row(harness: &mut Harness, label: &str, nth: usize, to: Pos2) {
    let from = row_at(harness, label, nth).center();
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
}

#[test]
fn struct_fields_drag_onto_the_grid_as_field_widgets() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let widgets = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.clone();
    let before = widgets(&harness).len();
    // A nested field, onto empty grid below the test widgets.
    drag_row(&mut harness, "y", 0, Pos2::new(1500.0, 1150.0));
    assert_eq!(widgets(&harness).len(), before + 1);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.topic(), Some("/Drive/pose"));
    let meta = made.struct_meta().expect("bound to a field");
    assert_eq!(meta.path, ["translation", "y"]);
    assert_eq!(meta.schema_name, "Pose2d");
    // A field of a struct array item, somewhere else.
    drag_row(&mut harness, "speed", 1, Pos2::new(1700.0, 900.0));
    assert_eq!(widgets(&harness).len(), before + 2);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.topic(), Some("/Drive/modules"));
    assert_eq!(made.struct_meta().unwrap().path, ["[1]", "speed"]);
    // Dragging a selected row takes the whole selection with it.
    click_row(&mut harness, "x", 0, Modifiers::NONE);
    click_row(&mut harness, "value", 2, Modifiers::COMMAND);
    drag_row(&mut harness, "x", 0, Pos2::new(1700.0, 650.0));
    assert_eq!(widgets(&harness).len(), before + 4);
}

#[test]
fn the_right_click_menu_adds_the_selection_and_locking_stops_it() {
    let robot = Robot::start();
    let mut harness = harness_for(&robot);
    open_drive(&mut harness);
    let count = |h: &Harness| h.state().layout.tabs[0].grid_layout.containers.len();
    let before = count(&harness);
    // Select two fields in different structs, then right-click one.
    click_row(&mut harness, "x", 0, Modifiers::NONE);
    click_row(&mut harness, "speed", 0, Modifiers::COMMAND);
    right_click_row(&mut harness, "x", 0);
    harness.get_by_label("Add 2 to grid").click();
    settle(&mut harness);
    assert_eq!(count(&harness), before + 2);
    // Right-clicking a row that isn't selected picks just it.
    right_click_row(&mut harness, "y", 0);
    assert_eq!(picked(&harness), ["/Drive/pose translation.y"]);
    // Into a new list, through the submenu.
    harness.get_by_label("Add to list").hover();
    settle(&mut harness);
    harness.get_by_label("New list").click();
    settle(&mut harness);
    let lists = harness.state().layout.tabs[0].grid_layout.layouts.clone();
    assert_eq!(lists.len(), 2);
    assert_eq!(
        lists[1].children[0].struct_meta().unwrap().path,
        ["translation", "y"]
    );
    // Locked: rows still select, but there's no menu and no dragging.
    harness.state_mut().locked = true;
    settle(&mut harness);
    right_click_row(&mut harness, "x", 0);
    assert_eq!(picked(&harness), ["/Drive/pose translation.x"]);
    assert!(harness.query_by_label("Add to grid").is_none());
    harness.key_press(Key::Escape);
    settle(&mut harness);
    let before = count(&harness);
    drag_row(&mut harness, "y", 0, Pos2::new(1500.0, 1150.0));
    assert_eq!(count(&harness), before);
}

/// A layout of writing widgets on one tab, each 256 pixels square along
/// the top row: a Toggle Switch and a Toggle Button on `/Test/enabled`, a
/// switch on a topic that's never sent, and one on a struct field. Below
/// them, a List Layout holding a Toggle Button on `/Test/enabled` and a
/// switch on a topic that's never sent.
fn writing_layout() -> ElasticLayout {
    let at = |i: usize| LayoutRect {
        x: i as f64 * 256.0,
        y: 0.0,
        width: 256.0,
        height: 256.0,
    };
    let mut tab = Tab::new("Writing");
    let grid = &mut tab.grid_layout;
    grid.containers = vec![
        widget("Toggle Switch", "/Test/enabled", "boolean", at(0)),
        widget("Toggle Button", "/Test/enabled", "boolean", at(1)),
        widget("Toggle Switch", "/Test/never_sent", "boolean", at(2)),
        widget("Toggle Switch", "/Drive/pose", "double", at(3)),
    ];
    grid.containers[3].properties.insert(
        "struct_meta".into(),
        json!({ "path": ["translation", "x"], "schema_name": "Pose2d", "type": "double" }),
    );
    let one = at(0);
    grid.layouts.push(list_layout::new_list(
        LayoutRect {
            x: 0.0,
            y: 256.0,
            width: 256.0,
            height: 512.0,
        },
        vec![
            widget("Toggle Button", "/Test/enabled", "boolean", one),
            widget("Toggle Switch", "/Test/never_sent", "boolean", one),
        ],
    ));
    ElasticLayout {
        tabs: vec![tab],
        ..ElasticLayout::default()
    }
}

/// The harness showing [`writing_layout`].
fn writing_harness(robot: &Robot) -> Harness {
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = writing_layout();
    app.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    harness
}

/// The middle of widget `index`'s body, and of its title bar, on screen.
fn body_and_title(harness: &Harness, index: usize) -> (Pos2, Pos2) {
    let app = harness.state();
    let r = app.layout.tabs[0].grid_layout.containers[index].rect;
    let min = app.grid_area.min + egui::vec2(r.x as f32, r.y as f32);
    let size = egui::vec2(r.width as f32, r.height as f32);
    let rect = Rect::from_min_size(min, size);
    (
        rect.center() + egui::vec2(0.0, 20.0),
        rect.center_top() + egui::vec2(0.0, 14.0),
    )
}

/// Wait until the server has `want` for `/Test/enabled`.
fn server_sees(harness: &mut Harness, robot: &Robot, want: bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while robot.seen_enabled.get() != Some(want) {
        assert!(Instant::now() < deadline, "the server never got {want}");
        settle(harness);
    }
}

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
    assert_eq!(harness.state().alert.played(), 0);
    assert!(harness.state().write_failed.is_empty());
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
    assert_eq!(app.alert.played(), 1);
    assert!(app.write_failed.contains_key(&(0, Sender::Widget(2))));
    // A struct field.
    let (body, _) = body_and_title(&harness, 3);
    click_at(&mut harness, body, primary, Modifiers::NONE);
    let app = harness.state();
    assert_eq!(
        app.status,
        "Can't send to /Drive/pose: struct values can't be changed"
    );
    assert_eq!(app.alert.played(), 2);
    // With the sound off: still flashes and says why, but no tone.
    harness.state_mut().alert_sound = false;
    click_at(&mut harness, body, primary, Modifiers::NONE);
    assert_eq!(harness.state().alert.played(), 2);
    // A later good send clears that widget's flash.
    harness
        .state_mut()
        .write_failed
        .insert((0, Sender::Widget(0)), Instant::now());
    let (good, _) = body_and_title(&harness, 0);
    click_at(&mut harness, good, primary, Modifiers::NONE);
    assert!(
        !harness
            .state()
            .write_failed
            .contains_key(&(0, Sender::Widget(0)))
    );
    // Disconnected: says so.
    harness.state_mut().custom_address = "".into();
    settle(&mut harness);
    click_at(&mut harness, good, primary, Modifiers::NONE);
    assert_eq!(
        harness.state().status,
        "Can't send: not connected to the robot"
    );
}

/// The middle of list `list`'s child `child`, on screen.
fn child_middle(harness: &Harness, list: usize, child: usize) -> Pos2 {
    let app = harness.state();
    let l = &app.layout.tabs[0].grid_layout.layouts[list];
    let min = app.grid_area.min + egui::vec2(l.rect.x as f32, l.rect.y as f32);
    let card = Rect::from_min_size(min, egui::vec2(l.rect.width as f32, l.rect.height as f32))
        .shrink(super::style::CELL_GAP);
    list_layout::rows_on_screen(card, l)
        .into_iter()
        .find(|row| row.index == child)
        .expect("the child is shown")
        .row
        .center()
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
        let want = robot.seen_enabled.get() != Some(true);
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
        app.write_failed
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
    let min = app.grid_area.min + egui::vec2(l.rect.x as f32, l.rect.y as f32);
    let card = Rect::from_min_size(min, egui::vec2(l.rect.width as f32, l.rect.height as f32))
        .shrink(super::style::CELL_GAP);
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
    let to = harness.state().grid_area.min + egui::vec2(640.0, 400.0);
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
    app.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    harness
}

/// Where grid widget `index`'s card is on screen.
fn card_on_screen(harness: &Harness, index: usize) -> Rect {
    let app = harness.state();
    let r = app.layout.tabs[0].grid_layout.containers[index].rect;
    let min = app.grid_area.min + egui::vec2(r.x as f32, r.y as f32);
    Rect::from_min_size(min, egui::vec2(r.width as f32, r.height as f32))
        .shrink(super::style::CELL_GAP)
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
    assert_eq!(harness.state().alert.played(), 0);
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
    let got = robot.seen_setpoint.get().unwrap();
    assert!(got > 0.0, "the list's slider sent nothing");
    let deadline = Instant::now() + Duration::from_secs(10);
    while (robot.seen_setpoint.get().unwrap() - 5.0).abs() > 1.0 {
        assert!(Instant::now() < deadline, "{:?}", robot.seen_setpoint.get());
        settle(&mut harness);
    }
}

/// Type `text` into whatever has the keyboard.
fn type_text(harness: &mut Harness, text: &str) {
    harness.event(Event::Text(text.into()));
    harness.step();
}

/// Press and let go of `k`.
fn press(harness: &mut Harness, k: Key) {
    for event in key(k, Modifiers::NONE) {
        harness.event(event);
    }
    settle(harness);
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
    let empty = harness.state().grid_area.min + egui::vec2(1400.0, 900.0);
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
