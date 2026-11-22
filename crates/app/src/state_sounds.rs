//! The robot state sounds (an FMSInfo widget's), played from the
//! NetworkTables thread as values arrive.
//!
//! The window doesn't draw while it's in the background or covered (macOS
//! stops sending it frames), so sounds checked from drawing came late or
//! not at all. Instead the app hands the first FMSInfo widget's [`Plan`]
//! to [`NtConnection::set_state_sounds`](crate::nt::NtConnection::set_state_sounds)
//! each frame it draws, and a task on the NT client's runtime ([`spawn`])
//! subscribes to the robot's state, finds the changes with
//! [`robot_state::Watch`] and plays them, whether the window draws or not.
//! It keeps the last plan it was given.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nt4::{Client, Event, SubscriptionOptions, Value};

use crate::sound::{Clip, SoundPlayer};
use crate::types::robot_state::{self, Event as StateEvent, Watch};

/// What to play, and from which table: the first FMSInfo widget's
/// settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The Driver Station's table: `/DriverStation` or `/FMSInfo`.
    pub table: String,
    /// The sound for each event that has one.
    pub sounds: Vec<(StateEvent, Clip)>,
    /// How loud, 0 to 1.
    pub volume: f32,
}

impl Plan {
    /// The sound to play for `events` (all found at once): the last one's
    /// that has a sound, since a new sound cuts the last off anyway.
    ///
    /// ```
    /// use raptaboard::sound::{Clip, Tone};
    /// use raptaboard::state_sounds::Plan;
    /// use raptaboard::types::robot_state::Event;
    /// let plan = Plan {
    ///     table: "/FMSInfo".into(),
    ///     sounds: vec![(Event::Teleop, Clip::Tone(Tone::GoGoGo))],
    ///     volume: 1.0,
    /// };
    /// assert_eq!(plan.sound_for(&[Event::FmsLost, Event::Teleop]), Some(&Clip::Tone(Tone::GoGoGo)));
    /// assert_eq!(plan.sound_for(&[Event::FmsLost]), None);
    /// ```
    pub fn sound_for(&self, events: &[StateEvent]) -> Option<&Clip> {
        events.iter().rev().find_map(|e| {
            self.sounds
                .iter()
                .find(|(event, _)| event == e)
                .map(|(_, clip)| clip)
        })
    }
}

/// The plan and the player it plays on, shared between the window (which
/// sets it) and the task (which reads it). `None`: nothing to play (no
/// FMSInfo widget, or sounds are off).
pub type Shared = Arc<Mutex<Option<(Plan, SoundPlayer)>>>;

/// Ends the task when dropped (when the client it watches is replaced).
pub struct Task(tokio::task::JoinHandle<()>);

impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// How often the task looks for a new plan when no values arrive.
const RECHECK: Duration = Duration::from_millis(250);

/// Start the task watching `client` for the robot's state, playing what
/// `shared` says. Call inside the client's tokio runtime.
pub fn spawn(client: Client, shared: Shared) -> Task {
    Task(tokio::spawn(run(client, shared)))
}

/// The task: follow the connection and the table's values, and play the
/// changes.
async fn run(client: Client, shared: Shared) {
    let mut events = client.events();
    let mut connected = client.is_connected();
    let mut recheck = tokio::time::interval(RECHECK);
    let mut table: Option<String> = None;
    let mut _subscription = None;
    let mut topics: Vec<String> = Vec::new();
    let mut values: HashMap<String, Value> = HashMap::new();
    let mut watch = Watch::default();
    loop {
        tokio::select! {
            event = events.recv() => match event {
                None => return,
                Some(Event::Connected { .. }) => connected = true,
                Some(Event::Disconnected { .. }) => {
                    connected = false;
                    values.clear();
                }
                Some(Event::Value { name, value, .. }) => {
                    if !topics.iter().any(|t| **t == *name) {
                        continue;
                    }
                    values.insert(name.to_string(), value.value);
                }
                Some(_) => continue,
            },
            _ = recheck.tick() => {}
        }
        let Some((plan, player)) = shared.lock().ok().and_then(|s| s.clone()) else {
            // Nothing to play: start over when there is.
            table = None;
            _subscription = None;
            topics.clear();
            continue;
        };
        if table.as_deref() != Some(plan.table.as_str()) {
            topics = robot_state::topics(&plan.table);
            _subscription = Some(client.subscribe_multiple(
                topics.clone(),
                SubscriptionOptions {
                    all: true,
                    ..Default::default()
                },
            ));
            table = Some(plan.table.clone());
            values.clear();
            watch = Watch::default();
        }
        let now: HashMap<&str, &Value> = values.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let found = watch.update(connected, robot_state::read(&plan.table, &now));
        if let Some(clip) = plan.sound_for(&found) {
            player.play_event(clip.clone(), plan.volume);
        }
    }
}

/// A sound asked for by a settings dialog's ▶ button, which can't play
/// sounds itself: the app plays it on its next frame ([`take_preview`]).
#[derive(Clone)]
struct Preview(Clip, f32);

fn preview_id() -> eframe::egui::Id {
    eframe::egui::Id::unique("state_sound_preview")
}

/// Ask for `clip` to be played at `volume`, from a settings dialog.
pub fn request_preview(ctx: &eframe::egui::Context, clip: Clip, volume: f32) {
    ctx.data_mut(|d| d.insert_temp(preview_id(), Preview(clip, volume)));
}

/// The sound a settings dialog asked for, once.
pub fn take_preview(ctx: &eframe::egui::Context) -> Option<(Clip, f32)> {
    ctx.data_mut(|d| {
        let asked = d.get_temp::<Preview>(preview_id());
        d.remove::<Preview>(preview_id());
        asked.map(|Preview(clip, volume)| (clip, volume))
    })
}
