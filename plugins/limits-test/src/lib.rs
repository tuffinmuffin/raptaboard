//! Misbehaves on purpose when drawn, as its `mode` property says, so the
//! plugin-host tests can check the limits: `spin` never returns, `panic`
//! panics, `grow` keeps asking for memory, `publish` and `sound` try what
//! plugin.toml doesn't allow, `print` writes to stdout. `ok` draws one line.

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
            _ => {
                picture.text(centered_text(&frame, "ok", 14.0));
            }
        }
        picture
    }
}

raptaboard_plugin::widget!(LimitsTest);
