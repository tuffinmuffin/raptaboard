//! FMSInfo: the Driver Station's view of the robot and the match. The
//! robot's state large, with an icon and in its own color (Disabled,
//! Autonomous, Teleop, Test, E-Stop or No robot); the alliance color; the
//! match, station, replay, event and game data as text; and whether the
//! DS and FMS are attached. Each part can be turned off in the settings
//! (`show_*`; replay and game data are off unless turned on).
//!
//! Bound to the Driver Station's table: WPILib 2027's `/DriverStation` or
//! 2026's `/FMSInfo` (Elastic's `"FMSInfo"` widget), read by
//! [`crate::types::robot_state`]. Dragging that table from the tree makes
//! one.
//!
//! It also has a sound for each change of state ([`crate::state_sounds`]):
//! `sound_<event>` is `off`, a sound's name ([`crate::sound::SoundRef`]: a
//! built-in [`crate::sound::Tone`] or an installed pack's), or the path of an
//! MP3 or WAV file; not set, the event's [`default_sound`]. `volume`, from
//! 0 to 1, is how loud. Only the first FMSInfo in the layout plays them, from
//! any tab, and only on a real change (never for the state found at
//! connect). The bell button mutes them.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, CornerRadius, Rect, Stroke};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use serde_json::{Map, Value as Json};

use crate::images;
use crate::sound::{self, Clip};
use crate::state_sounds::{self, Plan};
use crate::types::robot_state::{self, Event, MatchInfo, Mode, RobotState};
use crate::widgets::shared::{card, props, sound_pick};
use crate::widgets::{Live, Server, ValueInfo, WidgetType};

/// How loud its sounds are, 0 to 1 (raptaboard).
pub const VOLUME: &str = "volume";
const DEFAULT_VOLUME: f64 = 0.8;
/// A sound setting's value for no sound.
pub const OFF: &str = "off";

/// A part of the widget that can be shown or hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The robot's state, in its color.
    State,
    /// The state's icon beside it.
    Icon,
    /// A red or blue square for the alliance.
    Alliance,
    /// "Qualification 12".
    Match,
    /// "Station 2".
    Station,
    /// "Replay 1".
    Replay,
    /// The event's name.
    Event,
    /// The game-specific message.
    GameData,
    /// The DS and FMS marks.
    Connections,
}

impl Part {
    /// Every part, in the order the settings list them.
    pub const ALL: [Part; 9] = [
        Part::State,
        Part::Icon,
        Part::Alliance,
        Part::Match,
        Part::Station,
        Part::Replay,
        Part::Event,
        Part::GameData,
        Part::Connections,
    ];

    /// The setting turning it on or off.
    pub fn key(self) -> &'static str {
        match self {
            Part::State => "show_state",
            Part::Icon => "show_icon",
            Part::Alliance => "show_alliance",
            Part::Match => "show_match",
            Part::Station => "show_station",
            Part::Replay => "show_replay",
            Part::Event => "show_event",
            Part::GameData => "show_game_data",
            Part::Connections => "show_connections",
        }
    }

    /// Shown unless set otherwise: everything but the replay and the game
    /// data.
    pub fn shown_by_default(self) -> bool {
        !matches!(self, Part::Replay | Part::GameData)
    }

    fn label(self) -> &'static str {
        match self {
            Part::State => "Robot state",
            Part::Icon => "State icon",
            Part::Alliance => "Alliance color",
            Part::Match => "Match",
            Part::Station => "Station",
            Part::Replay => "Replay",
            Part::Event => "Event name",
            Part::GameData => "Game data",
            Part::Connections => "DS and FMS",
        }
    }
}

/// Whether `part` is shown with the settings `p`.
///
/// ```
/// use raptaboard::widgets::builtin::fms_info::{shows, Part};
/// let mut p = serde_json::Map::new();
/// assert!(shows(&p, Part::Match) && !shows(&p, Part::Replay));
/// p.insert("show_replay".into(), true.into());
/// assert!(shows(&p, Part::Replay));
/// ```
pub fn shows(p: &Map<String, Json>, part: Part) -> bool {
    props::flag(p, part.key(), part.shown_by_default())
}

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

const KEYS: [&str; 19] = [
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
    "show_state",
    "show_icon",
    "show_alliance",
    "show_match",
    "show_station",
    "show_replay",
    "show_event",
    "show_game_data",
    "show_connections",
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
/// use raptaboard::widgets::builtin::fms_info::default_sound;
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
/// use raptaboard::widgets::builtin::fms_info::sound_for;
/// let mut p = serde_json::Map::new();
/// assert_eq!(sound_for(&p, Event::Test), Some(Clip::Tone(Tone::Danger)));
/// p.insert("sound_test".into(), "off".into());
/// assert_eq!(sound_for(&p, Event::Test), None);
/// p.insert("sound_test".into(), "/sounds/horn.mp3".into());
/// assert_eq!(sound_for(&p, Event::Test), Some(Clip::File("/sounds/horn.mp3".into())));
/// // A pack's sound, by name.
/// p.insert("sound_test".into(), "pack:fan-clips/fire 1".into());
/// assert!(matches!(sound_for(&p, Event::Test), Some(Clip::Sound(_))));
/// ```
pub fn sound_for(p: &Map<String, Json>, event: Event) -> Option<Clip> {
    let setting = props::text(p, sound_key(event), default_sound(event)).trim();
    if setting.is_empty() || setting == OFF {
        return None;
    }
    if is_file(setting) {
        return Some(Clip::File(PathBuf::from(setting)));
    }
    Clip::named(setting)
}

/// Whether a sound setting is a file the team picked rather than a
/// sound's name: a path, or a name ending in .mp3 or .wav. A pack's sound
/// (`pack:` or `builtin:`) never is.
///
/// ```
/// use raptaboard::widgets::builtin::fms_info::is_file;
/// assert!(is_file("/sounds/horn.mp3") && is_file(r"C:\sounds\horn.wav") && is_file("horn.MP3"));
/// assert!(!is_file("horn") && !is_file("fire 1") && !is_file("pack:fan-clips/fire 1"));
/// ```
pub fn is_file(setting: &str) -> bool {
    if setting.starts_with("pack:") || setting.starts_with("builtin:") {
        return false;
    }
    let lower = setting.to_ascii_lowercase();
    setting.contains(['/', '\\']) || lower.ends_with(".mp3") || lower.ends_with(".wav")
}

/// The widget's volume, 0 to 1.
fn volume(p: &Map<String, Json>) -> f32 {
    props::number(p, VOLUME, DEFAULT_VOLUME).clamp(0.0, 1.0) as f32
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

/// The built-in icon for each mode (an image of the same name in an images
/// folder takes its place).
fn icon_name(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::NoRobot => None,
        Mode::Disabled => Some("disabled.svg"),
        Mode::Autonomous => Some("auto.svg"),
        Mode::Teleop => Some("teleop.svg"),
        Mode::Test => Some("test.svg"),
        Mode::EStop => Some("estop.svg"),
    }
}

/// Everything the widget draws from.
struct Shown {
    state: Option<RobotState>,
    info: MatchInfo,
}

fn shown(widget: &WidgetContainer, live: &Live<'_>) -> Shown {
    let table = widget.topic().unwrap_or_default();
    let values = live.frame.others;
    Shown {
        state: live
            .frame
            .connected
            .then(|| robot_state::read(table, values))
            .flatten(),
        info: if live.frame.connected {
            robot_state::read_match(table, values)
        } else {
            MatchInfo::default()
        },
    }
}

/// The text lines under the state, from the parts shown.
fn info_lines(p: &Map<String, Json>, shown: &Shown) -> Vec<String> {
    let info = &shown.info;
    let mut lines = Vec::new();
    let mut first = Vec::new();
    if shows(p, Part::Match) {
        let text = info.match_text();
        first.push(if text.is_empty() {
            "No match".to_owned()
        } else {
            text
        });
    }
    if shows(p, Part::Station) && info.station > 0 {
        first.push(format!("Station {}", info.station));
    }
    if shows(p, Part::Replay) {
        first.push(format!("Replay {}", info.replay));
    }
    if !first.is_empty() || shows(p, Part::Alliance) {
        lines.push(first.join("  ·  "));
    }
    if shows(p, Part::Event) && !info.event.is_empty() {
        lines.push(info.event.clone());
    }
    if shows(p, Part::GameData) {
        let data = if info.game_data.is_empty() {
            "none"
        } else {
            &info.game_data
        };
        lines.push(format!("Game data: {data}"));
    }
    if shows(p, Part::Connections) {
        let (ds, fms) = shown.state.map_or((false, false), |s| (s.ds, s.fms));
        let mark = |on: bool, name: &str| {
            if on {
                name.to_owned()
            } else {
                format!("no {name}")
            }
        };
        lines.push(format!("{}   {}", mark(ds, "DS"), mark(fms, "FMS")));
    }
    lines
}

/// Draw everything shown in `rect`. `time` (seconds) blinks an e-stop.
fn draw(
    painter: &egui::Painter,
    visuals: &egui::Visuals,
    rect: Rect,
    p: &Map<String, Json>,
    shown: &Shown,
    time: f64,
) {
    let lines = info_lines(p, shown);
    let state_shown = shows(p, Part::State);
    // The lines take what they need (up to all of it without the state).
    let line_h = if state_shown {
        (rect.height() * 0.15).clamp(12.0, 24.0)
    } else {
        (rect.height() / lines.len().max(1) as f32).clamp(12.0, 40.0)
    };
    let lines_h = (line_h * lines.len() as f32).min(rect.height());
    let state_rect =
        Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.bottom() - lines_h));
    if state_shown && state_rect.height() > 8.0 {
        draw_state(painter, state_rect, p, shown.state, time);
    }
    let ink = visuals.text_color();
    let font = egui::FontId::proportional(line_h * 0.75);
    let mut y = rect.bottom() - lines_h;
    for (i, line) in lines.iter().enumerate() {
        let row = Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), line_h));
        let mut text_left = row.left() + 4.0;
        // The alliance square leads the first line.
        if i == 0 && shows(p, Part::Alliance) {
            let side = line_h * 0.8;
            let square = Rect::from_center_size(
                egui::pos2(row.left() + 4.0 + side / 2.0, row.center().y),
                egui::vec2(side, side),
            );
            match shown.info.red {
                Some(red) => {
                    let color = if red {
                        Color32::from_rgb(0xE5, 0x39, 0x35)
                    } else {
                        Color32::from_rgb(0x1E, 0x88, 0xE5)
                    };
                    painter.rect_filled(square, CornerRadius::same(2), color);
                }
                None => {
                    painter.rect_stroke(
                        square,
                        CornerRadius::same(2),
                        Stroke::new(1.0, ink.gamma_multiply(0.5)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            text_left = square.right() + 6.0;
        }
        painter
            .with_clip_rect(row.intersect(painter.clip_rect()))
            .text(
                egui::pos2(text_left, row.center().y),
                egui::Align2::LEFT_CENTER,
                line,
                font.clone(),
                ink,
            );
        y += line_h;
    }
}

/// Draw the state, its icon and label, filling `rect` in its color.
fn draw_state(
    painter: &egui::Painter,
    rect: Rect,
    p: &Map<String, Json>,
    state: Option<RobotState>,
    time: f64,
) {
    let mode = state.map_or(Mode::NoRobot, |s| s.mode);
    let (fill, ink) = colors(mode);
    // E-stop blinks: full red half the time, darker the other half.
    let fill = if mode == Mode::EStop && (time * 2.0).fract() >= 0.5 {
        fill.gamma_multiply(0.45)
    } else {
        fill
    };
    painter.rect_filled(rect, CornerRadius::same(6), fill);
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
    let inner = rect.shrink(4.0);
    let mut label = inner;
    if shows(p, Part::Icon)
        && let Some(name) = icon_name(mode)
    {
        let side = inner.height().min(inner.width() * 0.4);
        let icon = Rect::from_min_size(
            egui::pos2(inner.left(), inner.center().y - side / 2.0),
            egui::vec2(side, side),
        );
        // On a light tile, so it reads on every state's color.
        painter.rect_filled(icon, CornerRadius::same(6), Color32::from_white_alpha(225));
        draw_icon(painter, icon.shrink(side * 0.06), name);
        label = Rect::from_min_max(egui::pos2(icon.right() + 4.0, inner.top()), inner.max);
    }
    card::draw_fitted(painter, label, mode.label(), ink);
}

/// Draw the image `name` (built in, or a team's own of that name) in
/// `rect`, as big as fits.
fn draw_icon(painter: &egui::Painter, rect: Rect, name: &str) {
    let Some(source) = images::find(name, &images::default_folders()) else {
        return;
    };
    let Some(texture) = images::texture(painter.ctx(), &source) else {
        return;
    };
    let [w, h] = texture.size().map(|n| n as f32);
    let scale = (rect.width() / w).min(rect.height() / h);
    let fitted = Rect::from_center_size(rect.center(), egui::vec2(w, h) * scale);
    painter.image(
        texture.id(),
        fitted,
        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
}

/// The FMSInfo widget.
pub struct FmsInfo;

impl FmsInfo {
    fn show_in(&self, ui: &mut egui::Ui, rect: Rect, widget: &WidgetContainer, live: &Live<'_>) {
        let shown = shown(widget, live);
        let time = ui.input(|i| i.time);
        draw(
            ui.painter(),
            ui.visuals(),
            rect,
            &widget.properties,
            &shown,
            time,
        );
        if shown.state.is_some_and(|s| s.mode == Mode::EStop) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

impl WidgetType for FmsInfo {
    fn names(&self) -> &'static [&'static str] {
        &["FMSInfo"]
    }

    fn accepts(&self, _: &str) -> bool {
        false
    }

    fn table_types(&self) -> &'static [&'static str] {
        &[robot_state::TYPE_2027, robot_state::TYPE_2026]
    }

    fn extra_topics(&self, p: &Map<String, Json>, _: Server<'_>) -> Vec<String> {
        p.get("topic")
            .and_then(Json::as_str)
            .map_or_else(Vec::new, robot_state::all_topics)
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
        visuals: &egui::Visuals,
        rect: Rect,
        p: &Map<String, Json>,
        _: Option<&Value>,
    ) {
        // Without the table's values (while dragged).
        let nothing = Shown {
            state: None,
            info: MatchInfo::default(),
        };
        draw(painter, visuals, rect, p, &nothing, 0.0);
    }

    fn state_sounds(&self, widget: &WidgetContainer) -> Option<Plan> {
        let p = &widget.properties;
        Some(Plan {
            table: widget.topic()?.to_owned(),
            sounds: Event::ALL
                .iter()
                .filter_map(|&e| Some((e, sound_for(p, e)?)))
                .collect(),
            volume: volume(p),
        })
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _: ValueInfo<'_>) {
        for part in Part::ALL {
            props::flag_row(
                ui,
                p,
                part.label(),
                "Show this part of the widget",
                part.key(),
                part.shown_by_default(),
            );
        }
        ui.label("Volume");
        let mut v = f64::from(volume(p));
        if ui
            .add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false))
            .on_hover_text(
                "How loud this widget's sounds are. Only the first FMSInfo in the layout plays them.",
            )
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

/// A settings row picking `event`'s sound: Off, a built-in or installed
/// pack's sound ([`sound_pick`]), or a file (File… opens a file dialog),
/// with ▶ to hear it.
fn sound_row(ui: &mut egui::Ui, p: &mut Map<String, Json>, event: Event) {
    let key = sound_key(event);
    let default = default_sound(event);
    ui.label(format!("Sound: {}", event_label(event)));
    let current = props::text(p, key, default).to_owned();
    let mut picked = current.clone();
    let layers = sound_pick::installed(ui);
    let mut found = true;
    let shown = match sound_for(p, event) {
        None => "Off".to_owned(),
        Some(Clip::File(path)) => file_name(&path),
        Some(_) => {
            let (label, there) = sound::sound_label(current.trim(), &layers);
            found = there;
            label
        }
    };
    ui.horizontal(|ui| {
        let name = format!("Sound: {}", event_label(event));
        sound_pick::sound_combo(ui, key, &name, &shown, |ui| {
            ui.selectable_value(&mut picked, OFF.to_owned(), "Off");
            sound_pick::sound_menu(ui, &mut picked, &layers);
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
            state_sounds::request_preview(ui.ctx(), clip, volume(p));
        }
        if !found {
            sound_pick::missing_note(ui, current.trim());
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
    use crate::sound::Tone;

    fn widget(props: serde_json::Value) -> WidgetContainer {
        let mut w: WidgetContainer = serde_json::from_value(serde_json::json!({
            "title": "FMS", "x": 0, "y": 0, "width": 256, "height": 128,
            "type": "FMSInfo", "properties": {"topic": "/FMSInfo"}
        }))
        .unwrap();
        if let serde_json::Value::Object(extra) = props {
            w.properties.extend(extra);
        }
        w
    }

    #[test]
    fn its_plan_has_the_set_sounds_and_skips_the_ones_off() {
        let w = widget(serde_json::json!({
            "sound_teleop": "off",
            "sound_disabled": "/sounds/horn.mp3",
            "sound_estop": "beep",
            "volume": 0.5,
        }));
        let plan = FmsInfo.state_sounds(&w).unwrap();
        assert_eq!(plan.table, "/FMSInfo");
        assert_eq!(plan.volume, 0.5);
        let sound = |e: Event| plan.sound_for(&[e]).cloned();
        assert_eq!(sound(Event::Teleop), None);
        assert_eq!(
            sound(Event::Disabled),
            Some(Clip::File("/sounds/horn.mp3".into()))
        );
        assert_eq!(sound(Event::EStop), Some(Clip::Tone(Tone::Beep)));
        assert_eq!(sound(Event::Autonomous), Some(Clip::Tone(Tone::AutoGhosts)));
        assert_eq!(sound(Event::DsLost), None, "off by default");
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

    #[test]
    fn every_state_icon_is_built_in() {
        for mode in [
            Mode::Disabled,
            Mode::Autonomous,
            Mode::Teleop,
            Mode::Test,
            Mode::EStop,
        ] {
            let name = icon_name(mode).unwrap();
            assert!(images::BUILT_IN.iter().any(|(n, _)| *n == name), "{name}");
        }
    }

    #[test]
    fn the_lines_follow_the_parts_shown() {
        let shown = Shown {
            state: Some(robot_state::from_fms_control_data(0x30)),
            info: MatchInfo {
                event: "Dallas".into(),
                match_type: 2,
                number: 12,
                replay: 1,
                red: Some(true),
                station: 3,
                game_data: "L".into(),
            },
        };
        let lines = |extra: serde_json::Value| info_lines(&widget(extra).properties, &shown);
        assert_eq!(
            lines(serde_json::json!({})),
            ["Qualification 12  ·  Station 3", "Dallas", "DS   FMS"]
        );
        assert_eq!(
            lines(serde_json::json!({
                "show_replay": true, "show_game_data": true,
                "show_event": false, "show_connections": false, "show_station": false,
            })),
            ["Qualification 12  ·  Replay 1", "Game data: L"]
        );
    }
}
