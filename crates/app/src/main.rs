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
use raptaboard::{content, packs_cli, settings};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(all(windows, not(debug_assertions)))]
    if prints(&args) {
        console::attach();
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
        std::process::exit(run_packs(&packs, sources, home.as_deref()));
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

/// Run `raptaboard packs …` in the packs folder under `home`: the exit
/// code, 0 if it all worked.
fn run_packs(
    packs: &packs_cli::Packs,
    sources: Vec<String>,
    home: Option<&std::path::Path>,
) -> i32 {
    use std::io::Write;
    let Some(home) = home else {
        eprintln!("raptaboard: no home folder to keep packs in");
        return 1;
    };
    let dir = home.join(settings::DIR_NAME).join(content::DIR_NAME);
    let mut asker = |question: &str| ask(question);
    let mut out = std::io::stdout();
    let mut env = packs_cli::Env {
        store: content::Store::new(dir),
        sources,
        keys: content::OFFICIAL_KEYS,
        fetch: &content::http_get,
        out: &mut out,
        ask: &mut asker,
    };
    let code = match packs_cli::run(packs, &mut env) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("raptaboard: {e}");
            1
        }
    };
    let _ = std::io::stdout().flush();
    code
}

/// Ask `question` on the terminal: whether the answer was yes.
#[cfg(not(all(windows, not(debug_assertions))))]
fn ask(question: &str) -> bool {
    use std::io::Write;
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).is_ok_and(|n| n > 0)
        && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// A Windows release build can't ask: its terminal doesn't wait for a
/// window program, and goes on reading the keyboard itself.
#[cfg(all(windows, not(debug_assertions)))]
fn ask(_question: &str) -> bool {
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

/// A Windows window program's printing goes nowhere, unless it joins the
/// console (terminal) it was started from.
#[cfg(all(windows, not(debug_assertions)))]
mod console {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process: u32) -> i32;
    }
    /// The console of the program that started this one.
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;

    /// Print to the terminal raptaboard was started from, if any.
    pub fn attach() {
        // SAFETY: AttachConsole takes a process id and nothing else; it
        // fails harmlessly when there's no console to join.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
