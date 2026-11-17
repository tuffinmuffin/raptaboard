//! Command-line options, used until settings are saved between runs.
//!
//! ```text
//! raptaboard [--team N] [--address ds|team|mdns|wifi|sim|<host>] [--port N] [layout.json]
//! ```

use crate::connection::AddressMode;

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
  -h, --help        Show this help";

/// What the app starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupOptions {
    /// Elastic layout file to open, if any.
    pub layout_path: Option<String>,
    /// FRC team number.
    pub team: u32,
    /// How to find the robot.
    pub mode: AddressMode,
    /// Host for [`AddressMode::Custom`]; empty otherwise.
    pub custom_address: String,
    /// NetworkTables server port.
    pub port: u16,
}

impl Default for StartupOptions {
    /// No layout, team 3200, robot found by team number, port 5810.
    fn default() -> Self {
        StartupOptions {
            layout_path: None,
            team: 3200,
            mode: AddressMode::TeamNumber,
            custom_address: String::new(),
            port: nt4::proto::ROBOT_CODE_PORT,
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

/// Read the options from the arguments after the program name.
///
/// ```
/// use raptaboard::cli::parse;
/// use raptaboard::connection::AddressMode;
/// let opts = parse(["--address", "sim", "layout.json"].map(String::from)).unwrap();
/// assert_eq!(opts.mode, AddressMode::Localhost);
/// assert_eq!(opts.layout_path.as_deref(), Some("layout.json"));
/// ```
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<StartupOptions, CliError> {
    let mut opts = StartupOptions::default();
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
                opts.team = v
                    .parse()
                    .ok()
                    .filter(|t| (1..=99999).contains(t))
                    .ok_or_else(|| CliError::Invalid(format!("bad team number '{v}'")))?;
            }
            "--address" => {
                let v = value("--address")?;
                opts.mode = match v.as_str() {
                    "ds" => AddressMode::DriverStation,
                    "team" => AddressMode::TeamNumber,
                    "mdns" => AddressMode::SystemCoreMdns,
                    "wifi" => AddressMode::SystemCoreWifi,
                    "sim" | "localhost" => AddressMode::Localhost,
                    host => {
                        opts.custom_address = host.to_owned();
                        AddressMode::Custom
                    }
                };
            }
            "--port" => {
                let v = value("--port")?;
                opts.port = v
                    .parse()
                    .ok()
                    .filter(|p| *p != 0)
                    .ok_or_else(|| CliError::Invalid(format!("bad port '{v}'")))?;
            }
            flag if flag.starts_with('-') => {
                return Err(CliError::Invalid(format!("unknown option '{flag}'")));
            }
            path => {
                if opts.layout_path.is_some() {
                    return Err(CliError::Invalid(format!("extra argument '{path}'")));
                }
                opts.layout_path = Some(path.to_owned());
            }
        }
    }
    Ok(opts)
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_arguments_gives_the_defaults() {
        assert_eq!(parse(args(&[])).unwrap(), StartupOptions::default());
    }

    #[test]
    fn port_is_checked() {
        assert_eq!(parse(args(&["--port", "6810"])).unwrap().port, 6810);
        assert_eq!(parse(args(&[])).unwrap().port, 5810);
        for bad in ["0", "70000", "x"] {
            assert!(matches!(
                parse(args(&["--port", bad])),
                Err(CliError::Invalid(_))
            ));
        }
    }

    #[test]
    fn team_and_mode_keywords() {
        let o = parse(args(&["--team", "254", "--address", "mdns"])).unwrap();
        assert_eq!((o.team, o.mode), (254, AddressMode::SystemCoreMdns));
        for (word, mode) in [
            ("ds", AddressMode::DriverStation),
            ("team", AddressMode::TeamNumber),
            ("wifi", AddressMode::SystemCoreWifi),
            ("sim", AddressMode::Localhost),
            ("localhost", AddressMode::Localhost),
        ] {
            assert_eq!(parse(args(&["--address", word])).unwrap().mode, mode);
        }
    }

    #[test]
    fn other_address_is_a_custom_host() {
        let o = parse(args(&["--address", "10.0.0.5"])).unwrap();
        assert_eq!(o.mode, AddressMode::Custom);
        assert_eq!(o.custom_address, "10.0.0.5");
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
