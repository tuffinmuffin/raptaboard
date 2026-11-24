//! Plugin widgets in the dashboard: loading plugins, running one worker per
//! widget, and the adapter that makes each plugin a widget type.
//!
//! Plugins are loaded once at startup, from `--plugin <folder>` (a folder
//! with `plugin.toml` and `widget.wasm`). A plugin's type is saved in
//! layouts as `plugin:<name>`; [`crate::widgets::find`] finds it here. While
//! the dashboard runs, a plugin whose `.wasm` changes is loaded again and its
//! widgets restart, for developing one.
//!
//! Each widget of a plugin type gets a [`worker::Worker`]: the plugin running
//! on a thread of its own. Workers are found by the widget's type and
//! settings, so moving or resizing a widget keeps it, and changing a setting
//! starts it again with the new one. Workers belong to the window (egui
//! context) that showed them, and [`frame_started`] drops a window's workers
//! whose widgets are gone from its layout.

pub mod convert;
mod draw;
mod widget;
pub mod worker;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use raptaboard_layout::{ElasticLayout, WidgetContainer};
use raptaboard_plugin_host::Manifest;
use raptaboard_plugin_host::runtime::{Plugin, Runtime};

use worker::Worker;

/// How a plugin widget type is saved in layouts: `plugin:<name>`.
pub const TYPE_PREFIX: &str = "plugin:";

/// How often a plugin's `.wasm` is checked for changes.
const RELOAD_CHECK: Duration = Duration::from_secs(1);

/// One loaded plugin, as a widget type.
pub struct PluginType {
    /// `["plugin:<name>"]`.
    names: &'static [&'static str],
    /// The plugin's name.
    pub name: String,
    folder: PathBuf,
    wasm: PathBuf,
    loaded: RwLock<Arc<Loaded>>,
}

struct Loaded {
    plugin: Arc<Plugin>,
    /// Counts reloads, so workers of an older build are dropped.
    generation: u64,
    modified: Option<SystemTime>,
    setting_keys: &'static [&'static str],
    table_types: &'static [&'static str],
}

impl PluginType {
    fn loaded(&self) -> Arc<Loaded> {
        self.loaded.read().unwrap().clone()
    }

    /// The folder the plugin was loaded from.
    pub fn folder(&self) -> &Path {
        &self.folder
    }
}

/// Plugin types and workers, for the whole program.
#[derive(Default)]
struct Registry {
    types: RwLock<Vec<&'static PluginType>>,
    /// By window (its [`owner`] number), then by [`key`].
    workers: Mutex<HashMap<u64, HashMap<String, Arc<Worker>>>>,
    last_check: Mutex<Option<Instant>>,
}

thread_local! {
    /// The window this thread last started a frame of, for starting workers
    /// outside drawing (a closed tab's mark).
    static CURRENT: std::cell::RefCell<Option<egui::Context>> = const { std::cell::RefCell::new(None) };
}

/// The number of the window `ctx` draws, given out the first time it's asked.
fn owner(ctx: &egui::Context) -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = egui::Id::unique("raptaboard plugin owner");
    ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || NEXT.fetch_add(1, Ordering::Relaxed)))
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}

fn runtime() -> Result<Arc<Runtime>, String> {
    static RUNTIME: OnceLock<Result<Arc<Runtime>, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| Runtime::new().map(Arc::new).map_err(|e| format!("{e:#}")))
        .clone()
}

fn leak(strings: Vec<String>) -> &'static [&'static str] {
    let strs: Vec<&'static str> = strings
        .into_iter()
        .map(|s| &*Box::leak(s.into_boxed_str()))
        .collect();
    Box::leak(strs.into_boxed_slice())
}

/// Where a plugin folder's `.wasm` is: `widget.wasm`, or while developing in
/// a cargo workspace, the build beside it
/// (`../target/wasm32-wasip2/release/<name>.wasm`).
fn wasm_path(folder: &Path, manifest: &Manifest) -> PathBuf {
    let own = folder.join("widget.wasm");
    if own.exists() {
        return own;
    }
    let built = folder
        .join("../target/wasm32-wasip2/release")
        .join(format!("{}.wasm", manifest.name.replace('-', "_")));
    if built.exists() { built } else { own }
}

fn compile(folder: &Path, generation: u64) -> Result<(Loaded, PathBuf), String> {
    let text = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|e| format!("{}: {e}", folder.join("plugin.toml").display()))?;
    let manifest = Manifest::from_toml(&text).map_err(|e| format!("{}: {e}", folder.display()))?;
    let wasm = wasm_path(folder, &manifest);
    let bytes = std::fs::read(&wasm).map_err(|e| format!("{}: {e}", wasm.display()))?;
    let modified = std::fs::metadata(&wasm).and_then(|m| m.modified()).ok();
    let setting_keys = leak(manifest.properties.keys().cloned().collect());
    let table_types = leak(manifest.match_rules.type_tag.iter().cloned().collect());
    let plugin = runtime()?
        .compile(manifest, &bytes, folder)
        .map_err(|e| e.to_string())?;
    Ok((
        Loaded {
            plugin: Arc::new(plugin),
            generation,
            modified,
            setting_keys,
            table_types,
        },
        wasm,
    ))
}

/// Load the plugin in `folder`. Returns its widget type's name
/// (`plugin:<name>`), or what's wrong. Loading a plugin of a name already
/// loaded replaces it.
pub fn load(folder: &Path) -> Result<String, String> {
    let (loaded, wasm) = compile(folder, 0)?;
    let name = loaded.plugin.manifest.name.clone();
    let type_name = format!("{TYPE_PREFIX}{name}");
    let mut types = registry().types.write().unwrap();
    if let Some(existing) = types.iter().find(|t| t.name == name) {
        let generation = existing.loaded().generation + 1;
        *existing.loaded.write().unwrap() = Arc::new(Loaded {
            generation,
            ..loaded
        });
        return Ok(type_name);
    }
    let ty: &'static PluginType = Box::leak(Box::new(PluginType {
        names: leak(vec![type_name.clone()]),
        name,
        folder: folder.to_path_buf(),
        wasm,
        loaded: RwLock::new(Arc::new(loaded)),
    }));
    types.push(ty);
    Ok(type_name)
}

/// The plugin widget type saved as `type_name`, if that plugin is loaded.
pub fn find(type_name: &str) -> Option<&'static PluginType> {
    let name = type_name.strip_prefix(TYPE_PREFIX)?;
    registry()
        .types
        .read()
        .unwrap()
        .iter()
        .copied()
        .find(|t| t.name == name)
}

/// Every loaded plugin widget type.
pub fn all() -> Vec<&'static PluginType> {
    registry().types.read().unwrap().clone()
}

/// The worker for `widget`'s key: its type, the plugin's build, and its
/// settings.
fn key(ty: &PluginType, generation: u64, widget: &WidgetContainer) -> String {
    format!(
        "{}#{generation}#{}",
        ty.name,
        serde_json::Value::Object(widget.properties.clone())
    )
}

/// `widget`'s worker in the window `ctx` (or the one this thread last
/// started a frame of), started if it isn't running.
fn worker_for(
    ty: &PluginType,
    widget: &WidgetContainer,
    ctx: Option<&egui::Context>,
) -> Option<Arc<Worker>> {
    let ctx = match ctx {
        Some(c) => c.clone(),
        None => CURRENT.with(|c| c.borrow().clone())?,
    };
    let owner = owner(&ctx);
    let reg = registry();
    let loaded = ty.loaded();
    let key = key(ty, loaded.generation, widget);
    let mut all = reg.workers.lock().unwrap();
    let workers = all.entry(owner).or_default();
    if let Some(w) = workers.get(&key) {
        return Some(w.clone());
    }
    let runtime = runtime().ok()?;
    let properties = convert::properties(&loaded.plugin.manifest.properties, &widget.properties);
    let worker = Arc::new(Worker::start(
        runtime,
        loaded.plugin.clone(),
        properties,
        ctx,
    ));
    workers.insert(key, worker.clone());
    Some(worker)
}

/// Start `widget`'s plugin in the window `ctx` again (after it stopped).
fn restart(ty: &PluginType, widget: &WidgetContainer, ctx: &egui::Context) {
    let key = key(ty, ty.loaded().generation, widget);
    if let Some(workers) = registry().workers.lock().unwrap().get_mut(&owner(ctx)) {
        workers.remove(&key);
    }
}

/// Once a frame of the window `ctx`: remember it for starting workers outside
/// drawing (tab marks), load again any plugin whose `.wasm` changed (once a
/// second), and drop the window's workers of widgets no longer in `layout`.
pub fn frame_started(ctx: &egui::Context, layout: &ElasticLayout) {
    let reg = registry();
    let types = all();
    if types.is_empty() {
        return;
    }
    CURRENT.with(|c| *c.borrow_mut() = Some(ctx.clone()));
    let due = {
        let mut last = reg.last_check.lock().unwrap();
        let due = last.is_none_or(|t| t.elapsed() >= RELOAD_CHECK);
        if due {
            *last = Some(Instant::now());
        }
        due
    };
    if due {
        for ty in &types {
            let modified = std::fs::metadata(&ty.wasm).and_then(|m| m.modified()).ok();
            if modified.is_some() && modified != ty.loaded().modified {
                match load(&ty.folder) {
                    Ok(_) => eprintln!("raptaboard: plugin {} loaded again", ty.name),
                    Err(e) => eprintln!("raptaboard: plugin {} not loaded again: {e}", ty.name),
                }
                ctx.request_repaint();
            }
        }
    }
    let wanted: Vec<String> = layout
        .widgets()
        .filter_map(|w| {
            let ty = find(&w.widget_type)?;
            Some(key(ty, ty.loaded().generation, w))
        })
        .collect();
    if let Some(workers) = reg.workers.lock().unwrap().get_mut(&owner(ctx)) {
        workers.retain(|k, _| wanted.iter().any(|w| w == k));
    }
}

/// Whether `widget` has a worker running now in the window `ctx` (for tests).
#[cfg(test)]
pub(crate) fn has_worker(ctx: &egui::Context, widget: &WidgetContainer) -> bool {
    let Some(ty) = find(&widget.widget_type) else {
        return false;
    };
    let key = key(ty, ty.loaded().generation, widget);
    registry()
        .workers
        .lock()
        .unwrap()
        .get(&owner(ctx))
        .is_some_and(|w| w.contains_key(&key))
}
