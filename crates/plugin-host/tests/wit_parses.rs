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
    assert_eq!(package.name.to_string(), "raptaboard:dashboard@0.1.0");
    assert!(package.worlds.contains_key("widget"));
}
