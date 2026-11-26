//! A topic plugin for raptaboard's own tests: a number topic as text, with
//! its unit (`unit`), greyed while disconnected, `decimal_places` (-1 for up
//! to 4) and `fit_text` (as large as fits, by `measure-text`). The published
//! examples (Number, Alerts) are in raptaboard-examples.

use raptaboard_plugin::prelude::*;

#[derive(Default)]
struct TopicTest;

impl Widget for TopicTest {
    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::Bound]
    }

    fn render(&mut self, frame: FrameInfo) -> Picture {
        let places = property_number("decimal_places", -1.0);
        let decimals = (places >= 0.0).then(|| places.min(10.0) as usize);
        let text = match number(&host::get("")) {
            None => "—".to_string(),
            Some(n) => match host::unit("") {
                Some(u) if !u.is_empty() => format!("{} {u}", format_number(n, decimals)),
                _ => format_number(n, decimals),
            },
        };
        let mut size = property_number("font_size", 14.0) as f32;
        if property_bool("fit_text", false) {
            let at_100 = host::measure_text(&text, 100.0, None, false);
            let by_width = frame.size.width * 0.9 / at_100.width.max(1.0) * 100.0;
            let by_height = frame.size.height * 0.8 / at_100.height.max(1.0) * 100.0;
            size = by_width.min(by_height).max(6.0);
        }
        let mut picture = Picture::empty();
        let mut line = centered_text(&frame, &text, size);
        if !frame.connected {
            line.color = frame.theme.weak_text;
        }
        picture.text(line);
        picture.summary = Some(text);
        picture
    }
}

raptaboard_plugin::widget!(TopicTest);
