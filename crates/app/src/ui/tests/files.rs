//! Opening layout files that are missing or aren't layouts.

// Tests written by Claude.

use super::*;

/// A window started with `path` as its layout, connected to nothing.
fn harness_opening(path: &std::path::Path) -> Harness {
    let options = StartupOptions {
        layout_path: Some(path.display().to_string()),
        ..StartupOptions::default()
    };
    let mut app = DashboardApp::new(options).expect("app starts");
    app.sound = crate::sound::Sound::silent();
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .allow_missing_glyphs()
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut DashboardApp| app.show(ui), app);
    settle(&mut harness);
    harness
}

/// A path in a new empty folder of the computer's temporary folder.
fn scratch_path(test: &str, file: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("raptaboard-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp folder");
    dir.join(file)
}

#[test]
fn a_missing_layout_file_says_so_in_a_dialog() {
    let path = scratch_path("missing", "nowhere.json");
    let mut harness = harness_opening(&path);
    // Not just the status line: a dialog, and Save doesn't quietly write
    // to the missing path.
    harness.get_by_label("Layout file not found");
    harness.get_by_label(&path.display().to_string());
    assert_eq!(harness.state().file.path, None);
    // Starting a new layout there is one of the choices.
    harness.get_by_label("New layout saved here").click();
    settle(&mut harness);
    assert!(harness.query_by_label("Layout file not found").is_none());
    assert_eq!(harness.state().file.path.as_deref(), Some(path.as_path()));
    // Closing instead leaves an untitled layout.
    let mut harness = harness_opening(&path);
    harness.get_by_label("Close").click();
    settle(&mut harness);
    assert!(harness.query_by_label("Layout file not found").is_none());
    assert_eq!(harness.state().file.path, None);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn a_file_that_isnt_a_layout_says_why_in_a_dialog() {
    let path = scratch_path("bad", "notes.json");
    std::fs::write(&path, "not a layout").unwrap();
    let mut harness = harness_opening(&path);
    harness.get_by_label("Couldn't open the layout");
    // No "new layout here" for a file that exists: that would overwrite it.
    assert!(harness.query_by_label("New layout saved here").is_none());
    harness.key_press(Key::Escape);
    settle(&mut harness);
    assert!(harness.query_by_label("Couldn't open the layout").is_none());
    assert_eq!(harness.state().file.path, None);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
