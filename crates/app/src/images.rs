//! Images widgets can show (a Boolean Box's image for true or false), kept
//! in `images/` folders: in the folder raptaboard is started in, beside the
//! program, and in `~/.raptaboard/images/`, where images imported from the
//! settings dialog are copied. Layouts save an image's file name, so the
//! same name in another computer's images folder shows the same image.
//!
//! PNG and JPEG (what [`fields::decode_image`] reads). Textures are made
//! once each, on their own thread, and kept while the program runs
//! ([`texture`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::fields;

/// The file types images can be.
pub const EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// The widest an image is made into a texture, pixels (shown on a card,
/// so small is plenty).
const MAX_TEXTURE_WIDTH: u32 = 512;

/// Whether `path` is a file of an image type.
fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The folders images are looked for in: `images/` in `start` (the folder
/// raptaboard was started in), beside the program `exe`, and in `home`'s
/// `.raptaboard/` (the last is where imports go).
pub fn folders(start: Option<&Path>, exe: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for folder in [
        start.map(|s| s.join("images")),
        exe.and_then(Path::parent).map(|d| d.join("images")),
        home.map(|h| h.join(".raptaboard").join("images")),
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

/// The folders images are looked for in on this computer (see
/// [`folders`]).
pub fn default_folders() -> Vec<PathBuf> {
    folders(
        std::env::current_dir().ok().as_deref(),
        std::env::current_exe().ok().as_deref(),
        std::env::home_dir().as_deref(),
    )
}

/// Where imported images are copied: `~/.raptaboard/images/`.
pub fn import_folder() -> Option<PathBuf> {
    std::env::home_dir().map(|h| h.join(".raptaboard").join("images"))
}

/// The images in `folders`, by file name, in name order. A name in more
/// than one folder is listed once (the first folder's is used).
pub fn available(folders: &[PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = folders
        .iter()
        .filter_map(|f| std::fs::read_dir(f).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_image(p))
        .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
        .collect();
    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));
    names.dedup();
    names
}

/// The file for the image named `name`: the first of `folders` that has
/// it.
pub fn find(name: &str, folders: &[PathBuf]) -> Option<PathBuf> {
    // A bare name only, never a path out of the folders.
    if name.is_empty() || name.contains(['/', '\\']) || name == ".." {
        return None;
    }
    folders.iter().map(|f| f.join(name)).find(|p| p.is_file())
}

/// Copy the image at `from` into the folder `into` (made if it isn't
/// there), and return the name it's saved under: its own name, or with
/// `-2`, `-3`, ... before the extension if a different image already has
/// it (the same image already there is used as it is).
pub fn import(from: &Path, into: &Path) -> Result<String, String> {
    if !is_image(from) {
        return Err(format!(
            "{} isn't an image raptaboard reads (PNG or JPEG)",
            from.display()
        ));
    }
    let bytes =
        std::fs::read(from).map_err(|e| format!("Couldn't read {}: {e}", from.display()))?;
    if fields::decode_image(&bytes, 16).is_none() {
        return Err(format!("{} doesn't read as an image", from.display()));
    }
    std::fs::create_dir_all(into).map_err(|e| format!("Couldn't make {}: {e}", into.display()))?;
    let stem = from.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let ext = from
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let to = into.join(&name);
        match std::fs::read(&to) {
            Ok(there) if there == bytes => return Ok(name),
            Ok(_) => continue,
            Err(_) => {
                std::fs::write(&to, &bytes)
                    .map_err(|e| format!("Couldn't save {}: {e}", to.display()))?;
                return Ok(name);
            }
        }
    }
    unreachable!("some name is free")
}

/// An image's texture, being made or made.
enum Slot {
    Loading(Arc<Mutex<Option<Option<fields::Decoded>>>>),
    Ready(egui::TextureHandle),
    Failed,
}

/// The textures made so far, by file path, kept in egui's memory.
#[derive(Clone, Default)]
struct Cache(Arc<Mutex<HashMap<PathBuf, Slot>>>);

/// The texture for the image file at `path`: `None` while it's being
/// decoded (on its own thread, asking for a repaint when done) or if it
/// can't be read.
pub fn texture(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    let cache = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Cache>(egui::Id::unique("raptaboard images"))
            .clone()
    });
    let mut slots = cache.0.lock().unwrap_or_else(|e| e.into_inner());
    let slot = slots.entry(path.to_owned()).or_insert_with(|| {
        let done = Arc::new(Mutex::new(None));
        let (filled, repaint, file) = (done.clone(), ctx.clone(), path.to_owned());
        std::thread::spawn(move || {
            let decoded = std::fs::read(&file)
                .ok()
                .and_then(|b| fields::decode_image(&b, MAX_TEXTURE_WIDTH));
            *filled.lock().unwrap_or_else(|e| e.into_inner()) = Some(decoded);
            repaint.request_repaint();
        });
        Slot::Loading(done)
    });
    if let Slot::Loading(done) = slot {
        let finished = done.lock().unwrap_or_else(|e| e.into_inner()).take();
        match finished {
            None => return None,
            Some(None) => *slot = Slot::Failed,
            Some(Some(d)) => {
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [d.width as usize, d.height as usize],
                    &d.pixels,
                );
                *slot = Slot::Ready(ctx.load_texture(
                    format!("image {}", path.display()),
                    color,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
    }
    match slot {
        Slot::Ready(handle) => Some(handle.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    /// A tiny PNG, `w` by `h`, one color.
    fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// A fresh, empty folder for one test.
    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("raptaboard-images-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn images_are_listed_by_name_and_found_in_the_first_folder() {
        let (a, b) = (temp("list-a"), temp("list-b"));
        std::fs::write(a.join("ready.png"), png(2, 2, [0, 255, 0])).unwrap();
        std::fs::write(b.join("ready.png"), png(2, 2, [255, 0, 0])).unwrap();
        std::fs::write(b.join("Down.JPG"), b"not checked when listing").unwrap();
        std::fs::write(b.join("arm.jpeg"), b"").unwrap();
        std::fs::write(b.join("notes.txt"), b"skip").unwrap();
        let folders = [a.clone(), b.clone()];
        // Name order, any case; a name in both folders once.
        assert_eq!(available(&folders), ["arm.jpeg", "Down.JPG", "ready.png"]);
        assert_eq!(find("ready.png", &folders), Some(a.join("ready.png")));
        assert_eq!(find("arm.jpeg", &folders), Some(b.join("arm.jpeg")));
        // Never a path out of the folders.
        assert_eq!(find("../notes.txt", &folders), None);
        assert_eq!(find("", &folders), None);
    }

    #[test]
    fn importing_copies_and_never_overwrites_a_different_image() {
        let (src, into) = (temp("import-src"), temp("import-into"));
        let green = src.join("ready.png");
        std::fs::write(&green, png(2, 2, [0, 255, 0])).unwrap();
        assert_eq!(import(&green, &into).unwrap(), "ready.png");
        // The same image again: the one already there.
        assert_eq!(import(&green, &into).unwrap(), "ready.png");
        // A different image with the same name: a new name beside it.
        let other = temp("import-other").join("ready.png");
        std::fs::write(&other, png(2, 2, [255, 0, 0])).unwrap();
        assert_eq!(import(&other, &into).unwrap(), "ready-2.png");
        assert_eq!(
            available(std::slice::from_ref(&into)),
            ["ready-2.png", "ready.png"]
        );
        // Not an image.
        let text = src.join("notes.txt");
        std::fs::write(&text, b"hi").unwrap();
        assert!(import(&text, &into).is_err());
        let fake = src.join("fake.png");
        std::fs::write(&fake, b"not a png").unwrap();
        assert!(import(&fake, &into).is_err());
    }
}
