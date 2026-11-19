//! The alert tone played when a widget's value can't be sent to the robot.
//!
//! The tone plays on a thread of its own, which opens the computer's audio
//! output the first time it's needed. A computer with no audio output (or
//! one that fails to open) just stays quiet: the red border and the
//! message in the status line still say what went wrong.

use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use rodio::Source;
use rodio::source::SineWave;

/// The two notes of the alert, high then low, in hertz and seconds.
const NOTES: [(f32, f32); 2] = [(880.0, 0.09), (587.0, 0.16)];
/// How loud the alert is, from 0 to 1: noticeable, not startling.
const VOLUME: f32 = 0.25;

/// Plays the alert tone, or counts it when silent (in tests, or when the
/// user turned the sound off).
///
/// ```
/// use raptaboard::alert::Alert;
/// let mut alert = Alert::silent();
/// alert.play();
/// assert_eq!(alert.played(), 1);
/// ```
#[derive(Debug)]
pub struct Alert {
    /// Sends "play" to the sound thread; `None` when silent.
    sound: Option<Sender<()>>,
    /// How many times the alert was asked for.
    played: u32,
}

impl Default for Alert {
    /// The same as [`Alert::new`].
    fn default() -> Self {
        Alert::new()
    }
}

impl Alert {
    /// An alert that plays through the computer's speakers.
    pub fn new() -> Alert {
        let (send, receive) = mpsc::channel::<()>();
        let started = std::thread::Builder::new()
            .name("raptaboard-alert".into())
            .spawn(move || {
                // Opened on the first alert, and kept open after that.
                let mut sink = None;
                let mut failed = false;
                while receive.recv().is_ok() {
                    if sink.is_none() && !failed {
                        match rodio::DeviceSinkBuilder::open_default_sink() {
                            Ok(mut s) => {
                                s.log_on_drop(false);
                                sink = Some(s);
                            }
                            Err(_) => failed = true,
                        }
                    }
                    if let Some(sink) = &sink {
                        let mut start = Duration::ZERO;
                        for (freq, secs) in NOTES {
                            let length = Duration::from_secs_f32(secs);
                            sink.mixer().add(
                                SineWave::new(freq)
                                    .take_duration(length)
                                    .amplify(VOLUME)
                                    .delay(start),
                            );
                            start += length;
                        }
                    }
                }
            });
        Alert {
            sound: started.ok().map(|_| send),
            played: 0,
        }
    }

    /// An alert that makes no sound, only counts.
    pub fn silent() -> Alert {
        Alert {
            sound: None,
            played: 0,
        }
    }

    /// Play the alert (if not silent). Never blocks.
    pub fn play(&mut self) {
        self.played += 1;
        if let Some(sound) = &self.sound {
            // The thread only ends if it panicked; then stay quiet.
            let _ = sound.send(());
        }
    }

    /// How many times [`play`](Self::play) was called.
    pub fn played(&self) -> u32 {
        self.played
    }
}
