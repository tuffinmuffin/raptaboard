//! App settings kept between runs: team, robot address, port, the last
//! layout file, whether the tree shows, and the window's size and place.
//!
//! The last layout opened or saved is always remembered, even one named on
//! the command line: it's the file last used, not an option for one run.
//!
//! Settings come from four places, the first that sets a value winning:
//!
//! 1. Command-line flags (this run only; never saved).
//! 2. `./.raptaboard/settings.toml`, in the folder raptaboard starts in.
//! 3. `~/.raptaboard/settings.toml`, in the user's home folder.
//! 4. Built-in defaults ([`defaults`]).
//!
//! Each file may set only some values; the rest come from the next place
//! down. Changes made in the app are written to the local file if there is
//! one, otherwise to the home file, which is created on the first run.
//!
//! ```toml
//! # ~/.raptaboard/settings.toml
//! team = 254        # no team at first: raptaboard isn't tied to one
//! address = "team"   # ds, team, mdns, wifi, sim, or a host name or IP
//! port = 5810
//! show_tree = true
//! locked = false
//! alert_sound = true # a tone when a value can't be sent
//! alert_history = 1000 # alert events kept in the alert history
//! alert_history_ignore = ["Frame Timing:"] # alerts not recorded, by text they contain
//! state_border = false # color the window's border by the robot's state
//! estop_popup = true   # a pop-up over everything when the robot is e-stopped
//! layout = "/Users/me/layouts/match.json"
//! tab = "Teleoperated"
//! tab_index = 0
//!
//! [window]
//! width = 1280.0
//! height = 720.0
//!
//! [state_border_colors] # "#RRGGBB", or "none" for no border in that state
//! disabled = "#C62828"
//! no_robot = "none"
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The folder settings live in, in the starting folder or the home folder.
pub const DIR_NAME: &str = ".raptaboard";
/// The settings file's name inside [`DIR_NAME`].
pub const FILE_NAME: &str = "settings.toml";

/// Settings as one place gives them: each value is `None` where that place
/// doesn't set it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// FRC team number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<u32>,
    /// Where the robot is: `ds`, `team`, `mdns`, `wifi`, `sim`, or a host
    /// name or IP (see [`crate::connection::AddressMode::from_keyword`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// NetworkTables server port.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Whether the NetworkTables tree panel shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_tree: Option<bool>,
    /// Whether the layout is locked, so nothing moves by accident.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// Whether a tone plays when a widget's value can't be sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_sound: Option<bool>,
    /// How many events the alert history keeps (the oldest go first).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_history: Option<usize>,
    /// Alerts the history doesn't record: those whose text contains one of
    /// these (ignoring case).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_history_ignore: Option<Vec<String>>,
    /// The layout file to open at start: the one last opened or saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The tab last open, by name, so it still matches if tabs move. See
    /// [`pick_tab`] for how it's found again.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab: Option<String>,
    /// The last open tab's position (0 for the first), to tell apart tabs
    /// with the same name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_index: Option<usize>,
    /// Whether the window's border shows the robot's state in color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_border: Option<bool>,
    /// The border's color in each state, where it isn't the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_border_colors: Option<StateColors>,
    /// Whether a pop-up covers the window when the robot is e-stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estop_popup: Option<bool>,
    /// The window's size and place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
}

/// The window border's color in each robot state, as `"#RRGGBB"`, or
/// `"none"` for no border; not set is the default (the FMSInfo widget's
/// color for that state, and none for no robot).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StateColors {
    /// Not connected, or no Driver Station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_robot: Option<String>,
    /// Disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<String>,
    /// Enabled in autonomous.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autonomous: Option<String>,
    /// Enabled in teleop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleop: Option<String>,
    /// Enabled in test.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    /// Emergency-stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estop: Option<String>,
}

/// The window's size and, if known, its place on screen, in points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// Width of the window's contents.
    pub width: f32,
    /// Height of the window's contents.
    pub height: f32,
    /// Left edge of the window on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    /// Top edge of the window on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
}

/// The built-in defaults: no team number (raptaboard isn't tied to one;
/// set it in the settings bar or file), robot found by team number, port
/// 5810 (robot code's NetworkTables server), tree shown, unlocked, no
/// layout file.
pub fn defaults() -> Settings {
    Settings {
        team: None,
        address: Some("team".into()),
        port: Some(nt4::proto::ROBOT_CODE_PORT),
        show_tree: Some(true),
        locked: Some(false),
        alert_sound: Some(true),
        alert_history: Some(crate::alert_history::DEFAULT_CAPACITY),
        alert_history_ignore: Some(Vec::new()),
        state_border: Some(false),
        state_border_colors: Some(StateColors::default()),
        estop_popup: Some(true),
        layout: None,
        tab: None,
        tab_index: None,
        window: None,
    }
}

/// Which of `names` (a layout's tabs, in order) to open, given the tab
/// last open: the one at `index` if it's still called `name` (two tabs may
/// share a name), else the first called `name` (tabs moved), else the one
/// at `index` (it was renamed), else the first.
///
/// ```
/// use raptaboard::settings::pick_tab;
/// let names = ["Teleop", "Pit", "Pit"];
/// assert_eq!(pick_tab(&names, Some("Pit"), Some(2)), 2);
/// assert_eq!(pick_tab(&names, Some("Pit"), Some(0)), 1);
/// assert_eq!(pick_tab(&names, Some("Gone"), Some(1)), 1);
/// assert_eq!(pick_tab(&names, None, None), 0);
/// ```
pub fn pick_tab(names: &[&str], name: Option<&str>, index: Option<usize>) -> usize {
    let at_index = index.filter(|&i| i < names.len());
    if let (Some(i), Some(n)) = (at_index, name)
        && names[i] == n
    {
        return i;
    }
    name.and_then(|n| names.iter().position(|t| *t == n))
        .or(at_index)
        .unwrap_or(0)
}

impl Settings {
    /// These settings, with values they don't set taken from `lower`.
    ///
    /// ```
    /// use raptaboard::settings::Settings;
    /// let local = Settings { team: Some(254), ..Default::default() };
    /// let home = Settings { team: Some(3200), port: Some(6810), ..Default::default() };
    /// let both = local.or(home);
    /// assert_eq!((both.team, both.port), (Some(254), Some(6810)));
    /// ```
    pub fn or(self, lower: Settings) -> Settings {
        Settings {
            team: self.team.or(lower.team),
            address: self.address.or(lower.address),
            port: self.port.or(lower.port),
            show_tree: self.show_tree.or(lower.show_tree),
            locked: self.locked.or(lower.locked),
            alert_sound: self.alert_sound.or(lower.alert_sound),
            alert_history: self.alert_history.or(lower.alert_history),
            alert_history_ignore: self.alert_history_ignore.or(lower.alert_history_ignore),
            state_border: self.state_border.or(lower.state_border),
            state_border_colors: self.state_border_colors.or(lower.state_border_colors),
            estop_popup: self.estop_popup.or(lower.estop_popup),
            layout: self.layout.or(lower.layout),
            tab: self.tab.or(lower.tab),
            tab_index: self.tab_index.or(lower.tab_index),
            window: self.window.or(lower.window),
        }
    }

    /// Read settings from TOML text. Keys raptaboard doesn't know are
    /// ignored, so files from newer versions still load.
    pub fn from_toml(text: &str) -> Result<Settings, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// The settings as TOML text, with a comment saying what the file is.
    pub fn to_toml(&self) -> String {
        let body = toml::to_string(self).expect("settings are always valid TOML");
        format!(
            "# raptaboard settings. Command-line flags override these for one run.\n\
             # address: ds, team, mdns, wifi, sim, or a host name or IP.\n\n{body}"
        )
    }
}

/// The settings file inside `folder`: `folder/.raptaboard/settings.toml`.
pub fn file_in(folder: &Path) -> PathBuf {
    folder.join(DIR_NAME).join(FILE_NAME)
}

/// The settings file changes are written to, and what it holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Store {
    /// The file, or `None` with nowhere to save (no home folder found).
    pub file: Option<PathBuf>,
    /// What the file holds; changed values are set here and the whole
    /// thing written back, so values it didn't set stay unset.
    pub own: Settings,
    /// `false` if the file couldn't be read, so saving won't overwrite
    /// what's in it.
    pub writable: bool,
}

impl Store {
    /// Write [`Store::own`] to the file. Does nothing without a writable
    /// file. Writes a temporary file and renames it, as layout saves do.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = self.file.as_deref().filter(|_| self.writable) else {
            return Ok(());
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let mut temp = path.as_os_str().to_owned();
        temp.push(".saving");
        let temp = PathBuf::from(temp);
        std::fs::write(&temp, self.own.to_toml())
            .and_then(|()| std::fs::rename(&temp, path))
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&temp);
            })
    }
}

/// How long settings must be still before they're saved, so dragging the
/// team number doesn't write the file every frame.
pub const SAVE_DELAY: Duration = Duration::from_secs(1);

/// Keeps the settings file up to date while the app runs.
///
/// Each frame the app gives [`Tracker::update`] its settings as they are
/// now. Values that changed since the last frame are set in the file's
/// contents, and the file is written once things have been still for
/// [`SAVE_DELAY`], when the window closes, or when the tracker is dropped.
/// Values never changed in the app (from the command line, say) aren't
/// written.
///
/// ```
/// use std::time::{Duration, Instant};
/// use raptaboard::settings::{Settings, Store, Tracker};
/// let start = Settings { team: Some(254), ..Default::default() };
/// let mut tracker = Tracker::new(Store::default(), start.clone());
/// let t0 = Instant::now();
/// let changed = Settings { team: Some(1678), ..start };
/// tracker.update_at(changed, false, t0).unwrap();
/// assert_eq!(tracker.store().own.team, Some(1678));
/// assert!(tracker.has_unsaved());
/// tracker.update_at(Settings { team: Some(1678), ..Default::default() }, false, t0 + Duration::from_secs(2)).unwrap();
/// assert!(!tracker.has_unsaved());
/// ```
#[derive(Debug)]
pub struct Tracker {
    store: Store,
    /// The settings as they were last frame.
    seen: Settings,
    /// When a value first changed since the last save.
    changed_at: Option<Instant>,
}

impl Tracker {
    /// Track changes to be saved to `store`, starting from `start`, the
    /// settings the app opened with (which aren't changes). A `None` in
    /// `start` makes the first value seen count as a change, except the
    /// window's size, which is only learned on the first frame.
    pub fn new(store: Store, start: Settings) -> Self {
        Tracker {
            store,
            seen: start,
            changed_at: None,
        }
    }

    /// The settings file and what it holds.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Whether there are changes not written yet.
    pub fn has_unsaved(&self) -> bool {
        self.changed_at.is_some()
    }

    /// Note the app's settings as they are `now`, and save if they've been
    /// still long enough or the window is `closing`. An error says why the
    /// file couldn't be written.
    pub fn update(&mut self, now: Settings, closing: bool) -> Result<(), String> {
        self.update_at(now, closing, Instant::now())
    }

    /// [`Tracker::update`] at time `at`, for tests.
    pub fn update_at(&mut self, now: Settings, closing: bool, at: Instant) -> Result<(), String> {
        let own = &mut self.store.own;
        let mut changed = false;
        macro_rules! note {
            ($field:ident) => {
                if now.$field != self.seen.$field {
                    own.$field = now.$field.clone();
                    changed = true;
                }
            };
        }
        note!(team);
        note!(address);
        note!(port);
        note!(show_tree);
        note!(locked);
        note!(alert_sound);
        note!(alert_history);
        note!(alert_history_ignore);
        note!(state_border);
        note!(state_border_colors);
        note!(estop_popup);
        note!(layout);
        note!(tab);
        note!(tab_index);
        // The first size learned is the starting size, not a change.
        if self.seen.window.is_some() {
            note!(window);
        }
        self.seen = now;
        if changed {
            self.changed_at.get_or_insert(at);
        }
        match self.changed_at {
            Some(since) if closing || at.duration_since(since) >= SAVE_DELAY => self.flush(),
            _ => Ok(()),
        }
    }

    /// Write any changes now.
    pub fn flush(&mut self) -> Result<(), String> {
        if self.changed_at.take().is_none() {
            return Ok(());
        }
        self.store.save().map_err(|e| {
            let file = self.store.file.as_deref().unwrap_or(Path::new("settings"));
            format!("Could not save {}: {e}", file.display())
        })
    }
}

impl Drop for Tracker {
    /// Save changes made in the last moment before closing.
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// The window's size and place on screen as egui last reported them, or
/// `None` before it knows.
pub fn window_of(ctx: &eframe::egui::Context) -> Option<Window> {
    ctx.input(|i| {
        let v = i.viewport();
        v.inner_rect.map(|r| Window {
            width: r.width(),
            height: r.height(),
            x: v.outer_rect.map(|o| o.min.x),
            y: v.outer_rect.map(|o| o.min.y),
        })
    })
}

/// Settings gathered from every place, ready to start with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loaded {
    /// Every value, from the first place that sets it (defaults last).
    pub settings: Settings,
    /// Where changes go.
    pub store: Store,
    /// Things to tell the user: a file that couldn't be read, or the home
    /// file being made.
    pub notes: Vec<String>,
}

/// Gather settings: `cli` (from the command line), then the file in `start`
/// (the folder raptaboard starts in), then the file in `home`, then
/// [`defaults`]. If neither file exists, the home file is made, holding the
/// defaults, so there's one to edit.
pub fn load(cli: Settings, start: Option<&Path>, home: Option<&Path>) -> Loaded {
    let mut notes = Vec::new();
    // A file's settings, or `None` if it doesn't exist; a file that can't be
    // read counts as empty and is noted.
    let mut read = |path: &Path| -> Option<Result<Settings, ()>> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                notes.push(format!("Could not read {}: {e}", path.display()));
                return Some(Err(()));
            }
        };
        Some(Settings::from_toml(&text).map_err(|e| {
            notes.push(format!("Ignored {}: {e}", path.display()));
        }))
    };
    let local_file = start.map(file_in);
    let home_file = home.map(file_in);
    let local = local_file.as_deref().and_then(&mut read);
    let home_settings = home_file.as_deref().and_then(&mut read);

    let (file, own) = match (&local, &home_settings) {
        (Some(local), _) => (local_file.clone(), local.clone()),
        (None, Some(home)) => (home_file.clone(), home.clone()),
        (None, None) => (home_file.clone(), Ok(defaults())),
    };
    let store = Store {
        file,
        writable: own.is_ok(),
        own: own.unwrap_or_default(),
    };
    if local.is_none()
        && home_settings.is_none()
        && let Some(path) = &store.file
    {
        match store.save() {
            Ok(()) => notes.push(format!("Made {}", path.display())),
            Err(e) => notes.push(format!("Could not make {}: {e}", path.display())),
        }
    }
    let flatten = |s: Option<Result<Settings, ()>>| s.and_then(Result::ok).unwrap_or_default();
    let settings = cli
        .or(flatten(local))
        .or(flatten(home_settings))
        .or(defaults());
    Loaded {
        settings,
        store,
        notes,
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    /// Empty folders standing in for the starting folder and home, unique
    /// to this test.
    fn folders(name: &str) -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("raptaboard-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (start, home) = (root.join("start"), root.join("home"));
        std::fs::create_dir_all(&start).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        (start, home)
    }

    fn write(folder: &Path, text: &str) {
        let path = file_in(folder);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn the_first_run_makes_the_home_file() {
        let (start, home) = folders("first_run");
        let loaded = load(Settings::default(), Some(&start), Some(&home));
        assert_eq!(loaded.settings, defaults());
        assert_eq!(loaded.store.file, Some(file_in(&home)));
        assert!(loaded.store.writable);
        // The new file holds the defaults, ready to edit.
        let text = std::fs::read_to_string(file_in(&home)).unwrap();
        assert_eq!(Settings::from_toml(&text).unwrap(), defaults());
        assert!(loaded.notes[0].starts_with("Made "), "{:?}", loaded.notes);
        // The next run reads it and makes nothing.
        let again = load(Settings::default(), Some(&start), Some(&home));
        assert!(again.notes.is_empty());
        assert!(!file_in(&start).exists());
    }

    #[test]
    fn each_place_overrides_the_ones_below() {
        let (start, home) = folders("layers");
        write(&home, "team = 3200\nport = 6810\naddress = \"mdns\"\n");
        write(&start, "team = 254\n");
        let cli = Settings {
            address: Some("sim".into()),
            ..Default::default()
        };
        let loaded = load(cli, Some(&start), Some(&home));
        let s = &loaded.settings;
        // The command line beats the files; the local file beats home; home
        // fills in the rest; defaults fill what nobody set.
        assert_eq!(s.address.as_deref(), Some("sim"));
        assert_eq!(s.team, Some(254));
        assert_eq!(s.port, Some(6810));
        assert_eq!(s.show_tree, Some(true));
        // Changes go to the local file, which holds only its own value.
        assert_eq!(loaded.store.file, Some(file_in(&start)));
        assert_eq!(
            loaded.store.own,
            Settings {
                team: Some(254),
                ..Default::default()
            }
        );
    }

    #[test]
    fn a_bad_file_is_noted_and_never_overwritten() {
        let (start, home) = folders("bad");
        write(&home, "team = \"lots\"\n");
        let loaded = load(Settings::default(), Some(&start), Some(&home));
        assert_eq!(loaded.settings.team, None);
        assert!(!loaded.store.writable);
        assert!(
            loaded.notes[0].starts_with("Ignored "),
            "{:?}",
            loaded.notes
        );
        // Saving leaves it alone, so the user can fix it.
        let mut store = loaded.store.clone();
        store.own.team = Some(1);
        store.save().unwrap();
        assert_eq!(
            std::fs::read_to_string(file_in(&home)).unwrap(),
            "team = \"lots\"\n"
        );
    }

    #[test]
    fn saved_settings_load_back() {
        let (start, home) = folders("round_trip");
        let store = Store {
            file: Some(file_in(&home)),
            own: Settings {
                team: Some(254),
                locked: Some(true),
                layout: Some("/tmp/match.json".into()),
                tab: Some("Autonomous".into()),
                tab_index: Some(1),
                window: Some(Window {
                    width: 1400.0,
                    height: 900.0,
                    x: Some(10.0),
                    y: None,
                }),
                ..Default::default()
            },
            writable: true,
        };
        store.save().unwrap();
        let loaded = load(Settings::default(), Some(&start), Some(&home));
        assert_eq!(loaded.store.own, store.own);
        assert_eq!(loaded.settings.team, Some(254));
        // Unknown keys (from a newer version) are fine.
        write(&home, "team = 1\nfuture_setting = true\n");
        let loaded = load(Settings::default(), Some(&start), Some(&home));
        assert_eq!(loaded.settings.team, Some(1));
        assert!(loaded.notes.is_empty());
    }

    #[test]
    fn with_no_home_nothing_is_saved() {
        let loaded = load(Settings::default(), None, None);
        assert_eq!(loaded.settings, defaults());
        assert_eq!(loaded.store.file, None);
        assert!(loaded.notes.is_empty());
        loaded.store.save().unwrap();
    }

    #[test]
    fn the_tracker_saves_changes_once_still() {
        let (_, home) = folders("tracker");
        let store = Store {
            file: Some(file_in(&home)),
            own: Settings::default(),
            writable: true,
        };
        let start = Settings {
            team: Some(254),
            address: Some("sim".into()),
            ..Default::default()
        };
        let mut tracker = Tracker::new(store, start.clone());
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        // Nothing changed: nothing to save.
        tracker.update_at(start.clone(), false, at(0)).unwrap();
        assert!(!tracker.has_unsaved());
        // The window's size, first learned, isn't a change.
        let window = Window {
            width: 800.0,
            height: 600.0,
            x: None,
            y: None,
        };
        let sized = Settings {
            window: Some(window),
            ..start.clone()
        };
        tracker.update_at(sized.clone(), false, at(10)).unwrap();
        assert!(!tracker.has_unsaved());
        // A change is set in the file's contents but waits to be written.
        let locked = Settings {
            locked: Some(true),
            ..sized.clone()
        };
        tracker.update_at(locked.clone(), false, at(100)).unwrap();
        assert!(tracker.has_unsaved());
        assert!(!file_in(&home).exists());
        // Still at 1.1 s after the change: written. Values the app started
        // with (team, address) aren't.
        tracker.update_at(locked.clone(), false, at(1100)).unwrap();
        assert!(!tracker.has_unsaved());
        let saved = Settings::from_toml(&std::fs::read_to_string(file_in(&home)).unwrap()).unwrap();
        assert_eq!(
            saved,
            Settings {
                locked: Some(true),
                ..Default::default()
            }
        );
        // Closing writes at once; so does dropping.
        let resized = Settings {
            window: Some(Window {
                width: 1000.0,
                ..window
            }),
            ..locked
        };
        tracker.update_at(resized, true, at(1200)).unwrap();
        assert!(!tracker.has_unsaved());
        assert!(
            std::fs::read_to_string(file_in(&home))
                .unwrap()
                .contains("width = 1000.0")
        );
    }
}
