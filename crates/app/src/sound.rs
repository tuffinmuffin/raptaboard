//! The tones raptaboard plays: when a widget's value can't be sent to the
//! robot, and the ones an Alerts widget can play when a new error arrives
//! ([`Tone`]): a few generated ones, and short sound clips built into the
//! program from `media/audio/` (MP3, decoded when played).
//!
//! A Robot State widget's sounds ([`Sound::play_event`]) are a channel of
//! their own: a built-in [`Tone`] or a sound file the team picks (MP3 or
//! WAV), at the widget's volume, and a new one stops one still playing.
//!
//! The tone plays on a thread of its own, which opens the computer's audio
//! output the first time it's needed. A computer with no audio output (or
//! one that fails to open) just stays quiet: the red border and the
//! message in the status line still say what went wrong.

use std::path::PathBuf;
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
    /// The clip `media/audio/disabled1.mp3`.
    Disabled1,
    /// The clip `media/audio/disabled2.mp3`.
    Disabled2,
    /// The clip `media/audio/fire1.mp3`.
    Fire1,
    /// The clip `media/audio/fire2.mp3`.
    Fire2,
    /// The clip `media/audio/danger.mp3`: "danger, danger" in a robot voice.
    Danger,
    /// The clip `media/audio/auto-ghosts.mp3`: "Auto Ghosts", for auto starting.
    AutoGhosts,
    /// The clip `media/audio/go-go-go.mp3`: "go go go", for teleop starting.
    GoGoGo,
    /// The clip `media/audio/fms-down.mp3`: "FMS down", for losing the field.
    FmsDown,
    /// The clip `media/audio/lost-comms.mp3`: "lost comms", for losing the robot.
    LostComms,
}

/// The clips built into the program, by the file they came from.
macro_rules! clip {
    ($file:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../media/audio/",
            $file
        ))
    };
}

impl Tone {
    /// The tones an Alerts widget offers, with their names in the settings.
    pub const CHOICES: [(Tone, &'static str); 12] = [
        (Tone::Beep, "beep"),
        (Tone::DoubleBeep, "double beep"),
        (Tone::Chirp, "chirp"),
        (Tone::Disabled1, "disabled 1"),
        (Tone::Disabled2, "disabled 2"),
        (Tone::Fire1, "fire 1"),
        (Tone::Fire2, "fire 2"),
        (Tone::Danger, "danger danger"),
        (Tone::AutoGhosts, "auto ghosts"),
        (Tone::GoGoGo, "go go go"),
        (Tone::FmsDown, "fms down"),
        (Tone::LostComms, "lost comms"),
    ];

    /// The MP3 a clip plays, if this is a clip.
    pub fn clip(self) -> Option<&'static [u8]> {
        match self {
            Tone::Disabled1 => Some(clip!("disabled1.mp3")),
            Tone::Disabled2 => Some(clip!("disabled2.mp3")),
            Tone::Fire1 => Some(clip!("fire1.mp3")),
            Tone::Fire2 => Some(clip!("fire2.mp3")),
            Tone::Danger => Some(clip!("danger.mp3")),
            Tone::AutoGhosts => Some(clip!("auto-ghosts.mp3")),
            Tone::GoGoGo => Some(clip!("go-go-go.mp3")),
            Tone::FmsDown => Some(clip!("fms-down.mp3")),
            Tone::LostComms => Some(clip!("lost-comms.mp3")),
            _ => None,
        }
    }

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
            // Clips play their file instead.
            _ => &[],
        }
    }
}
/// A sound for a Robot State event: a built-in tone or clip, or a sound
/// file the team picked.
#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
    /// One of raptaboard's own.
    Tone(Tone),
    /// An MP3 or WAV file on this computer. One that's missing or doesn't
    /// decode plays nothing.
    File(PathBuf),
}

/// What the sound thread is asked to play.
enum Request {
    /// A tone on its own (alerts, failed sends): mixed with whatever plays.
    Tone(Tone),
    /// A Robot State sound at `volume` (0 to 1), stopping the last one.
    Event { clip: Clip, volume: f32 },
}

/// How loud the alert is, from 0 to 1: noticeable, not startling.
const VOLUME: f32 = 0.25;
/// How loud the clips are: recordings, so less quiet than a pure tone.
const CLIP_VOLUME: f32 = 0.8;

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
    /// Sends what to play to the sound thread; `None` when silent.
    sound: Option<Sender<Request>>,
    /// How many times the alert was asked for.
    played: u32,
    /// Every tone asked for, in order (for tests).
    tones: Vec<Tone>,
    /// Every Robot State sound asked for, with its volume (for tests).
    events: Vec<(Clip, f32)>,
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
        let (send, receive) = mpsc::channel::<Request>();
        let started = std::thread::Builder::new()
            .name("raptaboard-sound".into())
            .spawn(move || {
                // Opened on the first alert, and kept open after that.
                let mut sink = None;
                let mut failed = false;
                // Robot State sounds play here, so a new one can stop it.
                let mut event_player: Option<rodio::Player> = None;
                while let Ok(request) = receive.recv() {
                    if sink.is_none() && !failed {
                        match rodio::DeviceSinkBuilder::open_default_sink() {
                            Ok(mut s) => {
                                s.log_on_drop(false);
                                sink = Some(s);
                            }
                            Err(_) => failed = true,
                        }
                    }
                    let Some(sink) = &sink else { continue };
                    let tone = match request {
                        Request::Tone(tone) => tone,
                        Request::Event { clip, volume } => {
                            if let Some(old) = event_player.take() {
                                old.stop();
                            }
                            let player = rodio::Player::connect_new(sink.mixer());
                            player.set_volume(volume.clamp(0.0, 1.0));
                            append_clip(&player, &clip);
                            event_player = Some(player);
                            continue;
                        }
                    };
                    if let Some(bytes) = tone.clip() {
                        // A clip that doesn't decode just stays quiet.
                        if let Ok(clip) = rodio::Decoder::new(std::io::Cursor::new(bytes)) {
                            sink.mixer().add(clip.amplify(CLIP_VOLUME));
                        }
                    } else {
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
            events: Vec::new(),
        }
    }

    /// An alert that makes no sound, only counts.
    pub fn silent() -> Sound {
        Sound {
            sound: None,
            played: 0,
            tones: Vec::new(),
            events: Vec::new(),
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
            let _ = sound.send(Request::Tone(tone));
        }
    }

    /// Play a Robot State sound at `volume` (0 to 1), stopping the last one
    /// if it's still playing (if not silent). Never blocks.
    pub fn play_event(&mut self, clip: Clip, volume: f32) {
        self.events.push((clip.clone(), volume));
        if let Some(sound) = &self.sound {
            let _ = sound.send(Request::Event { clip, volume });
        }
    }

    /// Every Robot State sound played so far, with its volume, in order.
    pub fn events(&self) -> &[(Clip, f32)] {
        &self.events
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

/// Queue `clip` on `player`, at full volume (the player sets the volume).
fn append_clip(player: &rodio::Player, clip: &Clip) {
    match clip {
        Clip::File(path) => {
            if let Ok(file) = std::fs::File::open(path)
                && let Ok(decoded) = rodio::Decoder::try_from(file)
            {
                player.append(decoded);
            }
        }
        Clip::Tone(tone) => {
            if let Some(bytes) = tone.clip() {
                if let Ok(decoded) = rodio::Decoder::new(std::io::Cursor::new(bytes)) {
                    player.append(decoded);
                }
                return;
            }
            // Generated notes, one after another (a rest is a silent note),
            // as soft as an alert's beside the clips.
            for &(freq, secs) in tone.notes() {
                let length = Duration::from_secs_f32(secs);
                let loud = if freq > 0.0 { VOLUME } else { 0.0 };
                player.append(
                    SineWave::new(freq.max(1.0))
                        .take_duration(length)
                        .amplify(loud),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn every_clip_is_built_in_and_decodes() {
        for (tone, name) in Tone::CHOICES {
            assert_eq!(Tone::named(name), tone);
            let Some(bytes) = tone.clip() else {
                assert!(
                    !tone.notes().is_empty(),
                    "{name} has neither notes nor a clip"
                );
                continue;
            };
            let clip = rodio::Decoder::new(std::io::Cursor::new(bytes))
                .unwrap_or_else(|e| panic!("{name} doesn't decode: {e}"));
            // Some sound in it: at least a tenth of a second at its rate.
            let rate = clip.sample_rate().get() as usize;
            let samples = clip.take(rate).count();
            assert!(samples >= rate / 10, "{name}: only {samples} samples");
        }
    }
}
