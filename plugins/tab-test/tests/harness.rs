//! The tab-test plugin with the harness: its own folder, a pixel frame,
//! keys and audio.

// Tests written by Claude.

#![cfg(not(target_arch = "wasm32"))]

use raptaboard_plugin_test::{Harness, Input, Key, Modifiers};

fn tab_test() -> Harness {
    let mut w = Harness::build(env!("CARGO_MANIFEST_DIR"));
    w.size(640.0, 440.0);
    w
}

fn key(name: &str) -> Input {
    Input::KeyDown(Key {
        name: name.into(),
        modifiers: Modifiers::empty(),
    })
}

#[test]
fn it_claims_its_tab_and_reads_its_own_folder() {
    let mut w = tab_test();
    assert_eq!(w.manifest().tab.as_deref(), Some("Tab test"));
    let drawn = w.render();
    let summary = drawn.summary().unwrap();
    assert!(
        summary.starts_with("Hello from the tab-test plugin's folder."),
        "{summary}"
    );
}

#[test]
fn it_draws_a_pixel_frame_that_moves_35_times_a_second() {
    let mut w = tab_test();
    let first = w.render();
    let pixels = first.pixels();
    assert_eq!(pixels.len(), 1);
    let p = pixels[0];
    assert_eq!((p.width, p.height, p.rgba.len()), (320, 200, 320 * 200 * 4));
    assert!(!p.smooth, "sharp pixels");
    assert_eq!((p.max.x, p.max.y), (640.0, 400.0));
    let rate = first.redraw_in_s().unwrap();
    assert!((rate - 1.0 / 35.0).abs() < 1e-9);
    let later = w.advance(0.5).render();
    assert_ne!(later.pixels()[0].rgba, p.rgba, "the square moved");
}

#[test]
fn keys_count_and_space_beeps() {
    let mut w = tab_test();
    w.input(key("A")).input(key("Space"));
    let audio = w.take_audio();
    assert_eq!(audio.len(), 1, "only Space beeps");
    assert_eq!((audio[0].rate, audio[0].channels), (22_050, 1));
    assert_eq!(audio[0].samples.len(), 2205);
    assert!(w.render().summary().unwrap().ends_with("Keys: 2"));
}

#[test]
fn it_counts_the_held_mouse_and_clicks_while_focused() {
    use raptaboard_plugin_test::{MouseButton, Point, PointerButton};
    let mut w = tab_test();
    assert!(w.manifest().capture_mouse);
    let click = || {
        Input::PointerDown(PointerButton {
            pos: Point { x: 1.0, y: 1.0 },
            button: MouseButton::Primary,
        })
    };
    // The click that gives it the keyboard doesn't count.
    w.input(click()).input(Input::Focus(true));
    w.input(Input::MouseMotion(Point { x: 12.0, y: 3.0 }))
        .input(click());
    assert!(
        w.render()
            .summary()
            .unwrap()
            .ends_with("Keys: 0, mouse 12, clicks 1"),
        "{:?}",
        w.render().summary()
    );
}

#[test]
fn q_gives_the_keyboard_back() {
    let mut w = tab_test();
    w.input(key("A"));
    assert_eq!(w.take_releases(), 0);
    w.input(key("Q"));
    assert_eq!(w.take_releases(), 1);
}
