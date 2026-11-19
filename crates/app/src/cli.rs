//! Command-line options. They override the settings files for one run and
//! are never saved, except that a layout opened is remembered as the last
//! layout (see [`crate::settings`]).
//!
//! ```text
//! raptaboard [--team N] [--address ds|team|mdns|wifi|sim|<host>] [--port N] [--hide-tree] [layout.json]
//! ```

use std::path::PathBuf;

use crate::connection::AddressMode;
use crate::settings::{self, Loaded, Settings};

/// Help text printed for `--help` or a bad option.
pub const USAGE: &str = "\
Usage: raptaboard [OPTIONS] [LAYOUT.json]

Options:
  --team N          Team number (default 3200)
  --address WHERE   Where the robot is (default team):
                      ds     ask the Driver Station on this computer
                      team   10.TE.AM.2 from the team number
                      mdns   SystemCore over mDNS (robot.local)
                      wifi   SystemCore's own WiFi (172.30.0.1)
                      sim    the simulator on this computer (localhost)
                      any other value is used as a host name or IP
  --port N          NetworkTables server port (default 5810, robot code;
                    SystemCore's own server is 6810)
  --hide-tree       Start with the NetworkTables tree panel hidden
  -h, --help        Show this help

LAYOUT.json is an Elastic layout file to open. If it doesn't exist yet, the
app starts with an empty layout and Save writes it there.

Settings are also read from ./.raptaboard/settings.toml, then
~/.raptaboard/settings.toml (made on the first run). Options given here win
for this run and aren't saved.";

/// What the app starts with: every setting resolved, and where changes
/// are saved.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupOptions {
    /// Layout file to open, if any.
    pub layout_path: Option<String>,
    /// Name of the tab to open in it, if it has one by that name.
    pub tab: Option<String>,
    /// FRC team number.
    pub team: u32,
    /// How to find the robot.
    pub mode: AddressMode,
    /// Host for [`AddressMode::Custom`]; empty otherwise.
    pub custom_address: String,
    /// NetworkTables server port.
    pub port: u16,
    /// Show the NetworkTables tree panel at startup.
    pub show_tree: bool,
    /// Start with the layout locked.
    pub locked: bool,
    /// The settings file changes go to.
    pub store: settings::Store,
    /// Things to tell the user at startup, such as a settings file that
    /// couldn't be read.
    pub notes: Vec<String>,
}

impl Default for StartupOptions {
    /// The defaults, with nowhere to save: team 3200, robot found by team
    /// number, port 5810, tree shown, no layout.
    fn default() -> Self {
        StartupOptions::from(Loaded {
            settings: settings::defaults(),
            ..Default::default()
        })
    }
}

impl From<Loaded> for StartupOptions {
    fn from(loaded: Loaded) -> Self {
        let s = loaded.settings.or(settings::defaults());
        let (mode, custom_address) =
            AddressMode::from_keyword(s.address.as_deref().unwrap_or("team"));
        StartupOptions {
            layout_path: s.layout,
            tab: s.tab,
            team: s.team.unwrap_or(3200),
            mode,
            custom_address,
            port: s.port.unwrap_or(nt4::proto::ROBOT_CODE_PORT),
            show_tree: s.show_tree.unwrap_or(true),
            locked: s.locked.unwrap_or(false),
            store: loaded.store,
            notes: loaded.notes,
        }
    }
}

/// Why the command line could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// `--help` was given: print [`USAGE`] and exit without an error.
    Help,
    /// Something was wrong; the message says what.
    Invalid(String),
}

/// Read the options from the arguments after the program name. Only the
/// options given are set; the rest come from the settings files.
///
/// ```
/// use raptaboard::cli::parse;
/// let given = parse(["--address", "sim", "layout.json"].map(String::from)).unwrap();
/// assert_eq!(given.address.as_deref(), Some("sim"));
/// assert_eq!(given.team, None);
/// assert!(given.layout.unwrap().ends_with("layout.json"));
/// ```
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Settings, CliError> {
    let mut given = Settings::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| CliError::Invalid(format!("{name} needs a value")))
        };
        match arg.as_str() {
            "-h" | "--help" => return Err(CliError::Help),
            "--team" => {
                let v = value("--team")?;
                given.team = Some(
                    v.parse()
                        .ok()
                        .filter(|t| (1..=99999).contains(t))
                        .ok_or_else(|| CliError::Invalid(format!("bad team number '{v}'")))?,
                );
            }
            "--address" => given.address = Some(value("--address")?),
            "--hide-tree" => given.show_tree = Some(false),
            "--port" => {
                let v = value("--port")?;
                given.port = Some(
                    v.parse()
                        .ok()
                        .filter(|p| *p != 0)
                        .ok_or_else(|| CliError::Invalid(format!("bad port '{v}'")))?,
                );
            }
            flag if flag.starts_with('-') => {
                return Err(CliError::Invalid(format!("unknown option '{flag}'")));
            }
            path => {
                if given.layout.is_some() {
                    return Err(CliError::Invalid(format!("extra argument '{path}'")));
                }
                given.layout = Some(absolute(path));
            }
        }
    }
    Ok(given)
}

/// `path` made absolute against the current folder, so a remembered layout
/// still opens when raptaboard starts somewhere else.
fn absolute(path: &str) -> String {
    let path = PathBuf::from(path);
    std::path::absolute(&path)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_sets_nothing() {
        assert_eq!(parse(args(&[])).unwrap(), Settings::default());
        // Which leaves the defaults.
        let start = StartupOptions::default();
        assert_eq!(
            (start.team, start.port, start.show_tree),
            (3200, 5810, true)
        );
        assert_eq!(start.mode, AddressMode::TeamNumber);
    }

    #[test]
    fn hide_tree() {
        assert_eq!(
            parse(args(&["--hide-tree"])).unwrap().show_tree,
            Some(false)
        );
    }

    #[test]
    fn port_is_checked() {
        assert_eq!(parse(args(&["--port", "6810"])).unwrap().port, Some(6810));
        for bad in ["0", "70000", "x"] {
            assert!(matches!(
                parse(args(&["--port", bad])),
                Err(CliError::Invalid(_))
            ));
        }
    }

    #[test]
    fn team_and_address_words() {
        let given = parse(args(&["--team", "254", "--address", "mdns"])).unwrap();
        assert_eq!(
            (given.team, given.address.as_deref()),
            (Some(254), Some("mdns"))
        );
        let start = StartupOptions::from(Loaded {
            settings: given,
            ..Default::default()
        });
        assert_eq!((start.team, start.mode), (254, AddressMode::SystemCoreMdns));
        for (word, mode) in [
            ("ds", AddressMode::DriverStation),
            ("team", AddressMode::TeamNumber),
            ("wifi", AddressMode::SystemCoreWifi),
            ("sim", AddressMode::Localhost),
            ("localhost", AddressMode::Localhost),
        ] {
            assert_eq!(AddressMode::from_keyword(word).0, mode);
            // Every mode reads back from its own word.
            assert_eq!(AddressMode::from_keyword(&mode.keyword("")).0, mode);
        }
    }

    #[test]
    fn other_address_is_a_custom_host() {
        let given = parse(args(&["--address", "10.0.0.5"])).unwrap();
        let start = StartupOptions::from(Loaded {
            settings: given,
            ..Default::default()
        });
        assert_eq!(start.mode, AddressMode::Custom);
        assert_eq!(start.custom_address, "10.0.0.5");
    }

    #[test]
    fn layout_paths_are_made_absolute() {
        let given = parse(args(&["layouts/match.json"])).unwrap();
        let path = PathBuf::from(given.layout.unwrap());
        assert!(path.is_absolute());
        assert!(path.ends_with("layouts/match.json"));
    }

    #[test]
    fn mistakes_are_reported() {
        assert_eq!(parse(args(&["--help"])), Err(CliError::Help));
        for bad in [
            &["--team"][..],
            &["--team", "abc"],
            &["--team", "0"],
            &["--address"],
            &["--bogus"],
            &["a.json", "b.json"],
        ] {
            assert!(
                matches!(parse(args(bad)), Err(CliError::Invalid(_))),
                "{bad:?} should fail"
            );
        }
    }
}
