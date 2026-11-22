//! Opening layout files that are missing, aren't layouts, or have old
//! widget types.

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

#[test]
fn a_large_text_display_opens_and_saves_as_a_fitted_text_display() {
    let path = scratch_path("large-text", "layout.json");
    // One on the grid, one in a list with "Fit text" set off, and a plain
    // Text Display that stays as it is.
    let large = |title: &str, extra: &str| {
        format!(
            r#"{{"title": "{title}", "x": 0, "y": 0, "width": 256, "height": 128,
            "type": "Large Text Display", "properties": {{"topic": "/V"{extra}}}}}"#
        )
    };
    let json = format!(
        r#"{{"version": 1.0, "grid_size": 128, "tabs": [{{"name": "A", "grid_layout": {{
        "containers": [{}, {{"title": "Plain", "x": 256, "y": 0, "width": 256,
            "height": 128, "type": "Text Display", "properties": {{"topic": "/V"}}}}],
        "layouts": [{{"title": "List", "x": 0, "y": 256, "width": 256, "height": 256,
            "type": "List Layout", "properties": {{}}, "children": [{}]}}]}}}}]}}"#,
        large("Grid", r#", "decimal_places": 1"#),
        large("Child", r#", "fit_text": false"#),
    );
    std::fs::write(&path, json).unwrap();
    let mut harness = harness_opening(&path);
    let app = harness.state();
    let by_title = |title: &str| app.layout.widgets().find(|w| w.title == title).unwrap();
    let grid = by_title("Grid");
    assert_eq!(grid.widget_type, "Text Display");
    assert_eq!(grid.properties["fit_text"], true);
    assert_eq!(grid.properties["decimal_places"], 1, "other settings kept");
    let child = by_title("Child");
    assert_eq!(child.widget_type, "Text Display");
    assert_eq!(child.properties["fit_text"], false, "a set Fit text stays");
    assert!(!by_title("Plain").properties.contains_key("fit_text"));
    // Not a problem fixed or an unsaved change: just how it reads.
    assert!(app.file.fixes.is_empty(), "{:?}", app.file.fixes);
    assert!(!app.unsaved());
    // Saving writes the new type.
    harness
        .state_mut()
        .run_file_action(crate::layout_file::FileAction::Save);
    settle(&mut harness);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("Large Text Display"), "{saved}");
    assert!(saved.contains("\"fit_text\": true"), "{saved}");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
