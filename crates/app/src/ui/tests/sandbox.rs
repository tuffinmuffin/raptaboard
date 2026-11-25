//! The plugin sandbox (`raptaboard plugin dev`): the window with one plugin
//! widget, its own server serving the plugin's sample values, and the panel.

// Tests written by Claude.

use std::path::Path;

use nt4::Value;

use super::*;
use crate::sandbox::Sandbox;

/// The sandbox window for the repo's plugin `name`.
fn sandbox_for(name: &str) -> Harness {
    raptaboard_plugin_host::dev::repo_plugins();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins")
        .join(name);
    let type_name = crate::plugins::load(&folder).unwrap();
    let manifest = crate::plugins::find(&type_name).unwrap().manifest();
    let sandbox = Sandbox::start(&manifest).unwrap();
    let mut app = DashboardApp::new(StartupOptions::default()).expect("app starts");
    app.sound = crate::sound::Sound::silent();
    app.open_sandbox(sandbox);
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
        .allow_missing_glyphs()
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut DashboardApp| app.show(ui), app);
    settle(&mut harness);
    harness
}

fn sandbox(harness: &mut Harness) -> &mut Sandbox {
    &mut harness.state_mut().sandbox.as_mut().unwrap().sandbox
}

#[test]
fn the_sandbox_serves_a_table_plugins_samples_and_changes_them() {
    let mut harness = sandbox_for("alerts");
    // plugin.toml's [sandbox] values, from the sandbox's own server.
    labelled_soon(&mut harness, "plugin:alerts: 1 error, 2 warnings, 1 info");
    assert!(harness.query_by_label("Plugin sandbox").is_some());
    sandbox(&mut harness)
        .set(
            "/Sandbox/Alerts/errors",
            Value::StringArray(vec!["A".into(), "B".into()]),
        )
        .unwrap();
    labelled_soon(&mut harness, "plugin:alerts: 2 errors, 2 warnings, 1 info");
    // Not served any more: as a robot that stopped publishing it.
    sandbox(&mut harness).remove("/Sandbox/Alerts/infos");
    labelled_soon(&mut harness, "plugin:alerts: 2 errors, 2 warnings, 0 infos");
    // Disconnected (the panel's switch): the plugin greys and says so.
    harness.get_by_label("Connected").click();
    labelled_soon(
        &mut harness,
        "plugin:alerts: 2 errors, 2 warnings, 0 infos (disconnected)",
    );
    harness.get_by_label("Connected").click();
    labelled_soon(&mut harness, "plugin:alerts: 2 errors, 2 warnings, 0 infos");
    // A key added from the panel goes under the widget's table.
    harness.get_by_label("New key").click();
    settle(&mut harness);
    harness.get_by_label("New key").type_text("angle");
    settle(&mut harness);
    harness.get_by_label("Add").click();
    settle(&mut harness);
    let angle = sandbox(&mut harness)
        .topic("/Sandbox/Alerts/angle")
        .map(|t| t.value.clone());
    assert_eq!(angle, Some(Value::Double(0.0)));
    // Nothing to save on closing.
    assert!(!harness.state().unsaved());
}

#[test]
fn the_sandbox_sizes_a_topic_plugins_widget() {
    let mut harness = sandbox_for("number");
    labelled_soon(&mut harness, "plugin:number: 1.2346");
    sandbox(&mut harness)
        .set("/Sandbox/value", Value::Double(-2.5))
        .unwrap();
    labelled_soon(&mut harness, "plugin:number: -2.5");
    // A topic added under the root, and the size picked.
    assert_eq!(
        sandbox(&mut harness).add("speed", nt4::NtType::Double),
        Ok("/Sandbox/speed".into())
    );
    sandbox(&mut harness).cells = (4, 3);
    settle(&mut harness);
    let widget = &harness.state().layout.tabs[0].grid_layout.containers[0];
    assert_eq!((widget.rect.width, widget.rect.height), (512.0, 384.0));
    assert!(!harness.state().unsaved());
}
