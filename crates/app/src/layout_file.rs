//! Opening and saving layout files.
//!
//! Layouts are Elastic's JSON format, so a file saved here opens in Elastic
//! and the other way round. Saving writes a temporary file next to the
//! layout and then renames it over the old one, so a crash or full disk
//! part way through never leaves a half-written layout.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use raptaboard_layout::ElasticLayout;

/// A file command from the settings bar or the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    /// Choose a layout file and open it (Ctrl+O).
    Open,
    /// Save to the open file, or choose one if there isn't one yet (Ctrl+S).
    Save,
    /// Choose a new file and save there (Ctrl+Shift+S).
    SaveAs,
}

impl FileAction {
    /// This action's keyboard shortcut. Ctrl is Cmd on macOS.
    pub fn shortcut(self) -> KeyboardShortcut {
        let cmd = Modifiers::COMMAND;
        match self {
            FileAction::Open => KeyboardShortcut::new(cmd, Key::O),
            FileAction::Save => KeyboardShortcut::new(cmd, Key::S),
            FileAction::SaveAs => KeyboardShortcut::new(cmd | Modifiers::SHIFT, Key::S),
        }
    }
}

/// Read this frame's file shortcut, if any, and use up its key press.
/// These work even while typing in a text box.
pub fn shortcut(ctx: &egui::Context) -> Option<FileAction> {
    ctx.input_mut(|input| {
        // Save as first: Ctrl+S would also match Ctrl+Shift+S.
        [FileAction::SaveAs, FileAction::Save, FileAction::Open]
            .into_iter()
            .find(|action| input.consume_shortcut(&action.shortcut()))
    })
}

/// Why a layout file couldn't be opened.
#[derive(Debug)]
pub enum LoadError {
    /// There's no file at that path.
    NotFound,
    /// The file couldn't be read or isn't an Elastic layout; says why.
    Bad(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound => f.write_str("file not found"),
            LoadError::Bad(why) => f.write_str(why),
        }
    }
}

/// Read the layout file at `path`.
pub fn load(path: &Path) -> Result<ElasticLayout, LoadError> {
    let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => LoadError::NotFound,
        _ => LoadError::Bad(e.to_string()),
    })?;
    ElasticLayout::from_json(&text).map_err(|e| LoadError::Bad(e.to_string()))
}

/// Write `layout` to `path`, replacing any file there. The old file stays
/// as it was if writing fails.
///
/// ```
/// use raptaboard::layout_file::{load, save};
/// use raptaboard_layout::ElasticLayout;
/// let path = std::env::temp_dir().join("raptaboard_doctest_layout.json");
/// save(&ElasticLayout::default(), &path).unwrap();
/// assert_eq!(load(&path).unwrap(), ElasticLayout::default());
/// # std::fs::remove_file(&path).unwrap();
/// ```
pub fn save(layout: &ElasticLayout, path: &Path) -> std::io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".saving");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, layout.to_json())
        .and_then(|()| std::fs::rename(&temp, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })
}

/// The name to show for a layout: its file name, or "Untitled" if it
/// hasn't been saved yet.
///
/// ```
/// use std::path::Path;
/// use raptaboard::layout_file::display_name;
/// assert_eq!(display_name(Some(Path::new("layouts/match.json"))), "match.json");
/// assert_eq!(display_name(None), "Untitled");
/// ```
pub fn display_name(path: Option<&Path>) -> String {
    path.and_then(Path::file_name).map_or_else(
        || "Untitled".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Ask the user for a layout file to open. Blocks until they choose or
/// cancel. `near` is a file whose folder the dialog starts in.
pub fn pick_open(near: Option<&Path>) -> Option<PathBuf> {
    dialog(near).pick_file()
}

/// Ask the user where to save a layout. Blocks until they choose or cancel.
/// `near` is the current file, if any, whose folder and name it starts with.
pub fn pick_save(near: Option<&Path>) -> Option<PathBuf> {
    let name = near.and_then(Path::file_name).map_or_else(
        || "layout.json".into(),
        |n| n.to_string_lossy().into_owned(),
    );
    dialog(near).set_file_name(name).save_file()
}

fn dialog(near: Option<&Path>) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new().add_filter("Elastic layout", &["json"]);
    match near.and_then(Path::parent).filter(|d| d.is_dir()) {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use raptaboard_layout::Tab;

    /// A path in the system temp folder unique to this test run.
    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("raptaboard-{}-{name}", std::process::id()))
    }

    #[test]
    fn saved_layouts_load_back_unchanged() {
        let path = temp_path("round_trip.json");
        let mut layout = ElasticLayout::default();
        layout.tabs.push(Tab::new("Pit"));
        layout
            .extra
            .insert("future_key".into(), serde_json::json!(42));
        save(&layout, &path).unwrap();
        assert_eq!(load(&path).unwrap(), layout);

        // Saving again replaces the file and leaves no temporary file behind.
        layout.tabs.remove(0);
        save(&layout, &path).unwrap();
        assert_eq!(load(&path).unwrap(), layout);
        let mut temp = path.clone().into_os_string();
        temp.push(".saving");
        assert!(!Path::new(&temp).exists());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn missing_and_bad_files_say_why() {
        assert!(matches!(
            load(&temp_path("never_written.json")),
            Err(LoadError::NotFound)
        ));
        let path = temp_path("not_a_layout.json");
        std::fs::write(&path, "{\"version\": 2.0, \"tabs\": []}").unwrap();
        let Err(LoadError::Bad(why)) = load(&path) else {
            panic!("a version 2 file should not load")
        };
        assert!(why.contains("version 2"), "{why}");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_failed_save_keeps_the_old_file() {
        // Saving into a folder that doesn't exist fails without touching
        // anything.
        let path = temp_path("no_such_folder").join("layout.json");
        assert!(save(&ElasticLayout::default(), &path).is_err());
        assert!(!path.exists());
    }
}
