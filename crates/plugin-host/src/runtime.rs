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
//! Plugins get WASI with no files, no network, no environment and no stdin;
//! what they print to stdout and stderr goes to the dashboard log, and the last
//! of stderr (a Rust panic's message) is kept in [`Stopped`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

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
    NtValue, Picture, Point, Polyline, Rect, Region, Role, Sample, ScalarValue, Shape, Size,
    Stroke, Subscription, Text, Theme,
};

use bindings::Widget;
use bindings::raptaboard::dashboard::{host as wit_host, types as wit_types};

/// How often the time-limit clock ticks.
const TICK: Duration = Duration::from_millis(5);

/// The limits each widget instance runs under.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Longest a single call into the plugin may take, in milliseconds.
    pub call_ms: u64,
    /// Most memory the plugin may use, in bytes.
    pub memory_bytes: usize,
}

impl Default for Limits {
    /// 100 ms a call and 64 MiB of memory.
    fn default() -> Self {
        Limits {
            call_ms: 100,
            memory_bytes: 64 << 20,
        }
    }
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
        source: ManifestError,
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
    fn play_sound(&mut self, _sound: &str) -> Result<(), String> {
        Err("sound isn't available here".into())
    }
    /// A line for the dashboard log, from the plugin's `log` or its stdout.
    fn log(&mut self, _message: &str) {}
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
            source,
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
        let wasi = WasiCtx::builder()
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .allow_tcp(false)
            .allow_udp(false)
            .build();
        let state = State {
            wasi,
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_bytes)
                .instances(16)
                .tables(64)
                .memories(4)
                .trap_on_grow_failure(true)
                .build(),
            host,
            values: HashMap::new(),
            properties,
            publish: plugin.manifest.permissions.publish,
            sound: plugin.manifest.permissions.sound,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.epoch_deadline_trap();
        let ticks = limits.call_ms.div_ceil(TICK.as_millis() as u64).max(1) + 1;
        store.set_epoch_deadline(ticks);
        let mut instance = Instance {
            store,
            widget: None,
            stopped: None,
            ticks,
            call_ms: limits.call_ms,
            stderr,
            stdout,
            printed: 0,
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
    stderr: MemoryOutputPipe,
    stdout: MemoryOutputPipe,
    printed: usize,
}

impl Instance {
    /// What values the widget wants.
    pub fn subscriptions(&mut self) -> Result<Vec<Subscription>, Stopped> {
        self.call(|w, s| w.call_subscriptions(s))
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
        self.call(|w, s| w.call_update(s, samples))
    }

    /// Draw the widget.
    pub fn render(&mut self, frame: FrameInfo) -> Result<Picture, Stopped> {
        self.call(|w, s| w.call_render(s, frame))
    }

    /// The mark for the widget's tab.
    pub fn tab_mark(&mut self) -> Result<Option<Level>, Stopped> {
        self.call(|w, s| w.call_tab_mark(s))
    }

    /// Input inside the widget.
    pub fn on_input(&mut self, event: &Input) -> Result<(), Stopped> {
        self.call(|w, s| w.call_on_input(s, event))
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
        self.store.set_epoch_deadline(self.ticks);
        let result = f(widget, &mut self.store);
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
            format!("ran past its {} ms time limit", self.call_ms)
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

/// The state behind one instance.
struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
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

    fn log(&mut self, message: String) {
        self.host.log(&message);
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
