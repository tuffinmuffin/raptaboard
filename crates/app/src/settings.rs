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
//! layout = "/Users/me/layouts/match.json"
//! tab = "Teleoperated"
//! tab_index = 0
//!
//! [window]
//! width = 1280.0
//! height = 720.0
//! ```

use std::path::{Path, PathBuf};

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
    /// The window's size and place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
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
}
