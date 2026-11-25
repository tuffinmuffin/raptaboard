// Tests written by Claude: real plugins from `plugins/` (built for
// wasm32-wasip2 the first time a test needs them) running under the runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use raptaboard_plugin_host::Manifest;
use raptaboard_plugin_host::runtime::*;

fn plugins_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins")
}

/// The `plugins/` workspace's `.wasm` files, built once.
fn built() -> &'static Path {
    raptaboard_plugin_host::dev::repo_plugins()
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("wasmtime starts"))
}

/// A plugin from `plugins/<name>`, with its manifest changed by `edit`.
fn plugin_with(name: &str, edit: impl FnOnce(&mut Manifest)) -> Plugin {
    let folder = plugins_dir().join(name);
    let text = std::fs::read_to_string(folder.join("plugin.toml")).unwrap();
    let mut manifest = Manifest::from_toml(&text).unwrap();
    edit(&mut manifest);
    let wasm = std::fs::read(built().join(format!("{}.wasm", name.replace('-', "_")))).unwrap();
    runtime().compile(manifest, &wasm, &folder).unwrap()
}

fn plugin(name: &str) -> Plugin {
    plugin_with(name, |_| {})
}

/// What a plugin asked of the host.
#[derive(Default)]
struct Seen {
    logs: Vec<String>,
    sounds: Vec<String>,
    published: Vec<(String, NtValue)>,
}

struct TestHost {
    seen: Arc<Mutex<Seen>>,
    unit: Option<String>,
    /// Where saved values go, if anywhere.
    storage: Option<PathBuf>,
}

impl Host for TestHost {
    fn unit(&self, topic: &str) -> Option<String> {
        if topic.is_empty() {
            self.unit.clone()
        } else {
            None
        }
    }
    fn publish(&mut self, key: &str, value: NtValue) -> Result<(), String> {
        self.seen
            .lock()
            .unwrap()
            .published
            .push((key.into(), value));
        Ok(())
    }
    /// Every character 0.6 of the size wide, one line the size high.
    fn measure_text(&self, text: &str, size: f32, _: Option<f32>, _: bool) -> Size {
        Size {
            width: text.chars().count() as f32 * size * 0.6,
            height: size,
        }
    }
    fn play_sound(&mut self, sound: &str) -> Result<(), String> {
        self.seen.lock().unwrap().sounds.push(sound.into());
        Ok(())
    }
    fn log(&mut self, message: &str) {
        self.seen.lock().unwrap().logs.push(message.into());
    }
    fn storage_dir(&self) -> Option<PathBuf> {
        self.storage.clone()
    }
}

fn start(
    plugin: &Plugin,
    properties: &[(&str, NtValue)],
    unit: Option<&str>,
    limits: Limits,
) -> (Instance, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let host = TestHost {
        seen: seen.clone(),
        unit: unit.map(str::to_string),
        storage: None,
    };
    let props: HashMap<String, NtValue> = properties
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    let instance = runtime()
        .instantiate(plugin, Box::new(host), props, limits)
        .unwrap();
    (instance, seen)
}

fn gray(v: u8) -> Color {
    Color {
        r: v,
        g: v,
        b: v,
        a: 255,
    }
}

fn frame(width: f32, height: f32, connected: bool) -> FrameInfo {
    FrameInfo {
        size: Size { width, height },
        time_s: 0.0,
        connected,
        locked: false,
        theme: Theme {
            dark: true,
            text: gray(230),
            weak_text: gray(120),
            background: gray(20),
            card: gray(40),
            accent: gray(200),
            error: gray(1),
            warning: gray(2),
            info: gray(3),
        },
    }
}

fn texts(picture: &Picture) -> Vec<&Text> {
    picture
        .shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn sample(value: f64) -> Sample {
    Sample {
        topic: String::new(),
        value: Some(NtValue::Double(value)),
        time_us: 0,
    }
}

#[test]
fn number_shows_its_value_with_decimals_and_unit() {
    let (mut w, _) = start(
        &plugin("number"),
        &[("decimal_places", NtValue::Double(2.0))],
        Some("m"),
        Limits::default(),
    );
    assert!(matches!(
        w.subscriptions().unwrap()[..],
        [Subscription::Bound]
    ));
    w.update(&[sample(1.0), sample(1.23456)]).unwrap();
    let picture = w.render(frame(200.0, 100.0, true)).unwrap();
    let text = texts(&picture);
    assert_eq!(text.len(), 1);
    assert_eq!(text[0].text, "1.23 m");
    assert_eq!(text[0].pos.x, 100.0);
    assert_eq!(text[0].color, gray(230));
    assert_eq!(picture.summary.as_deref(), Some("1.23 m"));
}

#[test]
fn number_without_a_value_shows_a_dash_and_greys_when_disconnected() {
    let (mut w, _) = start(&plugin("number"), &[], None, Limits::default());
    let picture = w.render(frame(200.0, 100.0, false)).unwrap();
    let text = texts(&picture);
    assert_eq!(text[0].text, "—");
    assert_eq!(text[0].color, gray(120));
    // A value that stops being published goes back to the dash.
    w.update(&[sample(2.5)]).unwrap();
    assert_eq!(
        texts(&w.render(frame(200.0, 100.0, true)).unwrap())[0].text,
        "2.5"
    );
    w.update(&[Sample {
        topic: String::new(),
        value: None,
        time_us: 1,
    }])
    .unwrap();
    assert_eq!(
        texts(&w.render(frame(200.0, 100.0, true)).unwrap())[0].text,
        "—"
    );
}

#[test]
fn number_fits_its_text_to_the_widget() {
    let (mut w, _) = start(
        &plugin("number"),
        &[("fit_text", NtValue::Boolean(true))],
        None,
        Limits::default(),
    );
    w.update(&[sample(12.0)]).unwrap();
    let wide = texts(&w.render(frame(600.0, 100.0, true)).unwrap())[0].size;
    let narrow = texts(&w.render(frame(60.0, 100.0, true)).unwrap())[0].size;
    // Wide: the height limits it (80 px). Narrow: two characters in 54 px.
    assert!((wide - 80.0).abs() < 0.01, "{wide}");
    assert!((narrow - 45.0).abs() < 0.01, "{narrow}");
}

fn mode(m: &str) -> [(&'static str, NtValue); 1] {
    [("mode", NtValue::Str(m.into()))]
}

#[test]
fn a_plugin_that_never_returns_is_stopped_at_its_time_limit() {
    let limits = Limits {
        call_ms: 50,
        first_render_ms: 50,
        ..Limits::default()
    };
    let (mut w, seen) = start(&plugin("limits-test"), &mode("spin"), None, limits);
    let began = Instant::now();
    let stopped = w.render(frame(10.0, 10.0, true)).unwrap_err();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(stopped.reason, "ran past its 50 ms time limit");
    // It stays stopped, and the log says why.
    assert_eq!(w.subscriptions().unwrap_err(), stopped);
    assert_eq!(w.stopped(), Some(&stopped));
    assert!(
        seen.lock()
            .unwrap()
            .logs
            .iter()
            .any(|l| l.contains("time limit"))
    );
}

#[test]
fn a_panic_stops_the_plugin_with_its_message() {
    let (mut w, _) = start(
        &plugin("limits-test"),
        &mode("panic"),
        None,
        Limits::default(),
    );
    let stopped = w.render(frame(10.0, 10.0, true)).unwrap_err();
    assert!(
        stopped.reason.contains("panicked on purpose"),
        "{}",
        stopped.reason
    );
}

#[test]
fn a_plugin_past_its_memory_limit_is_stopped() {
    let limits = Limits {
        memory_bytes: 16 << 20,
        ..Limits::default()
    };
    let (mut w, _) = start(&plugin("limits-test"), &mode("grow"), None, limits);
    assert!(w.render(frame(10.0, 10.0, true)).is_err());
}

#[test]
fn usage_counts_calls_and_memory_against_the_limits() {
    let limits = Limits {
        call_ms: 100,
        memory_bytes: 8 << 20,
        ..Limits::default()
    };
    let (mut w, _) = start(&plugin("limits-test"), &mode("ok"), None, limits);
    let before = w.usage();
    assert_eq!(before.limits, limits);
    assert_eq!(before.render.calls, 0);
    // Its memory from the start (a WebAssembly page is 64 KiB).
    assert!(before.memory_bytes >= 64 << 10, "{before:?}");
    for _ in 0..3 {
        w.render(frame(10.0, 10.0, true)).unwrap();
    }
    w.update(&[]).unwrap();
    let after = w.usage();
    assert_eq!((after.render.calls, after.update.calls), (3, 1));
    assert!(after.render.max_ms >= after.render.last_ms && after.render.last_ms >= 0.0);
    // Growing until refused: up near the limit, never past it.
    let (mut grows, _) = start(&plugin("limits-test"), &mode("grow"), None, limits);
    assert!(grows.render(frame(10.0, 10.0, true)).is_err());
    let used = grows.usage().memory_bytes;
    assert!(used > (4 << 20) && used <= limits.memory_bytes, "{used}");
}

#[test]
fn audio_needs_the_sound_permission() {
    let space = Input::KeyDown(Key {
        name: "Space".into(),
        modifiers: Modifiers::empty(),
    });
    // Allowed: it reaches the host (this one plays nothing, and says so).
    let (mut w, seen) = start(&plugin("tab-test"), &[], None, Limits::default());
    w.on_input(&space).unwrap();
    let logs = seen.lock().unwrap().logs.clone();
    assert_eq!(logs, ["no beep: this dashboard plays no audio"]);
    // Without sound = true: refused before the host.
    let quiet = plugin_with("tab-test", |m| m.permissions.sound = false);
    let (mut w, seen) = start(&quiet, &[], None, Limits::default());
    w.on_input(&space).unwrap();
    let logs = seen.lock().unwrap().logs.clone();
    assert_eq!(logs, ["no beep: plugin.toml doesn't have sound = true"]);
}

#[test]
fn the_first_render_may_take_longer_to_load() {
    // slow-start takes about 300 ms on its first render, little after: far
    // from both limits, so a busy machine's late clock can't blur them.
    let limits = Limits {
        call_ms: 20,
        first_render_ms: 3000,
        ..Limits::default()
    };
    let (mut w, _) = start(&plugin("limits-test"), &mode("slow-start"), None, limits);
    w.render(frame(10.0, 10.0, true)).unwrap();
    w.render(frame(10.0, 10.0, true)).unwrap();
    // Without the longer first render, it's stopped at the call limit.
    let tight = Limits {
        first_render_ms: 20,
        ..limits
    };
    let (mut w, _) = start(&plugin("limits-test"), &mode("slow-start"), None, tight);
    let stopped = w.render(frame(10.0, 10.0, true)).unwrap_err();
    assert_eq!(stopped.reason, "ran past its 20 ms time limit");
    // And a spin after the first render: the call limit, not the first's.
    let (mut w, _) = start(&plugin("limits-test"), &mode("spin-later"), None, limits);
    w.render(frame(10.0, 10.0, true)).unwrap();
    let stopped = w.render(frame(10.0, 10.0, true)).unwrap_err();
    assert_eq!(stopped.reason, "ran past its 20 ms time limit");
}

#[test]
fn a_restarted_plugin_runs_again() {
    let p = plugin("limits-test");
    let (mut w, _) = start(&p, &mode("panic"), None, Limits::default());
    assert!(w.render(frame(10.0, 10.0, true)).is_err());
    let (mut again, _) = start(&p, &mode("ok"), None, Limits::default());
    assert_eq!(
        texts(&again.render(frame(10.0, 10.0, true)).unwrap())[0].text,
        "ok"
    );
}

#[test]
fn publish_and_sound_need_their_permissions() {
    let p = plugin("limits-test");
    for m in ["publish", "sound"] {
        let (mut w, seen) = start(&p, &mode(m), None, Limits::default());
        let summary = w.render(frame(10.0, 10.0, true)).unwrap().summary.unwrap();
        assert!(summary.contains("doesn't have"), "{m}: {summary}");
        let seen = seen.lock().unwrap();
        assert!(seen.sounds.is_empty() && seen.published.is_empty());
    }

    let allowed = plugin_with("limits-test", |m| {
        m.permissions.publish = true;
        m.permissions.sound = true;
    });
    let (mut w, seen) = start(&allowed, &mode("sound"), None, Limits::default());
    assert_eq!(
        w.render(frame(10.0, 10.0, true)).unwrap().summary.unwrap(),
        "Ok(())"
    );
    assert_eq!(seen.lock().unwrap().sounds, ["fire 1"]);
    let (mut w, seen) = start(&allowed, &mode("publish"), None, Limits::default());
    w.render(frame(10.0, 10.0, true)).unwrap();
    assert!(
        matches!(&seen.lock().unwrap().published[..], [(k, NtValue::Double(v))] if k == "x" && *v == 1.0)
    );
}

/// `limits-test` in `mode`, with `storage = true`, saving in `dir`.
fn start_storing(mode_name: &str, dir: Option<&Path>, limits: Limits) -> Instance {
    let p = plugin_with("limits-test", |m| m.permissions.storage = true);
    let host = TestHost {
        seen: Arc::default(),
        unit: None,
        storage: dir.map(Path::to_path_buf),
    };
    let props = mode(mode_name).into_iter().map(|(k, v)| (k.to_string(), v));
    runtime()
        .instantiate(&p, Box::new(host), props.collect(), limits)
        .unwrap()
}

fn summary(w: &mut Instance) -> String {
    w.render(frame(10.0, 10.0, true)).unwrap().summary.unwrap()
}

#[test]
fn a_plugin_keeps_saved_values_between_runs() {
    let dir = std::env::temp_dir().join(format!("raptaboard-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut w = start_storing("storage", Some(&dir), Limits::default());
    assert_eq!(summary(&mut w), "Ok(1); path false; keys [\"renders\"]");
    assert_eq!(summary(&mut w), "Ok(2); path false; keys [\"renders\"]");
    // The next run (a new instance) has them.
    let mut again = start_storing("storage", Some(&dir), Limits::default());
    assert!(summary(&mut again).starts_with("Ok(3)"));
    assert_eq!(std::fs::read(dir.join("renders")).unwrap(), b"3");
    // Within its limit only: 100 KiB is past the usual 64, and fits in 128.
    let mut big = start_storing("storage-big", Some(&dir), Limits::default());
    assert!(summary(&mut big).contains("storage full"));
    let roomy = Limits {
        storage_bytes: 128 << 10,
        ..Limits::default()
    };
    let mut big = start_storing("storage-big", Some(&dir), roomy);
    assert_eq!(summary(&mut big), "Ok(())");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn storage_needs_its_permission_and_somewhere_to_save() {
    let (mut w, _) = start(
        &plugin("limits-test"),
        &mode("storage"),
        None,
        Limits::default(),
    );
    let s = summary(&mut w);
    assert!(
        s.starts_with("Err(\"plugin.toml doesn't have storage = true\")"),
        "{s}"
    );
    let mut nowhere = start_storing("storage", None, Limits::default());
    let s = summary(&mut nowhere);
    assert!(s.starts_with("Err(\"nowhere to save values here\")"), "{s}");
}

#[test]
fn what_a_plugin_prints_goes_to_the_log() {
    let (mut w, seen) = start(
        &plugin("limits-test"),
        &mode("print"),
        None,
        Limits::default(),
    );
    w.render(frame(10.0, 10.0, true)).unwrap();
    w.render(frame(10.0, 10.0, true)).unwrap();
    assert_eq!(
        seen.lock().unwrap().logs,
        ["hello from limits-test", "hello from limits-test"]
    );
}

#[test]
fn load_reads_a_plugin_folder_and_says_what_is_wrong() {
    let dir = std::env::temp_dir().join(format!("raptaboard-plugin-load-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        plugins_dir().join("number/plugin.toml"),
        dir.join("plugin.toml"),
    )
    .unwrap();

    let missing = runtime().load(&dir).err().unwrap();
    assert!(matches!(missing, LoadError::Read { ref path, .. } if path.ends_with("widget.wasm")));

    std::fs::write(dir.join("widget.wasm"), b"not wasm").unwrap();
    assert!(matches!(runtime().load(&dir), Err(LoadError::Wasm { .. })));

    std::fs::copy(built().join("number.wasm"), dir.join("widget.wasm")).unwrap();
    let loaded = runtime().load(&dir).unwrap();
    assert_eq!(loaded.manifest.name, "number");
    assert_eq!(loaded.folder, dir);

    std::fs::write(dir.join("plugin.toml"), "name = 'x'").unwrap();
    assert!(matches!(
        runtime().load(&dir),
        Err(LoadError::Manifest { .. })
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}
