//! The tones raptaboard plays: when a widget's value can't be sent to the
//! robot, and the ones an Alerts widget can play when a new error arrives
//! ([`Tone`]).
//!
//! The tone plays on a thread of its own, which opens the computer's audio
//! output the first time it's needed. A computer with no audio output (or
//! one that fails to open) just stays quiet: the red border and the
//! message in the status line still say what went wrong.

use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use rodio::Source;
use rodio::source::SineWave;

/// A tone: its notes, each a pitch (hertz) and a length (seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// A send failed: high then low.
    Failure,
    /// One short beep.
    Beep,
    /// Two short beeps.
    DoubleBeep,
    /// Three notes going up.
    Chirp,
}

impl Tone {
    /// The tones an Alerts widget offers, with their names in the settings.
    pub const CHOICES: [(Tone, &'static str); 3] = [
        (Tone::Beep, "beep"),
        (Tone::DoubleBeep, "double beep"),
        (Tone::Chirp, "chirp"),
    ];

    /// The tone saved as `name` (one of [`Tone::CHOICES`]), or a beep.
    ///
    /// ```
    /// use raptaboard::sound::Tone;
    /// assert_eq!(Tone::named("chirp"), Tone::Chirp);
    /// assert_eq!(Tone::named("??"), Tone::Beep);
    /// ```
    pub fn named(name: &str) -> Tone {
        Tone::CHOICES
            .iter()
            .find(|(_, n)| *n == name)
            .map_or(Tone::Beep, |(t, _)| *t)
    }

    /// Its notes, in order: pitch in hertz (0 for a rest) and length in
    /// seconds.
    fn notes(self) -> &'static [(f32, f32)] {
        match self {
            Tone::Failure => &[(880.0, 0.09), (587.0, 0.16)],
            Tone::Beep => &[(988.0, 0.15)],
            Tone::DoubleBeep => &[(988.0, 0.1), (0.0, 0.07), (988.0, 0.1)],
            Tone::Chirp => &[(659.0, 0.07), (880.0, 0.07), (1319.0, 0.12)],
        }
    }
}
/// How loud the alert is, from 0 to 1: noticeable, not startling.
const VOLUME: f32 = 0.25;

/// Plays raptaboard's sounds (tones and clips), or counts them when silent
/// (in tests, or when the user turned sound off).
///
/// ```
/// use raptaboard::sound::Sound;
/// let mut sound = Sound::silent();
/// sound.play();
/// assert_eq!(sound.played(), 1);
/// ```
#[derive(Debug)]
pub struct Sound {
    /// Sends the tone to play to the sound thread; `None` when silent.
    sound: Option<Sender<Tone>>,
    /// How many times the alert was asked for.
    played: u32,
    /// Every tone asked for, in order (for tests).
    tones: Vec<Tone>,
}

impl Default for Sound {
    /// The same as [`Sound::new`].
    fn default() -> Self {
        Sound::new()
    }
}

impl Sound {
    /// An alert that plays through the computer's speakers.
    pub fn new() -> Sound {
        let (send, receive) = mpsc::channel::<Tone>();
        let started = std::thread::Builder::new()
            .name("raptaboard-sound".into())
            .spawn(move || {
                // Opened on the first alert, and kept open after that.
                let mut sink = None;
                let mut failed = false;
                while let Ok(tone) = receive.recv() {
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
                        for &(freq, secs) in tone.notes() {
                            let length = Duration::from_secs_f32(secs);
                            if freq > 0.0 {
                                sink.mixer().add(
                                    SineWave::new(freq)
                                        .take_duration(length)
                                        .amplify(VOLUME)
                                        .delay(start),
                                );
                            }
                            start += length;
                        }
                    }
                }
            });
        Sound {
            sound: started.ok().map(|_| send),
            played: 0,
            tones: Vec::new(),
        }
    }

    /// An alert that makes no sound, only counts.
    pub fn silent() -> Sound {
        Sound {
            sound: None,
            played: 0,
            tones: Vec::new(),
        }
    }

    /// Play the failed-send tone (if not silent). Never blocks.
    pub fn play(&mut self) {
        self.play_tone(Tone::Failure);
    }

    /// Play `tone` (if not silent). Never blocks.
    pub fn play_tone(&mut self, tone: Tone) {
        self.played += 1;
        self.tones.push(tone);
        if let Some(sound) = &self.sound {
            // The thread only ends if it panicked; then stay quiet.
            let _ = sound.send(tone);
        }
    }

    /// Every tone played so far, in order.
    pub fn tones(&self) -> &[Tone] {
        &self.tones
    }

    /// How many times [`play`](Self::play) was called.
    pub fn played(&self) -> u32 {
        self.played
    }
}
