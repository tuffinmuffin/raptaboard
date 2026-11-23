//! The robot's state for the whole window: a colored border around it by
//! state (off unless turned on in ⚙ Settings), flashing red for a few
//! seconds after the robot is disabled, and a pop-up over everything when
//! the robot is e-stopped (on unless turned off), so the state shows at a
//! glance from any tab.
//!
//! These are look and feel, so they're kept in the layout file (its
//! raptaboard-only `"robot_state"` object, [`Look`]), not in the settings
//! file, which is for the computer's connection to a robot: whoever opens
//! the layout gets them.
//!
//! The state comes from the Driver Station's table, read by
//! [`robot_state`]: the first FMSInfo widget's table if the layout has one,
//! else `/DriverStation` (2027) or `/FMSInfo` (2026), whichever the server
//! has. Its topics are asked for whatever tab is open.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};
use nt4::{Topic, Value};
use raptaboard_layout::ElasticLayout;
use serde::{Deserialize, Serialize};

use crate::types::robot_state::{self, Event, Mode, Watch};
use crate::widgets;

use super::DashboardApp;

/// How long the border flashes after the robot is disabled.
pub const FLASH: Duration = Duration::from_secs(3);
/// How long each flash is on (and then off).
const FLASH_STEP: Duration = Duration::from_millis(250);
/// The border's width, in points.
pub const BORDER_WIDTH: f32 = 6.0;
/// How often the robot state's topics are sent (seconds).
pub(super) const STATE_PERIOD_S: f64 = 0.1;

/// The layout file's key for [`Look`] (raptaboard's; Elastic ignores it).
pub const LAYOUT_KEY: &str = "robot_state";

/// How the window shows the robot's state, as the layout file keeps it:
///
/// ```json
/// "robot_state": {"border": true, "border_colors": {"teleop": "none"}, "estop_popup": false}
/// ```
///
/// Each part is left out at its default (no border, the default colors,
/// the pop-up on).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    /// Color the window's border by the robot's state.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub border: bool,
    /// The border's colors, where they aren't the defaults.
    #[serde(skip_serializing_if = "StateColors::is_default")]
    pub border_colors: StateColors,
    /// Whether a pop-up covers the window when the robot is e-stopped; on
    /// if not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estop_popup: Option<bool>,
}

impl Look {
    /// What `layout` says (the defaults where it says nothing).
    ///
    /// ```
    /// use raptaboard::ui::robot_state_view::Look;
    /// let mut layout = raptaboard_layout::ElasticLayout::default();
    /// assert_eq!(Look::of(&layout), Look::default());
    /// let look = Look { border: true, ..Look::default() };
    /// look.write_to(&mut layout);
    /// assert_eq!(Look::of(&layout), look);
    /// // Back to the defaults: nothing left in the file.
    /// Look::default().write_to(&mut layout);
    /// assert!(layout.extra.is_empty());
    /// ```
    pub fn of(layout: &ElasticLayout) -> Look {
        layout
            .extra
            .get(LAYOUT_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Put this into `layout`, or take it out if it's all defaults.
    pub fn write_to(&self, layout: &mut ElasticLayout) {
        match serde_json::to_value(self) {
            Ok(v) if *self != Look::default() => {
                layout.extra.insert(LAYOUT_KEY.into(), v);
            }
            _ => {
                layout.extra.remove(LAYOUT_KEY);
            }
        }
    }

    /// Whether the e-stop pop-up is on.
    pub fn estop_popup(&self) -> bool {
        self.estop_popup.unwrap_or(true)
    }
}

/// The window border's color in each robot state, as `"#RRGGBB"`, or
/// `"none"` for no border; not set is the default ([`default_color`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StateColors {
    /// Not connected, or no Driver Station.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_robot: Option<String>,
    /// Disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<String>,
    /// Enabled in autonomous.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autonomous: Option<String>,
    /// Enabled in teleop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleop: Option<String>,
    /// Enabled in test.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    /// Emergency-stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estop: Option<String>,
}

impl StateColors {
    /// Every state at its default color.
    pub fn is_default(&self) -> bool {
        *self == StateColors::default()
    }
}

/// The window's view of the robot's state.
#[derive(Debug, Default)]
pub struct RobotView {
    /// The layout's border and pop-up settings, read each frame.
    pub look: Look,
    /// The state now, if known.
    pub mode: Option<Mode>,
    /// Finds the changes.
    watch: Watch,
    /// When the robot was last disabled, while the border flashes.
    flash_from: Option<Instant>,
    /// The e-stop pop-up is up.
    pub estopped: bool,
}

/// The default border color for `mode`: the FMSInfo widget's, and none for
/// no robot.
pub fn default_color(mode: Mode) -> Option<Color32> {
    match mode {
        Mode::NoRobot => None,
        Mode::Disabled => Some(Color32::from_rgb(0xC6, 0x28, 0x28)),
        Mode::Autonomous => Some(Color32::from_rgb(0x15, 0x65, 0xC0)),
        Mode::Teleop => Some(Color32::from_rgb(0x2E, 0x7D, 0x32)),
        Mode::Test => Some(Color32::from_rgb(0xEF, 0x6C, 0x00)),
        Mode::EStop => Some(Color32::from_rgb(0xD5, 0x00, 0x00)),
    }
}

/// The setting for `mode` in `colors`.
fn slot(colors: &mut StateColors, mode: Mode) -> &mut Option<String> {
    match mode {
        Mode::NoRobot => &mut colors.no_robot,
        Mode::Disabled => &mut colors.disabled,
        Mode::Autonomous => &mut colors.autonomous,
        Mode::Teleop => &mut colors.teleop,
        Mode::Test => &mut colors.test,
        Mode::EStop => &mut colors.estop,
    }
}

/// `"#RRGGBB"` as a color; `None` for `"none"` or anything else.
///
/// ```
/// use eframe::egui::Color32;
/// use raptaboard::ui::robot_state_view::parse_color;
/// assert_eq!(parse_color("#C62828"), Some(Color32::from_rgb(0xC6, 0x28, 0x28)));
/// assert_eq!(parse_color("none"), None);
/// ```
pub fn parse_color(text: &str) -> Option<Color32> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// The border's color for `mode` with `colors` set: the setting, or the
/// default if it isn't set.
pub fn color_for(colors: &StateColors, mode: Mode) -> Option<Color32> {
    let mut colors = colors.clone();
    match slot(&mut colors, mode).as_deref() {
        Some(text) => parse_color(text),
        None => default_color(mode),
    }
}

impl RobotView {
    /// With the border (and its `colors`) and the e-stop pop-up on or off,
    /// and no state seen yet.
    pub fn new(border: bool, colors: StateColors, estop_popup: bool) -> RobotView {
        RobotView {
            look: Look {
                border,
                border_colors: colors,
                estop_popup: Some(estop_popup),
            },
            ..Default::default()
        }
    }

    /// Take in the state now (`None` if unknown) and whether the server is
    /// connected, at `now`: a disable starts the flash, an e-stop puts the
    /// pop-up up (if it's on), and the robot coming back takes it down.
    pub fn update(
        &mut self,
        connected: bool,
        state: Option<robot_state::RobotState>,
        now: Instant,
    ) {
        self.mode = state.map(|s| s.mode).filter(|_| connected);
        for event in self.watch.update(connected, state) {
            match event {
                Event::Disabled => self.flash_from = Some(now),
                Event::EStop => self.estopped = self.look.estop_popup(),
                // An e-stopped robot stays so until it restarts: it's back.
                Event::RobotConnected => self.estopped = false,
                _ => {}
            }
        }
        if self.mode.is_some_and(|m| m != Mode::EStop) {
            self.estopped = false;
        }
        if self
            .flash_from
            .is_some_and(|t| now.duration_since(t) >= FLASH)
        {
            self.flash_from = None;
        }
    }

    /// The border's color at `now`, if there's a border: the state's color,
    /// flashing on and off for a few seconds after a disable.
    pub fn border_color(&self, now: Instant) -> Option<Color32> {
        if !self.look.border {
            return None;
        }
        let colors = &self.look.border_colors;
        let color = color_for(colors, self.mode.unwrap_or(Mode::NoRobot));
        match self.flash_from {
            Some(from) => {
                let step = now.duration_since(from).as_millis() / FLASH_STEP.as_millis();
                // On first, then off, and so on.
                step.is_multiple_of(2)
                    .then(|| color_for(colors, Mode::Disabled))
                    .flatten()
            }
            None => color,
        }
    }

    /// Whether the border is flashing, so the window should redraw often.
    pub fn flashing(&self) -> bool {
        self.look.border && self.flash_from.is_some()
    }
}

/// Every state, in the order the settings list them.
const MODES: [Mode; 6] = [
    Mode::Disabled,
    Mode::Autonomous,
    Mode::Teleop,
    Mode::Test,
    Mode::EStop,
    Mode::NoRobot,
];

/// The ⚙ Settings window's rows for the robot state: the pop-up, the
/// border, and (with the border on) each state's color, or none.
pub(super) fn settings_rows(ui: &mut egui::Ui, look: &mut Look) {
    ui.label(
        egui::RichText::new("Kept in the layout file: save the layout to keep them.")
            .small()
            .weak(),
    );
    let mut popup = look.estop_popup();
    if ui
        .checkbox(&mut popup, "A pop-up when the robot is e-stopped")
        .on_hover_text("Over the whole window, until clicked or the robot restarts")
        .changed()
    {
        look.estop_popup = (!popup).then_some(false);
    }
    ui.checkbox(
        &mut look.border,
        "Color the window's border by the robot's state",
    )
    .on_hover_text("Seen from any tab; it flashes for 3 s after the robot is disabled");
    if !look.border {
        return;
    }
    let view = look;
    egui::Grid::new("state_border_colors")
        .num_columns(3)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for mode in MODES {
                ui.label(mode.label());
                let shown = color_for(&view.border_colors, mode);
                let setting = slot(&mut view.border_colors, mode);
                let mut on = shown.is_some();
                let mut color = shown.or(default_color(mode)).unwrap_or(Color32::GRAY);
                ui.horizontal(|ui| {
                    let toggled = ui
                        .checkbox(&mut on, "")
                        .on_hover_text("Off: no border in this state")
                        .changed();
                    let picked = on
                        && egui::color_picker::color_edit_button_srgba(
                            ui,
                            &mut color,
                            egui::color_picker::Alpha::Opaque,
                        )
                        .changed();
                    if toggled || picked {
                        let wanted = on.then_some(color);
                        *setting = if wanted == default_color(mode) {
                            None
                        } else {
                            Some(match wanted {
                                Some(c) => format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b()),
                                None => "none".into(),
                            })
                        };
                    }
                });
                if ui
                    .add_enabled(setting.is_some(), egui::Button::new("Default").small())
                    .clicked()
                {
                    *setting = None;
                }
                ui.end_row();
            }
        });
}

impl DashboardApp {
    /// The Driver Station table the robot's state comes from: the first
    /// FMSInfo widget's in the layout, else `/DriverStation` or `/FMSInfo`
    /// if the server has it.
    fn robot_table(&self, topics: &[Topic]) -> Option<String> {
        let from_widget = self
            .layout
            .tabs
            .iter()
            .flat_map(super::live::in_order)
            .find_map(|w| {
                let ty = widgets::find(&w.widget_type)?;
                ty.table_types()
                    .iter()
                    .any(|t| matches!(*t, robot_state::TYPE_2027 | robot_state::TYPE_2026))
                    .then(|| w.topic().map(str::to_owned))
                    .flatten()
            });
        from_widget.or_else(|| {
            [robot_state::TYPE_2027, robot_state::TYPE_2026]
                .into_iter()
                .map(|name| format!("/{name}"))
                .find(|table| {
                    let type_topic = format!("{table}/.type");
                    topics.iter().any(|t| *t.name == *type_topic)
                })
        })
    }

    /// The robot state's topics, to ask for whatever tab is open, while the
    /// border or the e-stop pop-up is on.
    pub(super) fn robot_topics(&self, topics: &[Topic]) -> Vec<String> {
        if !(self.robot.look.border || self.robot.look.estop_popup()) {
            return Vec::new();
        }
        self.robot_table(topics)
            .map(|table| robot_state::topics(&table))
            .unwrap_or_default()
    }

    /// Read the robot's state from `values` this frame.
    pub(super) fn watch_robot(
        &mut self,
        topics: &[Topic],
        values: &HashMap<&str, &Value>,
        now: Instant,
    ) {
        let connected = matches!(
            self.nt.status(),
            crate::nt::ConnectionStatus::Connected { .. }
        );
        let state = self
            .robot_table(topics)
            .and_then(|table| robot_state::read(&table, values));
        self.robot.look = Look::of(&self.layout);
        self.robot.update(connected, state, now);
    }

    /// Draw the border around the whole window, over everything.
    pub(super) fn draw_robot_border(&self, ctx: &egui::Context, now: Instant) {
        if self.robot.flashing() {
            ctx.request_repaint_after(FLASH_STEP / 2);
        }
        let Some(color) = self.robot.border_color(now) else {
            return;
        };
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("robot_state_border"),
        ));
        painter.rect_stroke(
            ctx.content_rect(),
            egui::CornerRadius::ZERO,
            egui::Stroke::new(BORDER_WIDTH, color),
            egui::StrokeKind::Inside,
        );
    }

    /// The e-stop pop-up, while it's up: large, over everything, saying so
    /// plainly. A click anywhere takes it down.
    pub(super) fn estop_dialog(&mut self, ctx: &egui::Context) {
        if !self.robot.estopped {
            return;
        }
        let modal = egui::Modal::new(egui::Id::unique("estop_popup"))
            .backdrop_color(Color32::from_rgba_unmultiplied(0x80, 0, 0, 0xB0))
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("ROBOT E-STOPPED")
                            .size(40.0)
                            .strong()
                            .color(Color32::from_rgb(0xFF, 0x52, 0x52)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "The robot is emergency-stopped. It stays stopped until it's \
                             restarted (power it off and on).",
                        )
                        .size(16.0),
                    );
                    ui.add_space(12.0);
                    ui.button(egui::RichText::new("OK").size(18.0)).clicked()
                })
                .inner
            });
        let clicked_anywhere = ctx.input(|i| i.pointer.any_click());
        if modal.inner || modal.should_close() || clicked_anywhere {
            self.robot.estopped = false;
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::types::robot_state::from_fms_control_data;

    const DISABLED: i64 = 0x20;
    const TELEOP: i64 = 0x21;
    const ESTOP: i64 = 0x28;

    fn ms(start: Instant, n: u64) -> Instant {
        start + Duration::from_millis(n)
    }

    #[test]
    fn the_border_follows_the_state_and_flashes_after_a_disable() {
        let t = Instant::now();
        let mut view = RobotView::new(true, StateColors::default(), true);
        let red = default_color(Mode::Disabled);
        let green = default_color(Mode::Teleop);
        // No robot yet: no border.
        view.update(false, None, t);
        assert_eq!(view.border_color(t), None);
        view.update(true, Some(from_fms_control_data(TELEOP)), t);
        assert_eq!(view.border_color(t), green);
        // Disabled: red, flashing on and off for 3 s, then red.
        let d = ms(t, 1000);
        view.update(true, Some(from_fms_control_data(DISABLED)), d);
        assert_eq!(view.border_color(d), red);
        assert_eq!(view.border_color(ms(d, 300)), None);
        assert_eq!(view.border_color(ms(d, 600)), red);
        assert!(view.flashing());
        view.update(true, Some(from_fms_control_data(DISABLED)), ms(d, 3100));
        assert!(!view.flashing());
        assert_eq!(view.border_color(ms(d, 3300)), red);
        // Off in the settings: no border at all.
        view.look.border = false;
        assert_eq!(view.border_color(ms(d, 3300)), None);
    }

    #[test]
    fn border_colors_can_be_changed_or_turned_off_per_state() {
        let colors = StateColors {
            teleop: Some("none".into()),
            disabled: Some("#010203".into()),
            ..Default::default()
        };
        assert_eq!(color_for(&colors, Mode::Teleop), None);
        assert_eq!(
            color_for(&colors, Mode::Disabled),
            Some(Color32::from_rgb(1, 2, 3))
        );
        assert_eq!(
            color_for(&colors, Mode::Autonomous),
            default_color(Mode::Autonomous)
        );
        assert_eq!(color_for(&colors, Mode::NoRobot), None);
    }

    #[test]
    fn the_estop_popup_comes_up_on_an_estop_and_down_when_the_robot_is_back() {
        let t = Instant::now();
        let mut view = RobotView::new(false, StateColors::default(), true);
        view.update(true, Some(from_fms_control_data(TELEOP)), t);
        assert!(!view.estopped);
        view.update(true, Some(from_fms_control_data(ESTOP)), t);
        assert!(view.estopped, "up on the e-stop");
        // Still e-stopped: stays up.
        view.update(true, Some(from_fms_control_data(ESTOP)), t);
        assert!(view.estopped);
        // The robot restarts: the server goes, and comes back disabled.
        view.update(false, None, t);
        view.update(true, Some(from_fms_control_data(DISABLED)), t);
        assert!(!view.estopped, "down once the robot is back");
        // Turned off: no pop-up.
        let mut off = RobotView::new(false, StateColors::default(), false);
        off.update(true, Some(from_fms_control_data(TELEOP)), t);
        off.update(true, Some(from_fms_control_data(ESTOP)), t);
        assert!(!off.estopped);
        // Already e-stopped when the dashboard connects: no event, no pop-up.
        let mut late = RobotView::new(false, StateColors::default(), true);
        late.update(true, Some(from_fms_control_data(ESTOP)), t);
        assert!(!late.estopped);
    }
}
