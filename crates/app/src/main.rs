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
//!
//! `raptaboard plugin dev <folder>` opens the plugin sandbox instead of the
//! dashboard (see `raptaboard::sandbox`).

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
    let cli::Parsed {
        settings: given,
        command,
        plugins,
    } = cli::parse_all(args).unwrap_or_else(|e| e.exit());
    // The sandbox shows the robot or sim only when told where it is.
    let address_given = given.address.is_some();
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
    let sandbox = match command {
        Some(cli::Command::Plugin(cli::PluginCommand::Dev { folder, topic })) => {
            Some(open_sandbox(&folder, topic.as_deref(), address_given))
        }
        _ => None,
    };
    for folder in &plugins {
        match raptaboard::plugins::load(folder) {
            Ok(name) => eprintln!("raptaboard: loaded {name} from {}", folder.display()),
            Err(e) => eprintln!("raptaboard: plugin not loaded: {e}"),
        }
    }
    let window = loaded.settings.window;
    let mut options = StartupOptions::from(loaded);
    options.content_dir = home.as_deref().map(|h| {
        h.join(settings::DIR_NAME)
            .join(raptaboard::content::DIR_NAME)
    });
    if sandbox.is_some() {
        // The sandbox changes the address and the layout: none of it is saved,
        // and no packs are offered or loaded.
        options.store.file = None;
        options.content_dir = None;
        options.layout_path = None;
    }

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
        Box::new(move |_cc| {
            let mut app = raptaboard::ui::DashboardApp::new(options)?;
            if let Some(sandbox) = sandbox {
                app.open_sandbox(sandbox);
            }
            Ok(Box::new(app))
        }),
    )
}

/// Load the plugin in `folder` for the sandbox and start it: with a server
/// of its own, or showing `topic` on the robot or sim when an address was
/// given. Exits, saying why, if the plugin can't be loaded.
fn open_sandbox(
    folder: &std::path::Path,
    topic: Option<&str>,
    address_given: bool,
) -> raptaboard::sandbox::Sandbox {
    let fail = |e: String| -> ! {
        eprintln!("raptaboard: plugin not loaded: {e}");
        std::process::exit(1);
    };
    let name = raptaboard::plugins::load(folder).unwrap_or_else(|e| fail(e));
    let Some(ty) = raptaboard::plugins::find(&name) else {
        fail(format!("{name} isn't loaded"));
    };
    let manifest = ty.manifest();
    eprintln!("raptaboard: sandbox for {name} from {}", folder.display());
    if address_given {
        return raptaboard::sandbox::Sandbox::bound_to(&manifest, topic);
    }
    raptaboard::sandbox::Sandbox::start(&manifest)
        .unwrap_or_else(|e| fail(format!("the sandbox's server didn't start: {e}")))
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
