// Tests written by Claude: make sure the plugin interface file stays valid WIT.

use std::path::Path;

#[test]
fn dashboard_wit_parses() {
    let wit = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wit");
    let mut resolve = wit_parser::Resolve::default();
    let (pkg, _) = resolve
        .push_dir(&wit)
        .expect("wit/dashboard.wit should parse");
    let package = &resolve.packages[pkg];
    // The WIT package version and HOST_API move together.
    let expected = format!(
        "raptaboard:dashboard@{}.0",
        raptaboard_plugin_host::HOST_API
    );
    assert_eq!(package.name.to_string(), expected);
    let world = &resolve.worlds[package.worlds["widget"]];
    let exports: Vec<String> = world
        .exports
        .keys()
        .map(|k| resolve.name_world_key(k))
        .collect();
    for name in ["subscriptions", "update", "render", "tab-mark", "on-input"] {
        assert!(exports.iter().any(|e| e == name), "widget exports {name}");
    }
}
