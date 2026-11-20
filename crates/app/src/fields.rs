//! The fields raptaboard can show: 2026 REBUILT, built into the program,
//! and any in a `fields/` folder (WPILib `fields` JSONs and their images;
//! see the repo's `fields/README.md`). No drawing here: the Field widget
//! turns the image into a texture.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::types::field::FieldInfo;

/// The field a Field widget shows when none is set.
pub const DEFAULT_ID: &str = "2026-rebuilt-welded";

/// The image every built-in field uses.
const BUILT_IN_IMAGE: &[u8] = include_bytes!("../../../fields/frc/2026-rebuilt.png");

/// The built-in fields: id and JSON.
const BUILT_IN: [(&str, &str); 2] = [
    (
        "2026-rebuilt-welded",
        include_str!("../../../fields/frc/2026-rebuilt-welded.json"),
    ),
    (
        "2026-rebuilt-andymark",
        include_str!("../../../fields/frc/2026-rebuilt-andymark.json"),
    ),
];

/// How deep into a fields folder's subfolders to look.
const MAX_DEPTH: usize = 3;

/// One field that can be shown.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldEntry {
    /// What a widget saves to name it: the JSON's file name without
    /// `.json` (`2026-rebuilt-welded`), or the JSON's whole path for one
    /// picked with a file dialog.
    pub id: String,
    /// What the JSON says.
    pub info: FieldInfo,
    /// Where it came from.
    pub source: FieldSource,
}

/// Where a field came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldSource {
    /// Built into the program.
    BuiltIn,
    /// A JSON file, at this path.
    File(PathBuf),
}

impl FieldEntry {
    /// The image's bytes (PNG or JPEG), if the field has one and it can be
    /// read.
    pub fn image_bytes(&self) -> Option<Cow<'static, [u8]>> {
        let image = self.info.image.as_ref()?;
        match &self.source {
            FieldSource::BuiltIn => Some(Cow::Borrowed(BUILT_IN_IMAGE)),
            FieldSource::File(json) => {
                let path = json.parent()?.join(&image.path);
                std::fs::read(path).ok().map(Cow::Owned)
            }
        }
    }
}

/// The built-in fields.
///
/// ```
/// use raptaboard::fields::{built_in, DEFAULT_ID};
/// assert!(built_in().iter().any(|f| f.id == DEFAULT_ID));
/// ```
pub fn built_in() -> Vec<FieldEntry> {
    BUILT_IN
        .iter()
        .filter_map(|(id, json)| {
            Some(FieldEntry {
                id: (*id).to_owned(),
                info: FieldInfo::parse(json).ok()?,
                source: FieldSource::BuiltIn,
            })
        })
        .collect()
}

/// The folders fields are looked for in: `fields/` in `start` (the folder
/// raptaboard was started in), beside the program `exe`, and in `home`'s
/// `.raptaboard/`.
pub fn folders(start: Option<&Path>, exe: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for folder in [
        start.map(|s| s.join("fields")),
        exe.and_then(Path::parent).map(|d| d.join("fields")),
        home.map(|h| h.join(".raptaboard").join("fields")),
    ]
    .into_iter()
    .flatten()
    {
        if !found.contains(&folder) {
            found.push(folder);
        }
    }
    found
}

/// The fields in `folders` (and their subfolders): each JSON that reads
/// as a field, in name order. Files that don't are skipped.
pub fn scan(folders: &[PathBuf]) -> Vec<FieldEntry> {
    let mut jsons = Vec::new();
    for folder in folders {
        find_jsons(folder, 0, &mut jsons);
    }
    let mut fields: Vec<FieldEntry> = jsons.into_iter().filter_map(|p| from_file(&p)).collect();
    fields.sort_by(|a, b| a.info.name.cmp(&b.info.name));
    fields
}

/// Add the `.json` files in `folder` (and its subfolders, up to
/// [`MAX_DEPTH`]) to `out`.
fn find_jsons(folder: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if depth < MAX_DEPTH {
                find_jsons(&path, depth + 1, out);
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            out.push(path);
        }
    }
}

/// The field in the JSON file at `path`, if it reads as one. Its id is the
/// file name without `.json`.
pub fn from_file(path: &Path) -> Option<FieldEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    let info = FieldInfo::parse(&text).ok()?;
    Some(FieldEntry {
        id: path.file_stem()?.to_string_lossy().into_owned(),
        info,
        source: FieldSource::File(path.to_owned()),
    })
}

/// Every field that can be shown: the built-in ones, then those in
/// `folders` whose id isn't already listed.
pub fn available(folders: &[PathBuf]) -> Vec<FieldEntry> {
    let mut all = built_in();
    for f in scan(folders) {
        if !all.iter().any(|a| a.id == f.id) {
            all.push(f);
        }
    }
    all
}

/// The folders fields are looked for in on this computer (see [`folders`]).
pub fn default_folders() -> Vec<PathBuf> {
    folders(
        std::env::current_dir().ok().as_deref(),
        std::env::current_exe().ok().as_deref(),
        std::env::home_dir().as_deref(),
    )
}

/// The field a widget names with `key`: an id from [`available`], or the
/// path of a field JSON (one picked with a file dialog).
pub fn find(key: &str, folders: &[PathBuf]) -> Option<FieldEntry> {
    if key.ends_with(".json") && Path::new(key).is_file() {
        let mut entry = from_file(Path::new(key))?;
        entry.id = key.to_owned();
        return Some(entry);
    }
    available(folders).into_iter().find(|f| f.id == key)
}

/// A field image decoded to RGBA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// Width, in pixels, after any scaling down.
    pub width: u32,
    /// Height, in pixels.
    pub height: u32,
    /// Four bytes a pixel (red, green, blue, alpha), row by row.
    pub pixels: Vec<u8>,
    /// The image's size before scaling, which a field's pixel bounds
    /// ([`ImageBounds`](crate::types::field::ImageBounds)) are measured in.
    pub source_size: (u32, u32),
}

/// An image (PNG or JPEG) decoded, and made at most `max_width` pixels
/// wide (keeping its shape) so it fits in a texture.
pub fn decode_image(bytes: &[u8], max_width: u32) -> Option<Decoded> {
    let image = image::load_from_memory(bytes).ok()?;
    let source_size = (image.width(), image.height());
    let image = if image.width() > max_width {
        let height = (u64::from(image.height()) * u64::from(max_width) / u64::from(image.width()))
            .max(1) as u32;
        image.resize_exact(max_width, height, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    Some(Decoded {
        width: rgba.width(),
        height: rgba.height(),
        pixels: rgba.into_raw(),
        source_size,
    })
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn fields_in_a_folder_are_found_and_the_built_in_one_wins() {
        let dir = std::env::temp_dir().join(format!("raptaboard-fields-{}", std::process::id()));
        let sub = dir.join("frc");
        std::fs::create_dir_all(&sub).unwrap();
        let field = r#"{"name": "Practice", "field-dimensions": {"length": 10, "width": 5},
            "field-image": {"path": "practice.png", "top": 0, "left": 0, "bottom": 5, "right": 10}}"#;
        std::fs::write(sub.join("practice.json"), field).unwrap();
        std::fs::write(sub.join("notes.json"), "not a field").unwrap();
        // Same id as a built-in field: the built-in one is used.
        std::fs::write(sub.join("2026-rebuilt-welded.json"), field).unwrap();
        let folders = vec![dir.clone()];
        let all = available(&folders);
        let ids: Vec<&str> = all.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            ["2026-rebuilt-welded", "2026-rebuilt-andymark", "practice"]
        );
        assert_eq!(all[0].source, FieldSource::BuiltIn);
        // Found by id, or by the JSON's path; its image is beside it.
        let practice = find("practice", &folders).unwrap();
        assert_eq!(practice.info.length_m, 10.0);
        assert!(practice.image_bytes().is_none(), "no image file yet");
        std::fs::write(sub.join("practice.png"), [1, 2, 3]).unwrap();
        assert_eq!(practice.image_bytes().as_deref(), Some(&[1u8, 2, 3][..]));
        let path = sub.join("practice.json").display().to_string();
        assert_eq!(find(&path, &[]).unwrap().id, path);
        assert!(find("nowhere", &folders).is_none());
        std::fs::remove_dir_all(&dir).ok();
        // The three places looked, without repeats.
        let here = Path::new("/a");
        assert_eq!(
            folders_of(Some(here), Some(Path::new("/a/raptaboard")), None),
            [PathBuf::from("/a/fields")]
        );
    }

    fn folders_of(a: Option<&Path>, b: Option<&Path>, c: Option<&Path>) -> Vec<PathBuf> {
        folders(a, b, c)
    }

    #[test]
    fn the_built_in_image_decodes_small_enough_for_a_texture() {
        let field = &built_in()[0];
        let d = decode_image(&field.image_bytes().unwrap(), 2048).unwrap();
        assert_eq!(d.source_size, (4196, 2035));
        // Scaled down to 2048 wide, the same shape.
        assert_eq!((d.width, d.height), (2048, 2035 * 2048 / 4196));
        assert_eq!(d.pixels.len(), (d.width * d.height * 4) as usize);
        assert!(decode_image(b"not an image", 2048).is_none());
    }
}
