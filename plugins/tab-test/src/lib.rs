//! A plugin that fills a tab: everything a game like Doom needs, small. It
//! reads `hello.txt` from its own folder (`/pack`), draws a square bouncing
//! across a 320 × 200 pixel frame (Doom's size) 35 times a second, moving a
//! whole number of pixels each step as a game does, counts the keys pressed,
//! and beeps through `play-audio` when Space is pressed.

use raptaboard_plugin::prelude::*;

/// The pixel frame's size, in pixels: Doom's.
const FRAME: (u32, u32) = (320, 200);
/// The square's side, in pixels.
const SQUARE: u32 = 24;
/// How far the square moves each step, in pixels (across; down is less).
const STEP: (u32, u32) = (3, 2);
/// Frames a second, as Doom's tics.
const RATE: f64 = 35.0;
/// The beep: its pitch (Hz), length (s) and sample rate.
const BEEP: (f32, f32, u32) = (880.0, 0.1, 22_050);

#[derive(Default)]
struct TabTest {
    /// hello.txt's first line, read on the first frame.
    hello: Option<String>,
    /// Keys pressed so far.
    keys: u32,
}

/// Where the square's top-left corner is at `time_s`: bouncing between the
/// frame's edges, moving [`STEP`] pixels every 35th of a second (a step, as a
/// game's tic), so each frame moves it the same whole number of pixels.
fn square_at(time_s: f64) -> (u32, u32) {
    let step = (time_s * RATE).floor() as u64;
    let bounce = |span: u32, by: u32| -> u32 {
        let span = u64::from(span.max(1));
        let t = (step * u64::from(by)) % (2 * span);
        (if t < span { t } else { 2 * span - t }) as u32
    };
    (
        bounce(FRAME.0 - SQUARE, STEP.0),
        bounce(FRAME.1 - SQUARE, STEP.1),
    )
}

/// The frame: dark blue, with the square in the theme's accent color.
fn frame_pixels(corner: (u32, u32), color: Color) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((FRAME.0 * FRAME.1 * 4) as usize);
    for y in 0..FRAME.1 {
        for x in 0..FRAME.0 {
            let inside = (corner.0..corner.0 + SQUARE).contains(&x)
                && (corner.1..corner.1 + SQUARE).contains(&y);
            if inside {
                rgba.extend([color.r, color.g, color.b, 255]);
            } else {
                rgba.extend([10, 20, 60, 255]);
            }
        }
    }
    rgba
}

/// The beep's samples: a sine wave, mono, fading out.
fn beep() -> Vec<i16> {
    let (pitch, length, rate) = BEEP;
    let n = (length * rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let fade = 1.0 - i as f32 / n as f32;
            ((t * pitch * std::f32::consts::TAU).sin() * fade * 8000.0) as i16
        })
        .collect()
}

impl Widget for TabTest {
    fn subscriptions(&self) -> Vec<Subscription> {
        Vec::new()
    }

    fn render(&mut self, frame: FrameInfo) -> Picture {
        let hello = self
            .hello
            .get_or_insert_with(|| match pack_file("hello.txt") {
                Ok(bytes) => String::from_utf8_lossy(&bytes)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
                Err(e) => format!("no hello.txt: {e}"),
            });
        let mut picture = Picture::empty();
        let (w, h) = (frame.size.width, frame.size.height);
        picture.shape(Shape::Pixels(Pixels {
            min: Point { x: 0.0, y: 0.0 },
            max: Point { x: w, y: h - 40.0 },
            width: FRAME.0,
            height: FRAME.1,
            rgba: frame_pixels(square_at(frame.time_s), frame.theme.accent),
            smooth: false,
        }));
        let line = format!("{hello} Keys: {}", self.keys);
        picture.text(Text {
            pos: Point {
                x: 8.0,
                y: h - 32.0,
            },
            text: line.clone(),
            size: 14.0,
            color: frame.theme.text,
            align: Align::Left,
            max_width: None,
            monospace: false,
        });
        picture.redraw_in_s = Some(1.0 / RATE);
        picture.summary = Some(line);
        picture
    }

    fn on_input(&mut self, event: Input) {
        if let Input::KeyDown(key) = event {
            self.keys += 1;
            if key.name == "Space"
                && let Err(e) = host::play_audio(&beep(), BEEP.2, 1)
            {
                host::log(&format!("no beep: {e}"));
            }
        }
    }
}

raptaboard_plugin::widget!(TabTest);

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn the_square_bounces_inside_the_frame() {
        for t in 0..2000 {
            let (x, y) = square_at(f64::from(t) * 0.037);
            assert!(x + SQUARE <= FRAME.0 && y + SQUARE <= FRAME.1, "{t}");
        }
        // One step: a whole step's pixels; within a step, nothing.
        let (a, b) = (square_at(0.0), square_at(1.0 / RATE + 1e-9));
        assert_eq!((b.0 - a.0, b.1 - a.1), STEP);
        assert_eq!(square_at(0.0), square_at(0.5 / RATE));
    }

    #[test]
    fn the_frame_has_every_pixel() {
        let rgba = frame_pixels((0, 0), color(1, 2, 3));
        assert_eq!(rgba.len(), (FRAME.0 * FRAME.1 * 4) as usize);
        assert_eq!(&rgba[..4], &[1, 2, 3, 255]);
        assert_eq!(beep().len(), 2205);
    }
}
