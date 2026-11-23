//! Command-line options, read with `clap` ([`Args`]). They override the
//! settings files for one run and are never saved, except that a layout
//! opened is remembered as the last layout (see [`crate::settings`]).
//!
//! ```text
//! raptaboard [--team N] [--address ds|team|mdns|wifi|sim|<host>] [--port N] [--hide-tree] [layout.json]
//! raptaboard --help
//! ```

use std::path::PathBuf;

use clap::Parser;

use crate::connection::AddressMode;
use crate::settings::{self, Loaded, Settings};

/// What can be given on the command line. Each option overrides the
/// settings files for this run only.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(
    name = "raptaboard",
    version,
    about = "An FRC driver dashboard.",
    after_help = "Settings are also read from ./.raptaboard/settings.toml, then \
                  ~/.raptaboard/settings.toml (made on the first run). Options given here win \
                  for this run and aren't saved."
)]
pub struct Args {
    /// Team number (none until set here or in settings).
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=99999))]
    pub team: Option<u32>,
    /// Where the robot is: ds, team, mdns, wifi, sim, or a host name or IP.
    ///
    ///   ds     ask the Driver Station on this computer
    ///   team   10.TE.AM.2 from the team number
    ///   mdns   SystemCore over mDNS (robot.local)
    ///   wifi   SystemCore's own WiFi (172.30.0.1)
    ///   sim    the simulator on this computer (localhost)
    ///   any other value is used as a host name or IP
    #[arg(long, value_name = "WHERE", verbatim_doc_comment)]
    pub address: Option<String>,
    /// NetworkTables server port (5810 for robot code; SystemCore's own
    /// server is 6810).
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    pub port: Option<u16>,
    /// Start with the NetworkTables tree panel hidden.
    #[arg(long)]
    pub hide_tree: bool,
    /// An Elastic layout file to open. If it doesn't exist yet, the app
    /// starts with an empty layout and Save writes it there.
    #[arg(value_name = "LAYOUT.json")]
    pub layout: Option<PathBuf>,
}

impl Args {
    /// The settings these options give: only the ones set.
    pub fn settings(&self) -> Settings {
        Settings {
            team: self.team,
            address: self.address.clone(),
            port: self.port,
            show_tree: self.hide_tree.then_some(false),
            layout: self.layout.as_deref().map(absolute),
            ..Default::default()
        }
    }
}

/// What the app starts with: every setting resolved, and where changes
/// are saved.
#[derive(Debug, Clone, PartialEq)]
pub struct StartupOptions {
    /// Layout file to open, if any.
    pub layout_path: Option<String>,
    /// The tab last open in it, by name and position (see
    /// [`settings::pick_tab`]).
    pub tab: Option<String>,
    /// See [`StartupOptions::tab`].
    pub tab_index: Option<usize>,
    /// FRC team number; 0 if none has been set.
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
    /// Play a tone when a widget's value can't be sent.
    pub alert_sound: bool,
    /// How many events the alert history keeps.
    pub alert_history: usize,
    /// Alerts the history doesn't record, by text they contain.
    pub alert_history_ignore: Vec<String>,
    /// Content pack sources added (catalog URLs), beside the official one.
    pub content_sources: Vec<String>,
    /// How often to check for content updates, in minutes; 0 for never.
    pub content_check_minutes: u32,
    /// Where content packs are installed (`~/.raptaboard/content`); `None`
    /// for none at all (tests, or no home folder): no packs, and nothing
    /// downloaded.
    pub content_dir: Option<std::path::PathBuf>,
    /// The settings file changes go to.
    pub store: settings::Store,
    /// Things to tell the user at startup, such as a settings file that
    /// couldn't be read.
    pub notes: Vec<String>,
}

impl Default for StartupOptions {
    /// The defaults, with nowhere to save: no team (0), robot found by team
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
            tab_index: s.tab_index,
            team: s.team.unwrap_or(0),
            mode,
            custom_address,
            port: s.port.unwrap_or(nt4::proto::ROBOT_CODE_PORT),
            show_tree: s.show_tree.unwrap_or(true),
            locked: s.locked.unwrap_or(false),
            alert_sound: s.alert_sound.unwrap_or(true),
            alert_history: s
                .alert_history
                .unwrap_or(crate::alert_history::DEFAULT_CAPACITY),
            alert_history_ignore: s.alert_history_ignore.unwrap_or_default(),
            content_sources: s.content_sources.unwrap_or_default(),
            content_check_minutes: s.content_check_minutes.unwrap_or(60),
            // main.rs sets it: the settings files don't say where.
            content_dir: None,
            store: loaded.store,
            notes: loaded.notes,
        }
    }
}

/// Read the options from the arguments after the program name. Only the
/// options given are set; the rest come from the settings files. A bad
/// option, `--help` and `--version` come back as clap's error, which knows
/// how to print itself and which exit code to use.
///
/// ```
/// use raptaboard::cli::parse;
/// let given = parse(["--address", "sim", "layout.json"].map(String::from)).unwrap();
/// assert_eq!(given.address.as_deref(), Some("sim"));
/// assert_eq!(given.team, None);
/// assert!(given.layout.unwrap().ends_with("layout.json"));
/// ```
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Settings, clap::Error> {
    let args = std::iter::once("raptaboard".to_owned()).chain(args);
    Ok(Args::try_parse_from(args)?.settings())
}

/// `path` made absolute against the current folder, so a remembered layout
/// still opens when raptaboard starts somewhere else.
fn absolute(path: &std::path::Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_owned())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use clap::error::ErrorKind;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_options_are_well_formed() {
        // clap checks the definition itself: names, values, help.
        use clap::CommandFactory;
        Args::command().debug_assert();
    }

    #[test]
    fn no_arguments_sets_nothing() {
        assert_eq!(parse(args(&[])).unwrap(), Settings::default());
        // Which leaves the defaults.
        let start = StartupOptions::default();
        assert_eq!((start.team, start.port, start.show_tree), (0, 5810, true));
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
            assert!(parse(args(&["--port", bad])).is_err(), "{bad}");
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
    fn mistakes_and_help_are_reported() {
        let kind = |list: &[&str]| parse(args(list)).unwrap_err().kind();
        assert_eq!(kind(&["--help"]), ErrorKind::DisplayHelp);
        assert_eq!(kind(&["--version"]), ErrorKind::DisplayVersion);
        for bad in [
            &["--team"][..],
            &["--team", "abc"],
            &["--team", "0"],
            &["--address"],
            &["--bogus"],
            &["a.json", "b.json"],
        ] {
            assert!(parse(args(bad)).is_err(), "{bad:?} should fail");
        }
    }
}
