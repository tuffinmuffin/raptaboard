//! Start raptaboard. Settings come from the command line, then
//! `./.raptaboard/settings.toml`, then `~/.raptaboard/settings.toml`, then
//! defaults (see `raptaboard::settings`). A path opens an Elastic layout:
//!
//! ```text
//! cargo run -p raptaboard -- --address sim path/to/layout.json
//! cargo run -p raptaboard -- --help
//! ```
//!
//! Release builds on Windows are window programs: no console window opens
//! beside the dashboard (so startup notes print nothing there; debug builds
//! still have the console). `raptaboard packs …`, `--help` and `--version`
//! print to the terminal they were typed in.

// A Windows release build opens no console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use raptaboard::cli::{self, StartupOptions};
use raptaboard::{packs_cli, settings};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(all(windows, not(debug_assertions)))]
    if prints(&args) {
        raptaboard_win_console::attach_parent();
    }
    // Bad options, --help and --version print and exit here.
    let (given, command) = cli::parse_command(args).unwrap_or_else(|e| e.exit());
    let start = std::env::current_dir().ok();
    let home = std::env::home_dir();
    let loaded = settings::load(given, start.as_deref(), home.as_deref());
    for note in &loaded.notes {
        eprintln!("raptaboard: {note}");
    }
    if let Some(cli::Command::Packs(packs)) = command {
        let sources = loaded.settings.content_sources.clone().unwrap_or_default();
        let code = packs_cli::run_in_home("raptaboard", &packs, sources, home.as_deref(), &mut ask);
        std::process::exit(code);
    }
    let window = loaded.settings.window;
    let mut options = StartupOptions::from(loaded);
    options.content_dir = home.as_deref().map(|h| {
        h.join(settings::DIR_NAME)
            .join(raptaboard::content::DIR_NAME)
    });

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("raptaboard")
        .with_icon(raptaboard::app_icon::window_icon())
        .with_inner_size(window.map_or([1280.0, 720.0], |w| [w.width, w.height]));
    if let Some(w) = window
        && let (Some(x), Some(y)) = (w.x, w.y)
    {
        viewport = viewport.with_position([x, y]);
    }
    let native = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "raptaboard",
        native,
        Box::new(move |_cc| Ok(Box::new(raptaboard::ui::DashboardApp::new(options)?))),
    )
}

/// Ask a question on the terminal, for `raptaboard packs`.
#[cfg(not(all(windows, not(debug_assertions))))]
fn ask(question: &str) -> bool {
    packs_cli::ask_terminal(question)
}

/// A Windows release build can't ask: its terminal doesn't wait for a
/// window program, and goes on reading the keyboard itself. It says to
/// use raptaboard-cli (a console program, which can) or --yes.
#[cfg(all(windows, not(debug_assertions)))]
fn ask(_question: &str) -> bool {
    println!("(raptaboard.exe can't ask in a terminal: use raptaboard-cli, or add --yes)");
    false
}

/// Whether these arguments print rather than open the window.
#[cfg(all(windows, not(debug_assertions)))]
fn prints(args: &[String]) -> bool {
    args.iter().any(|a| {
        matches!(
            a.as_str(),
            "packs" | "--help" | "-h" | "help" | "--version" | "-V"
        )
    })
}
