//! Make the icon files in `media/icon/` from `media/icon/raptaboard.svg`:
//! `raptaboard.ico` (Windows), `raptaboard.png` (512 px, Linux), the
//! Microsoft Store package's logos in `msix/`, the Store listing's poster
//! art (`media/store/poster-*.png`, 9:16), and on macOS
//! `raptaboard.icns` (through the system's `iconutil`). Run it after
//! changing the SVG, from the repo's top folder:
//!
//! ```text
//! cargo run -p raptaboard --example make_icons
//! ```

use std::path::Path;
use std::process::Command;

use raptaboard::app_icon;

fn main() {
    let out = Path::new("media/icon");
    assert!(
        out.join("raptaboard.svg").exists(),
        "run this from the repo's top folder"
    );
    write(
        &out.join("raptaboard.ico"),
        &app_icon::ico(&app_icon::ICO_SIZES),
    );
    write(&out.join("raptaboard.png"), &app_icon::png(512));
    // The MSIX package's logos, at the sizes its manifest names.
    let msix = out.join("msix");
    std::fs::create_dir_all(&msix).expect("make the msix folder");
    for (name, size) in app_icon::MSIX_LOGOS {
        write(&msix.join(name), &app_icon::png(size));
    }

    // The Store listing's poster art: the icon on a dark background, at the
    // two sizes the Store takes.
    let store = Path::new("media/store");
    std::fs::create_dir_all(store).expect("make media/store");
    for (w, h) in [(720, 1080), (1440, 2160)] {
        write(&store.join(format!("poster-{w}x{h}.png")), &poster(w, h));
    }

    // macOS: an .iconset folder of PNGs, which iconutil packs into .icns.
    if cfg!(target_os = "macos") {
        let set = std::env::temp_dir().join("raptaboard.iconset");
        let _ = std::fs::remove_dir_all(&set);
        std::fs::create_dir_all(&set).expect("make the iconset folder");
        for size in [16, 32, 128, 256, 512] {
            write(
                &set.join(format!("icon_{size}x{size}.png")),
                &app_icon::png(size),
            );
            write(
                &set.join(format!("icon_{size}x{size}@2x.png")),
                &app_icon::png(size * 2),
            );
        }
        let icns = out.join("raptaboard.icns");
        let status = Command::new("iconutil")
            .args(["-c", "icns", "-o"])
            .arg(&icns)
            .arg(&set)
            .status()
            .expect("run iconutil");
        assert!(status.success(), "iconutil failed");
        println!("wrote {}", icns.display());
        let _ = std::fs::remove_dir_all(&set);
    } else {
        println!("raptaboard.icns is made on macOS only; left as it is");
    }
}

/// The icon `width` x `height` pixels: centered a little above the middle,
/// 80% of the width across, on a dark blue background that fades darker
/// toward the bottom (the icon's own dark disc's colors).
fn poster(width: u32, height: u32) -> Vec<u8> {
    let (top, bottom) = ([0x1B, 0x2A, 0x38], [0x0B, 0x11, 0x18]);
    let mut image = image::RgbaImage::from_fn(width, height, |_, y| {
        let t = y as f32 / (height - 1) as f32;
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        image::Rgba([
            mix(top[0], bottom[0]),
            mix(top[1], bottom[1]),
            mix(top[2], bottom[2]),
            255,
        ])
    });
    let size = width * 8 / 10;
    let icon = image::RgbaImage::from_raw(size, size, app_icon::rgba(size)).expect("size matches");
    let x = i64::from((width - size) / 2);
    let y = i64::from(height * 42 / 100) - i64::from(size / 2);
    image::imageops::overlay(&mut image, &icon, x, y);
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("PNG writes to memory");
    out.into_inner()
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!("wrote {}", path.display());
}
