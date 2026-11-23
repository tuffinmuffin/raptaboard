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

/// A state file saying this version already started and its list was
/// answered, having offered `shown`.
fn answered_before(dir: &std::path::Path, shown: &[&str]) {
    crate::content::Store::new(dir)
        .save_state(&crate::content::State {
            last_version: Some(crate::content::app_version().into()),
            offer_pending: false,
            shown: shown.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
        .unwrap();
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
    // The window stays to say what was installed, until Done.
    assert!(
        harness
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
    harness.get_by_label("Content packs installed");
    harness
        .get_by_label_contains("Installed IT Crowd clips 1.0.0. Its sounds: fire 1, disabled 1.");
    harness.get_by_label("Done").click();
    settle(&mut harness);
    assert!(harness.query_by_label("Content packs installed").is_none());
    assert!(harness.state().packs.offer.is_none());
    // Its sounds play in place of the built-in names.
    let files = harness.state().sound.files();
    assert_eq!(
        files.read().unwrap().layers("fire 1")[0].path,
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
    answered_before(&dir, &[]);
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
    harness.get_by_label_contains("Installed IT Crowd clips 1.0.0.");
    harness.get_by_label("Remove").click();
    until_idle(&mut harness);
    assert!(installed(&harness).is_empty());
    harness.get_by_label("Removed IT Crowd clips.");
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
    answered_before(&dir, &[]);
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

#[test]
fn an_upgrade_offers_the_packs_again_with_those_seen_unticked() {
    let dir = folder("ui-upgrade");
    let (web, public) = official_web();
    // An older version offered the fan clips, and they were turned down.
    crate::content::Store::new(&dir)
        .save_state(&crate::content::State {
            last_version: Some("0.2.0".into()),
            offer_pending: false,
            shown: vec!["fan-clips".into()],
            ..Default::default()
        })
        .unwrap();
    let mut harness = harness_with(&dir, &web, public);
    labelled_soon(&mut harness, "Extra content for raptaboard");
    let picked = harness.state().packs.offer.as_ref().unwrap().picked.clone();
    assert!(!picked.unwrap()["fan-clips"]);
    harness.get_by_label("Not now").click();
    until_idle(&mut harness);
    let state = crate::content::Store::new(&dir).state();
    assert_eq!(
        state.last_version.as_deref(),
        Some(crate::content::app_version())
    );
    assert!(!state.offer_pending);
    assert_eq!(state.shown, ["fan-clips", "extra"]);
}

#[test]
fn the_same_version_again_reads_nothing_at_start() {
    let dir = folder("ui-same");
    let (web, public) = official_web();
    answered_before(&dir, &["fan-clips"]);
    let mut harness = harness_with(&dir, &web, public);
    settle(&mut harness);
    assert!(web.asked.lock().unwrap().is_empty());
    assert!(harness.state().packs.offer.is_none());
}

#[test]
fn offline_after_an_upgrade_offers_at_the_next_start() {
    let dir = folder("ui-upgrade-offline");
    let (web, public) = official_web();
    crate::content::Store::new(&dir)
        .save_state(&crate::content::State {
            last_version: Some("0.2.0".into()),
            ..Default::default()
        })
        .unwrap();
    let mut offline = harness_with(&dir, &Web::default(), public);
    until_idle(&mut offline);
    assert!(
        offline
            .query_by_label("Extra content for raptaboard")
            .is_none()
    );
    // The version is recorded, but the offer still waits.
    let state = crate::content::Store::new(&dir).state();
    assert_eq!(
        state.last_version.as_deref(),
        Some(crate::content::app_version())
    );
    assert!(state.offer_pending);
    let mut again = harness_with(&dir, &web, public);
    labelled_soon(&mut again, "Extra content for raptaboard");
}

#[test]
fn a_download_that_fails_says_why_and_leaves_done() {
    let dir = folder("ui-download-fails");
    let (web, public) = official_web();
    // The catalog is there, a file isn't.
    web.files
        .lock()
        .unwrap()
        .remove("https://x/fan-clips.fire1.mp3");
    let mut harness = harness_with(&dir, &web, public);
    labelled_soon(&mut harness, "Download");
    harness.get_by_label("Download").click();
    until_idle(&mut harness);
    assert!(installed(&harness).is_empty());
    harness.get_by_label("Some packs didn't install");
    harness.get_by_label_contains("Couldn't install IT Crowd clips");
    harness.get_by_label_contains("404");
    harness.get_by_label("You can try again in ⚙ Settings → Content packs.");
    harness.get_by_label("Done").click();
    settle(&mut harness);
    assert!(harness.state().packs.offer.is_none());
}

/// Make the background check due now.
fn check_now(harness: &mut Harness) {
    harness.state_mut().content_timer =
        crate::content::CheckTimer::starting(Instant::now(), Duration::ZERO);
}

#[test]
fn the_background_check_finds_an_update_and_the_notice_dismisses() {
    let dir = folder("ui-notice");
    let (web, public) = official_web();
    // An older fan-clips installed; both packs offered before.
    let old = sound_pack(
        "fan-clips",
        "0.9.0",
        &[("fire 1", "fire1.mp3"), ("disabled 1", "disabled1.mp3")],
        "https://x",
    );
    crate::content::Store::new(&dir)
        .install(OFFICIAL, &old, &*web.fetch())
        .unwrap();
    answered_before(&dir, &["fan-clips", "extra"]);
    web.asked.lock().unwrap().clear();
    let mut harness = harness_with(&dir, &web, public);
    settle(&mut harness);
    assert!(web.asked.lock().unwrap().is_empty(), "not due yet");
    check_now(&mut harness);
    until_idle(&mut harness);
    labelled_soon(&mut harness, "📦 Content: 1 update");
    // Review opens the Content window, where Update is.
    harness.get_by_label("Review").click();
    settle(&mut harness);
    assert!(harness.state().packs.open);
    harness.get_by_label("Update");
    // ✕ hides it, and it stays hidden after a restart.
    harness.get_by_label("✕").click();
    settle(&mut harness);
    assert!(harness.query_by_label("📦 Content: 1 update").is_none());
    let mut again = harness_with(&dir, &web, public);
    check_now(&mut again);
    until_idle(&mut again);
    assert!(again.query_by_label("📦 Content: 1 update").is_none());
}

#[test]
fn turned_off_the_background_check_reads_nothing() {
    let dir = folder("ui-check-off");
    let (web, public) = official_web();
    answered_before(&dir, &[]);
    let mut harness = harness_with(&dir, &web, public);
    harness.state_mut().content_check_minutes = 0;
    check_now(&mut harness);
    settle(&mut harness);
    assert!(web.asked.lock().unwrap().is_empty());
    assert_eq!(
        harness.state().current_settings(None).content_check_minutes,
        Some(0)
    );
    // On, it reads the catalog, and with nothing new says nothing.
    harness.state_mut().content_check_minutes = 60;
    check_now(&mut harness);
    until_idle(&mut harness);
    assert!(!web.asked.lock().unwrap().is_empty());
    assert!(
        harness.query_by_label_contains("📦 Content").is_some(),
        "two packs never offered"
    );
}
