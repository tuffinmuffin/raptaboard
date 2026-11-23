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
//! beside the dashboard (so `--help` and startup notes print nothing there;
//! debug builds still have the console).

// A Windows release build opens no console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use raptaboard::cli::{self, StartupOptions};
use raptaboard::settings;

fn main() -> eframe::Result {
    // Bad options, --help and --version print and exit here.
    let given = cli::parse(std::env::args().skip(1)).unwrap_or_else(|e| e.exit());
    let start = std::env::current_dir().ok();
    let home = std::env::home_dir();
    let loaded = settings::load(given, start.as_deref(), home.as_deref());
    for note in &loaded.notes {
        eprintln!("raptaboard: {note}");
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
