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

/// Run frames until a label containing `text` shows.
fn contains_soon(harness: &mut Harness, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.query_all_by_label_contains(text).next().is_none() {
        assert!(Instant::now() < deadline, "never showed {text:?}");
        settle(harness);
    }
}

/// The sandbox's widget's setting `key` set to `value` (its worker restarts).
fn set_mode(harness: &mut Harness, mode: &str) {
    harness.state_mut().layout.tabs[0].grid_layout.containers[0]
        .properties
        .insert("mode".into(), json!(mode));
    settle(harness);
}

#[test]
fn the_overlay_shows_usage_the_log_and_a_stop_with_restart() {
    let mut harness = sandbox_for("limits-test");
    // Its calls against the limits, once it has drawn.
    contains_soon(&mut harness, "of 100");
    contains_soon(&mut harness, "MiB of 64");
    // What it prints is its log.
    set_mode(&mut harness, "print");
    contains_soon(&mut harness, "hello from limits-test");
    // A region the plugin drew, used in the window: what it logs shows.
    set_mode(&mut harness, "region");
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.query_by_label("Bell").is_none() {
        assert!(Instant::now() < deadline, "no Bell region");
        settle(&mut harness);
    }
    harness.get_by_label("Bell").click();
    settle(&mut harness);
    contains_soon(&mut harness, "bell pressed");
    // A panic: why, in the panel, and Restart runs it again (it panics again).
    set_mode(&mut harness, "panic");
    contains_soon(&mut harness, "Stopped: ");
    contains_soon(&mut harness, "panicked on purpose");
    let widget = harness.state().layout.tabs[0].grid_layout.containers[0].clone();
    let ctx = harness.ctx.clone();
    // The panel's (the widget has one too, on its card).
    let panel_restart = harness
        .get_all_by_label("Restart")
        .max_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x))
        .unwrap()
        .rect()
        .center();
    click_at(
        &mut harness,
        panel_restart,
        egui::PointerButton::Primary,
        Modifiers::NONE,
    );
    settle(&mut harness);
    contains_soon(&mut harness, "Stopped: ");
    assert!(crate::plugins::dev_view(&ctx, &widget).is_some());
    // The regions switch is the window's.
    assert!(!crate::plugins::outline_regions(&ctx));
    harness.get_by_label("Outline its regions").click();
    settle(&mut harness);
    assert!(crate::plugins::outline_regions(&ctx));
}

#[test]
fn a_tab_plugin_opens_as_its_tab_in_the_sandbox() {
    let mut harness = sandbox_for("tab-test");
    let tabs: Vec<String> = harness
        .state()
        .layout
        .tabs
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(tabs, ["Tab test"]);
    assert!(
        harness.state().layout.tabs[0]
            .grid_layout
            .containers
            .is_empty()
    );
    labelled_soon(
        &mut harness,
        "Tab test: Hello from the tab-test plugin's folder. Keys: 0",
    );
    // Its overlay: the stand-in's usage, once it has drawn.
    contains_soon(&mut harness, "of 100");
    assert!(harness.query_by_label("Size").is_none(), "no widget size");
}

/// The tab-test square's left edge in `widget`'s newest picture, in frame
/// pixels: the first column that isn't the background.
fn square_x(ctx: &egui::Context, widget: &raptaboard_layout::WidgetContainer) -> Option<i64> {
    let p = crate::plugins::newest_pixels(ctx, widget)?;
    let width = p.width as usize;
    (0..p.height as usize)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|(x, y)| p.rgba[(y * width + x) * 4..][..3] != [10, 20, 60])
        .map(|(x, _)| x as i64)
        .min()
}

#[test]
fn an_animation_keeps_its_rate_when_the_window_draws_only_when_asked() {
    use std::sync::{Arc, Mutex};
    // As eframe does: the window sleeps until a repaint is asked for (now or
    // after a delay, from its own frames or other threads), then draws at the
    // next 120 Hz refresh, a little late. tab-test asks for 35 frames a
    // second and moves its square one 3-pixel step a frame.
    let mut harness = sandbox_for("tab-test");
    // It fills its tab: its stand-in widget.
    let ty = crate::plugins::find("plugin:tab-test").unwrap();
    let widget = crate::plugins::tab_widget(ty, "Tab test");
    let ctx = harness.ctx.clone();
    let wake: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(Some(Instant::now())));
    let asked = wake.clone();
    ctx.set_request_repaint_callback(move |info| {
        let at = Instant::now() + info.delay;
        let mut w = asked.lock().unwrap();
        *w = Some(w.map_or(at, |x| x.min(at)));
    });
    let start = Instant::now();
    let refresh_us = 8_333;
    let (mut xs, mut frames) = (Vec::new(), 0u64);
    while start.elapsed() < Duration::from_millis(1500) {
        let at = loop {
            let soonest = *wake.lock().unwrap();
            match soonest {
                Some(at) if at <= Instant::now() => break at,
                _ if start.elapsed() > Duration::from_secs(5) => break Instant::now(),
                _ => std::thread::sleep(Duration::from_millis(1)),
            }
        };
        wake.lock().unwrap().take();
        let since = at.saturating_duration_since(start).as_micros() as u64;
        let late = (frames * 2_654_435_761) % 6_000;
        let draw_at = start + Duration::from_micros(since.div_ceil(refresh_us) * refresh_us + late);
        if let Some(wait) = draw_at.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
        xs.extend(square_x(&ctx, &widget));
        harness.step();
        frames += 1;
        let delay = harness
            .output()
            .viewport_output
            .values()
            .map(|v| v.repaint_delay)
            .min();
        if let Some(d) = delay.filter(|d| *d < Duration::from_secs(10)) {
            let at = Instant::now() + d;
            let mut w = wake.lock().unwrap();
            *w = Some(w.map_or(at, |x| x.min(at)));
        }
    }
    let mut moves: Vec<i64> = xs
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .filter(|m| *m > 0)
        .collect();
    moves.sort_unstable();
    // About 52 in 1.5 s; when the window slept between pictures, about 6,
    // each jumping 30 pixels.
    assert!(
        moves.len() >= 25,
        "only {} new pictures: {moves:?}",
        moves.len()
    );
    assert_eq!(moves[moves.len() / 2], 3, "{moves:?}");
}
