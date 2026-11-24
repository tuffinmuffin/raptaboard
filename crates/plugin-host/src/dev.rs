//! For developing and testing plugins: building a plugins workspace.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Build the cargo workspace in `dir` for `wasm32-wasip2` (release), and
/// return the folder its `.wasm` files are in. The environment of a build
/// running this (a test's coverage flags, its target folder) isn't passed on.
pub fn build_workspace(dir: &Path) -> Result<PathBuf, String> {
    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo
        .current_dir(dir)
        .args(["build", "--release", "--target", "wasm32-wasip2"]);
    for (key, _) in std::env::vars() {
        if (key.starts_with("CARGO_") && key != "CARGO_HOME")
            || key.starts_with("RUSTC")
            || key.starts_with("RUSTFLAGS")
            || key == "RUSTDOCFLAGS"
            || key.starts_with("LLVM_PROFILE")
        {
            cargo.env_remove(key);
        }
    }
    let status = cargo
        .status()
        .map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "building {} failed (is the wasm32-wasip2 target installed? \
             rustup target add wasm32-wasip2)",
            dir.display()
        ));
    }
    Ok(dir.join("target/wasm32-wasip2/release"))
}

/// The repo's own `plugins/` workspace, built once per test run.
pub fn repo_plugins() -> &'static Path {
    static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        build_workspace(&dir).unwrap_or_else(|e| panic!("{e}"))
    })
}
