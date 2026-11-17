//! Start raptaboard. Options pick the robot address; a path opens an Elastic layout:
//!
//! ```text
//! cargo run -p raptaboard -- --address sim path/to/layout.json
//! cargo run -p raptaboard -- --help
//! ```

use raptaboard::cli::{self, CliError};

fn main() -> eframe::Result {
    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(CliError::Help) => {
            println!("{}", cli::USAGE);
            return Ok(());
        }
        Err(CliError::Invalid(message)) => {
            eprintln!("raptaboard: {message}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    let native = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("raptaboard")
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "raptaboard",
        native,
        Box::new(move |_cc| Ok(Box::new(raptaboard::ui::DashboardApp::new(options)?))),
    )
}
