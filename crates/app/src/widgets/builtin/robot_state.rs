//! Robot State (raptaboard): what the robot is doing, large, in its own
//! color (Disabled, Autonomous, Teleop, Test, E-Stop, or No robot), with
//! small DS and FMS marks, and a sound when it changes: on disable, the
//! start of auto, teleop and test, e-stop, and the robot, DS or FMS being
//! lost (or the robot coming back).
//!
//! Bound to the Driver Station's table: WPILib 2027's `/DriverStation` or
//! 2026's `/FMSInfo` ([`crate::types::robot_state`] reads either). Dragging
//! that table from the tree makes one.
//!
//! Each event's sound is a setting, `sound_<event>`: `off`, one of the
//! built-in [`Tone`]s by name, or the path of an MP3 or WAV file; not set,
//! the event's default ([`default_sound`]). `volume` (0 to 1) is how loud.
//! Sounds play from [`WidgetType::watch`], so from any tab, only for the
//! first Robot State in the layout, and only on a real change (never for
//! the state found at connect). The bell button mutes them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, CornerRadius, Rect, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::sound::{Clip, Tone};
use crate::types::robot_state::{self, Event, Mode, RobotState, Watch};
use crate::widgets::shared::{card, props};
use crate::widgets::{Effect, Live, Server, ValueInfo, WidgetType};

/// How loud its sounds are, 0 to 1 (raptaboard).
pub const VOLUME: &str = "volume";
const DEFAULT_VOLUME: f64 = 0.8;
/// A sound setting's value for no sound.
pub const OFF: &str = "off";

/// The setting key for `event`'s sound.
pub fn sound_key(event: Event) -> &'static str {
    match event {
        Event::Disabled => "sound_disabled",
        Event::Autonomous => "sound_autonomous",
        Event::Teleop => "sound_teleop",
        Event::Test => "sound_test",
        Event::EStop => "sound_estop",
        Event::RobotConnected => "sound_robot_connected",
        Event::RobotLost => "sound_robot_lost",
        Event::DsLost => "sound_ds_lost",
        Event::FmsLost => "sound_fms_lost",
    }
}

const KEYS: [&str; 10] = [
    VOLUME,
    "sound_disabled",
    "sound_autonomous",
    "sound_teleop",
    "sound_test",
    "sound_estop",
    "sound_robot_connected",
    "sound_robot_lost",
    "sound_ds_lost",
    "sound_fms_lost",
];

/// How each event is named in the settings.
fn event_label(event: Event) -> &'static str {
    match event {
        Event::Disabled => "Disabled",
        Event::Autonomous => "Autonomous",
        Event::Teleop => "Teleop",
        Event::Test => "Test",
        Event::EStop => "E-Stop",
        Event::RobotConnected => "Robot connected",
        Event::RobotLost => "Robot lost",
        Event::DsLost => "DS lost",
        Event::FmsLost => "FMS lost",
    }
}

/// The sound `event` plays when its setting isn't set: a built-in clip's
/// name, or [`OFF`].
///
/// ```
/// use raptaboard::types::robot_state::Event;
/// use raptaboard::widgets::builtin::robot_state::default_sound;
/// assert_eq!(default_sound(Event::Teleop), "go go go");
/// assert_eq!(default_sound(Event::RobotConnected), "off");
/// ```
pub fn default_sound(event: Event) -> &'static str {
    match event {
        Event::Disabled => "disabled 1",
        Event::Autonomous => "auto ghosts",
        Event::Teleop => "go go go",
        Event::Test => "danger danger",
        Event::RobotLost => "lost comms",
        Event::FmsLost => "fms down",
        Event::EStop | Event::RobotConnected | Event::DsLost => OFF,
    }
}

/// The sound set for `event` in `p`: `None` for off.
///
/// ```
/// use raptaboard::sound::{Clip, Tone};
/// use raptaboard::types::robot_state::Event;
/// use raptaboard::widgets::builtin::robot_state::sound_for;
/// let mut p = serde_json::Map::new();
/// assert_eq!(sound_for(&p, Event::Test), Some(Clip::Tone(Tone::Danger)));
/// p.insert("sound_test".into(), "off".into());
/// assert_eq!(sound_for(&p, Event::Test), None);
/// p.insert("sound_test".into(), "/sounds/horn.mp3".into());
/// assert_eq!(sound_for(&p, Event::Test), Some(Clip::File("/sounds/horn.mp3".into())));
/// ```
pub fn sound_for(p: &Map<String, Json>, event: Event) -> Option<Clip> {
    let setting = props::text(p, sound_key(event), default_sound(event)).trim();
    if setting.is_empty() || setting == OFF {
        return None;
    }
    match Tone::CHOICES.iter().find(|(_, name)| *name == setting) {
        Some((tone, _)) => Some(Clip::Tone(*tone)),
        None => Some(Clip::File(PathBuf::from(setting))),
    }
}

/// The widget's volume, 0 to 1.
fn volume(p: &Map<String, Json>) -> f32 {
    props::number(p, VOLUME, DEFAULT_VOLUME).clamp(0.0, 1.0) as f32
}

/// What the widget remembers between frames: the changes it has seen.
#[derive(Clone, Default)]
struct Memory {
    /// Which table it was watching: another means starting over.
    table: String,
    watch: Watch,
}

/// A sound asked for from the settings dialog's ▶ button, played by the
/// next [`WidgetType::watch`] (the dialog can't play sounds itself).
#[derive(Clone)]
struct Preview(Clip, f32);

fn memory_id() -> egui::Id {
    egui::Id::unique("robot_state_watch")
}

fn preview_id() -> egui::Id {
    egui::Id::unique("robot_state_preview")
}

/// The fill and text color for each mode.
fn colors(mode: Mode) -> (Color32, Color32) {
    match mode {
        Mode::NoRobot => (Color32::from_rgb(0x61, 0x61, 0x61), Color32::WHITE),
        Mode::Disabled => (Color32::from_rgb(0xC6, 0x28, 0x28), Color32::WHITE),
        Mode::Autonomous => (Color32::from_rgb(0x15, 0x65, 0xC0), Color32::WHITE),
        Mode::Teleop => (Color32::from_rgb(0x2E, 0x7D, 0x32), Color32::WHITE),
        Mode::Test => (Color32::from_rgb(0xEF, 0x6C, 0x00), Color32::BLACK),
        Mode::EStop => (Color32::from_rgb(0xD5, 0x00, 0x00), Color32::WHITE),
    }
}

/// The state to show: none while disconnected or before any arrives.
fn shown_state(widget: &WidgetContainer, live: &Live<'_>) -> Option<RobotState> {
    if !live.frame.connected {
        return None;
    }
    robot_state::read(widget.topic()?, live.frame.others)
}

/// Draw `state` filling `rect`. `time` (seconds) blinks an e-stop.
fn draw_state(painter: &egui::Painter, rect: Rect, state: Option<RobotState>, time: f64) {
    let mode = state.map_or(Mode::NoRobot, |s| s.mode);
    let (fill, ink) = colors(mode);
    let radius = CornerRadius::same(6);
    // E-stop blinks: full red half the time, darker the other half.
    let fill = if mode == Mode::EStop && (time * 2.0).fract() >= 0.5 {
        fill.gamma_multiply(0.45)
    } else {
        fill
    };
    painter.rect_filled(rect, radius, fill);
    if mode == Mode::Test {
        // Test is striped, so it can't be taken for a match mode.
        let stripes = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        let gap = 18.0;
        let dark = Color32::from_rgba_unmultiplied(0, 0, 0, 40);
        let mut x = rect.left() - rect.height();
        while x < rect.right() {
            stripes.line_segment(
                [
                    egui::pos2(x, rect.bottom()),
                    egui::pos2(x + rect.height(), rect.top()),
                ],
                Stroke::new(gap / 2.0, dark),
            );
            x += gap;
        }
    }
    // The label as large as fits above the DS and FMS marks.
    let marks_h = (rect.height() * 0.22).clamp(12.0, 28.0);
    let label = Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.bottom() - marks_h));
    card::draw_fitted(painter, label.shrink(4.0), mode.label(), ink);
    let marks = Rect::from_min_max(egui::pos2(rect.left(), label.bottom()), rect.max);
    let (ds, fms) = state.map_or((false, false), |s| (s.ds, s.fms));
    let font = egui::FontId::proportional(marks_h * 0.7);
    for (text, on, x) in [("DS", ds, 0.3), ("FMS", fms, 0.7)] {
        let color = if on { ink } else { ink.gamma_multiply(0.35) };
        let at = egui::pos2(marks.left() + marks.width() * x, marks.center().y);
        let what = if on {
            text.to_owned()
        } else {
            format!("no {text}")
        };
        painter.text(at, egui::Align2::CENTER_CENTER, what, font.clone(), color);
    }
}

/// The Robot State widget.
pub struct RobotStateWidget;

impl RobotStateWidget {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let state = shown_state(widget, live);
        let time = ui.input(|i| i.time);
        draw_state(ui.painter(), rect, state, time);
        if state.is_some_and(|s| s.mode == Mode::EStop) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

impl WidgetType for RobotStateWidget {
    fn names(&self) -> &'static [&'static str] {
        &["Robot State"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[robot_state::TYPE_2027, robot_state::TYPE_2026]
    }

    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        table_topics(p)
    }

    fn background_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        table_topics(p)
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        self.show_in(ui, body.shrink(card::BODY_MARGIN), widget, live);
        None
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        self.show_in(ui, rect, widget, live);
        None
    }

    fn draw_body(
        &self,
        painter: &egui::Painter,
        _: &egui::Visuals,
        rect: Rect,
        _: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        // Without the table's values (while dragged).
        draw_state(painter, rect, None, 0.0);
    }

    fn watch(
        &self,
        ctx: &egui::Context,
        widget: &WidgetContainer,
        values: &HashMap<&str, &Value>,
        connected: bool,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        let preview = ctx.data_mut(|d| {
            let asked = d.get_temp::<Preview>(preview_id());
            d.remove::<Preview>(preview_id());
            asked
        });
        if let Some(Preview(clip, volume)) = preview {
            effects.push(Effect::Event { clip, volume });
        }
        let Some(table) = widget.topic() else {
            return effects;
        };
        let mut memory: Memory = ctx.data(|d| d.get_temp(memory_id())).unwrap_or_default();
        if memory.table != table {
            memory = Memory {
                table: table.to_owned(),
                watch: Watch::default(),
            };
        }
        let state = robot_state::read(table, values);
        let events = memory.watch.update(connected, state);
        ctx.data_mut(|d| d.insert_temp(memory_id(), memory));
        let p = &widget.properties;
        // A new event cuts the last one off, so only the newest plays.
        if let Some(clip) = events.iter().rev().find_map(|e| sound_for(p, *e)) {
            effects.push(Effect::Event {
                clip,
                volume: volume(p),
            });
        }
        effects
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        ui.label("Volume");
        let mut v = f64::from(volume(p));
        if ui
            .add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false))
            .on_hover_text("How loud this widget's sounds are")
            .changed()
        {
            let v = (v * 100.0).round() / 100.0;
            props::set_property(p, VOLUME, (v != DEFAULT_VOLUME).then(|| v.into()));
        }
        ui.end_row();
        for event in Event::ALL {
            sound_row(ui, p, event);
        }
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        &KEYS
    }
}

/// The table's topics this widget reads.
fn table_topics(p: &Map<String, Json>) -> Vec<String> {
    p.get("topic")
        .and_then(Json::as_str)
        .map_or_else(Vec::new, robot_state::topics)
}

/// A settings row picking `event`'s sound: Off, a built-in clip or tone, or
/// a file (File… opens a file dialog), with ▶ to hear it.
fn sound_row(ui: &mut egui::Ui, p: &mut Map<String, Json>, event: Event) {
    let key = sound_key(event);
    let default = default_sound(event);
    ui.label(event_label(event));
    let current = props::text(p, key, default).to_owned();
    let mut picked = current.clone();
    let shown = match sound_for(p, event) {
        None => "Off".to_owned(),
        Some(Clip::Tone(_)) => current.clone(),
        Some(Clip::File(path)) => file_name(&path),
    };
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(key)
            .selected_text(shown)
            .width(170.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut picked, OFF.to_owned(), "Off");
                for (_, name) in Tone::CHOICES {
                    ui.selectable_value(&mut picked, name.to_owned(), name);
                }
                ui.separator();
                if ui.selectable_label(false, "File… (MP3 or WAV)").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Sound", &["mp3", "wav"])
                        .pick_file()
                {
                    picked = path.display().to_string();
                }
            });
        let clip = sound_for(p, event);
        if ui
            .add_enabled(clip.is_some(), egui::Button::new("▶").small())
            .on_hover_text("Play it")
            .clicked()
            && let Some(clip) = clip
        {
            ui.ctx()
                .data_mut(|d| d.insert_temp(preview_id(), Preview(clip, volume(p))));
        }
        if let Some(Clip::File(path)) = sound_for(p, event)
            && !path.exists()
        {
            ui.colored_label(ui.visuals().error_fg_color, "file not found")
                .on_hover_text(path.display().to_string());
        }
    });
    ui.end_row();
    if picked != current {
        props::set_property(p, key, (picked != default).then(|| picked.into()));
    }
}

/// The last part of `path`, for showing a picked file.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn widget(props: serde_json::Value) -> WidgetContainer {
        let mut w: WidgetContainer = serde_json::from_value(serde_json::json!({
            "title": "Robot", "x": 0, "y": 0, "width": 256, "height": 128,
            "type": "Robot State", "properties": {"topic": "/FMSInfo"}
        }))
        .unwrap();
        if let serde_json::Value::Object(extra) = props {
            w.properties.extend(extra);
        }
        w
    }

    fn frame(
        ctx: &egui::Context,
        w: &WidgetContainer,
        connected: bool,
        bits: Option<i64>,
    ) -> Vec<Effect> {
        let value = bits.map(Value::Int);
        let mut values = HashMap::new();
        if let Some(v) = &value {
            values.insert("/FMSInfo/FMSControlData", v);
        }
        RobotStateWidget.watch(ctx, w, &values, connected)
    }

    fn clips(effects: &[Effect]) -> Vec<Clip> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Event { clip, .. } => Some(clip.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn changes_play_their_sounds_and_the_state_at_connect_does_not() {
        let ctx = egui::Context::default();
        let w = widget(serde_json::json!({}));
        assert!(
            frame(&ctx, &w, true, Some(0x21)).is_empty(),
            "found enabled"
        );
        assert_eq!(
            clips(&frame(&ctx, &w, true, Some(0x20))),
            [Clip::Tone(Tone::Disabled1)]
        );
        assert_eq!(
            clips(&frame(&ctx, &w, true, Some(0x23))),
            [Clip::Tone(Tone::AutoGhosts)]
        );
        assert!(frame(&ctx, &w, true, Some(0x23)).is_empty(), "no change");
        assert_eq!(
            clips(&frame(&ctx, &w, false, Some(0x23))),
            [Clip::Tone(Tone::LostComms)]
        );
    }

    #[test]
    fn a_sound_set_off_or_to_a_file_is_used() {
        let ctx = egui::Context::default();
        let w = widget(serde_json::json!({
            "sound_teleop": "off",
            "sound_disabled": "/sounds/horn.mp3",
            "volume": 0.5,
        }));
        frame(&ctx, &w, true, Some(0x20));
        assert!(frame(&ctx, &w, true, Some(0x21)).is_empty(), "teleop off");
        let effects = frame(&ctx, &w, true, Some(0x20));
        let [Effect::Event { clip, volume }] = effects.as_slice() else {
            panic!("one sound");
        };
        assert_eq!(*clip, Clip::File("/sounds/horn.mp3".into()));
        assert_eq!(*volume, 0.5);
    }

    #[test]
    fn every_built_in_default_is_a_built_in_clip() {
        for event in Event::ALL {
            let name = default_sound(event);
            assert!(
                name == OFF || Tone::CHOICES.iter().any(|(_, n)| *n == name),
                "{event:?}: {name}"
            );
        }
    }
}
