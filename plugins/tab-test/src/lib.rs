//! A plugin that fills a tab: everything a game like Doom needs, small. It
//! reads `hello.txt` from its own folder (`/pack`), draws a square bouncing
//! across a pixel frame 35 times a second, counts the keys pressed, and beeps
//! through `play-audio` when Space is pressed.

use raptaboard_plugin::prelude::*;

/// The pixel frame's size, in pixels.
const FRAME: (u32, u32) = (64, 40);
/// The square's side, in pixels.
const SQUARE: u32 = 8;
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
/// frame's edges, `per_s` pixels a second each way.
fn square_at(time_s: f64, per_s: f64) -> (u32, u32) {
    let bounce = |span: u32, speed: f64| -> u32 {
        let span = f64::from(span.max(1));
        let t = (time_s * speed) % (2.0 * span);
        (if t < span { t } else { 2.0 * span - t }) as u32
    };
    (
        bounce(FRAME.0 - SQUARE, per_s),
        bounce(FRAME.1 - SQUARE, per_s * 0.7),
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
            rgba: frame_pixels(square_at(frame.time_s, 20.0), frame.theme.accent),
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
        for t in 0..200 {
            let (x, y) = square_at(f64::from(t) * 0.37, 20.0);
            assert!(x + SQUARE <= FRAME.0 && y + SQUARE <= FRAME.1, "{t}");
        }
        assert_ne!(square_at(0.0, 20.0), square_at(1.0, 20.0));
    }

    #[test]
    fn the_frame_has_every_pixel() {
        let rgba = frame_pixels((0, 0), color(1, 2, 3));
        assert_eq!(rgba.len(), (FRAME.0 * FRAME.1 * 4) as usize);
        assert_eq!(&rgba[..4], &[1, 2, 3, 255]);
        assert_eq!(beep().len(), 2205);
    }
}
