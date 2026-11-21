//! Images widgets can show (a Boolean Box's image for true or false), kept
//! in `images/` folders: in the folder raptaboard is started in, beside the
//! program, and in `~/.raptaboard/images/`, where images imported from the
//! settings dialog are copied. Layouts save an image's file name, so the
//! same name in another computer's images folder shows the same image.
//!
//! A few icons are built in ([`BUILT_IN`], from the repo's `images/`
//! folder, public domain), so they're always offered; a file of the same
//! name in a folder takes their place.
//!
//! PNG, JPEG ([`fields::decode_image`]) and SVG (drawn with `resvg`).
//! Textures are made once each, on their own thread, and kept while the
//! program runs ([`texture`]).

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::fields;

/// The file types images can be.
pub const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "svg"];

/// The icons built into raptaboard, by file name: drawn for it and
/// dedicated to the public domain (CC0; see the repo's `images/README.md`).
pub const BUILT_IN: [(&str, &[u8]); 5] = [
    ("armed.svg", include_bytes!("../../../images/armed.svg")),
    (
        "checkmark.svg",
        include_bytes!("../../../images/checkmark.svg"),
    ),
    ("motor.svg", include_bytes!("../../../images/motor.svg")),
    (
        "motor-off.svg",
        include_bytes!("../../../images/motor-off.svg"),
    ),
    (
        "motor-on.svg",
        include_bytes!("../../../images/motor-on.svg"),
    ),
];

/// The widest an image is made into a texture, pixels (shown on a card,
/// so small is plenty); an SVG is drawn this big on its longer side.
const MAX_TEXTURE_SIZE: u32 = 256;

/// Where an image comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A file in an images folder.
    File(PathBuf),
    /// One of [`BUILT_IN`]: its name and bytes.
    BuiltIn(&'static str, &'static [u8]),
}

impl Source {
    /// Its name for the texture cache.
    fn key(&self) -> String {
        match self {
            Source::File(path) => path.display().to_string(),
            Source::BuiltIn(name, _) => format!("built in {name}"),
        }
    }

    /// Its file name (an SVG is drawn, anything else decoded).
    fn name(&self) -> String {
        match self {
            Source::File(path) => path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned(),
            Source::BuiltIn(name, _) => (*name).to_owned(),
        }
    }

    /// Its bytes (read from the file).
    fn bytes(&self) -> Option<Cow<'static, [u8]>> {
        match self {
            Source::File(path) => std::fs::read(path).ok().map(Cow::Owned),
            Source::BuiltIn(_, bytes) => Some(Cow::Borrowed(bytes)),
        }
    }
}

/// Whether a file named `name` is an image type.
fn is_image_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Whether `name` is an SVG's.
fn is_svg(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".svg")
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

/// The images in `folders` and the built-in ones, by file name, in name
/// order, each name once.
pub fn available(folders: &[PathBuf]) -> Vec<String> {
    let mut names: Vec<String> = folders
        .iter()
        .filter_map(|f| std::fs::read_dir(f).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
        .filter(|n| is_image_name(n))
        .chain(BUILT_IN.iter().map(|(n, _)| (*n).to_owned()))
        .collect();
    names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));
    names.dedup();
    names
}

/// The image named `name`: a file in the first of `folders` that has it,
/// else a built-in one.
pub fn find(name: &str, folders: &[PathBuf]) -> Option<Source> {
    // A bare name only, never a path out of the folders.
    if name.is_empty() || name.contains(['/', '\\']) || name == ".." {
        return None;
    }
    folders
        .iter()
        .map(|f| f.join(name))
        .find(|p| p.is_file())
        .map(Source::File)
        .or_else(|| {
            BUILT_IN
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(n, bytes)| Source::BuiltIn(n, bytes))
        })
}

/// An SVG drawn at most `max` pixels on its longer side.
fn render_svg(bytes: &[u8], max: u32) -> Option<fields::Decoded> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = max as f32 / size.width().max(size.height());
    let (w, h) = (
        ((size.width() * scale).round() as u32).max(1),
        ((size.height() * scale).round() as u32).max(1),
    );
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia keeps colors premultiplied by alpha; egui wants them not.
    let pixels = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(fields::Decoded {
        width: w,
        height: h,
        pixels,
        source_size: (size.width().round() as u32, size.height().round() as u32),
    })
}

/// The image in `bytes`, from a file named `name`: drawn if it's an SVG,
/// else decoded, at most `max` pixels wide (an SVG on its longer side).
pub fn decode(bytes: &[u8], name: &str, max: u32) -> Option<fields::Decoded> {
    if is_svg(name) {
        render_svg(bytes, max)
    } else {
        fields::decode_image(bytes, max)
    }
}

/// Copy the image at `from` into the folder `into` (made if it isn't
/// there), and return the name it's saved under: its own name, or with
/// `-2`, `-3`, ... before the extension if a different image already has
/// it (the same image already there is used as it is).
pub fn import(from: &Path, into: &Path) -> Result<String, String> {
    let file_name = from
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if !is_image_name(file_name) {
        return Err(format!(
            "{} isn't an image raptaboard reads (PNG, JPEG or SVG)",
            from.display()
        ));
    }
    let bytes =
        std::fs::read(from).map_err(|e| format!("Couldn't read {}: {e}", from.display()))?;
    if decode(&bytes, file_name, 16).is_none() {
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

/// The textures made so far, by source, kept in egui's memory.
#[derive(Clone, Default)]
struct Cache(Arc<Mutex<HashMap<String, Slot>>>);

/// The texture for the image from `source`: `None` while it's being
/// decoded (on its own thread, asking for a repaint when done) or if it
/// can't be read.
pub fn texture(ctx: &egui::Context, source: &Source) -> Option<egui::TextureHandle> {
    let cache = ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Cache>(egui::Id::unique("raptaboard images"))
            .clone()
    });
    let mut slots = cache.0.lock().unwrap_or_else(|e| e.into_inner());
    let key = source.key();
    let slot = slots.entry(key.clone()).or_insert_with(|| {
        let done = Arc::new(Mutex::new(None));
        let (filled, repaint, source) = (done.clone(), ctx.clone(), source.clone());
        std::thread::spawn(move || {
            let decoded = source
                .bytes()
                .and_then(|b| decode(&b, &source.name(), MAX_TEXTURE_SIZE));
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
                    format!("image {key}"),
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
        // Name order, any case; a name in both folders once; the built-in
        // icons too.
        assert_eq!(
            available(&folders),
            [
                "arm.jpeg",
                "armed.svg",
                "checkmark.svg",
                "Down.JPG",
                "motor-off.svg",
                "motor-on.svg",
                "motor.svg",
                "ready.png"
            ]
        );
        assert_eq!(
            find("ready.png", &folders),
            Some(Source::File(a.join("ready.png")))
        );
        assert_eq!(
            find("arm.jpeg", &folders),
            Some(Source::File(b.join("arm.jpeg")))
        );
        // Never a path out of the folders.
        assert_eq!(find("../notes.txt", &folders), None);
        assert_eq!(find("", &folders), None);
    }

    #[test]
    fn the_built_in_icons_draw_and_a_folders_file_takes_their_place() {
        // With no folders at all, the built-in ones are there.
        let none: [PathBuf; 0] = [];
        assert_eq!(
            available(&none),
            [
                "armed.svg",
                "checkmark.svg",
                "motor-off.svg",
                "motor-on.svg",
                "motor.svg"
            ]
        );
        for (name, _) in BUILT_IN {
            let Some(Source::BuiltIn(found, bytes)) = find(name, &none) else {
                panic!("{name} isn't built in");
            };
            assert_eq!(found, name);
            // Drawn square at the texture size, with something visible.
            let d = decode(bytes, name, MAX_TEXTURE_SIZE).unwrap();
            assert_eq!(
                (d.width, d.height),
                (MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE),
                "{name}"
            );
            let opaque = d.pixels.chunks(4).filter(|p| p[3] == 255).count();
            assert!(opaque > 1000, "{name} drew only {opaque} pixels");
        }
        // A file of the same name in a folder is used instead.
        let dir = temp("override");
        std::fs::write(dir.join("motor.svg"), BUILT_IN[0].1).unwrap();
        assert_eq!(
            find("motor.svg", std::slice::from_ref(&dir)),
            Some(Source::File(dir.join("motor.svg")))
        );
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
        let names = available(std::slice::from_ref(&into));
        assert!(
            names.contains(&"ready.png".to_owned()) && names.contains(&"ready-2.png".to_owned())
        );
        // Not an image.
        let text = src.join("notes.txt");
        std::fs::write(&text, b"hi").unwrap();
        assert!(import(&text, &into).is_err());
        let fake = src.join("fake.png");
        std::fs::write(&fake, b"not a png").unwrap();
        assert!(import(&fake, &into).is_err());
        // An SVG imports; a broken one doesn't.
        let svg = src.join("dot.svg");
        std::fs::write(&svg, br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><circle cx="2" cy="2" r="2"/></svg>"#).unwrap();
        assert_eq!(import(&svg, &into).unwrap(), "dot.svg");
        let broken = src.join("broken.svg");
        std::fs::write(&broken, b"<svg").unwrap();
        assert!(import(&broken, &into).is_err());
    }
}
