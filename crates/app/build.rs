//! Build steps before the app compiles. On Windows: put raptaboard's icon
//! (`media/icon/raptaboard.ico`) into the program, so Explorer, the taskbar
//! and shortcuts show it. Nothing on other systems.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=windows/raptaboard.rc");
    println!("cargo::rerun-if-changed=../../media/icon/raptaboard.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_icon();
    }
}

#[cfg(windows)]
fn windows_icon() {
    embed_resource::compile("windows/raptaboard.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("the Windows icon resource compiles");
}

#[cfg(not(windows))]
fn windows_icon() {
    // Building for Windows from another system: no resource compiler here,
    // so the program has no icon of its own (the window still has one).
}
