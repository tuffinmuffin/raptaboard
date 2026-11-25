//! The harness itself, with the repo's `limits-test` plugin, which
//! misbehaves on purpose as its `mode` property says.

// Tests written by Claude.

use std::path::PathBuf;

use raptaboard_plugin_host::runtime::Limits;
use raptaboard_plugin_test::Harness;

/// The repo's `limits-test` plugin, from the `plugins/` build the app's tests
/// use too (one build per CI job, not two).
fn limits_test() -> Harness {
    let built = raptaboard_plugin_host::dev::repo_plugins();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/limits-test");
    Harness::from_files(
        &dir.join("plugin.toml"),
        &built.join("limits_test.wasm"),
        &dir,
    )
    .unwrap()
}

// `Harness::build` itself (building a plugin crate) is what the Number and
// Alerts plugins' own tests use, in the plugins workspace.

#[test]
fn a_plugin_draws_with_its_property_defaults() {
    let mut w = limits_test();
    assert_eq!(w.manifest().name, "limits-test");
    assert!(w.render().has_text("ok"));
}

#[test]
fn a_panic_or_a_spin_stops_the_plugin_and_says_why() {
    let mut w = limits_test();
    w.property("mode", "panic");
    let why = w.try_render().unwrap_err().reason;
    assert!(why.contains("panicked on purpose"), "{why}");
    assert!(w.stopped().is_some());
    w.property("mode", "spin").with_limits(Limits {
        call_ms: 20,
        first_render_ms: 20,
        ..Limits::default()
    });
    let why = w.try_render().unwrap_err().reason;
    assert!(why.contains("20 ms"), "{why}");
    // Mended, it draws again.
    w.property("mode", "ok");
    assert!(w.render().has_text("ok"));
}

#[test]
fn what_plugin_toml_doesnt_allow_is_refused_and_not_recorded() {
    let mut w = limits_test();
    w.property("mode", "publish");
    let summary = w.render().summary().unwrap_or_default().to_string();
    assert!(summary.contains("publish = true"), "{summary}");
    assert!(w.take_published().is_empty());
    w.property("mode", "sound");
    let summary = w.render().summary().unwrap_or_default().to_string();
    assert!(summary.contains("sound = true"), "{summary}");
    assert!(w.take_sounds().is_empty());
}

#[test]
fn a_plugin_reads_its_own_folder_and_nothing_else() {
    let mut w = limits_test();
    w.property("mode", "files");
    // Its plugin.toml at /pack; not /etc/hosts, not out of /pack with ..,
    // and nothing written.
    assert_eq!(
        w.render().summary(),
        Some("own true; outside false; up false; write false")
    );
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/limits-test");
    assert!(!folder.join("written.txt").exists());
}

#[test]
fn printing_goes_to_the_log() {
    let mut w = limits_test();
    w.property("mode", "print").render();
    let logs = w.take_logs();
    assert!(
        logs.iter().any(|l| l.contains("hello from limits-test")),
        "{logs:?}"
    );
    assert!(w.take_logs().is_empty(), "taken");
}

#[test]
fn a_region_is_clicked_by_its_label() {
    let mut w = limits_test();
    w.property("mode", "region");
    let drawn = w.render();
    assert_eq!(drawn.region("Bell").map(|r| r.id.as_str()), Some("bell"));
    w.activate("Bell");
    assert_eq!(w.take_logs(), ["bell pressed"]);
}

#[test]
#[should_panic(expected = "no region labelled \"bell\"")]
fn activating_a_region_that_isnt_there_says_so() {
    limits_test().activate("bell");
}

#[test]
#[should_panic(expected = "no property \"colour\"")]
fn a_property_plugin_toml_doesnt_have_is_a_mistake() {
    limits_test().property("colour", "red");
}
