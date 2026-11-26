//! `raptaboard-cli`: raptaboard's commands that don't open the dashboard,
//! as a console program (`raptaboard-cli packs list`).
//!
//! On Windows, `raptaboard.exe` is a window program: a terminal doesn't
//! wait for it, so its `raptaboard packs …` output lands after the next
//! prompt and it can't ask before installing. This one is a console
//! program, so the terminal waits, the output comes in order and it asks.
//! The Windows packages ship both; the Store's puts `raptaboard-cli` on
//! the PATH (an app execution alias), the installer can too. macOS and
//! Linux have only `raptaboard`, which is already a terminal program there.
//!
//! ```text
//! raptaboard-cli packs list|install|update|remove|sources …
//! raptaboard-cli --help
//! ```

use clap::Parser;
use raptaboard::cli::Command;
use raptaboard::{packs_cli, settings};

/// What can be given on the command line.
#[derive(Debug, Parser)]
#[command(
    name = "raptaboard-cli",
    version,
    about = "raptaboard's commands that don't open the dashboard (content packs).",
    after_help = "Settings (the content sources added) are read as raptaboard reads them: \
                  ./.raptaboard/settings.toml, then ~/.raptaboard/settings.toml."
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

fn main() {
    let args = Args::parse();
    let start = std::env::current_dir().ok();
    let home = std::env::home_dir();
    let loaded = settings::load(
        settings::Settings::default(),
        start.as_deref(),
        home.as_deref(),
    );
    for note in &loaded.notes {
        eprintln!("raptaboard-cli: {note}");
    }
    let sources = loaded.settings.content_sources.unwrap_or_default();
    let prereleases = loaded.settings.content_prereleases.unwrap_or(false);
    let code = match &args.command {
        Command::Packs(packs) => packs_cli::run_in_home(
            "raptaboard-cli",
            packs,
            sources,
            prereleases,
            home.as_deref(),
            &mut packs_cli::ask_terminal,
        ),
        // It opens a window, which this console program doesn't.
        Command::Plugin(_) => {
            eprintln!(
                "raptaboard-cli: the plugin sandbox opens a window: run `raptaboard plugin dev …`"
            );
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn the_options_are_well_formed_and_take_the_same_commands() {
        use clap::CommandFactory;
        Args::command().debug_assert();
        let args =
            Args::try_parse_from(["raptaboard-cli", "packs", "remove", "fan-clips"]).unwrap();
        assert_eq!(
            args.command,
            Command::Packs(packs_cli::Packs::Remove {
                ids: vec!["fan-clips".into()]
            })
        );
        // A command is needed: it never opens the dashboard.
        assert!(Args::try_parse_from(["raptaboard-cli"]).is_err());
        assert!(Args::try_parse_from(["raptaboard-cli", "layout.json"]).is_err());
    }
}
