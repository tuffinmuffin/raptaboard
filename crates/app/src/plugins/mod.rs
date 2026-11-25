//! Plugin widgets in the dashboard: loading plugins, running one worker per
//! widget, and the adapter that makes each plugin a widget type.
//!
//! Plugins come from installed content packs ([`use_packs`], at startup and
//! after each install or remove) and from `--plugin <folder>` (a folder with
//! `plugin.toml` and `widget.wasm`, for developing one), which wins over a
//! pack's plugin of the same name. A plugin's type is saved in layouts as
//! `plugin:<name>`; [`crate::widgets::find`] finds it here. While the
//! dashboard runs, a folder plugin whose `.wasm` changes is loaded again and
//! its widgets restart.
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
    loaded: RwLock<Arc<Loaded>>,
}

/// Where a plugin was loaded from.
#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    /// A folder given with `--plugin`: reloaded when its `.wasm` changes.
    Folder,
    /// An installed content pack (its id), installed at that time.
    Pack { id: String, installed_at: u64 },
}

/// A plugin's files.
#[derive(Debug, Clone, PartialEq)]
struct Files {
    manifest: PathBuf,
    wasm: PathBuf,
    /// Where its other files are (images, a game's data).
    folder: PathBuf,
}

struct Loaded {
    plugin: Arc<Plugin>,
    /// Counts reloads, so workers of an older build are dropped.
    generation: u64,
    modified: Option<SystemTime>,
    setting_keys: &'static [&'static str],
    table_types: &'static [&'static str],
    files: Files,
    origin: Origin,
}

impl PluginType {
    fn loaded(&self) -> Arc<Loaded> {
        self.loaded.read().unwrap().clone()
    }

    /// The folder the plugin was loaded from.
    pub fn folder(&self) -> PathBuf {
        self.loaded().files.folder.clone()
    }

    /// Where it was loaded from.
    pub fn origin(&self) -> Origin {
        self.loaded().origin.clone()
    }

    /// Its `plugin.toml`, as loaded now.
    pub fn manifest(&self) -> Manifest {
        self.loaded().plugin.manifest.clone()
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

/// A folder plugin's files: its `plugin.toml`, and its `.wasm` ([`wasm_path`]),
/// and its name.
fn folder_files(folder: &Path) -> Result<(Files, String), String> {
    let manifest = folder.join("plugin.toml");
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let m = Manifest::from_toml(&text).map_err(|e| format!("{}: {e}", folder.display()))?;
    let files = Files {
        wasm: wasm_path(folder, &m),
        manifest,
        folder: folder.to_path_buf(),
    };
    Ok((files, m.name))
}

/// Compile the plugin in `files`. `allowed`: the permissions its pack said
/// it asks for; asking for any other stops it loading.
fn compile(
    files: Files,
    origin: Origin,
    allowed: Option<&[String]>,
    generation: u64,
) -> Result<Loaded, String> {
    let text = std::fs::read_to_string(&files.manifest)
        .map_err(|e| format!("{}: {e}", files.manifest.display()))?;
    let manifest =
        Manifest::from_toml(&text).map_err(|e| format!("{}: {e}", files.manifest.display()))?;
    if let Some(allowed) = allowed {
        let more: Vec<&str> = manifest
            .permissions
            .names()
            .into_iter()
            .filter(|p| !allowed.iter().any(|a| a == p))
            .collect();
        if !more.is_empty() {
            return Err(format!(
                "{} asks for {}, which its pack didn't list",
                manifest.name,
                more.join(", ")
            ));
        }
    }
    let bytes = std::fs::read(&files.wasm).map_err(|e| format!("{}: {e}", files.wasm.display()))?;
    let modified = std::fs::metadata(&files.wasm)
        .and_then(|m| m.modified())
        .ok();
    let setting_keys = leak(manifest.properties.keys().cloned().collect());
    let table_types = leak(manifest.match_rules.type_tag.iter().cloned().collect());
    let plugin = runtime()?
        .compile(manifest, &bytes, &files.folder)
        .map_err(|e| e.to_string())?;
    Ok(Loaded {
        plugin: Arc::new(plugin),
        generation,
        modified,
        setting_keys,
        table_types,
        files,
        origin,
    })
}

/// Put `loaded` in the registry: in place of a plugin of its name, or new.
/// Returns its widget type's name.
fn register(loaded: Loaded) -> String {
    let name = loaded.plugin.manifest.name.clone();
    let type_name = format!("{TYPE_PREFIX}{name}");
    let mut types = registry().types.write().unwrap();
    if let Some(existing) = types.iter().find(|t| t.name == name) {
        let generation = existing.loaded().generation + 1;
        *existing.loaded.write().unwrap() = Arc::new(Loaded {
            generation,
            ..loaded
        });
        return type_name;
    }
    let ty: &'static PluginType = Box::leak(Box::new(PluginType {
        names: leak(vec![type_name.clone()]),
        name,
        loaded: RwLock::new(Arc::new(loaded)),
    }));
    types.push(ty);
    type_name
}

/// Load the plugin in `folder` (`plugin.toml` and its `.wasm`), for
/// developing one. Returns its widget type's name (`plugin:<name>`), or
/// what's wrong. It replaces any plugin of its name, a pack's too. Loading
/// the same folder again with its `.wasm` unchanged does nothing, so its
/// widgets keep running.
pub fn load(folder: &Path) -> Result<String, String> {
    // One load at a time, so two loads of one folder at once (tests) compile
    // it once: the second finds the first's.
    static LOADING: Mutex<()> = Mutex::new(());
    let _one_at_a_time = LOADING.lock().unwrap_or_else(|e| e.into_inner());
    let (files, name) = folder_files(folder)?;
    let type_name = format!("{TYPE_PREFIX}{name}");
    if let Some(ty) = find(&type_name) {
        let loaded = ty.loaded();
        let modified = std::fs::metadata(&files.wasm)
            .and_then(|m| m.modified())
            .ok();
        if loaded.origin == Origin::Folder && loaded.files == files && loaded.modified == modified {
            return Ok(type_name);
        }
    }
    Ok(register(compile(files, Origin::Folder, None, 0)?))
}

/// How many times the plugin of widget type `type_name` has been loaded
/// again (for tests).
#[cfg(test)]
pub(crate) fn generation(type_name: &str) -> Option<u64> {
    find(type_name).map(|t| t.loaded().generation)
}

/// Use the plugins of the packs installed in the packs folder `store`:
/// load those not loaded (or changed), and drop that folder's whose pack is
/// gone. Where two packs have a
/// plugin of one name, the newest install's is used; a folder plugin
/// (`--plugin`) keeps its name. Returns what couldn't be loaded, a line each.
pub fn use_packs(store: &Path, installed: &[crate::content::InstalledPlugin]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut newest_first: Vec<&crate::content::InstalledPlugin> = installed.iter().collect();
    newest_first.sort_by_key(|p| std::cmp::Reverse(p.installed_at));
    let mut used: Vec<String> = Vec::new();
    for p in newest_first {
        let files = Files {
            manifest: p.manifest.clone(),
            wasm: p.wasm.clone(),
            folder: p.folder.clone(),
        };
        let name = match &p.name {
            Some(n) => n.clone(),
            None => match std::fs::read_to_string(&files.manifest)
                .map_err(|e| e.to_string())
                .and_then(|t| Manifest::from_toml(&t).map_err(|e| e.to_string()))
            {
                Ok(m) => m.name,
                Err(e) => {
                    problems.push(format!("{} (pack {}): {e}", p.wasm.display(), p.pack));
                    continue;
                }
            },
        };
        if used.contains(&name) {
            continue;
        }
        let origin = Origin::Pack {
            id: p.pack.clone(),
            installed_at: p.installed_at,
        };
        if let Some(ty) = find(&format!("{TYPE_PREFIX}{name}")) {
            let loaded = ty.loaded();
            if loaded.origin == Origin::Folder || (loaded.origin == origin && loaded.files == files)
            {
                used.push(name);
                continue;
            }
        }
        match compile(files, origin, Some(&p.permissions), 0) {
            Ok(loaded) if loaded.plugin.manifest.name != name => problems.push(format!(
                "plugin {name} (pack {}): its manifest names it {}",
                p.pack, loaded.plugin.manifest.name
            )),
            Ok(loaded) => {
                register(loaded);
                used.push(name);
            }
            Err(e) => problems.push(format!("plugin {name} (pack {}): {e}", p.pack)),
        }
    }
    registry().types.write().unwrap().retain(|t| {
        let loaded = t.loaded();
        loaded.origin == Origin::Folder
            || !loaded.files.folder.starts_with(store)
            || used.contains(&t.name)
    });
    problems
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
    // Nothing loaded and nothing running (a pack's plugin removed leaves
    // workers to drop).
    if types.is_empty() && reg.workers.lock().unwrap().is_empty() {
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
            let loaded = ty.loaded();
            if loaded.origin != Origin::Folder {
                continue;
            }
            let modified = std::fs::metadata(&loaded.files.wasm)
                .and_then(|m| m.modified())
                .ok();
            if modified.is_some() && modified != loaded.modified {
                match load(&loaded.files.folder) {
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

/// What the plugin sandbox's overlay shows about one plugin widget.
#[derive(Debug, Clone, Default)]
pub struct DevView {
    /// What it has used (call times, memory) against its limits.
    pub usage: Option<raptaboard_plugin_host::runtime::Usage>,
    /// Its last log lines, oldest first.
    pub log: Vec<String>,
    /// Why it stopped, if it has.
    pub stopped: Option<String>,
}

/// What the sandbox's overlay shows about `widget` in the window `ctx`, if
/// it's a plugin widget with a worker running.
pub fn dev_view(ctx: &egui::Context, widget: &WidgetContainer) -> Option<DevView> {
    let ty = find(&widget.widget_type)?;
    let key = key(ty, ty.loaded().generation, widget);
    let worker = registry()
        .workers
        .lock()
        .unwrap()
        .get(&owner(ctx))?
        .get(&key)?
        .clone();
    let latest = worker.latest.lock().unwrap();
    Some(DevView {
        usage: latest.usage,
        log: latest.log.iter().cloned().collect(),
        stopped: latest.stopped.as_ref().map(|s| s.reason.clone()),
    })
}

/// Start `widget`'s plugin again from scratch in the window `ctx`.
pub fn restart_widget(ctx: &egui::Context, widget: &WidgetContainer) {
    if let Some(ty) = find(&widget.widget_type) {
        restart(ty, widget, ctx);
        ctx.request_repaint();
    }
}

/// Outline plugin widgets' clickable regions in the window `ctx`, each with
/// its label (the sandbox's overlay), or not.
pub fn set_outline_regions(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(outline_id(), on));
}

/// Whether plugin widgets' regions are outlined in the window `ctx`.
pub fn outline_regions(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(outline_id())).unwrap_or(false)
}

fn outline_id() -> egui::Id {
    egui::Id::unique("raptaboard plugin region outlines")
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
