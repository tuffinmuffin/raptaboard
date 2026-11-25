//! Running plugins: wasmtime, with a time and memory limit on each widget.
//!
//! [`Runtime`] compiles plugins and keeps the clock that enforces time limits.
//! [`Plugin`] is one compiled plugin folder. [`Instance`] is one running
//! widget: it calls the plugin's exports and answers its calls to the host
//! through a [`Host`] the dashboard (or a test) provides.
//!
//! An instance that traps (a panic, an out-of-bounds access, running past its
//! time limit, asking for too much memory) is stopped for good: every call
//! after that returns the same [`Stopped`]. Making a new instance restarts it.
//!
//! Plugins get WASI with no network, no environment and no stdin, and no
//! files but their own folder, read-only at `/pack`; what they print to
//! stdout and stderr goes to the dashboard log, and the last of stderr (a
//! Rust panic's message) is kept in [`Stopped`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant as Clock};

use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, ResourceLimiter, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxView, WasiView};

/// Where a plugin finds its own folder (its pack's files), read-only.
pub const PACK_DIR: &str = "/pack";

/// The audio sample rates a plugin may send, a second.
pub const AUDIO_RATES: std::ops::RangeInclusive<u32> = 8_000..=96_000;

use crate::{Manifest, ManifestError};

mod bindings {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "widget",
        additional_derives: [PartialEq],
    });
}

pub use bindings::raptaboard::dashboard::types::Image as ImageShape;
pub use bindings::raptaboard::dashboard::types::{
    Align, Arc as ArcShape, Circle, Color, Field, FrameInfo, Input, Key, Level, Line, Modifiers,
    MouseButton, NtValue, Picture, Pixels, Point, PointerButton, Polyline, Rect, Region, Role,
    Sample, ScalarValue, Shape, Size, Stroke, Subscription, Text, Theme,
};

use bindings::Widget;
use bindings::raptaboard::dashboard::{host as wit_host, types as wit_types};

/// How often the time-limit clock ticks.
const TICK: Duration = Duration::from_millis(5);

/// A property's default value, as the plugin gets it from `property`: a
/// number property as a double, a bool as a boolean, the rest as text.
///
/// ```
/// use raptaboard_plugin_host::{Manifest, runtime::{default_properties, NtValue}};
/// let m = Manifest::from_toml("name='n'\nversion='0'\napi='0.3'\n[match]\ntopic_types=['double']\n\
///     [properties]\nfont_size={type='number',default=14}\n").unwrap();
/// assert_eq!(default_properties(&m)["font_size"], NtValue::Double(14.0));
/// ```
pub fn default_value(spec: &crate::PropertySpec) -> NtValue {
    use crate::PropertySpec as P;
    match spec {
        P::Number { default } => NtValue::Double(*default),
        P::Bool { default } => NtValue::Boolean(*default),
        P::String { default }
        | P::Color { default }
        | P::Sound { default }
        | P::Enum { default, .. } => NtValue::Str(default.clone()),
    }
}

/// Every property in `manifest`, at its default value.
pub fn default_properties(manifest: &Manifest) -> HashMap<String, NtValue> {
    manifest
        .properties
        .iter()
        .map(|(name, spec)| (name.clone(), default_value(spec)))
        .collect()
}

/// How long one kind of call into a plugin has taken.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CallTime {
    /// How many calls there have been.
    pub calls: u64,
    /// The last one's time, in milliseconds.
    pub last_ms: f64,
    /// The longest, in milliseconds.
    pub max_ms: f64,
}

impl CallTime {
    fn record(&mut self, ms: f64) {
        self.calls += 1;
        self.last_ms = ms;
        self.max_ms = self.max_ms.max(ms);
    }
}

/// What a widget instance has used, against its [`Limits`]: for the plugin
/// sandbox's overlay, and for spotting a plugin near a limit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    /// `update` calls (new values).
    pub update: CallTime,
    /// `render` calls.
    pub render: CallTime,
    /// `on-input` calls.
    pub input: CallTime,
    /// `tab-mark` calls.
    pub tab_mark: CallTime,
    /// Its memory now, in bytes.
    pub memory_bytes: usize,
    /// Its limits.
    pub limits: Limits,
}

/// The kind of call, for [`Usage`].
#[derive(Clone, Copy)]
enum Call {
    Subscriptions,
    Update,
    Render,
    Input,
    TabMark,
}

/// The limits each widget instance runs under.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Longest a single call into the plugin may take, in milliseconds.
    pub call_ms: u64,
    /// Longest its first `render` may take, in milliseconds: the time to
    /// load what it needs (a game's data) before its first picture.
    pub first_render_ms: u64,
    /// Most memory the plugin may use, in bytes.
    pub memory_bytes: usize,
}

impl Default for Limits {
    /// 100 ms a call (3 s for the first render) and 64 MiB of memory.
    fn default() -> Self {
        Limits {
            call_ms: 100,
            first_render_ms: 3000,
            memory_bytes: 64 << 20,
        }
    }
}

impl Limits {
    /// The limits a plugin runs under: the defaults, with the memory its
    /// plugin.toml asks for (`[limits] memory_mb`).
    ///
    /// ```
    /// use raptaboard_plugin_host::{Manifest, runtime::Limits};
    /// let m = Manifest::from_toml("name='g'\nversion='0'\napi='0.3'\ntab='Game'\n\
    ///     [limits]\nmemory_mb=192\n").unwrap();
    /// assert_eq!(Limits::for_manifest(&m).memory_bytes, 192 << 20);
    /// ```
    pub fn for_manifest(manifest: &Manifest) -> Limits {
        let mut limits = Limits::default();
        if let Some(mb) = manifest.limits.memory_mb {
            limits.memory_bytes = (mb as usize) << 20;
        }
        limits
    }
}

/// The epoch ticks a call of `ms` milliseconds may run.
fn ticks_for(ms: u64) -> u64 {
    ms.div_ceil(TICK.as_millis() as u64).max(1) + 1
}

/// Problems loading a plugin folder.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{}: {source}", path.display())]
    Manifest {
        path: PathBuf,
        // Boxed: a TOML error is large, and this keeps every LoadError small.
        source: Box<ManifestError>,
    },
    #[error("{}: not a raptaboard widget: {message}", path.display())]
    Wasm { path: PathBuf, message: String },
}

/// Why an instance stopped running.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("plugin stopped: {reason}")]
pub struct Stopped {
    /// What happened, like "ran past its 100 ms time limit" or a panic message.
    pub reason: String,
}

/// What the plugin can ask of the dashboard, besides the values it was sent.
///
/// The dashboard implements this for each widget; tests implement it to watch
/// what a plugin asks for. Calls come on the thread running the instance.
pub trait Host: Send {
    /// The topic's `unit` property ("m", "rad"), named as in [`Sample`].
    fn unit(&self, _topic: &str) -> Option<String> {
        None
    }
    /// Write a value under the bound table. Only called when the plugin has
    /// the `publish` permission.
    fn publish(&mut self, _key: &str, _value: NtValue) -> Result<(), String> {
        Err("publishing isn't available here".into())
    }
    /// How big this text is drawn: one line, or wrapped at `max_width` pixels.
    fn measure_text(&self, text: &str, size: f32, max_width: Option<f32>, monospace: bool) -> Size;
    /// Play a sound the user picked in a `sound` property. Only called when
    /// the plugin has the `sound` permission.
    /// Queue sound: 16-bit samples, `channels` (1 or 2) interleaved, `rate`
    /// a second (checked before this is called). Only called with
    /// `sound = true` in plugin.toml.
    fn play_audio(&mut self, _samples: &[i16], _rate: u32, _channels: u8) -> Result<(), String> {
        Err("this dashboard plays no audio".into())
    }

    fn play_sound(&mut self, _sound: &str) -> Result<(), String> {
        Err("sound isn't available here".into())
    }
    /// A line for the dashboard log, from the plugin's `log` or its stdout.
    fn log(&mut self, _message: &str) {}

    /// Give the keyboard and mouse back to the dashboard (a game's Quit).
    fn release_focus(&mut self) {}
}

/// Compiles plugins and runs the clock behind their time limits. Make one and
/// share it: compiling is the slow part.
pub struct Runtime {
    engine: Engine,
    linker: Arc<Linker<State>>,
}

impl Runtime {
    /// A runtime with plugins' WASI limited to clocks and random numbers.
    pub fn new() -> wasmtime::Result<Runtime> {
        let mut config = Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        Widget::add_to_linker::<_, HasSelf<_>>(&mut linker, |state: &mut State| state)?;
        start_clock(engine.weak());
        Ok(Runtime {
            engine,
            linker: Arc::new(linker),
        })
    }

    /// Load and compile a plugin folder: `plugin.toml` and `widget.wasm`.
    pub fn load(&self, folder: &Path) -> Result<Plugin, LoadError> {
        let read = |name: &str| {
            let path = folder.join(name);
            std::fs::read(&path).map_err(|source| LoadError::Read { path, source })
        };
        let toml_path = folder.join("plugin.toml");
        let text = String::from_utf8_lossy(&read("plugin.toml")?).into_owned();
        let manifest = Manifest::from_toml(&text).map_err(|source| LoadError::Manifest {
            path: toml_path,
            source: Box::new(source),
        })?;
        let wasm = read("widget.wasm")?;
        self.compile(manifest, &wasm, folder)
    }

    /// Compile a plugin from its manifest and component bytes.
    pub fn compile(
        &self,
        manifest: Manifest,
        wasm: &[u8],
        folder: &Path,
    ) -> Result<Plugin, LoadError> {
        let wasm_error = |message: String| LoadError::Wasm {
            path: folder.join("widget.wasm"),
            message,
        };
        let component =
            Component::new(&self.engine, wasm).map_err(|e| wasm_error(format!("{e:#}")))?;
        // Check it links now, so a plugin built for another interface fails
        // when it's loaded rather than when it's placed.
        self.linker
            .instantiate_pre(&component)
            .map_err(|e| wasm_error(format!("{e:#}")))?;
        Ok(Plugin {
            manifest,
            component,
            folder: folder.to_path_buf(),
        })
    }

    /// Start a widget of this plugin, with its property values.
    pub fn instantiate(
        &self,
        plugin: &Plugin,
        host: Box<dyn Host>,
        properties: HashMap<String, NtValue>,
        limits: Limits,
    ) -> Result<Instance, Stopped> {
        let stderr = MemoryOutputPipe::new(16 << 10);
        let stdout = MemoryOutputPipe::new(16 << 10);
        let mut wasi = WasiCtx::builder();
        wasi.stdout(stdout.clone())
            .stderr(stderr.clone())
            .allow_tcp(false)
            .allow_udp(false);
        // Its own folder, to read (a game's data). Without it (gone), no files.
        if plugin.folder.is_dir() {
            let _ = wasi.preopened_dir(&plugin.folder, PACK_DIR, FsPerms::ReadOnly);
        }
        let wasi = wasi.build();
        let state = State {
            wasi,
            table: ResourceTable::new(),
            limits: Limiter {
                limits: StoreLimitsBuilder::new()
                    .memory_size(limits.memory_bytes)
                    .instances(16)
                    .tables(64)
                    .memories(4)
                    .trap_on_grow_failure(true)
                    .build(),
                memory_bytes: 0,
            },
            host,
            values: HashMap::new(),
            properties,
            publish: plugin.manifest.permissions.publish,
            sound: plugin.manifest.permissions.sound,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.epoch_deadline_trap();
        let ticks = ticks_for(limits.call_ms);
        store.set_epoch_deadline(ticks);
        let mut instance = Instance {
            store,
            widget: None,
            stopped: None,
            ticks,
            call_ms: limits.call_ms,
            first_render_ms: limits.first_render_ms,
            limit_ms: limits.call_ms,
            stderr,
            stdout,
            printed: 0,
            usage: Usage {
                limits,
                ..Usage::default()
            },
        };
        match Widget::instantiate(&mut instance.store, &plugin.component, &self.linker) {
            Ok(widget) => instance.widget = Some(widget),
            Err(e) => return Err(instance.stop(e)),
        }
        Ok(instance)
    }
}

/// One compiled plugin.
pub struct Plugin {
    /// Its `plugin.toml`.
    pub manifest: Manifest,
    component: Component,
    /// The folder it was loaded from (its images are found here).
    pub folder: PathBuf,
}

/// One running widget of a plugin.
pub struct Instance {
    store: Store<State>,
    widget: Option<Widget>,
    stopped: Option<Stopped>,
    ticks: u64,
    call_ms: u64,
    first_render_ms: u64,
    /// The time limit of the call running now, in milliseconds.
    limit_ms: u64,
    stderr: MemoryOutputPipe,
    stdout: MemoryOutputPipe,
    printed: usize,
    usage: Usage,
}

impl Instance {
    /// What it has used so far: each kind of call's time, and its memory
    /// now, against its limits.
    pub fn usage(&self) -> Usage {
        Usage {
            memory_bytes: self.store.data().limits.memory_bytes,
            ..self.usage
        }
    }

    /// What values the widget wants.
    pub fn subscriptions(&mut self) -> Result<Vec<Subscription>, Stopped> {
        self.call(Call::Subscriptions, |w, s| w.call_subscriptions(s))
    }

    /// Send the changes since the last call, oldest first. Each sample's value
    /// is also kept for the plugin's `get`.
    pub fn update(&mut self, samples: &[Sample]) -> Result<(), Stopped> {
        let values = &mut self.store.data_mut().values;
        for sample in samples {
            match &sample.value {
                Some(v) => values.insert(sample.topic.clone(), v.clone()),
                None => values.remove(&sample.topic),
            };
        }
        self.call(Call::Update, |w, s| w.call_update(s, samples))
    }

    /// Draw the widget.
    pub fn render(&mut self, frame: FrameInfo) -> Result<Picture, Stopped> {
        self.call(Call::Render, |w, s| w.call_render(s, frame))
    }

    /// The mark for the widget's tab.
    pub fn tab_mark(&mut self) -> Result<Option<Level>, Stopped> {
        self.call(Call::TabMark, |w, s| w.call_tab_mark(s))
    }

    /// Input inside the widget.
    pub fn on_input(&mut self, event: &Input) -> Result<(), Stopped> {
        self.call(Call::Input, |w, s| w.call_on_input(s, event))
    }

    /// Change the widget's property values (after the edit dialog).
    pub fn set_properties(&mut self, properties: HashMap<String, NtValue>) {
        self.store.data_mut().properties = properties;
    }

    /// Why the instance stopped, if it has.
    pub fn stopped(&self) -> Option<&Stopped> {
        self.stopped.as_ref()
    }

    fn call<R>(
        &mut self,
        kind: Call,
        f: impl FnOnce(&Widget, &mut Store<State>) -> wasmtime::Result<R>,
    ) -> Result<R, Stopped> {
        if let Some(stopped) = &self.stopped {
            return Err(stopped.clone());
        }
        let Some(widget) = &self.widget else {
            return Err(Stopped {
                reason: "never started".into(),
            });
        };
        // The first render loads what the plugin needs: a longer limit.
        let first_render = matches!(kind, Call::Render) && self.usage.render.calls == 0;
        self.limit_ms = if first_render {
            self.first_render_ms.max(self.call_ms)
        } else {
            self.call_ms
        };
        self.store.set_epoch_deadline(if first_render {
            ticks_for(self.limit_ms)
        } else {
            self.ticks
        });
        let started = Clock::now();
        let result = f(widget, &mut self.store);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        match kind {
            Call::Subscriptions => {}
            Call::Update => self.usage.update.record(ms),
            Call::Render => self.usage.render.record(ms),
            Call::Input => self.usage.input.record(ms),
            Call::TabMark => self.usage.tab_mark.record(ms),
        }
        self.pass_on_stdout();
        result.map_err(|e| self.stop(e))
    }

    /// Send any new lines the plugin printed to the log.
    fn pass_on_stdout(&mut self) {
        let out = self.stdout.contents();
        if out.len() > self.printed {
            let text = String::from_utf8_lossy(&out[self.printed..]).into_owned();
            self.printed = out.len();
            for line in text.lines() {
                self.store.data_mut().host.log(line);
            }
        }
    }

    fn stop(&mut self, error: wasmtime::Error) -> Stopped {
        let reason = if matches!(error.downcast_ref(), Some(wasmtime::Trap::Interrupt)) {
            format!("ran past its {} ms time limit", self.limit_ms)
        } else {
            // A Rust panic prints "thread '..' panicked at src/lib.rs:4:5:",
            // its message, then a "note:" about backtraces.
            let stderr = self.stderr.contents();
            let printed = String::from_utf8_lossy(&stderr);
            let said: Vec<&str> = printed
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with("note:"))
                .collect();
            let start = said
                .iter()
                .rposition(|l| l.contains(" panicked at "))
                .unwrap_or(0);
            let message = said[start.min(said.len())..].join(" ");
            if message.is_empty() {
                format!("{error:#}")
            } else {
                message.chars().take(500).collect()
            }
        };
        let stopped = Stopped { reason };
        self.store.data_mut().host.log(&format!("{stopped}"));
        self.stopped = Some(stopped.clone());
        self.widget = None;
        stopped
    }
}

/// wasmtime's limits, counting the memory they let the plugin have.
struct Limiter {
    limits: StoreLimits,
    /// Every memory's size now, in bytes.
    memory_bytes: usize,
}

impl ResourceLimiter for Limiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let allowed = self.limits.memory_growing(current, desired, maximum)?;
        if allowed {
            self.memory_bytes = (self.memory_bytes + desired).saturating_sub(current);
        }
        Ok(allowed)
    }

    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.limits.table_growing(current, desired, maximum)
    }

    fn memory_grow_failed(&mut self, error: wasmtime::Error) -> wasmtime::Result<()> {
        self.limits.memory_grow_failed(error)
    }

    fn table_grow_failed(&mut self, error: wasmtime::Error) -> wasmtime::Result<()> {
        self.limits.table_grow_failed(error)
    }

    fn instances(&self) -> usize {
        self.limits.instances()
    }

    fn tables(&self) -> usize {
        self.limits.tables()
    }

    fn memories(&self) -> usize {
        self.limits.memories()
    }
}

/// The state behind one instance.
struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: Limiter,
    host: Box<dyn Host>,
    values: HashMap<String, NtValue>,
    properties: HashMap<String, NtValue>,
    publish: bool,
    sound: bool,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl wit_types::Host for State {}

impl wit_host::Host for State {
    fn get(&mut self, topic: String) -> Option<NtValue> {
        self.values.get(&topic).cloned()
    }

    fn unit(&mut self, topic: String) -> Option<String> {
        self.host.unit(&topic)
    }

    fn publish(&mut self, key: String, value: NtValue) -> Result<(), String> {
        if !self.publish {
            return Err("plugin.toml doesn't have publish = true".into());
        }
        self.host.publish(&key, value)
    }

    fn property(&mut self, name: String) -> Option<NtValue> {
        self.properties.get(&name).cloned()
    }

    fn measure_text(
        &mut self,
        text: String,
        size: f32,
        max_width: Option<f32>,
        monospace: bool,
    ) -> Size {
        self.host.measure_text(&text, size, max_width, monospace)
    }

    fn play_sound(&mut self, sound: String) -> Result<(), String> {
        if !self.sound {
            return Err("plugin.toml doesn't have sound = true".into());
        }
        self.host.play_sound(&sound)
    }

    fn play_audio(&mut self, samples: Vec<i16>, rate: u32, channels: u8) -> Result<(), String> {
        if !self.sound {
            return Err("plugin.toml doesn't have sound = true".into());
        }
        if !(1..=2).contains(&channels) {
            return Err(format!("{channels} channels: 1 or 2"));
        }
        if !AUDIO_RATES.contains(&rate) {
            return Err(format!(
                "{rate} samples a second: {} to {}",
                AUDIO_RATES.start(),
                AUDIO_RATES.end()
            ));
        }
        self.host.play_audio(&samples, rate, channels)
    }

    fn log(&mut self, message: String) {
        self.host.log(&message);
    }

    fn release_focus(&mut self) {
        self.host.release_focus();
    }
}

/// Tick the engine's epoch until the engine is dropped.
fn start_clock(engine: wasmtime::EngineWeak) {
    let _ = thread::Builder::new()
        .name("plugin clock".into())
        .spawn(move || {
            while let Some(engine) = engine.upgrade() {
                engine.increment_epoch();
                drop(engine);
                thread::sleep(TICK);
            }
        });
}
