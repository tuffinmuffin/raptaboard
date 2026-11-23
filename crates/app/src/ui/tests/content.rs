//! Content packs in the window: the list offered on the first run, and the
//! Content window's Install and Remove, against a pretend web.

// Tests written by Claude.

use super::*;
use crate::content::tests::{Web, folder, sound_pack, test_key};
use crate::content::{Catalog, Content, FORMAT, OFFICIAL};

/// The official catalog on a pretend web: the fan clips pack (suggested)
/// and one that isn't, signed with the test key.
fn official_web() -> (Web, [u8; 32]) {
    let (key, public) = test_key();
    let web = Web::default();
    let mut other = sound_pack("extra", "1.0.0", &[("boop", "boop.mp3")], "https://x");
    other.suggested = false;
    other.name = "Extra sounds".into();
    let mut clips = sound_pack(
        "fan-clips",
        "1.0.0",
        &[("fire 1", "fire1.mp3"), ("disabled 1", "disabled1.mp3")],
        "https://x",
    );
    clips.name = "IT Crowd clips".into();
    let catalog = Catalog {
        format: FORMAT,
        release: "v1".into(),
        packs: vec![clips, other],
    };
    web.serve(OFFICIAL, &catalog, Some(&key));
    (web, public)
}

/// A window, connected to nothing, keeping packs in `dir` and downloading
/// from `web`, started as the app starts (so on a first run, it offers).
fn harness_with(dir: &std::path::Path, web: &Web, public: [u8; 32]) -> Harness {
    let mut app = DashboardApp::new(StartupOptions::default()).expect("app starts");
    app.sound = crate::sound::Sound::silent();
    app.content = Some(Content::with_fetch(
        dir,
        app.sound.files(),
        vec![public],
        web.fetch(),
    ));
    app.start_content();
    egui_kittest::Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .allow_missing_glyphs()
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(|ui, app: &mut DashboardApp| app.show(ui), app)
}

/// Run frames until the download thread has nothing left to do.
fn until_idle(harness: &mut Harness) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        settle(harness);
        if harness.state().content.as_ref().is_some_and(|c| c.idle()) {
            settle(harness);
            return;
        }
        assert!(Instant::now() < deadline, "the downloads didn't finish");
    }
}

fn installed(harness: &Harness) -> Vec<String> {
    let content = harness.state().content.as_ref().unwrap();
    content
        .installed
        .iter()
        .map(|i| i.pack.id.clone())
        .collect()
}

#[test]
fn the_first_run_offers_the_packs_and_downloads_those_ticked() {
    let dir = folder("ui-first-run");
    let (web, public) = official_web();
    let mut harness = harness_with(&dir, &web, public);
    labelled_soon(&mut harness, "Extra content for raptaboard");
    // Licenses show before anything downloads, and nothing has yet.
    assert_eq!(harness.get_all_by_label("License: CC0").count(), 2);
    assert!(installed(&harness).is_empty());
    assert!(
        web.asked
            .lock()
            .unwrap()
            .iter()
            .all(|u| u.contains("catalog"))
    );
    // The suggested pack is ticked, the other isn't.
    harness.get_by_label("Download").click();
    until_idle(&mut harness);
    assert_eq!(installed(&harness), ["fan-clips"]);
    assert!(
        harness
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
    // Its sounds play in place of the built-in names.
    let files = harness.state().sound.files();
    assert_eq!(
        files.read().unwrap()["fire 1"],
        dir.join("fan-clips/fire1.mp3")
    );
    // Asked once: the next start doesn't offer again.
    let mut again = harness_with(&dir, &web, public);
    until_idle(&mut again);
    assert!(
        again
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
}

#[test]
fn not_now_downloads_nothing_and_isnt_asked_again() {
    let dir = folder("ui-not-now");
    let (web, public) = official_web();
    let mut harness = harness_with(&dir, &web, public);
    labelled_soon(&mut harness, "Not now");
    harness.get_by_label("Not now").click();
    until_idle(&mut harness);
    assert!(installed(&harness).is_empty());
    assert!(
        harness
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
    let mut again = harness_with(&dir, &web, public);
    until_idle(&mut again);
    assert!(
        again
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
}

#[test]
fn offline_the_first_run_says_nothing_and_offers_next_time() {
    let dir = folder("ui-offline");
    let (_, public) = official_web();
    let offline = Web::default();
    let mut harness = harness_with(&dir, &offline, public);
    until_idle(&mut harness);
    assert!(
        harness
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
    // Back online, the next start offers them.
    let (web, _) = official_web();
    let mut again = harness_with(&dir, &web, public);
    labelled_soon(&mut again, "Extra content for raptaboard");
}

#[test]
fn a_catalog_not_signed_by_raptaboard_isnt_offered() {
    let dir = folder("ui-unsigned");
    let (web, _) = official_web();
    let stranger = crate::content::tests::Web::default();
    *stranger.files.lock().unwrap() = web.files.lock().unwrap().clone();
    let other_key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32])
        .verifying_key()
        .to_bytes();
    let mut harness = harness_with(&dir, &stranger, other_key);
    until_idle(&mut harness);
    assert!(
        harness
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
    assert!(installed(&harness).is_empty());
}

#[test]
fn the_content_window_installs_and_removes_packs() {
    let dir = folder("ui-window");
    let (web, public) = official_web();
    // Already offered, so only the window asks.
    crate::content::Store::new(&dir)
        .save_state(&crate::content::State { offered: true })
        .unwrap();
    let mut harness = harness_with(&dir, &web, public);
    settle(&mut harness);
    assert!(web.asked.lock().unwrap().is_empty(), "nothing read unasked");
    harness.get_by_label("⚙").click();
    settle(&mut harness);
    harness.get_by_label("Content packs…").click();
    labelled_soon(&mut harness, "IT Crowd clips");
    harness.get_by_label("Extra sounds");
    // Credits show when asked.
    harness
        .get_all_by_label("Credits and license")
        .next()
        .unwrap()
        .click();
    settle(&mut harness);
    harness.get_by_label("Made by us.");
    harness.get_all_by_label("Install").next().unwrap().click();
    until_idle(&mut harness);
    assert_eq!(installed(&harness), ["fan-clips"]);
    harness.get_by_label("Remove").click();
    until_idle(&mut harness);
    assert!(installed(&harness).is_empty());
    assert!(!dir.join("fan-clips").exists());
}

#[test]
fn a_source_added_is_saved_and_its_packs_listed() {
    let dir = folder("ui-source");
    let (web, public) = official_web();
    let theirs = Catalog {
        format: FORMAT,
        release: "v3".into(),
        packs: vec![{
            let mut p = sound_pack(
                "horns",
                "2.0.0",
                &[("horn", "horn.mp3")],
                "https://team.example",
            );
            p.name = "Team horns".into();
            p
        }],
    };
    web.serve("https://team.example/catalog.json", &theirs, None);
    crate::content::Store::new(&dir)
        .save_state(&crate::content::State { offered: true })
        .unwrap();
    let mut harness = harness_with(&dir, &web, public);
    harness.state_mut().packs.open = true;
    until_idle(&mut harness);
    harness.get_by_label("Source URL").click();
    settle(&mut harness);
    harness.event(egui::Event::Text(
        "https://team.example/catalog.json".into(),
    ));
    settle(&mut harness);
    harness.get_by_label("Add source").click();
    labelled_soon(&mut harness, "Team horns");
    harness.get_by_label("not signed");
    assert_eq!(
        harness.state().current_settings(None).content_sources,
        Some(vec!["https://team.example/catalog.json".to_string()])
    );
}
