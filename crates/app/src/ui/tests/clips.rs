//! The book's Field clip, made by running the app headless against a
//! scripted robot and saving the frames as a GIF, and the book's and the
//! Microsoft Store listing's screenshots, made the same way against
//! robot-2027's sim. Ignored in normal test
//! runs (they need a GPU and take a while); make them again after changing
//! what they show:
//!
//! ```sh
//! cargo test -p raptaboard --lib record_ -- --ignored --nocapture
//! ```

// Tests written by Claude.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::path::Path;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, RgbaImage, imageops};

use super::*;
use crate::types::field::FieldInfo;

/// Where the book's Field clip goes, from the repo's root.
const FIELD_CLIP: &str = "book/src/images/field.gif";
/// Frames a second in the clip.
const FPS: u32 = 10;
/// How long the clip is, seconds: a robot's lap of its orbit.
const CLIP_S: f64 = TAU;
/// The clip's width, pixels.
const CLIP_WIDTH: u32 = 720;

/// Robot-2026's cameras: forward, left (meters), facing (degrees), FOV.
const CAMERAS: [(f64, f64, f64, f64); 2] = [
    (-0.1524, 0.3048, 90.0, 70.0),
    (-0.2286, 0.1905, 180.0, 70.0),
];

/// Bytes of a struct array of `items`, each some doubles.
fn doubles(items: &[Vec<f64>]) -> nt4::Value {
    nt4::Value::Raw(
        items
            .iter()
            .flatten()
            .flat_map(|d| d.to_le_bytes())
            .collect(),
    )
}

/// The robot on its orbit at time `t`: 2 m round the field's center,
/// facing along its path.
fn orbit(t: f64) -> [f64; 3] {
    [2.0 * t.cos(), 2.0 * t.sin(), t + FRAC_PI_2]
}

/// `p` (field) seen from a robot at `r`: (forward, left).
fn to_robot(r: [f64; 3], p: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (p.0 - r[0], p.1 - r[1]);
    let (sin, cos) = r[2].sin_cos();
    (dx * cos + dy * sin, -dx * sin + dy * cos)
}

/// The tags the cameras see from `robot`, as robot-2027's sim works them
/// out: in a camera's FOV, within 6 m, facing it within 60 degrees.
fn seen_tags(robot: [f64; 3], field: &FieldInfo) -> Vec<i64> {
    let mut seen = Vec::new();
    for (x, y, facing, fov) in CAMERAS {
        let (sin, cos) = robot[2].sin_cos();
        let cam = (robot[0] + x * cos - y * sin, robot[1] + x * sin + y * cos);
        let look = robot[2] + facing.to_radians();
        for tag in &field.tags {
            let (dx, dy) = (tag.x - cam.0, tag.y - cam.1);
            let off = (dy.atan2(dx) - look + PI).rem_euclid(TAU) - PI;
            let back = ((-dy).atan2(-dx) - tag.yaw + PI).rem_euclid(TAU) - PI;
            if dx.hypot(dy) <= 6.0
                && off.abs() <= (fov / 2.0).to_radians()
                && back.abs() <= 60f64.to_radians()
            {
                seen.push(i64::from(tag.id));
            }
        }
    }
    seen.sort_unstable();
    seen.dedup();
    seen
}

/// The Field in the clip: the robot on `/Clip/robot` with Robot-2026's
/// size, the other robots, its path, balls, the balls ahead, the tags seen,
/// its cameras and an intake.
fn clip_field() -> WidgetContainer {
    let mut w = widget(
        "Field",
        "/Clip/robot",
        "struct:Pose2d",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 1024.0,
            height: 512.0,
        },
    );
    w.title = "Field".into();
    let p = &mut w.properties;
    p.insert("robot_length".into(), json!(0.673));
    p.insert("robot_width".into(), json!(0.711));
    p.insert(
        "layers".into(),
        json!([
            {"kind": "robots", "topic": "/Clip/fleet", "data_type": "struct:Pose2d[]"},
            {"kind": "path", "topic": "/Clip/path", "data_type": "struct:Pose2d[]"},
            {"kind": "seen_tags", "topic": "/Clip/seenTags", "data_type": "int[]"},
            {"kind": "objects", "topic": "/Clip/fuel", "data_type": "struct:Translation2d[]", "marker": "circle", "size": 0.15},
            {"kind": "objects", "topic": "/Clip/seenFuel", "data_type": "struct:Translation2d[]",
             "marker": "ring", "size": 0.35, "robot_relative": true, "color": 4283215696u32}
        ]),
    );
    p.insert(
        "attachments".into(),
        json!([
            {"kind": "camera", "name": "Side", "x": CAMERAS[0].0, "y": CAMERAS[0].1, "yaw": CAMERAS[0].2},
            {"kind": "camera", "name": "Back", "x": CAMERAS[1].0, "y": CAMERAS[1].1, "yaw": CAMERAS[1].2},
            {"kind": "part", "name": "Intake", "x": 0.3366, "width": 0.55, "shown_by": "/Clip/intakeOut",
             "length_topic": "/Clip/intakeIn", "scale": 0.0254}
        ]),
    );
    w
}

#[test]
#[ignore = "makes the README's clip: needs a GPU; run with --ignored"]
fn record_field_clip() {
    let robot = Robot::start();
    let pose_of = |name: &str, ty: &str| {
        robot
            ._server
            .publish_generic(name, ty.parse().unwrap(), nt4::Properties::new())
    };
    let me = pose_of("/Clip/robot", "struct:Pose2d");
    let fleet = pose_of("/Clip/fleet", "struct:Pose2d[]");
    let path = pose_of("/Clip/path", "struct:Pose2d[]");
    let fuel = pose_of("/Clip/fuel", "struct:Translation2d[]");
    let seen_fuel = pose_of("/Clip/seenFuel", "struct:Translation2d[]");
    let tags = robot._server.publish::<Vec<i64>>("/Clip/seenTags");
    let intake_out = robot._server.publish::<bool>("/Clip/intakeOut");
    let intake_in = robot._server.publish::<f64>("/Clip/intakeIn");
    let field = crate::fields::built_in().remove(0).info;
    let balls: Vec<(f64, f64)> = (0..10)
        .map(|i| {
            let a = f64::from(i) * TAU / 10.0;
            (3.2 * a.cos(), 3.2 * a.sin())
        })
        .collect();
    path.set(doubles(
        &(0..=36)
            .map(|i| orbit(f64::from(i) * TAU / 36.0).to_vec())
            .collect::<Vec<_>>(),
    ))
    .unwrap();
    fuel.set(doubles(
        &balls.iter().map(|b| vec![b.0, b.1]).collect::<Vec<_>>(),
    ))
    .unwrap();

    let mut tab = Tab::new("Field");
    tab.grid_layout.containers.push(clip_field());
    let mut app = app_for(&robot);
    app.layout = ElasticLayout {
        tabs: vec![tab],
        ..Default::default()
    };
    app.file.saved = app.layout.clone();
    app.history = History::new(&app.layout);
    app.tree.shown = false;
    app.locked = true;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1060.0, 620.0))
        .with_pixels_per_point(1.0)
        .allow_missing_glyphs()
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut DashboardApp| app.show(ui), app);

    let mut frames = Vec::new();
    let steps = (CLIP_S * f64::from(FPS)).round() as u32;
    for step in 0..steps {
        let t = f64::from(step) / f64::from(FPS);
        let r = orbit(t);
        me.set(doubles(&[r.to_vec()])).unwrap();
        fleet
            .set(doubles(&[
                orbit(t + TAU / 3.0).to_vec(),
                orbit(t + 2.0 * TAU / 3.0).to_vec(),
            ]))
            .unwrap();
        tags.set(seen_tags(r, &field));
        let ahead: Vec<Vec<f64>> = balls
            .iter()
            .map(|b| to_robot(r, *b))
            .filter(|(f, l)| f.hypot(*l) <= 4.0 && l.atan2(*f).abs() <= 35f64.to_radians())
            .map(|(f, l)| vec![f, l])
            .collect();
        seen_fuel.set(doubles(&ahead)).unwrap();
        let into = t % 4.0;
        intake_out.set(into < 2.0);
        intake_in.set(if into < 2.0 {
            12.0 * (PI * into / 2.0).sin()
        } else {
            0.0
        });
        // Wait until the widget shows this step's pose.
        let want = format!("Field: robot at x {:.2} m, y {:.2} m", r[0], r[1]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while harness.query_by_label_contains(&want).is_none() {
            assert!(Instant::now() < deadline, "the Field never showed {want:?}");
            harness.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        if step == 0 {
            // The field image decodes on its own thread the first time.
            for _ in 0..40 {
                harness.step();
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        harness.step();
        let image = harness.render().expect("render a frame");
        frames.push(crop_to_card(&harness, image));
    }
    save_gif(&frames, FIELD_CLIP);
    println!("wrote {FIELD_CLIP}: {} frames", frames.len());
}

/// The rendered window cut to the Field's card, scaled to the clip's width.
fn crop_to_card(harness: &Harness, image: RgbaImage) -> RgbaImage {
    let area = harness.state().grid.area;
    let card = &harness.state().layout.tabs[0].grid_layout.containers[0].rect;
    let (x, y) = (area.min.x as u32, area.min.y as u32);
    let w = (card.width as u32).min(image.width() - x);
    let h = (card.height as u32).min(image.height() - y);
    let cut = imageops::crop_imm(&image, x, y, w, h).to_image();
    let height = cut.height() * CLIP_WIDTH / cut.width();
    imageops::resize(&cut, CLIP_WIDTH, height, imageops::FilterType::Triangle)
}

/// Save `frames` as a GIF at `path` (from the repo's root) that loops.
fn save_gif(frames: &[RgbaImage], path: &str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let file = std::fs::File::create(root.join(path)).expect("make the GIF file");
    let mut gif = GifEncoder::new_with_speed(std::io::BufWriter::new(file), 10);
    gif.set_repeat(Repeat::Infinite).unwrap();
    let delay = Delay::from_numer_denom_ms(1000, FPS);
    gif.encode_frames(
        frames
            .iter()
            .map(|f| Frame::from_parts(f.clone(), 0, 0, delay)),
    )
    .expect("write the GIF");
}

/// The window, connected to robot-2027's sim on this computer and showing
/// `layouts/robot2027_sim.json`, locked and without the tree, `size` points
/// big at `pixels_per_point`.
fn sim_harness(size: egui::Vec2, pixels_per_point: f32) -> Harness {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("layouts/robot2027_sim.json");
    let layout = crate::layout_file::load(&path).expect("the sim layout opens");
    let mut app = DashboardApp::new(StartupOptions::default()).expect("app starts");
    app.show_layout(layout, path);
    app.sound = crate::sound::Sound::silent();
    app.address.mode = AddressMode::Custom;
    app.address.custom = "127.0.0.1".into();
    app.address.port = nt4::proto::ROBOT_CODE_PORT;
    app.locked = true;
    app.tree.shown = false;
    let mut harness = egui_kittest::Harness::builder()
        .with_size(size)
        .with_pixels_per_point(pixels_per_point)
        .allow_missing_glyphs()
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut DashboardApp| app.show(ui), app);
    // Symbols egui's own fonts lack (⚙, the bell, the pause button) from
    // the computer's fonts, as the app gets them through eframe.
    harness.ctx.add_font_provider(std::sync::Arc::new(
        egui_system_font_provider::SystemFontProvider::new(),
    ));
    let deadline = Instant::now() + Duration::from_secs(15);
    while !connected(harness.state()) {
        assert!(
            Instant::now() < deadline,
            "never connected: is the sim running?"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(16));
    }
    harness
}

/// Show the tab named `tab` and run `seconds` of frames, about 60 a
/// second, so its values arrive and its graphs fill.
fn show_tab(harness: &mut Harness, tab: &str, seconds: u64) {
    let index = harness
        .state()
        .layout
        .tabs
        .iter()
        .position(|t| t.name == tab)
        .unwrap_or_else(|| panic!("no {tab} tab"));
    harness.state_mut().current_tab = index;
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        harness.step();
        std::thread::sleep(Duration::from_millis(16));
    }
}

/// Save `image` at `path` (from the repo's root), making its folder.
fn save_png(image: &RgbaImage, path: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join(path);
    std::fs::create_dir_all(out.parent().unwrap()).expect("make the folder");
    image.save(&out).expect("save the screenshot");
    println!("wrote {path} ({}x{})", image.width(), image.height());
}

/// Where the Microsoft Store's screenshots go, from the repo's root.
const STORE_DIR: &str = "media/store";

#[test]
#[ignore = "makes the Microsoft Store's screenshots: needs a GPU and robot-2027's sim running \
            on this computer (layouts/robot2027_sim.json's topics); run with --ignored"]
fn record_store_screenshots() {
    // 1920 x 1080 pixels, at 1.25 points a pixel as a laptop shows it.
    let mut harness = sim_harness(egui::vec2(1536.0, 864.0), 1.25);
    for (tab, name) in [
        ("Field", "screenshot-field.png"),
        ("More widgets", "screenshot-graphs.png"),
        ("Widgets", "screenshot-widgets.png"),
    ] {
        show_tab(&mut harness, tab, 10);
        let image = harness.render().expect("render a frame");
        save_png(&image, &format!("{STORE_DIR}/{name}"));
    }
}

/// Where the book's screenshots go, from the repo's root.
const BOOK_IMAGES: &str = "book/src/images";

/// The card to cut out for a widget type, by title, where the first one
/// in the layout isn't the best example: (type, card title).
const PICTURED: &[(&str, &str)] = &[
    ("Graph", "Sine and orbit"),
    ("Chassis Speeds", "Scripted speeds (lagging robot)"),
];

/// Whether the card titled `title` is the one to cut out for `kind`: the
/// one [`PICTURED`] names, or else the first (`seen` has the types done).
fn pictured(kind: &str, title: &str, seen: &std::collections::BTreeSet<String>) -> bool {
    match PICTURED.iter().find(|(k, _)| *k == kind) {
        Some((_, want)) => *want == title,
        None => !seen.contains(kind),
    }
}

#[test]
fn a_type_named_in_pictured_is_cut_from_that_card_only() {
    let mut seen = std::collections::BTreeSet::new();
    assert!(!pictured("Graph", "Loop time", &seen));
    assert!(pictured("Graph", "Sine and orbit", &seen));
    assert!(pictured("Gyro", "Any", &seen));
    seen.insert("Gyro".to_string());
    assert!(!pictured("Gyro", "Another", &seen));
}

/// A name as a file name: `More widgets` → `more-widgets`, `3D Pose` →
/// `3d-pose`.
fn slug(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();
    words.join("-")
}

#[test]
fn slugs_are_lowercase_words_with_dashes() {
    assert_eq!(slug("More widgets"), "more-widgets");
    assert_eq!(slug("3D Pose"), "3d-pose");
    assert_eq!(slug("  Split Button   Chooser "), "split-button-chooser");
}

#[test]
#[ignore = "makes the book's screenshots: needs a GPU and robot-2027's sim running on this \
            computer (layouts/robot2027_sim.json's topics); run with --ignored"]
fn record_docs_screenshots() {
    const MARGIN: f32 = 16.0;
    let mut harness = sim_harness(egui::vec2(1280.0, 800.0), 1.0);
    let tabs: Vec<String> = harness
        .state()
        .layout
        .tabs
        .iter()
        .map(|t| t.name.clone())
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    for tab in &tabs {
        let index = tabs.iter().position(|t| t == tab).unwrap();
        let grid = &harness.state().layout.tabs[index].grid_layout;
        let cards: Vec<(String, String, LayoutRect)> = grid
            .containers
            .iter()
            .map(|c| (c.widget_type.clone(), c.title.clone(), c.rect))
            .chain(
                grid.layouts
                    .iter()
                    .map(|l| ("List Layout".to_string(), l.title.clone(), l.rect)),
            )
            .collect();
        if cards.is_empty() {
            continue;
        }
        // The window just big enough for every card: the grid starts below
        // the tab bar, at the area's top left.
        show_tab(&mut harness, tab, 1);
        let top = harness.state().grid.area.min;
        let right = cards
            .iter()
            .map(|(_, _, r)| r.x + r.width)
            .fold(0.0, f64::max) as f32;
        let bottom = cards
            .iter()
            .map(|(_, _, r)| r.y + r.height)
            .fold(0.0, f64::max) as f32;
        harness.set_size(egui::vec2(
            (top.x + right + MARGIN).max(1280.0),
            top.y + bottom + MARGIN,
        ));
        show_tab(&mut harness, tab, 8);
        let image = harness.render().expect("render a frame");
        save_png(&image, &format!("{BOOK_IMAGES}/tabs/{}.png", slug(tab)));
        // Each widget type's first card, cut out with a little of the grid
        // around it.
        let area = harness.state().grid.area;
        for (kind, title, r) in &cards {
            if !pictured(kind, title, &seen) {
                continue;
            }
            seen.insert(kind.clone());
            let pad = 4.0;
            let x = (area.min.x + r.x as f32 - pad).max(0.0) as u32;
            let y = (area.min.y + r.y as f32 - pad).max(0.0) as u32;
            let w = ((r.width as f32 + 2.0 * pad) as u32).min(image.width() - x);
            let h = ((r.height as f32 + 2.0 * pad) as u32).min(image.height() - y);
            let cut = imageops::crop_imm(&image, x, y, w, h).to_image();
            save_png(&cut, &format!("{BOOK_IMAGES}/widgets/{}.png", slug(kind)));
        }
    }
    for (kind, title) in PICTURED {
        assert!(
            seen.contains(*kind),
            "no {kind} card titled {title:?} in the layout"
        );
    }
    println!(
        "widget types shown: {}",
        seen.into_iter().collect::<Vec<_>>().join(", ")
    );
    // A widget's settings, with the value text's Style rows open: the
    // Styles tab's serif Text Display.
    harness.set_size(egui::vec2(1280.0, 800.0));
    harness.state_mut().locked = false;
    show_tab(&mut harness, "Styles", 1);
    let tab = harness.state().current_tab;
    let index = harness.state().layout.tabs[tab]
        .grid_layout
        .containers
        .iter()
        .position(|w| w.title == "Sine (Serif)")
        .expect("the Styles tab's Sine (Serif)");
    let original = harness.state().layout.tabs[tab].grid_layout.containers[index].clone();
    harness.state_mut().selected = Some(Item::Widget(index));
    harness.state_mut().editing = Some(Editing::Widget {
        tab,
        index,
        original,
    });
    show_tab(&mut harness, "Styles", 1);
    // Title text's Style… comes first, the value text's second.
    harness
        .get_all_by_label("Style… (set)")
        .last()
        .expect("the value text's Style…")
        .click();
    show_tab(&mut harness, "Styles", 1);
    let image = harness.render().expect("render a frame");
    save_png(&image, &format!("{BOOK_IMAGES}/window/widget-settings.png"));
    harness.state_mut().editing = None;
    harness.state_mut().selected = None;

    // The window as it's set up: unlocked, the tree shown, at a laptop's
    // size.
    harness.set_size(egui::vec2(1280.0, 800.0));
    harness.state_mut().locked = false;
    harness.state_mut().tree.shown = true;
    show_tab(&mut harness, "Widgets", 3);
    let image = harness.render().expect("render a frame");
    save_png(&image, &format!("{BOOK_IMAGES}/window/editing.png"));
    // ⚙ Settings open, locked, without the tree, set up as a team would:
    // the robot by team number (the last shot: it leaves the sim).
    harness.state_mut().address.mode = AddressMode::TeamNumber;
    harness.state_mut().address.team = 3200;
    harness.state_mut().locked = true;
    harness.state_mut().tree.shown = false;
    harness.state_mut().gear.settings = true;
    show_tab(&mut harness, "Widgets", 1);
    let image = harness.render().expect("render a frame");
    save_png(&image, &format!("{BOOK_IMAGES}/window/settings.png"));
    harness.state_mut().gear.settings = false;
}
