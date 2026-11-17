//! Start raptaboard. Pass an Elastic layout JSON path to open it:
//!
//! ```text
//! cargo run -p raptaboard -- path/to/layout.json
//! ```

fn main() -> eframe::Result {
    let layout_path = std::env::args().nth(1);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("raptaboard")
            .with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "raptaboard",
        options,
        Box::new(move |_cc| Ok(Box::new(raptaboard::ui::DashboardApp::new(layout_path)))),
    )
}
