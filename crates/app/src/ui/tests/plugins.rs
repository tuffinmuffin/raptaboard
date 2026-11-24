//! Plugin widgets in the window: the Number plugin from `plugins/` on the
//! grid beside the built-in Text Display, and a plugin that stops.

// Tests written by Claude.

use std::path::{Path, PathBuf};

use super::*;

/// The repo's plugins, built, and the one named `name` loaded. Returns its
/// widget type name.
fn load(name: &str) -> String {
    raptaboard_plugin_host::dev::repo_plugins();
    let folder: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(name);
    crate::plugins::load(&folder).unwrap()
}

fn at(x: f64) -> LayoutRect {
    LayoutRect {
        x,
        y: 0.0,
        width: 256.0,
        height: 128.0,
    }
}

fn harness_with(robot: &Robot, widgets: Vec<WidgetContainer>) -> Harness {
    let mut tab = Tab::new("Plugins");
    tab.grid_layout.containers = widgets;
    let mut harness = harness_for(robot);
    let app = harness.state_mut();
    app.layout = ElasticLayout {
        tabs: vec![tab],
        ..Default::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.current_tab = 0;
    settle(&mut harness);
    harness
}

#[test]
fn the_number_plugin_shows_a_topic_beside_the_built_in() {
    let number = load("number");
    assert_eq!(number, "plugin:number");
    // It's offered for number topics, after the built-ins.
    // (Other tests load plugins too, all in one program, so it's not
    // necessarily the last.)
    let offered = widgets::types_showing("double");
    let place = offered.iter().position(|t| *t == "plugin:number").unwrap();
    assert!(
        offered[place..].iter().all(|t| t.starts_with("plugin:")),
        "{offered:?}"
    );
    assert!(!widgets::types_showing("string").contains(&"plugin:number"));

    let robot = Robot::start();
    let value = robot._server.publish::<f64>("/Plugin/value");
    value.set(1.23456);
    let mut built_in = widget("Text Display", "/Plugin/value", "double", at(0.0));
    built_in.title = "Built in".into();
    let mut plugin = widget(&number, "/Plugin/value", "double", at(256.0));
    plugin.title = "Plugin".into();
    let mut two_places = widget(&number, "/Plugin/value", "double", at(512.0));
    two_places.title = "Two places".into();
    two_places
        .properties
        .insert("decimal_places".into(), json!(2));
    let mut harness = harness_with(&robot, vec![built_in, plugin, two_places]);

    // The same text the built-in shows (up to 4 places).
    labelled_soon(&mut harness, "Plugin: 1.2346");
    labelled_soon(&mut harness, "Two places: 1.23");
    value.set(-7.5);
    labelled_soon(&mut harness, "Plugin: -7.5");
    labelled_soon(&mut harness, "Two places: -7.50");
}

#[test]
fn a_plugin_that_stops_offers_a_restart() {
    let name = load("limits-test");
    let robot = Robot::start();
    let mut broken = widget(&name, "/Plugin/other", "double", at(0.0));
    broken.properties.insert("mode".into(), json!("panic"));
    let mut harness = harness_with(&robot, vec![broken]);
    labelled_soon(&mut harness, "Restart");
    // Restarting runs it again, and it stops again.
    harness.get_by_label("Restart").click();
    settle(&mut harness);
    labelled_soon(&mut harness, "Restart");
    // Mended (the setting changed), it draws.
    harness.state_mut().layout.tabs[0].grid_layout.containers[0]
        .properties
        .insert("mode".into(), json!("ok"));
    settle(&mut harness);
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.query_by_label("Restart").is_some() {
        assert!(Instant::now() < deadline, "still stopped");
        settle(&mut harness);
    }
}

#[test]
fn a_deleted_plugin_widget_stops_its_worker() {
    let number = load("number");
    let robot = Robot::start();
    let gone = widget(&number, "/Plugin/deleted", "double", at(0.0));
    let mut harness = harness_with(&robot, vec![gone.clone()]);
    let ctx = harness.ctx.clone();
    assert!(crate::plugins::has_worker(&ctx, &gone));
    harness.state_mut().layout.tabs[0]
        .grid_layout
        .containers
        .clear();
    settle(&mut harness);
    assert!(!crate::plugins::has_worker(&ctx, &gone));
}
