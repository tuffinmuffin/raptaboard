//! Plugin widgets in the window: the test plugins from `plugins/` (a topic
//! plugin and a table plugin) on the grid beside the built-ins they copy, and
//! a plugin that stops.

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
fn a_topic_plugin_shows_a_topic_beside_the_built_in() {
    let number = load("topic-test");
    assert_eq!(number, "plugin:topic-test");
    // It's offered for number topics, after the built-ins.
    // (Other tests load plugins too, all in one program, so it's not
    // necessarily the last.)
    let offered = widgets::types_showing("double");
    let place = offered
        .iter()
        .position(|t| *t == "plugin:topic-test")
        .unwrap();
    assert!(
        offered[place..].iter().all(|t| t.starts_with("plugin:")),
        "{offered:?}"
    );
    assert!(!widgets::types_showing("string").contains(&"plugin:topic-test"));

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
fn loading_a_plugin_again_unchanged_keeps_its_widgets_running() {
    // Tests load plugins at the same time; one test's load mustn't restart
    // another's widgets.
    let number = load("topic-test");
    let before = crate::plugins::generation(&number);
    load("topic-test");
    assert_eq!(crate::plugins::generation(&number), before);
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
    let number = load("topic-test");
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

/// An alerts widget of `type_name` on the group `topic`, titled `title`, with
/// settings `extra`.
fn alerts(
    type_name: &str,
    topic: &str,
    title: &str,
    x: f64,
    extra: serde_json::Value,
) -> WidgetContainer {
    let mut w = widget(type_name, topic, "", at(x));
    w.properties.remove("data_type");
    w.title = title.into();
    w.rect.height = 256.0;
    for (k, v) in extra.as_object().unwrap() {
        w.properties.insert(k.clone(), v.clone());
    }
    w
}

#[test]
fn a_table_plugin_counts_and_sounds_like_the_built_in() {
    let plugin = load("table-test");
    assert_eq!(plugin, "plugin:table-test");
    // Offered for alert groups' tables, like the built-in.
    let ty = crate::plugins::find(&plugin).unwrap();
    assert_eq!(widgets::WidgetType::table_types(ty), ["Alerts"]);

    let robot = Robot::start();
    let mut harness = harness_with(
        &robot,
        vec![
            alerts("Alerts", "/Test/Alerts", "Built in", 0.0, json!({})),
            alerts(
                &plugin,
                "/Test/Alerts",
                "Plugin",
                256.0,
                json!({"sound": true}),
            ),
            alerts(
                &plugin,
                "/Test/Alerts",
                "Every group",
                512.0,
                json!({"all_groups": true}),
            ),
        ],
    );
    harness.state_mut().alert_sound = true;
    labelled_soon(&mut harness, "Built in: 0 errors, 1 warning, 1 info");
    labelled_soon(&mut harness, "Plugin: 0 errors, 1 warning, 1 info");
    labelled_soon(&mut harness, "Every group: 0 errors, 1 warning, 1 info");
    // Already active when it first looked: no tone for those.
    assert!(harness.state().sound.tones().is_empty());
    // A new error: the same count, and one tone (only the plugin sounds).
    robot.alert_errors.set(vec!["Encoder lost".into()]);
    labelled_soon(&mut harness, "Built in: 1 error, 1 warning, 1 info");
    labelled_soon(&mut harness, "Plugin: 1 error, 1 warning, 1 info");
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.state().sound.tones().is_empty() {
        assert!(Instant::now() < deadline, "no tone");
        settle(&mut harness);
    }
    assert_eq!(harness.state().sound.tones(), [crate::sound::Tone::Beep]);
    // Every group: the arm's errors too.
    robot.arm_errors.set(vec!["Arm motor fault".into()]);
    labelled_soon(&mut harness, "Every group: 2 errors, 1 warning, 1 info");
    labelled_soon(&mut harness, "Plugin: 1 error, 1 warning, 1 info");
    // The robot goes away: the last alerts seen stay, marked.
    drop(robot);
    labelled_soon(
        &mut harness,
        "Built in: 1 error, 1 warning, 1 info (disconnected)",
    );
    labelled_soon(
        &mut harness,
        "Plugin: 1 error, 1 warning, 1 info (disconnected)",
    );
}

#[test]
fn a_table_plugin_marks_its_tab_while_it_is_closed() {
    let plugin = load("table-test");
    let robot = Robot::start();
    let mut harness = harness_with(&robot, vec![]);
    let mut closed = Tab::new("Arm tab");
    closed.grid_layout.containers = vec![alerts(&plugin, "/Test/Arm", "Arm", 0.0, json!({}))];
    harness.state_mut().layout.tabs.push(closed);
    use crate::types::alerts::Level;
    let mark = |h: &mut Harness| {
        settle(h);
        let app = h.state();
        let topics = app.nt.topics();
        let values: std::collections::HashMap<&str, &nt4::Value> = topics
            .iter()
            .filter_map(|t| Some((&*t.name, &t.value.as_ref()?.value)))
            .collect();
        app.tab_marks(&values)[1]
    };
    let wait_for = |h: &mut Harness, want: Option<Level>| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while mark(h) != want {
            assert!(Instant::now() < deadline, "the mark never became {want:?}");
        }
    };
    wait_for(&mut harness, None);
    robot.arm_errors.set(vec!["Arm motor fault".into()]);
    wait_for(&mut harness, Some(Level::Error));
    robot.arm_errors.set(vec![]);
    robot.arm_warnings.set(vec!["Arm hot".into()]);
    wait_for(&mut harness, Some(Level::Warning));
    assert_eq!(
        harness.state().current_tab,
        0,
        "the arm's tab stayed closed"
    );
}

/// A pack folder `id` holding the repo's plugin `plugin` (built), renamed
/// `id`: its `.wasm`, its manifest beside it, and `pack.toml`.
fn plugin_pack(id: &str, plugin: &str) -> PathBuf {
    raptaboard_plugin_host::dev::repo_plugins();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    let dir = crate::content::tests::folder(id);
    std::fs::copy(
        repo.join(format!(
            "target/wasm32-wasip2/release/{}.wasm",
            plugin.replace('-', "_")
        )),
        dir.join(format!("{id}.wasm")),
    )
    .unwrap();
    let manifest = std::fs::read_to_string(repo.join(plugin).join("plugin.toml")).unwrap();
    let renamed = manifest.replace(&format!("name = \"{plugin}\""), &format!("name = \"{id}\""));
    assert_ne!(renamed, manifest);
    std::fs::write(dir.join(format!("{id}.toml")), renamed).unwrap();
    std::fs::write(
        dir.join("pack.toml"),
        format!(
            "id = \"{id}\"\nname = \"Test {id}\"\nversion = \"1.0.0\"\nmin_app = \"0.1.0\"\n\
             summary = \"A plugin.\"\nlicense = \"MIT\"\ncredits = \"Us.\"\n\n\
             [[plugin]]\nfile = \"{id}.wasm\"\n"
        ),
    )
    .unwrap();
    dir
}

/// Run frames until `harness`'s packs are done downloading and loaded.
fn packs_settled(harness: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        settle(harness);
        let content = harness.state().content.as_ref().unwrap();
        if content.idle() && !content.plugins_changed {
            return;
        }
        assert!(Instant::now() < deadline, "the packs didn't settle");
    }
}

#[test]
fn a_packs_plugin_is_used_while_its_installed() {
    let pack = crate::pack_folder::load(&plugin_pack("pack-number", "topic-test")).unwrap();
    // The catalog names it and says what it may do.
    assert_eq!(pack.plugins[0].name.as_deref(), Some("pack-number"));
    assert_eq!(
        pack.plugin_lines(),
        ["Plugin pack-number: nothing more than draw"]
    );
    let robot = Robot::start();
    let value = robot._server.publish::<f64>("/Plugin/packed");
    value.set(1.5);
    let mut packed = widget("plugin:pack-number", "/Plugin/packed", "double", at(0.0));
    packed.title = "Packed".into();
    let mut harness = harness_with(&robot, vec![packed.clone()]);
    // Not installed: a placeholder, its settings kept.
    assert!(crate::plugins::find("plugin:pack-number").is_none());
    assert!(harness.query_by_label("Packed: 1.5").is_none());

    let store = crate::content::tests::folder("pack-number-store");
    let app = harness.state_mut();
    app.content = Some(crate::content::Content::with_fetch(
        &store,
        app.sound.files(),
        vec![],
        Box::new(crate::pack_folder::read_file),
    ));
    harness
        .state_mut()
        .content
        .as_mut()
        .unwrap()
        .install("local", &pack);
    packs_settled(&mut harness);
    labelled_soon(&mut harness, "Packed: 1.5");
    let ty = crate::plugins::find("plugin:pack-number").unwrap();
    assert!(matches!(ty.origin(), crate::plugins::Origin::Pack { id, .. } if id == "pack-number"));

    // Removed: gone again, and its widget's worker stopped.
    harness
        .state_mut()
        .content
        .as_mut()
        .unwrap()
        .remove("pack-number");
    packs_settled(&mut harness);
    settle(&mut harness);
    assert!(crate::plugins::find("plugin:pack-number").is_none());
    assert!(!crate::plugins::has_worker(&harness.ctx.clone(), &packed));
    assert!(harness.query_by_label("Packed: 1.5").is_none());
}

#[test]
fn a_packs_plugin_asking_for_more_than_its_catalog_said_isnt_loaded() {
    let mut pack = crate::pack_folder::load(&plugin_pack("pack-alerts", "table-test")).unwrap();
    assert_eq!(
        pack.plugin_lines(),
        ["Plugin pack-alerts: plays sounds, runs while its tab is closed"]
    );
    // A catalog that hides what it asks for.
    pack.plugins[0].permissions = vec!["sound".into()];
    let robot = Robot::start();
    let mut harness = harness_with(&robot, vec![]);
    let store = crate::content::tests::folder("pack-alerts-store");
    let app = harness.state_mut();
    app.content = Some(crate::content::Content::with_fetch(
        &store,
        app.sound.files(),
        vec![],
        Box::new(crate::pack_folder::read_file),
    ));
    harness
        .state_mut()
        .content
        .as_mut()
        .unwrap()
        .install("local", &pack);
    packs_settled(&mut harness);
    assert!(crate::plugins::find("plugin:pack-alerts").is_none());
    let status = &harness.state().status;
    assert!(
        status.contains("asks for background, which its pack didn't list"),
        "{status}"
    );
}

#[test]
fn a_packs_plugin_needing_more_than_its_catalog_said_isnt_loaded() {
    use crate::content::InstalledPlugin;
    use crate::plugins::use_packs;
    let dir = plugin_pack("pack-tab", "tab-test");
    let pack = crate::pack_folder::load(&dir).unwrap();
    // What it needs is shown, the tab and mouse first.
    assert_eq!(
        pack.plugin_lines(),
        ["Plugin pack-tab: fills the Tab test tab, \
          holds the mouse while its tab has the keyboard, plays sounds"]
    );
    let installed = |needs| InstalledPlugin {
        pack: "tabs".into(),
        pack_name: "Tabs".into(),
        installed_at: 0,
        name: Some("pack-tab".into()),
        wasm: dir.join("pack-tab.wasm"),
        manifest: dir.join("pack-tab.toml"),
        folder: dir.clone(),
        permissions: vec!["sound".into()],
        needs,
    };
    let store = crate::content::tests::folder("pack-tab-store");
    // A catalog that hides that it holds the mouse.
    let mut hidden = pack.plugins[0].needs.clone().unwrap();
    hidden.capture_mouse = false;
    let problems = use_packs(&store, &[installed(Some(hidden))]);
    assert!(crate::plugins::find("plugin:pack-tab").is_none());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("asks for the mouse, which its pack didn't list"),
        "{problems:?}"
    );
    // Its catalog's needs, or a catalog from before needs: it loads.
    for needs in [pack.plugins[0].needs.clone(), None] {
        assert!(use_packs(&store, &[installed(needs)]).is_empty());
        assert!(crate::plugins::find("plugin:pack-tab").is_some());
        assert!(use_packs(&store, &[]).is_empty());
    }
}

#[test]
fn the_newest_pack_wins_and_a_folder_plugin_beats_a_pack() {
    use crate::content::InstalledPlugin;
    use crate::plugins::{Origin, use_packs};
    raptaboard_plugin_host::dev::repo_plugins();
    let store = crate::content::tests::folder("pack-layers-store");
    // Two packs with a plugin of one name, and one named as a folder plugin.
    let copy = |pack: &str, id: &str, plugin: &str| {
        let made = plugin_pack(id, plugin);
        let to = store.join(pack);
        std::fs::create_dir_all(&to).unwrap();
        for f in [format!("{id}.wasm"), format!("{id}.toml")] {
            std::fs::copy(made.join(&f), to.join(&f)).unwrap();
        }
        InstalledPlugin {
            pack: pack.into(),
            pack_name: pack.into(),
            installed_at: 0,
            name: Some(id.into()),
            wasm: to.join(format!("{id}.wasm")),
            manifest: to.join(format!("{id}.toml")),
            folder: to,
            permissions: vec![],
            needs: None,
        }
    };
    let mut older = copy("older", "layered", "topic-test");
    older.installed_at = 100;
    let mut newer = copy("newer", "layered", "topic-test");
    newer.installed_at = 200;
    assert!(use_packs(&store, &[older.clone(), newer.clone()]).is_empty());
    let origin = || crate::plugins::find("plugin:layered").unwrap().origin();
    assert!(matches!(origin(), Origin::Pack { id, .. } if id == "newer"));
    // The newer one removed: the older one.
    assert!(use_packs(&store, &[older]).is_empty());
    assert!(matches!(origin(), Origin::Pack { id, .. } if id == "older"));
    assert!(use_packs(&store, &[]).is_empty());
    assert!(crate::plugins::find("plugin:layered").is_none());

    // A folder plugin keeps its name.
    load("topic-test");
    let number = copy("numbers", "number-folder-test", "topic-test");
    let named_number = InstalledPlugin {
        name: Some("topic-test".into()),
        ..number
    };
    let problems = use_packs(&store, &[named_number]);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        crate::plugins::find("plugin:topic-test").unwrap().origin(),
        Origin::Folder
    );
}
