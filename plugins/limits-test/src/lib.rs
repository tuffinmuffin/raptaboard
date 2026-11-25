//! Misbehaves on purpose when drawn, as its `mode` property says, so the
//! plugin-host tests can check the limits: `spin` never returns, `panic`
//! panics, `grow` keeps asking for memory, `publish` and `sound` try what
//! plugin.toml doesn't allow, `print` writes to stdout, `region` draws a
//! "Bell" button that logs when it's pressed, `files` tries to read its own
//! plugin.toml (allowed), a file outside its folder and to write in its
//! folder (both refused), saying how each went. `ok` draws one line.

use raptaboard_plugin::prelude::*;

#[derive(Default)]
struct LimitsTest {
    kept: Vec<Vec<u8>>,
}

impl Widget for LimitsTest {
    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::Bound]
    }

    fn render(&mut self, frame: FrameInfo) -> Picture {
        let mut picture = Picture::empty();
        match property_string("mode", "ok").as_str() {
            "spin" => {
                let mut n: u64 = 0;
                loop {
                    n = std::hint::black_box(n.wrapping_add(1));
                }
            }
            "panic" => panic!("limits-test panicked on purpose"),
            "grow" => loop {
                self.kept.push(vec![1; 1 << 20]);
            },
            "publish" => {
                let result = host::publish("x", &NtValue::Double(1.0));
                picture.summary = Some(format!("{result:?}"));
            }
            "sound" => {
                let result = host::play_sound("fire 1");
                picture.summary = Some(format!("{result:?}"));
            }
            "print" => println!("hello from limits-test"),
            "files" => {
                let own = pack_file("plugin.toml").map(|b| b.len());
                let outside = std::fs::read("/etc/hosts").map(|b| b.len());
                let up = std::fs::read("/pack/../Cargo.toml").map(|b| b.len());
                let write = std::fs::write("/pack/written.txt", b"x");
                picture.summary = Some(format!(
                    "own {}; outside {}; up {}; write {}",
                    own.is_ok(),
                    outside.is_ok(),
                    up.is_ok(),
                    write.is_ok()
                ));
            }
            "region" => {
                picture.text(centered_text(&frame, "bell", 14.0));
                picture.regions.push(Region {
                    id: "bell".into(),
                    min: Point { x: 10.0, y: 10.0 },
                    max: Point {
                        x: frame.size.width - 10.0,
                        y: frame.size.height - 10.0,
                    },
                    label: "Bell".into(),
                    role: Role::Button,
                    checked: None,
                });
            }
            _ => {
                picture.text(centered_text(&frame, "ok", 14.0));
            }
        }
        picture
    }

    fn on_input(&mut self, event: Input) {
        if let Input::Activate(id) = event {
            host::log(&format!("{id} pressed"));
        }
    }
}

raptaboard_plugin::widget!(LimitsTest);
