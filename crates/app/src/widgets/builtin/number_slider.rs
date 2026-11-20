//! Number Slider: a number shown on a slider, which can be dragged (or
//! clicked) to send a new value to the topic.
//!
//! The value is written above the slider and labels run along it, like a
//! Number Bar. Press anywhere on the widget's input area (locked, the
//! whole card; unlocked, the body below the title bar, which moves the
//! widget) and the knob jumps to the pointer; drag and it follows, even
//! when the pointer leaves the widget. The value is sent when the button
//! is let go, or all the way through the drag with `publish_all` (as in
//! Elastic). Until the server has the sent value, or a second has passed,
//! the knob stays where it was let go, so it doesn't jump back while the
//! value is on its way.
//!
//! Elastic's settings: `min_value`, `max_value`, `divisions` (how many
//! labels) and `publish_all`. raptaboard's: `step` (values snap to a
//! multiple of it; 0 for any value) and the [`scale`] unit and labels
//! settings. An int topic gets whole numbers.

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Rect, Sense, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use super::number_bar::{DIVISIONS, MAX_VALUE, MIN_VALUE, fraction, labels};
use crate::widgets::shared::format::{number_of, short_number};
use crate::widgets::shared::{card, props, scale};
use crate::widgets::{Live, ValueInfo, WidgetType};

/// Send every value during a drag, not only the last (Elastic).
pub const PUBLISH_ALL: &str = "publish_all";
/// Values snap to a multiple of this; 0 (the default) for any value.
pub const STEP: &str = "step";

const KEYS: [&str; 8] = [
    MIN_VALUE,
    MAX_VALUE,
    DIVISIONS,
    PUBLISH_ALL,
    STEP,
    scale::UNIT,
    scale::SHOW_LABELS,
    scale::SHOW_VALUE,
];

/// The range when `min_value` and `max_value` aren't set (Elastic's).
const DEFAULT_MIN: f64 = -1.0;
const DEFAULT_MAX: f64 = 1.0;
/// Track color left of the knob (Material blue, like the Number Bar).
const FILL_COLOR: Color32 = Color32::from_rgb(0x21, 0x96, 0xF3);
/// Thickness of the track, in points.
const TRACK_THICKNESS: f32 = 6.0;
/// The knob's radius, in points.
const KNOB_RADIUS: f32 = 9.0;
/// Size of the labels along the track, in points.
const LABEL_SIZE: f32 = 11.0;
/// Size of the value text, in points.
const VALUE_SIZE: f32 = 16.0;
/// Space between the parts, in points; the track ends this far in from
/// each side plus the knob's radius, so the knob fits at the ends.
const PAD: f32 = 6.0;
/// How long the knob waits at a sent value for the server to have it, in
/// seconds, before showing the server's value again.
const HOLD_S: f64 = 1.0;

/// The value at `at` (0 at the slider's left end, 1 at its right) between
/// `min` and `max`, kept inside the range. It snaps to a multiple of `step`
/// if `step` is above 0, and otherwise to a thousandth of the range rounded
/// down to a power of ten (0.01 for 0 to 10), so a drag sends 3.07 rather
/// than 3.0712345.
///
/// ```
/// use raptaboard::widgets::builtin::number_slider::value_at;
/// assert_eq!(value_at(0.5, -1.0, 1.0, 0.0), 0.0);
/// assert_eq!(value_at(0.8, 0.0, 10.0, 5.0), 10.0);
/// assert_eq!(value_at(0.30712, 0.0, 10.0, 0.0), 3.07);
/// assert_eq!(value_at(2.0, -1.0, 1.0, 0.0), 1.0);
/// ```
pub fn value_at(at: f32, min: f64, max: f64, step: f64) -> f64 {
    let (lo, hi) = (min.min(max), min.max(max));
    if max == min {
        return min;
    }
    let raw = min + (max - min) * f64::from(at.clamp(0.0, 1.0));
    let step = if step > 0.0 {
        step
    } else {
        10f64.powf(((hi - lo) / 1000.0).log10().floor())
    };
    // Multiples of the step counted from the minimum, so a range of 1 to 6
    // with step 2 gives 1, 3, 5; one past the far end goes back a step.
    let step = step.copysign(max - min);
    let steps = ((raw - min) / step).round();
    let mut snapped = min + steps * step;
    if snapped < lo || snapped > hi {
        snapped = min + (steps - 1.0).max(0.0) * step;
    }
    // Without the last bits of float noise (0.30000000000000004 is 0.3).
    let scale = 1e9;
    ((snapped * scale).round() / scale).clamp(lo, hi)
}

/// What the slider is set up as, from its settings and the value's type.
struct Setup {
    min: f64,
    max: f64,
    step: f64,
    whole: bool,
    divisions: usize,
}

impl Setup {
    fn new(p: &Map<String, Json>, value: Option<&Value>) -> Setup {
        let whole = matches!(value, Some(Value::Int(_)));
        let step = props::number(p, STEP, 0.0).max(0.0);
        Setup {
            min: props::number(p, MIN_VALUE, DEFAULT_MIN),
            max: props::number(p, MAX_VALUE, DEFAULT_MAX),
            // An int topic can only take whole numbers.
            step: if whole { step.max(1.0).round() } else { step },
            whole,
            divisions: props::number(p, DIVISIONS, 5.0).clamp(2.0, 50.0) as usize,
        }
    }
}

/// Where the track runs in `body`: below the value (if shown), with room
/// for the knob at both ends.
fn track_rect(body: Rect, show_value: bool) -> Rect {
    let top = if show_value {
        body.min.y + VALUE_SIZE * 1.5 + PAD
    } else {
        body.min.y + PAD
    };
    let inset = PAD + KNOB_RADIUS;
    let center_y = top + KNOB_RADIUS;
    Rect::from_min_max(
        egui::pos2(body.min.x + inset, center_y - TRACK_THICKNESS / 2.0),
        egui::pos2(
            (body.max.x - inset).max(body.min.x + inset),
            center_y + TRACK_THICKNESS / 2.0,
        ),
    )
}

/// Where the track of a Number Slider whose card is `card` runs on screen,
/// with its settings `p`: the left end is the minimum, the right end the
/// maximum.
pub fn track_on_card(card: Rect, p: &Map<String, Json>) -> Rect {
    let body = card.with_min_y(card.min.y + card::TITLE_HEIGHT);
    track_rect(body.shrink(card::BODY_MARGIN), scale::show_value(p))
}

/// Draw the slider in `body`: the value `shown` (none: "no value"), the
/// track filled to it with the knob, and labels under it.
fn paint(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    body: Rect,
    p: &Map<String, Json>,
    setup: &Setup,
    shown: Option<f64>,
) {
    let show_value = scale::show_value(p);
    let track = track_rect(body, show_value);
    let label_color = visuals.weak_text_color();
    if show_value {
        let text = match shown {
            Some(v) => scale::value_with_unit(v, setup.whole, p, ""),
            None => "no value".to_owned(),
        };
        painter.text(
            egui::pos2(body.center().x, body.min.y + VALUE_SIZE * 0.75),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::monospace(VALUE_SIZE),
            if shown.is_some() {
                visuals.text_color()
            } else {
                label_color
            },
        );
    }
    let round = CornerRadius::same((TRACK_THICKNESS / 2.0) as u8);
    painter.rect_filled(track, round, visuals.widgets.inactive.bg_fill);
    if let Some(v) = shown {
        let x = track.min.x + track.width() * fraction(v, setup.min, setup.max);
        painter.rect_filled(track.with_max_x(x), round, FILL_COLOR);
        let knob = egui::pos2(x, track.center().y);
        painter.circle_filled(knob, KNOB_RADIUS, Color32::WHITE);
        painter.circle_stroke(knob, KNOB_RADIUS, Stroke::new(2.0, FILL_COLOR));
        // Past an end: the dot the bars use, just beyond it.
        if let Some(end) = scale::past_end(v, setup.min, setup.max) {
            let x = match end {
                scale::Past::Min => track.min.x - KNOB_RADIUS - 4.0,
                scale::Past::Max => track.max.x + KNOB_RADIUS + 4.0,
            };
            scale::draw_past_dot(painter, egui::pos2(x, track.center().y));
        }
    }
    if scale::show_labels(p) {
        let marks = labels(setup.min, setup.max, setup.divisions);
        let steps = (marks.len() - 1) as f32;
        let y = track.center().y + KNOB_RADIUS + 2.0;
        for (i, n) in marks.into_iter().enumerate() {
            let x = track.min.x + track.width() * i as f32 / steps;
            painter.vline(x, y..=y + 4.0, Stroke::new(1.0, label_color));
            painter.text(
                egui::pos2(x, y + 5.0),
                egui::Align2::CENTER_TOP,
                short_number(n),
                egui::FontId::proportional(LABEL_SIZE),
                label_color,
            );
        }
    }
}

/// A value the slider has set and shows instead of the topic's: while the
/// button is down, and after it's let go until the topic has it (or
/// [`HOLD_S`] has passed).
#[derive(Clone, Copy, Debug)]
struct Held {
    value: f64,
    /// The last value sent during this drag, with `publish_all`.
    sent: Option<f64>,
    /// When the button was let go (egui time, seconds); `None` while down.
    released: Option<f64>,
}

/// The slider with input over `area` and drawn in `body`. The value to
/// send, if any.
fn show_slider(
    ui: &mut egui::Ui,
    area: Rect,
    body: Rect,
    widget: &WidgetContainer,
    live: &Live<'_>,
) -> Option<Value> {
    let p = &widget.properties;
    let visuals = ui.visuals().clone();
    let topic = live.value.and_then(number_of);
    if live.value.is_some() && topic.is_none() {
        card::draw_message(ui.painter(), &visuals, body, "not a number");
        return None;
    }
    let setup = Setup::new(p, live.value);
    let track = track_rect(body, scale::show_value(p));
    let id = ui.scope_id().with("number_slider");
    let response = ui.interact(area, id, Sense::click_and_drag());
    if response.hovered() || response.is_pointer_button_down_on() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let now = ui.input(|i| i.time);
    let mut held: Option<Held> = ui.data(|d| d.get_temp(id));
    let mut send = None;
    if response.is_pointer_button_down_on()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let at = (pos.x - track.min.x) / track.width().max(1.0);
        let value = value_at(at, setup.min, setup.max, setup.step);
        let sent = held.and_then(|h| h.released.is_none().then_some(h.sent).flatten());
        let mut h = Held {
            value,
            sent,
            released: None,
        };
        if props::flag(p, PUBLISH_ALL, false) && sent != Some(value) {
            h.sent = Some(value);
            send = Some(value);
        }
        held = Some(h);
    } else if let Some(h) = held.as_mut()
        && h.released.is_none()
    {
        // Let go: send where it ended (unless that was sent already).
        h.released = Some(now);
        if h.sent != Some(h.value) {
            send = Some(h.value);
        }
    }
    // Stop holding once the topic has the value, or after a while.
    if let Some(h) = held
        && let Some(released) = h.released
        && (topic == Some(h.value) || now - released > HOLD_S)
    {
        held = None;
    }
    match held {
        Some(h) => {
            ui.data_mut(|d| d.insert_temp(id, h));
            if h.released.is_some() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(HOLD_S));
            }
        }
        None => ui.data_mut(|d| d.remove::<Held>(id)),
    }
    let shown = held.map(|h| h.value).or(topic);
    let painter = ui.painter().with_clip_rect(body.intersect(ui.clip_rect()));
    paint(&painter, &visuals, body, p, &setup, shown);
    send.map(|v| {
        if setup.whole {
            Value::Int(v.round() as i64)
        } else {
            Value::Double(v)
        }
    })
}

/// The Number Slider widget (Elastic's `"Number Slider"`).
pub struct NumberSlider;

impl WidgetType for NumberSlider {
    fn names(&self) -> &'static [&'static str] {
        &["Number Slider"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        matches!(nt_type, "double" | "float" | "int")
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        let area = if live.locked { rect } else { body };
        show_slider(ui, area, body.shrink(card::BODY_MARGIN), widget, live)
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        show_slider(ui, live.input.unwrap_or(rect), rect, widget, live)
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        props: &Map<String, Json>,
        value: Option<&Value>,
    ) {
        match (value, value.and_then(number_of)) {
            (Some(_), None) => card::draw_message(painter, visuals, rect, "not a number"),
            (_, shown) => paint(
                painter,
                visuals,
                rect,
                props,
                &Setup::new(props, value),
                shown,
            ),
        }
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        props::number_row(ui, p, "Minimum", MIN_VALUE, DEFAULT_MIN, -1e9..=1e9);
        props::number_row(ui, p, "Maximum", MAX_VALUE, DEFAULT_MAX, -1e9..=1e9);
        props::number_row(ui, p, "Step (0 for any)", STEP, 0.0, 0.0..=1e9);
        props::whole_row(ui, p, "Labels", DIVISIONS, 5, 2..=50);
        props::flag_row(
            ui,
            p,
            "Send",
            "Every value while dragging",
            PUBLISH_ALL,
            false,
        );
        ui.label("Unit");
        let mut unit = props::text(p, scale::UNIT, "").to_owned();
        if ui.text_edit_singleline(&mut unit).changed() {
            props::set_property(p, scale::UNIT, (!unit.is_empty()).then(|| unit.into()));
        }
        ui.end_row();
        props::flag_row(ui, p, "Value", "Show the value", scale::SHOW_VALUE, true);
        props::flag_row(ui, p, "Labels", "Show labels", scale::SHOW_LABELS, true);
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn positions_become_values_in_range() {
        assert_eq!(value_at(0.0, -1.0, 1.0, 0.0), -1.0);
        assert_eq!(value_at(1.0, -1.0, 1.0, 0.0), 1.0);
        assert_eq!(value_at(-3.0, 0.0, 10.0, 0.0), 0.0);
        // Steps count from the minimum, and never go past the maximum.
        assert_eq!(value_at(0.5, 1.0, 6.0, 2.0), 3.0);
        assert_eq!(value_at(1.0, 1.0, 6.0, 2.0), 5.0);
        // A backwards range still stays inside its ends.
        assert_eq!(value_at(0.0, 1.0, -1.0, 0.0), 1.0);
        assert_eq!(value_at(1.0, 1.0, -1.0, 0.0), -1.0);
    }

    #[test]
    fn int_topics_get_whole_steps() {
        let none = Map::new();
        let int = Setup::new(&none, Some(&Value::Int(3)));
        assert!(int.whole);
        assert_eq!(int.step, 1.0);
        let half = json_props(json!({"step": 0.5}));
        assert_eq!(Setup::new(&half, Some(&Value::Int(3))).step, 1.0);
        assert_eq!(Setup::new(&half, Some(&Value::Double(3.0))).step, 0.5);
        let range = json_props(json!({"min_value": 0, "max_value": 100}));
        let s = Setup::new(&range, None);
        assert_eq!((s.min, s.max, s.step, s.whole), (0.0, 100.0, 0.0, false));
    }

    #[test]
    fn the_track_leaves_room_for_the_knob() {
        let body = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 100.0));
        let t = track_rect(body, true);
        assert_eq!(t.min.x, PAD + KNOB_RADIUS);
        assert_eq!(t.max.x, 200.0 - PAD - KNOB_RADIUS);
        assert!(t.min.y > VALUE_SIZE);
        // Without the value it moves up.
        assert!(track_rect(body, false).min.y < t.min.y);
        // Too narrow: an empty track, not a backwards one.
        let thin = track_rect(
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 50.0)),
            true,
        );
        assert_eq!(thin.width(), 0.0);
    }

    use serde_json::json;
    fn json_props(v: Json) -> Map<String, Json> {
        v.as_object().unwrap().clone()
    }
}
