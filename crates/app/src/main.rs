//! Start raptaboard. Settings come from the command line, then
//! `./.raptaboard/settings.toml`, then `~/.raptaboard/settings.toml`, then
//! defaults (see `raptaboard::settings`). A path opens an Elastic layout:
//!
//! ```text
//! cargo run -p raptaboard -- --address sim path/to/layout.json
//! cargo run -p raptaboard -- --help
//! ```

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
    let options = StartupOptions::from(loaded);

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("raptaboard")
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
