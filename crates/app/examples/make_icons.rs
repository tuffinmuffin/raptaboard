//! Make the icon files in `media/icon/` from `media/icon/raptaboard.svg`:
//! `raptaboard.ico` (Windows), `raptaboard.png` (512 px, Linux), and on
//! macOS `raptaboard.icns` (through the system's `iconutil`). Run it after
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

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!("wrote {}", path.display());
}
