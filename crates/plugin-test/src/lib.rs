//! Test a raptaboard widget plugin with `cargo test`: no dashboard, robot or
//! window needed.
//!
//! A [`Harness`] runs the plugin's built `.wasm` in the dashboard's own plugin
//! runtime, with the same time and memory limits. A test gives it values, a
//! size, the connection state and the theme, sends it clicks and keys, and
//! checks what it drew ([`Drawn`]), its tab mark, and the sounds, values and
//! log lines it asked for.
//!
//! In a plugin crate, add it as a dev-dependency for native builds only (the
//! plugin itself builds for WebAssembly):
//!
//! ```toml
//! [target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
//! raptaboard-plugin-test = { git = "https://github.com/tuffinmuffin/raptaboard" }
//! ```
//!
//! and write tests in `tests/`:
//!
//! ```no_run
//! use raptaboard_plugin_test::Harness;
//!
//! let mut widget = Harness::build(env!("CARGO_MANIFEST_DIR"));
//! widget.size(200.0, 100.0).set("", 1.5);
//! assert!(widget.render().has_text("1.5"));
//! ```
//!
//! [`Harness::build`] builds the plugin for `wasm32-wasip2` first (`rustup
//! target add wasm32-wasip2`), into `target/raptaboard-plugin-test/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use raptaboard_plugin_host::Manifest;
use raptaboard_plugin_host::runtime::{
    self, Color, FrameInfo, Host, Instance, Limits, Picture, Plugin, Region, Runtime, Sample,
    Shape, Size, Stopped, Subscription,
};

pub use raptaboard_plugin_host::runtime::{
    Input, Key, Level, Modifiers, MouseButton, NtValue, Pixels, Point, PointerButton, Theme,
};

/// A value a test can give a plugin: a number, a true/false, text, a list of
/// them, or an [`NtValue`].
///
/// ```
/// use raptaboard_plugin_test::{IntoValue, NtValue};
/// assert_eq!(1.5.into_value(), NtValue::Double(1.5));
/// assert_eq!(vec!["a"].into_value(), NtValue::StrArray(vec!["a".into()]));
/// ```
pub trait IntoValue {
    /// It as the plugin gets it.
    fn into_value(self) -> NtValue;
}

impl IntoValue for NtValue {
    fn into_value(self) -> NtValue {
        self
    }
}
impl IntoValue for f64 {
    fn into_value(self) -> NtValue {
        NtValue::Double(self)
    }
}
impl IntoValue for i64 {
    fn into_value(self) -> NtValue {
        NtValue::Int(self)
    }
}
impl IntoValue for bool {
    fn into_value(self) -> NtValue {
        NtValue::Boolean(self)
    }
}
impl IntoValue for &str {
    fn into_value(self) -> NtValue {
        NtValue::Str(self.into())
    }
}
impl IntoValue for String {
    fn into_value(self) -> NtValue {
        NtValue::Str(self)
    }
}
impl IntoValue for Vec<&str> {
    fn into_value(self) -> NtValue {
        NtValue::StrArray(self.into_iter().map(String::from).collect())
    }
}
impl IntoValue for Vec<String> {
    fn into_value(self) -> NtValue {
        NtValue::StrArray(self)
    }
}
impl IntoValue for Vec<f64> {
    fn into_value(self) -> NtValue {
        NtValue::DoubleArray(self)
    }
}
impl IntoValue for Vec<bool> {
    fn into_value(self) -> NtValue {
        NtValue::BoolArray(self)
    }
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

/// The dashboard's dark theme, as plugins get it (the default).
pub fn dark_theme() -> Theme {
    Theme {
        dark: true,
        text: rgba(140, 140, 140, 255),
        weak_text: rgba(140, 140, 140, 153),
        background: rgba(27, 27, 27, 255),
        card: rgba(10, 10, 10, 255),
        accent: rgba(0, 92, 128, 255),
        error: rgba(244, 67, 54, 255),
        warning: rgba(255, 193, 7, 255),
        info: rgba(33, 150, 243, 255),
    }
}

/// The dashboard's light theme, as plugins get it.
pub fn light_theme() -> Theme {
    Theme {
        dark: false,
        text: rgba(80, 80, 80, 255),
        weak_text: rgba(80, 80, 80, 153),
        background: rgba(248, 248, 248, 255),
        card: rgba(255, 255, 255, 255),
        accent: rgba(144, 209, 255, 255),
        ..dark_theme()
    }
}

/// About how big text is in the dashboard's font: each character about
/// 0.55 of the size wide (0.6 in monospace), lines 1.25 of it tall, wrapped
/// at `max_width` between words. Close enough to lay out a widget; the
/// dashboard measures exactly.
///
/// ```
/// use raptaboard_plugin_test::measure_text;
/// let one = measure_text("abcd", 20.0, None, false);
/// assert_eq!(one.height, 25.0);
/// let wrapped = measure_text("abcd abcd", 20.0, Some(50.0), false);
/// assert_eq!(wrapped.height, 50.0);
/// ```
pub fn measure_text(text: &str, size: f32, max_width: Option<f32>, monospace: bool) -> Size {
    let per_char = size * if monospace { 0.6 } else { 0.55 };
    let line_height = size * 1.25;
    let mut widest: f32 = 0.0;
    let mut lines = 0;
    for paragraph in text.split('\n') {
        let mut width: f32 = 0.0;
        lines += 1;
        for (i, word) in paragraph.split(' ').enumerate() {
            let word_width = word.chars().count() as f32 * per_char;
            let space = if i == 0 { 0.0 } else { per_char };
            if max_width.is_some_and(|m| width > 0.0 && width + space + word_width > m) {
                widest = widest.max(width);
                lines += 1;
                width = word_width;
            } else {
                width += space + word_width;
            }
        }
        widest = widest.max(width);
    }
    Size {
        width: widest,
        height: lines as f32 * line_height,
    }
}

/// Sound a plugin queued with `play-audio`.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    /// 16-bit samples, `channels` interleaved.
    pub samples: Vec<i16>,
    /// Samples a second.
    pub rate: u32,
    /// 1 or 2.
    pub channels: u8,
}

/// What the plugin asked the dashboard for, kept for the test to check.
#[derive(Default)]
struct Asked {
    units: HashMap<String, String>,
    sounds: Vec<String>,
    audio: Vec<Audio>,
    published: Vec<(String, NtValue)>,
    logs: Vec<String>,
}

struct TestHost(Arc<Mutex<Asked>>);

impl Host for TestHost {
    fn unit(&self, topic: &str) -> Option<String> {
        self.0.lock().unwrap().units.get(topic).cloned()
    }
    fn publish(&mut self, key: &str, value: NtValue) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .published
            .push((key.to_string(), value));
        Ok(())
    }
    fn measure_text(&self, text: &str, size: f32, max_width: Option<f32>, monospace: bool) -> Size {
        measure_text(text, size, max_width, monospace)
    }
    fn play_sound(&mut self, sound: &str) -> Result<(), String> {
        self.0.lock().unwrap().sounds.push(sound.to_string());
        Ok(())
    }
    fn play_audio(&mut self, samples: &[i16], rate: u32, channels: u8) -> Result<(), String> {
        self.0.lock().unwrap().audio.push(Audio {
            samples: samples.to_vec(),
            rate,
            channels,
        });
        Ok(())
    }
    fn log(&mut self, message: &str) {
        self.0.lock().unwrap().logs.push(message.to_string());
    }
}

/// The plugin runtime, shared by every test in the program.
fn shared_runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("the plugin runtime starts"))
}

/// The plugin crate in `crate_dir`, built for WebAssembly (once per test
/// program): its `.wasm`.
fn built(crate_dir: &Path) -> Result<PathBuf, String> {
    static BUILT: OnceLock<Mutex<HashMap<PathBuf, PathBuf>>> = OnceLock::new();
    let mut built = BUILT.get_or_init(Default::default).lock().unwrap();
    if let Some(wasm) = built.get(crate_dir) {
        return Ok(wasm.clone());
    }
    let wasm = build(crate_dir)?;
    built.insert(crate_dir.to_path_buf(), wasm.clone());
    Ok(wasm)
}

/// Build the plugin crate in `crate_dir` for `wasm32-wasip2` (release), into
/// its own target folder: the test's own may be locked by the `cargo test`
/// running it.
fn build(crate_dir: &Path) -> Result<PathBuf, String> {
    let output = raptaboard_plugin_host::dev::cargo_in(crate_dir)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("cargo metadata in {}: {e}", crate_dir.display()))?;
    let target = Path::new(metadata["target_directory"].as_str().unwrap_or("target"))
        .join("raptaboard-plugin-test");
    let output = raptaboard_plugin_host::dev::cargo_in(crate_dir)
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-wasip2",
            "--message-format",
            "json-render-diagnostics",
            "--target-dir",
        ])
        .arg(&target)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "building {} for wasm32-wasip2 failed (is the target installed? \
             rustup target add wasm32-wasip2)",
            crate_dir.display()
        ));
    }
    let manifest = crate_dir.join("Cargo.toml");
    let manifest = manifest.canonicalize().unwrap_or(manifest);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|m| m["reason"] == "compiler-artifact")
        .filter(|m| {
            m["manifest_path"]
                .as_str()
                .is_some_and(|p| Path::new(p).canonicalize().ok().as_ref() == Some(&manifest))
        })
        .flat_map(|m| {
            m["filenames"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|f| f.as_str().map(PathBuf::from))
        })
        .find(|f| f.extension().is_some_and(|e| e == "wasm"))
        .ok_or_else(|| format!("building {} made no .wasm", crate_dir.display()))
}

/// Compile a plugin (once per test program for each `.wasm`).
fn compiled(manifest_path: &Path, wasm: &Path, folder: &Path) -> Result<Arc<Plugin>, String> {
    static COMPILED: OnceLock<Mutex<HashMap<PathBuf, Arc<Plugin>>>> = OnceLock::new();
    let mut compiled = COMPILED.get_or_init(Default::default).lock().unwrap();
    if let Some(plugin) = compiled.get(wasm) {
        return Ok(plugin.clone());
    }
    let text = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest =
        Manifest::from_toml(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let bytes = std::fs::read(wasm).map_err(|e| format!("{}: {e}", wasm.display()))?;
    let plugin = Arc::new(
        shared_runtime()
            .compile(manifest, &bytes, folder)
            .map_err(|e| e.to_string())?,
    );
    compiled.insert(wasm.to_path_buf(), plugin.clone());
    Ok(plugin)
}

/// What a plugin drew: its picture, with ways to look through it.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    /// The whole picture, as the plugin returned it.
    pub picture: Picture,
}

impl Drawn {
    /// Every piece of text drawn, in order.
    pub fn texts(&self) -> Vec<&str> {
        self.picture
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Whether some piece of text drawn is exactly `text`.
    pub fn has_text(&self, text: &str) -> bool {
        self.texts().contains(&text)
    }

    /// The text drawn exactly as `text`, with its position, size and color.
    pub fn text(&self, text: &str) -> Option<&runtime::Text> {
        self.picture.shapes.iter().find_map(|s| match s {
            Shape::Text(t) if t.text == text => Some(t),
            _ => None,
        })
    }

    /// The clickable region labelled `label`.
    pub fn region(&self, label: &str) -> Option<&Region> {
        self.picture.regions.iter().find(|r| r.label == label)
    }

    /// What a screen reader says for the widget.
    pub fn summary(&self) -> Option<&str> {
        self.picture.summary.as_deref()
    }

    /// In how many seconds the plugin asked to be drawn again, if it did.
    pub fn redraw_in_s(&self) -> Option<f64> {
        self.picture.redraw_in_s
    }

    /// The pixel images drawn (a game's frame), in order.
    pub fn pixels(&self) -> Vec<&Pixels> {
        self.picture
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Pixels(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    /// Its shapes.
    pub fn shapes(&self) -> &[Shape] {
        &self.picture.shapes
    }
}

/// One widget of a plugin, run in the dashboard's plugin runtime with a
/// test standing in for the dashboard.
pub struct Harness {
    plugin: Arc<Plugin>,
    instance: Instance,
    asked: Arc<Mutex<Asked>>,
    properties: HashMap<String, NtValue>,
    frame: FrameInfo,
    limits: Limits,
    last: Option<Picture>,
}

impl Harness {
    /// The plugin crate in `crate_dir` (pass `env!("CARGO_MANIFEST_DIR")`),
    /// built for WebAssembly, with its `plugin.toml`. Panics, saying why, if
    /// it doesn't build or load.
    pub fn build(crate_dir: impl AsRef<Path>) -> Harness {
        let dir = crate_dir.as_ref();
        let wasm = built(dir).unwrap_or_else(|e| panic!("{e}"));
        Harness::from_files(&dir.join("plugin.toml"), &wasm, dir).unwrap_or_else(|e| panic!("{e}"))
    }

    /// A plugin already built: its manifest and `.wasm`, and the folder its
    /// other files are in.
    pub fn from_files(manifest: &Path, wasm: &Path, folder: &Path) -> Result<Harness, String> {
        let plugin = compiled(manifest, wasm, folder)?;
        let asked = Arc::new(Mutex::new(Asked::default()));
        let properties = runtime::default_properties(&plugin.manifest);
        let limits = Limits::default();
        let instance = shared_runtime()
            .instantiate(
                &plugin,
                Box::new(TestHost(asked.clone())),
                properties.clone(),
                limits,
            )
            .map_err(|s| s.reason)?;
        Ok(Harness {
            plugin,
            instance,
            asked,
            properties,
            frame: FrameInfo {
                size: Size {
                    width: 256.0,
                    height: 128.0,
                },
                time_s: 0.0,
                connected: true,
                locked: false,
                theme: dark_theme(),
            },
            limits,
            last: None,
        })
    }

    /// Its `plugin.toml`.
    pub fn manifest(&self) -> &Manifest {
        &self.plugin.manifest
    }

    /// Start the widget again from scratch, with the same settings, under
    /// `limits` (the dashboard's are 100 ms a call and 64 MiB).
    pub fn with_limits(&mut self, limits: Limits) -> &mut Self {
        self.limits = limits;
        self.restart()
    }

    /// Start the widget again from scratch (as the dashboard's Restart
    /// does), keeping its settings. Values must be set again.
    pub fn restart(&mut self) -> &mut Self {
        self.instance = shared_runtime()
            .instantiate(
                &self.plugin,
                Box::new(TestHost(self.asked.clone())),
                self.properties.clone(),
                self.limits,
            )
            .unwrap_or_else(|s| panic!("the plugin didn't start: {}", s.reason));
        self.last = None;
        self
    }

    /// Set the property `name` (as the edit dialog would). The dashboard
    /// starts a widget again when a setting changes; so does this, so set
    /// properties before values.
    pub fn property(&mut self, name: &str, value: impl IntoValue) -> &mut Self {
        assert!(
            self.plugin.manifest.properties.contains_key(name),
            "plugin.toml has no property {name:?}"
        );
        self.properties.insert(name.to_string(), value.into_value());
        self.restart()
    }

    /// The values the widget asks for.
    pub fn subscriptions(&mut self) -> Vec<Subscription> {
        self.instance
            .subscriptions()
            .unwrap_or_else(|s| panic!("the plugin stopped: {}", s.reason))
    }

    /// Send the plugin a new value: `""` for the topic the widget is bound
    /// to, a key under its table (`"errors"`), or a full path (from a
    /// `tables-of-type` subscription).
    pub fn set(&mut self, topic: &str, value: impl IntoValue) -> &mut Self {
        self.send(vec![(topic, Some(value.into_value()))])
    }

    /// Send several values at once, in one update.
    pub fn set_all<V: IntoValue>(&mut self, values: Vec<(&str, V)>) -> &mut Self {
        let values = values
            .into_iter()
            .map(|(t, v)| (t, Some(v.into_value())))
            .collect();
        self.send(values)
    }

    /// Say a topic stopped being published.
    pub fn remove(&mut self, topic: &str) -> &mut Self {
        self.send(vec![(topic, None)])
    }

    fn send(&mut self, values: Vec<(&str, Option<NtValue>)>) -> &mut Self {
        let time_us = (self.frame.time_s * 1e6) as i64;
        let samples: Vec<Sample> = values
            .into_iter()
            .map(|(topic, value)| Sample {
                topic: topic.to_string(),
                value,
                time_us,
            })
            .collect();
        if let Err(s) = self.instance.update(&samples) {
            panic!("the plugin stopped: {}", s.reason);
        }
        self
    }

    /// The `unit` topic property the robot gave `topic`.
    pub fn unit(&mut self, topic: &str, unit: &str) -> &mut Self {
        self.asked
            .lock()
            .unwrap()
            .units
            .insert(topic.to_string(), unit.to_string());
        self
    }

    /// The widget's size, in pixels (256 × 128 unless set).
    pub fn size(&mut self, width: f32, height: f32) -> &mut Self {
        self.frame.size = Size { width, height };
        self
    }

    /// Whether the dashboard is connected (it is unless set).
    pub fn connected(&mut self, connected: bool) -> &mut Self {
        self.frame.connected = connected;
        self
    }

    /// Whether the layout is locked.
    pub fn locked(&mut self, locked: bool) -> &mut Self {
        self.frame.locked = locked;
        self
    }

    /// The theme ([`dark_theme`] unless set).
    pub fn theme(&mut self, theme: Theme) -> &mut Self {
        self.frame.theme = theme;
        self
    }

    /// Let `seconds` pass: the frames' time, and the next values' time.
    pub fn advance(&mut self, seconds: f64) -> &mut Self {
        self.frame.time_s += seconds;
        self
    }

    /// The frames' time now, in seconds since the widget was made.
    pub fn time_s(&self) -> f64 {
        self.frame.time_s
    }

    /// Draw the widget. Panics, saying why, if the plugin stops.
    pub fn render(&mut self) -> Drawn {
        self.try_render()
            .unwrap_or_else(|s| panic!("the plugin stopped: {}", s.reason))
    }

    /// Draw the widget, or say why the plugin stopped (a panic, a trap,
    /// running past the time or memory limit).
    pub fn try_render(&mut self) -> Result<Drawn, Stopped> {
        let picture = self.instance.render(self.frame)?;
        self.last = Some(picture.clone());
        Ok(Drawn { picture })
    }

    /// Click the region labelled `label` in the last picture (drawing it
    /// first if it hasn't been). Panics if there's no such region.
    pub fn activate(&mut self, label: &str) -> &mut Self {
        if self.last.is_none() {
            self.render();
        }
        let picture = self.last.as_ref().expect("drawn");
        let Some(region) = picture.regions.iter().find(|r| r.label == label) else {
            let labels: Vec<&str> = picture.regions.iter().map(|r| r.label.as_str()).collect();
            panic!("no region labelled {label:?}; there are {labels:?}");
        };
        let id = region.id.clone();
        self.input(Input::Activate(id))
    }

    /// Send the plugin input: a key, a pointer move, focus.
    pub fn input(&mut self, event: Input) -> &mut Self {
        if let Err(s) = self.instance.on_input(&event) {
            panic!("the plugin stopped: {}", s.reason);
        }
        self
    }

    /// The mark the widget wants on its tab.
    pub fn tab_mark(&mut self) -> Option<Level> {
        self.instance
            .tab_mark()
            .unwrap_or_else(|s| panic!("the plugin stopped: {}", s.reason))
    }

    /// Why the plugin stopped, if it has.
    pub fn stopped(&self) -> Option<&Stopped> {
        self.instance.stopped()
    }

    /// The sounds it asked to play since the last look.
    pub fn take_sounds(&mut self) -> Vec<String> {
        std::mem::take(&mut self.asked.lock().unwrap().sounds)
    }

    /// The sound it queued with `play-audio` since the last look.
    pub fn take_audio(&mut self) -> Vec<Audio> {
        std::mem::take(&mut self.asked.lock().unwrap().audio)
    }

    /// The values it asked to publish since the last look: key and value.
    pub fn take_published(&mut self) -> Vec<(String, NtValue)> {
        std::mem::take(&mut self.asked.lock().unwrap().published)
    }

    /// The lines it logged (and printed) since the last look.
    pub fn take_logs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.asked.lock().unwrap().logs)
    }
}
