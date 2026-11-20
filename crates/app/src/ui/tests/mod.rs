//! Whole-window tests: the app, headless, against a real NT server on
//! localhost, with one widget of every type on the grid.

// Tests written by Claude.

mod alerts;
mod choosers;
mod clips;
mod enums;
mod field;
mod files;
mod gyro;
mod sliders;
mod tree;
mod window;
mod writing;

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
    ("/Drive/pose", "struct:Pose2d"),
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
    /// A SendableChooser's table, `/Test/Auto`, as robot code makes it:
    /// its options and the robot's active choice, and what dashboards
    /// picked.
    auto_options: Publisher<Vec<String>>,
    auto_active: Publisher<String>,
    seen_selected: nt4::Subscriber<String>,
    /// A state robot code and dashboards both change: `/Test/mode`, with
    /// its names in `/Test/modeOptions`, and an int one, `/Test/level`.
    mode: Publisher<String>,
    seen_mode: nt4::Subscriber<String>,
    /// Two alert groups as WPILib's Alert makes them: `/Test/Alerts` (a
    /// warning and an info to start) and `/Test/Arm` (nothing to start).
    alert_errors: Publisher<Vec<String>>,
    alert_warnings: Publisher<Vec<String>>,
    arm_errors: Publisher<Vec<String>>,
    arm_warnings: Publisher<Vec<String>>,
    _alert_rest: Vec<Publisher<String>>,
    _alert_lists: Vec<Publisher<Vec<String>>>,
    level: Publisher<i64>,
    seen_level: nt4::Subscriber<i64>,
    _mode_options: Publisher<Vec<String>>,
    _auto: (Publisher<String>, Publisher<String>),
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
        let auto_type = server.publish::<String>("/Test/Auto/.type");
        auto_type.set(crate::types::chooser::CHOOSER_TYPE.into());
        let auto_options = server.publish::<Vec<String>>("/Test/Auto/options");
        auto_options.set(vec!["Left".into(), "Middle".into(), "Right".into()]);
        let auto_default = server.publish::<String>("/Test/Auto/default");
        auto_default.set("Middle".into());
        let auto_active = server.publish::<String>("/Test/Auto/active");
        auto_active.set("Middle".into());
        let seen_selected = server.subscribe::<String>("/Test/Auto/selected", Default::default());
        let list = |name: &str, items: &[&str]| {
            let p = server.publish::<Vec<String>>(name);
            p.set(items.iter().map(|s| s.to_string()).collect());
            p
        };
        let alerts_type = |table: &str| {
            let p = server.publish::<String>(&format!("{table}/.type"));
            p.set(crate::types::alerts::ALERTS_TYPE.into());
            p
        };
        let alert_rest = vec![alerts_type("/Test/Alerts"), alerts_type("/Test/Arm")];
        let alert_errors = list("/Test/Alerts/errors", &[]);
        let alert_warnings = list("/Test/Alerts/warnings", &["Battery low"]);
        let arm_errors = list("/Test/Arm/errors", &[]);
        let arm_warnings = list("/Test/Arm/warnings", &[]);
        let alert_lists = vec![
            list("/Test/Alerts/infos", &["Vision connected"]),
            list("/Test/Arm/infos", &[]),
        ];
        let mode = server.publish::<String>("/Test/mode");
        mode.set("Stowed".into());
        let seen_mode = server.subscribe::<String>("/Test/mode", Default::default());
        let mode_options = server.publish::<Vec<String>>("/Test/modeOptions");
        mode_options.set(vec!["Stowed".into(), "Intaking".into(), "Shooting".into()]);
        let level = server.publish::<i64>("/Test/level");
        level.set(0);
        let seen_level = server.subscribe::<i64>("/Test/level", Default::default());
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
            auto_options,
            auto_active,
            seen_selected,
            mode,
            seen_mode,
            alert_errors,
            alert_warnings,
            arm_errors,
            arm_warnings,
            _alert_rest: alert_rest,
            _alert_lists: alert_lists,
            level,
            seen_level,
            _mode_options: mode_options,
            _auto: (auto_type, auto_default),
            _others: (color, colors, numbers),
            _structs: (pose, modules),
        }
    }

    /// What robot code's SendableChooser does each loop: take the picked
    /// option (if it's one) as the active one.
    fn take_choice(&self) {
        if let Some(picked) = self.seen_selected.get()
            && ["Left", "Middle", "Right"].contains(&picked.as_str())
        {
            self.auto_active.set(picked);
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
        // Choosers and alerts show the chooser table (a Field shows a
        // Pose2d topic instead).
        if !ty.table_types().is_empty() && !TOPICS.iter().any(|(_, t)| ty.shows(t)) {
            let mut w = widget(ty.names()[0], "/Test/Auto", "", at(i));
            w.properties.remove("data_type");
            grid.containers.push(w);
            continue;
        }
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
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.sound = crate::sound::Sound::silent();
    app.current_tab = 0;
    app.locked = false;
    app.address.mode = AddressMode::Custom;
    app.address.custom = "127.0.0.1".into();
    app.address.port = robot.port;
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

/// Like [`row_at`], waiting up to 10 s for the row to be drawn: a struct's
/// row shows once its value has arrived, which a slow machine (CI's
/// coverage build) takes longer to do.
fn row_soon(harness: &mut Harness, label: &str, nth: usize) -> Rect {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let found = harness
            .query_all_by_label(label)
            .filter(|n| n.rect().min.x < TREE_RIGHT)
            .count();
        if found > nth || Instant::now() >= deadline {
            return row_at(harness, label, nth);
        }
        settle(harness);
    }
}

/// Click the `nth` tree row labelled `label`, holding `modifiers`.
fn click_row(harness: &mut Harness, label: &str, nth: usize, modifiers: Modifiers) {
    let pos = row_soon(harness, label, nth).center();
    click_at(harness, pos, egui::PointerButton::Primary, modifiers);
}

/// Right-click the `nth` tree row labelled `label`.
fn right_click_row(harness: &mut Harness, label: &str, nth: usize) {
    let pos = row_soon(harness, label, nth).center();
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

/// Open the folder or struct whose header starts with `start`, waiting for
/// it to be drawn first: a struct's row shows once its value has arrived,
/// which a slow machine (CI's Windows) takes longer to do.
fn open(harness: &mut Harness, start: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.query_by_label_contains(start).is_none() {
        assert!(Instant::now() < deadline, "no row starting {start:?}");
        settle(harness);
    }
    harness.get_by_label_contains(start).click();
    settle(harness);
}

/// What's selected in the tree, as topic names with their field paths.
fn picked(harness: &Harness) -> Vec<String> {
    harness
        .state()
        .tree
        .selection
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

/// Drag the `nth` tree row labelled `label` to `to`.
fn drag_row(harness: &mut Harness, label: &str, nth: usize, to: Pos2) {
    let from = row_soon(harness, label, nth).center();
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

/// The whole window in an egui_kittest harness, so tree rows can be found
/// by their labels and clicked or dragged where they're drawn.
type Harness = egui_kittest::Harness<'static, DashboardApp>;

/// The harness showing [`writing_layout`].
fn writing_harness(robot: &Robot) -> Harness {
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = writing_layout();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    harness
}

/// The middle of widget `index`'s body, and of its title bar, on screen.
fn body_and_title(harness: &Harness, index: usize) -> (Pos2, Pos2) {
    let app = harness.state();
    let r = app.layout.tabs[0].grid_layout.containers[index].rect;
    let min = app.grid.area.min + egui::vec2(r.x as f32, r.y as f32);
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
    // And the dashboard has it back, so the next click on a toggle flips
    // this value, not the one before.
    dashboard_has(harness, "/Test/enabled", nt4::Value::Boolean(want));
}

/// Wait until the dashboard has `want` for `topic` from the server, and
/// has drawn with it: before clicking a widget whose click depends on its
/// value (a toggle flips it, an enum toggle steps from it). On a slow
/// machine (CI's) a click can otherwise come before the value has arrived.
fn dashboard_has(harness: &mut Harness, topic: &str, want: nt4::Value) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let has = harness
            .state()
            .nt
            .client()
            .and_then(|c| c.topic(topic))
            .and_then(|t| t.value)
            .map(|v| v.value);
        if has.as_ref() == Some(&want) {
            // A widget takes a new value once per its period (0.02 s here).
            settle(harness);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the dashboard never had {want:?} for {topic}, has {has:?}"
        );
        settle(harness);
    }
}

/// The middle of list `list`'s child `child`, on screen.
fn child_middle(harness: &Harness, list: usize, child: usize) -> Pos2 {
    let app = harness.state();
    let l = &app.layout.tabs[0].grid_layout.layouts[list];
    let min = app.grid.area.min + egui::vec2(l.rect.x as f32, l.rect.y as f32);
    let card = Rect::from_min_size(min, egui::vec2(l.rect.width as f32, l.rect.height as f32))
        .shrink(crate::ui::style::CELL_GAP);
    list_layout::rows_on_screen(card, l)
        .into_iter()
        .find(|row| row.index == child)
        .expect("the child is shown")
        .row
        .center()
}

/// Where grid widget `index`'s card is on screen.
fn card_on_screen(harness: &Harness, index: usize) -> Rect {
    let app = harness.state();
    let r = app.layout.tabs[0].grid_layout.containers[index].rect;
    let min = app.grid.area.min + egui::vec2(r.x as f32, r.y as f32);
    Rect::from_min_size(min, egui::vec2(r.width as f32, r.height as f32))
        .shrink(crate::ui::style::CELL_GAP)
}

/// Press and let go of `k`.
fn press(harness: &mut Harness, k: Key) {
    for event in key(k, Modifiers::NONE) {
        harness.event(event);
    }
    settle(harness);
}

/// The harness showing [`chooser_layout`].
fn chooser_harness(robot: &Robot) -> Harness {
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = chooser_layout();
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    harness
}

/// Wait until the server has `want` for `/Test/Auto/selected`.
fn server_has_pick(harness: &mut Harness, robot: &Robot, want: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while robot.seen_selected.get().as_deref() != Some(want) {
        assert!(
            Instant::now() < deadline,
            "the server never got {want:?}, has {:?}",
            robot.seen_selected.get()
        );
        settle(harness);
    }
}

/// Click the thing labelled `label` that's inside `area`.
fn click_label_in(harness: &mut Harness, label: &str, area: Rect) {
    let pos = harness
        .get_all_by_label(label)
        .map(|n| n.rect())
        .find(|r| area.contains(r.center()))
        .unwrap_or_else(|| panic!("no {label:?} in {area:?}"))
        .center();
    click_at(harness, pos, egui::PointerButton::Primary, Modifiers::NONE);
}

/// A layout of writing widgets on one tab, each 256 pixels square along
/// the top row: a Toggle Switch and a Toggle Button on `/Test/enabled`, a
/// switch on a topic that's never sent, one on a struct field, and an
/// editable Text Display on `/Test/setpoint`. Lower down, an editable Text
/// Display on `/Test/enabled` and one that isn't editable, and a List Layout holding a Toggle Button on `/Test/enabled` and a
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
    // Text Displays: editable on a number and on a boolean, then one that
    // isn't editable.
    let below = |i: usize| LayoutRect {
        x: 512.0 + i as f64 * 256.0,
        y: 768.0,
        ..at(0)
    };
    let mut typed = widget("Text Display", "/Test/setpoint", "double", at(4));
    typed.properties.insert("editable".into(), json!(true));
    let mut typed_flag = widget("Text Display", "/Test/enabled", "boolean", below(0));
    typed_flag.properties.insert("editable".into(), json!(true));
    grid.containers.push(typed);
    grid.containers.push(typed_flag);
    grid.containers
        .push(widget("Text Display", "/Test/setpoint", "double", below(1)));
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

/// A layout of choosers on `/Test/Auto` along the top: a Dropdown Chooser
/// (512 by 512) and a Split Button Chooser (512 by 256); and below the
/// drop-down a List Layout (labels inside) holding a Split Button Chooser.
fn chooser_layout() -> ElasticLayout {
    let at = |i: usize| LayoutRect {
        x: i as f64 * 512.0,
        y: 0.0,
        width: 512.0,
        height: 256.0,
    };
    let chooser = |ty: &str, title: &str, rect| {
        let mut w = widget(ty, "/Test/Auto", "", rect);
        w.properties.remove("data_type");
        w.title = title.into();
        w
    };
    let mut tab = Tab::new("Choosers");
    let grid = &mut tab.grid_layout;
    // The drop-down is twice as tall, so the bigger list it opens stays
    // inside it.
    let tall = LayoutRect {
        height: 512.0,
        ..at(0)
    };
    grid.containers = vec![
        chooser("Dropdown Chooser", "Auto dropdown", tall),
        chooser("Split Button Chooser", "Auto buttons", at(1)),
    ];
    grid.layouts.push(list_layout::new_list(
        LayoutRect {
            x: 0.0,
            y: 512.0,
            width: 512.0,
            height: 256.0,
        },
        vec![chooser("Split Button Chooser", "Auto in a list", at(0))],
    ));
    grid.layouts[0]
        .properties
        .insert(list_layout::LABEL_INSIDE.into(), json!(true));
    ElasticLayout {
        tabs: vec![tab],
        ..ElasticLayout::default()
    }
}

/// Wait until something is labelled exactly `label`.
fn labelled_soon(harness: &mut Harness, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.query_by_label(label).is_none() {
        assert!(Instant::now() < deadline, "nothing labelled {label:?}");
        settle(harness);
    }
}

/// Tap at `tap` the way a touchscreen on macOS sends one (from a real
/// input log): one frame with a move to the tap, the press and release
/// there, then the cursor put back at `mouse` (where the mouse was) and the
/// pointer gone. The frames go through the app's [`TapHold`](super::taps::TapHold) as
/// eframe's `raw_input_hook` does. All in one frame: `Harness::event` would
/// run a frame for each, which hides what goes wrong. The mouse is moved to
/// `mouse` first.
fn tap_burst(harness: &mut Harness, tap: Pos2, mouse: Pos2) {
    harness.hover_at(mouse);
    harness.step();
    let button = |pressed| Event::PointerButton {
        pos: tap,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut frame = vec![
        Event::PointerMoved(tap),
        button(true),
        Event::PointerMoved(tap),
        button(false),
        Event::PointerMoved(mouse),
        Event::PointerGone,
    ];
    harness.state_mut().taps.apply(&mut frame);
    harness.input_mut().events.extend(frame);
    harness.step();
    // The next frame, with what was held back.
    let mut next = Vec::new();
    harness.state_mut().taps.apply(&mut next);
    harness.input_mut().events.extend(next);
    harness.step();
    settle(harness);
}
