//! The robot's state as the Driver Station sets it (disabled, autonomous,
//! teleop, test, e-stopped; whether the DS and the FMS are attached), read
//! from the table WPILib publishes it in, and the changes a dashboard sounds
//! for ([`Watch`]).
//!
//! WPILib 2027 publishes a `ControlWord` struct in `/DriverStation` (`.type`
//! `DriverStation`): 64 bits, little-endian, `opModeHash:56`, `robotMode:2`
//! (unknown 0, autonomous 1, teleoperated 2, utility 3), then one bit each
//! for enabled, e-stop, FMS attached and DS attached. WPILib 2026 publishes
//! an integer `FMSControlData` in `/FMSInfo` (`.type` `FMSInfo`): bits for
//! enabled (0x01), autonomous (0x02), test (0x04), e-stop (0x08), FMS
//! attached (0x10) and DS attached (0x20). 2027's "utility" mode is what
//! 2026 calls test, and is shown as Test.

use std::collections::HashMap;

use nt4::Value;

/// WPILib 2027's table's `.type`.
pub const TYPE_2027: &str = "DriverStation";
/// WPILib 2026's table's `.type`.
pub const TYPE_2026: &str = "FMSInfo";
/// The 2027 topic in the table: a `ControlWord` struct.
pub const CONTROL_WORD: &str = "ControlWord";
/// The 2026 topic in the table: an integer of state bits.
pub const FMS_CONTROL_DATA: &str = "FMSControlData";

/// What the robot is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// No state to show: not connected to the server, nothing published
    /// yet, or no Driver Station attached to the robot.
    NoRobot,
    /// Disabled (not e-stopped).
    Disabled,
    /// Enabled in autonomous.
    Autonomous,
    /// Enabled in teleop (also an enabled robot whose mode isn't known).
    Teleop,
    /// Enabled in test (2027's "utility").
    Test,
    /// Emergency-stopped: stays so until the robot restarts.
    EStop,
}

impl Mode {
    /// How it's written on the widget.
    pub fn label(self) -> &'static str {
        match self {
            Mode::NoRobot => "No robot",
            Mode::Disabled => "Disabled",
            Mode::Autonomous => "Autonomous",
            Mode::Teleop => "Teleop",
            Mode::Test => "Test",
            Mode::EStop => "E-Stop",
        }
    }
}

/// The robot's state at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RobotState {
    /// The mode, worked out from the bits.
    pub mode: Mode,
    /// A Driver Station is attached.
    pub ds: bool,
    /// The FMS (a match's field system) is attached.
    pub fms: bool,
}

/// The state from its flags: e-stop wins, then no DS means no robot to
/// drive, then disabled, then the enabled mode.
fn state(enabled: bool, mode: Mode, estop: bool, fms: bool, ds: bool) -> RobotState {
    let mode = if estop {
        Mode::EStop
    } else if !ds {
        Mode::NoRobot
    } else if !enabled {
        Mode::Disabled
    } else {
        mode
    };
    RobotState { mode, ds, fms }
}

/// The state in WPILib 2027's `ControlWord` (its 64 bits).
///
/// ```
/// use raptaboard::types::robot_state::{from_control_word, Mode};
/// // Robot mode 1 (autonomous) in bits 56-57, enabled (58), DS (61).
/// let word = (1u64 << 56) | (1 << 58) | (1 << 61);
/// assert_eq!(from_control_word(word).mode, Mode::Autonomous);
/// ```
pub fn from_control_word(word: u64) -> RobotState {
    let bit = |n: u32| word & (1 << n) != 0;
    let mode = match (word >> 56) & 0b11 {
        1 => Mode::Autonomous,
        3 => Mode::Test,
        _ => Mode::Teleop,
    };
    state(bit(58), mode, bit(59), bit(60), bit(61))
}

/// The state in WPILib 2026's `FMSControlData` bits.
///
/// ```
/// use raptaboard::types::robot_state::{from_fms_control_data, Mode};
/// assert_eq!(from_fms_control_data(0x20).mode, Mode::Disabled); // DS only
/// assert_eq!(from_fms_control_data(0x21).mode, Mode::Teleop); // + enabled
/// ```
pub fn from_fms_control_data(bits: i64) -> RobotState {
    let bit = |mask: i64| bits & mask != 0;
    let mode = if bit(0x02) {
        Mode::Autonomous
    } else if bit(0x04) {
        Mode::Test
    } else {
        Mode::Teleop
    };
    state(bit(0x01), mode, bit(0x08), bit(0x10), bit(0x20))
}

/// The topics a widget bound to `table` reads: both years', so either
/// works.
pub fn topics(table: &str) -> Vec<String> {
    vec![
        format!("{table}/{CONTROL_WORD}"),
        format!("{table}/{FMS_CONTROL_DATA}"),
    ]
}

/// The state in `table`, from the server's `values` (2027's `ControlWord`,
/// else 2026's `FMSControlData`), or `None` if neither has arrived.
pub fn read(table: &str, values: &HashMap<&str, &Value>) -> Option<RobotState> {
    let word = values.get(format!("{table}/{CONTROL_WORD}").as_str());
    if let Some(Value::Raw(bytes)) = word
        && let Ok(eight) = <[u8; 8]>::try_from(&bytes[..bytes.len().min(8)])
    {
        return Some(from_control_word(u64::from_le_bytes(eight)));
    }
    match values.get(format!("{table}/{FMS_CONTROL_DATA}").as_str()) {
        Some(Value::Int(bits)) => Some(from_fms_control_data(*bits)),
        _ => None,
    }
}

/// Something a dashboard can play a sound for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// The robot was disabled.
    Disabled,
    /// Autonomous started.
    Autonomous,
    /// Teleop started.
    Teleop,
    /// Test started.
    Test,
    /// The robot was e-stopped.
    EStop,
    /// The server came back after being lost.
    RobotConnected,
    /// The server (the robot) was lost.
    RobotLost,
    /// The Driver Station left while the robot stayed connected.
    DsLost,
    /// The FMS left while the DS and robot stayed connected.
    FmsLost,
}

impl Event {
    /// Every event, in the order the settings list them.
    pub const ALL: [Event; 9] = [
        Event::Disabled,
        Event::Autonomous,
        Event::Teleop,
        Event::Test,
        Event::EStop,
        Event::RobotConnected,
        Event::RobotLost,
        Event::DsLost,
        Event::FmsLost,
    ];

    /// The event for entering `mode`, if it has one.
    fn entering(mode: Mode) -> Option<Event> {
        match mode {
            Mode::NoRobot => None,
            Mode::Disabled => Some(Event::Disabled),
            Mode::Autonomous => Some(Event::Autonomous),
            Mode::Teleop => Some(Event::Teleop),
            Mode::Test => Some(Event::Test),
            Mode::EStop => Some(Event::EStop),
        }
    }
}

/// Spots the [`Event`]s in a stream of (connected, state) readings, one per
/// frame. Only real changes count: never the state found at connect, after
/// a reconnect or when watching starts, and nothing while disconnected.
///
/// ```
/// use raptaboard::types::robot_state::{from_fms_control_data, Event, Watch};
/// let mut watch = Watch::default();
/// let disabled = Some(from_fms_control_data(0x20));
/// let teleop = Some(from_fms_control_data(0x21));
/// assert!(watch.update(true, disabled).is_empty()); // found at connect
/// assert_eq!(watch.update(true, teleop), [Event::Teleop]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Watch {
    /// Whether the last reading was connected; `None` before the first.
    connected: Option<bool>,
    /// The last state read while connected, since connecting.
    last: Option<RobotState>,
}

impl Watch {
    /// The events between the last reading and this one.
    pub fn update(&mut self, connected: bool, now: Option<RobotState>) -> Vec<Event> {
        let mut events = Vec::new();
        match (self.connected, connected) {
            (Some(true), false) => events.push(Event::RobotLost),
            (Some(false), true) => events.push(Event::RobotConnected),
            _ => {}
        }
        self.connected = Some(connected);
        if !connected {
            self.last = None;
            return events;
        }
        let Some(now) = now else {
            return events;
        };
        if let Some(before) = self.last.replace(now) {
            if before.ds && !now.ds {
                events.push(Event::DsLost);
            } else if before.fms && !now.fms {
                events.push(Event::FmsLost);
            }
            // Entering a mode counts only from another known mode: a DS
            // attaching (from no robot) shows the state it finds, quietly.
            if now.mode != before.mode
                && before.mode != Mode::NoRobot
                && let Some(event) = Event::entering(now.mode)
            {
                events.push(event);
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    const DS: i64 = 0x20;
    const FMS: i64 = 0x10;
    const ENABLED: i64 = 0x01;
    const AUTO: i64 = 0x02;
    const TEST: i64 = 0x04;
    const ESTOP: i64 = 0x08;

    fn s(bits: i64) -> Option<RobotState> {
        Some(from_fms_control_data(bits))
    }

    fn word(mode: u64, enabled: bool, estop: bool, fms: bool, ds: bool) -> u64 {
        // Some op mode hash in the low bits, which mustn't matter.
        0x00AB_CDEF_0123_4567
            | (mode << 56)
            | (u64::from(enabled) << 58)
            | (u64::from(estop) << 59)
            | (u64::from(fms) << 60)
            | (u64::from(ds) << 61)
    }

    #[test]
    fn both_years_read_the_same_states() {
        for (bits, w, mode) in [
            (DS, word(2, false, false, false, true), Mode::Disabled),
            (
                DS | ENABLED | AUTO,
                word(1, true, false, false, true),
                Mode::Autonomous,
            ),
            (
                DS | ENABLED,
                word(2, true, false, false, true),
                Mode::Teleop,
            ),
            (
                DS | ENABLED | TEST,
                word(3, true, false, false, true),
                Mode::Test,
            ),
            (DS | ESTOP, word(2, false, true, false, true), Mode::EStop),
            (0, word(0, false, false, false, false), Mode::NoRobot),
        ] {
            assert_eq!(from_fms_control_data(bits).mode, mode, "2026 {bits:#x}");
            assert_eq!(from_control_word(w).mode, mode, "2027 {w:#x}");
        }
        let both = from_control_word(word(2, true, false, true, true));
        assert!(both.ds && both.fms);
        assert_eq!(
            from_fms_control_data(DS | FMS),
            from_control_word(word(2, false, false, true, true))
        );
    }

    #[test]
    fn the_table_is_read_2027_first_then_2026() {
        let raw = Value::Raw(word(1, true, false, false, true).to_le_bytes().to_vec());
        let int = Value::Int(DS);
        let mut values = HashMap::new();
        assert_eq!(read("/T", &values), None);
        values.insert("/T/FMSControlData", &int);
        assert_eq!(read("/T", &values).unwrap().mode, Mode::Disabled);
        values.insert("/T/ControlWord", &raw);
        assert_eq!(read("/T", &values).unwrap().mode, Mode::Autonomous);
    }

    #[test]
    fn the_state_found_is_quiet_and_changes_sound() {
        let mut w = Watch::default();
        // Opened while connected and enabled: nothing.
        assert!(w.update(true, s(DS | ENABLED)).is_empty());
        assert!(w.update(true, s(DS | ENABLED)).is_empty());
        assert_eq!(w.update(true, s(DS)), [Event::Disabled]);
        assert_eq!(w.update(true, s(DS | ENABLED | AUTO)), [Event::Autonomous]);
        assert_eq!(w.update(true, s(DS | ENABLED | TEST)), [Event::Test]);
        assert_eq!(w.update(true, s(DS | ESTOP)), [Event::EStop]);
    }

    #[test]
    fn a_reconnect_is_heard_but_the_state_it_finds_is_not() {
        let mut w = Watch::default();
        assert!(w.update(false, None).is_empty(), "starting disconnected");
        assert_eq!(w.update(true, None), [Event::RobotConnected]);
        assert!(w.update(true, s(DS | ENABLED)).is_empty(), "first state");
        assert_eq!(w.update(false, s(DS | ENABLED)), [Event::RobotLost]);
        assert!(w.update(false, s(DS)).is_empty(), "nothing while down");
        assert_eq!(w.update(true, s(DS)), [Event::RobotConnected]);
        assert!(w.update(true, s(DS)).is_empty());
        assert_eq!(w.update(true, s(DS | ENABLED)), [Event::Teleop]);
    }

    #[test]
    fn the_ds_and_fms_leaving_are_their_own_events() {
        let mut w = Watch::default();
        w.update(true, s(DS | FMS | ENABLED));
        assert_eq!(w.update(true, s(DS | ENABLED)), [Event::FmsLost]);
        // The DS leaving: its own event, and no robot is no mode to enter.
        assert_eq!(w.update(true, s(ENABLED)), [Event::DsLost]);
        // Back: the state it finds is quiet.
        assert!(w.update(true, s(DS)).is_empty());
        // FMS and DS gone together: only the DS is reported.
        w.update(true, s(DS | FMS));
        assert_eq!(w.update(true, s(0)), [Event::DsLost]);
    }
}
