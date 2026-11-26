//! The clips in the README, made by running the app headless against a
//! scripted robot and saving the frames as a GIF, and the Microsoft Store
//! listing's screenshots, made the same way against robot-2027's sim. Ignored in normal test
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

/// Where the Microsoft Store's screenshots go, from the repo's root.
const STORE_DIR: &str = "media/store";

#[test]
#[ignore = "makes the Microsoft Store's screenshots: needs a GPU and robot-2027's sim running \
            on this computer (layouts/robot2027_sim.json's topics); run with --ignored"]
fn record_store_screenshots() {
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
    // 1920 x 1080 pixels, at 1.25 points a pixel as a laptop shows it.
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1536.0, 864.0))
        .with_pixels_per_point(1.25)
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
    let shoot = |harness: &mut Harness, tab: &str, name: &str| {
        let index = harness
            .state()
            .layout
            .tabs
            .iter()
            .position(|t| t.name == tab)
            .unwrap_or_else(|| panic!("no {tab} tab"));
        harness.state_mut().current_tab = index;
        // Ten seconds of values, at about 60 frames a second.
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            harness.step();
            std::thread::sleep(Duration::from_millis(16));
        }
        let image = harness.render().expect("render a frame");
        let out = root.join(STORE_DIR).join(name);
        image.save(&out).expect("save the screenshot");
        println!(
            "wrote {} ({}x{})",
            out.display(),
            image.width(),
            image.height()
        );
    };
    shoot(&mut harness, "Field", "screenshot-field.png");
    shoot(&mut harness, "More widgets", "screenshot-graphs.png");
    shoot(&mut harness, "Widgets", "screenshot-widgets.png");
}
