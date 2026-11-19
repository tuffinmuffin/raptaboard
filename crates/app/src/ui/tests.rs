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
    for _ in 0..2 {
        frame(&ctx, &mut app, key(Key::Escape, Modifiers::NONE));
        frame(&ctx, &mut app, vec![]);
    }
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
    // too), in name order: count, flag, number.
    click_row(&mut harness, "count", 0, none);
    click_row(&mut harness, "number", 0, Modifiers::SHIFT);
    assert_eq!(
        picked(&harness),
        ["/Test/count", "/Test/flag", "/Test/number"]
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
    drag_row(&mut harness, "speed", 1, Pos2::new(1500.0, 900.0));
    assert_eq!(widgets(&harness).len(), before + 2);
    let made = widgets(&harness).pop().unwrap();
    assert_eq!(made.topic(), Some("/Drive/modules"));
    assert_eq!(made.struct_meta().unwrap().path, ["[1]", "speed"]);
    // Dragging a selected row takes the whole selection with it.
    click_row(&mut harness, "x", 0, Modifiers::NONE);
    click_row(&mut harness, "value", 2, Modifiers::COMMAND);
    drag_row(&mut harness, "x", 0, Pos2::new(1500.0, 650.0));
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
