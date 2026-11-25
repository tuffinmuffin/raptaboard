//! The tones raptaboard plays: when a widget's value can't be sent to the
//! robot, and the ones an Alerts widget can play when a new error arrives
//! ([`Tone`]): a few generated ones, and short sound clips built into the
//! program from `media/audio/` (MP3, decoded when played). "disabled 1"
//! and "2" and "fire 1" and "2" play the IT Crowd clips once their content
//! pack is installed (they aren't ours to build in; see [`crate::content`]),
//! and generated tones until then: any installed pack's sound with a
//! built-in name plays in its place ([`SoundFiles`]).
//!
//! An FMSInfo widget's robot state sounds ([`Sound::play_event`]) are a channel of
//! their own: a built-in [`Tone`] or a sound file the team picks (MP3 or
//! WAV), at the widget's volume, and a new one stops one still playing.
//!
//! The tone plays on a thread of its own, which opens the computer's audio
//! output the first time it's needed. A computer with no audio output (or
//! one that fails to open) just stays quiet: the red border and the
//! message in the status line still say what went wrong.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, RwLock};
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
    /// "I'm disabled" from the fan-clips pack, else a falling tone.
    Disabled1,
    /// The other "I'm disabled" from the fan-clips pack, else a falling tone.
    Disabled2,
    /// "Fire!" from the fan-clips pack, else an alarm.
    Fire1,
    /// The other "Fire!" from the fan-clips pack, else an alarm.
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
            Tone::Danger => Some(clip!("danger.mp3")),
            Tone::AutoGhosts => Some(clip!("auto-ghosts.mp3")),
            Tone::GoGoGo => Some(clip!("go-go-go.mp3")),
            Tone::FmsDown => Some(clip!("fms-down.mp3")),
            Tone::LostComms => Some(clip!("lost-comms.mp3")),
            _ => None,
        }
    }

    /// Its name in the settings (one of [`Tone::CHOICES`]); `None` for
    /// the failed-send tone, which isn't offered.
    ///
    /// ```
    /// use raptaboard::sound::Tone;
    /// assert_eq!(Tone::Fire1.name(), Some("fire 1"));
    /// ```
    pub fn name(self) -> Option<&'static str> {
        Tone::CHOICES
            .iter()
            .find(|(t, _)| *t == self)
            .map(|(_, n)| *n)
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
            // Until the fan-clips pack is installed: a falling "powering
            // down" for disabled, a quick two-note alarm for fire.
            Tone::Disabled1 | Tone::Disabled2 => &[(784.0, 0.12), (587.0, 0.12), (392.0, 0.28)],
            Tone::Fire1 | Tone::Fire2 => &[
                (1319.0, 0.1),
                (988.0, 0.1),
                (1319.0, 0.1),
                (988.0, 0.1),
                (1319.0, 0.1),
                (988.0, 0.1),
            ],
            // Clips play their file instead.
            _ => &[],
        }
    }
}

/// The installed content packs' sounds, shared between [`Sound`] and
/// [`crate::content::Content`], which fills it.
pub type SoundFiles = Arc<RwLock<SoundLayers>>;

/// One installed pack's sound under a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayeredSound {
    /// The pack's id.
    pub pack: String,
    /// The pack's name, for the sound pickers ("fire 1 (IT Crowd clips)").
    pub pack_name: String,
    /// The file it plays.
    pub path: PathBuf,
    /// When the pack was installed, in seconds since 1970.
    pub installed_at: u64,
}

/// The installed packs' sounds by name: a pack may add a sound or give
/// one of the built-in sounds' names to play in its place, and where packs
/// share a name, the newest install wins. See [`SoundRef`] for how a
/// setting picks one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundLayers {
    /// Each name, with every pack that has it, newest install first.
    by_name: HashMap<String, Vec<LayeredSound>>,
}

impl SoundLayers {
    /// Add `sound` under `name`.
    pub fn add(&mut self, name: &str, sound: LayeredSound) {
        let layers = self.by_name.entry(name.to_owned()).or_default();
        layers.push(sound);
        layers.sort_by(|a, b| {
            b.installed_at
                .cmp(&a.installed_at)
                .then_with(|| a.pack.cmp(&b.pack))
        });
    }

    /// Whether no pack has any sound.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Every pack that has a sound called `name`, newest install first.
    pub fn layers(&self, name: &str) -> &[LayeredSound] {
        self.by_name.get(name).map_or(&[], Vec::as_slice)
    }

    /// Every name a pack has a sound for, in order.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.by_name.keys().map(String::as_str).collect();
        names.sort();
        names
    }

    /// The file `sound` plays from a pack, or `None` to play the built-in
    /// sound of its name (or nothing, if there's none):
    ///
    /// - a plain name: the newest install's;
    /// - pinned to a pack: that pack's, if it's installed;
    /// - built in: never a pack's.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use raptaboard::sound::{LayeredSound, SoundLayers, SoundRef};
    /// let mut layers = SoundLayers::default();
    /// let sound = |pack: &str, at| LayeredSound {
    ///     pack: pack.into(), pack_name: pack.into(),
    ///     path: PathBuf::from(format!("/{pack}/fire.mp3")), installed_at: at,
    /// };
    /// layers.add("fire 1", sound("old", 1));
    /// layers.add("fire 1", sound("new", 2));
    /// let file = |s: &str| layers.resolve(&SoundRef::parse(s));
    /// assert_eq!(file("fire 1"), Some(PathBuf::from("/new/fire.mp3")));
    /// assert_eq!(file("pack:old/fire 1"), Some(PathBuf::from("/old/fire.mp3")));
    /// assert_eq!(file("pack:gone/fire 1"), None); // the built-in one
    /// assert_eq!(file("builtin:fire 1"), None);
    /// ```
    pub fn resolve(&self, sound: &SoundRef) -> Option<PathBuf> {
        match sound {
            SoundRef::Name(name) => self.layers(name).first(),
            SoundRef::Pack { pack, name } => self.layers(name).iter().find(|s| s.pack == *pack),
            SoundRef::BuiltIn(_) => None,
        }
        .map(|s| s.path.clone())
    }
}

/// A sound setting: which sound of a name to play. Saved as text: `fire 1`
/// (the newest pack's, else the built-in one), `pack:fan-clips/fire 1`
/// (that pack's, else the built-in one) or `builtin:fire 1` (the built-in
/// one, whatever packs are installed). Whatever isn't found falls back to
/// the built-in sound of that name, or none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundRef {
    /// The newest installed pack's sound of this name, else the built-in one.
    Name(String),
    /// This pack's sound of this name, else the built-in one.
    Pack {
        /// The pack's id.
        pack: String,
        /// The sound's name.
        name: String,
    },
    /// The built-in sound of this name, ignoring packs.
    BuiltIn(String),
}

impl SoundRef {
    /// Read a setting.
    ///
    /// ```
    /// use raptaboard::sound::SoundRef;
    /// assert_eq!(SoundRef::parse("fire 1"), SoundRef::Name("fire 1".into()));
    /// let pinned = SoundRef::parse("pack:fan-clips/fire 1");
    /// assert_eq!(pinned, SoundRef::Pack { pack: "fan-clips".into(), name: "fire 1".into() });
    /// assert_eq!(pinned.setting(), "pack:fan-clips/fire 1");
    /// ```
    pub fn parse(setting: &str) -> SoundRef {
        if let Some(name) = setting.strip_prefix("builtin:") {
            return SoundRef::BuiltIn(name.to_owned());
        }
        if let Some(rest) = setting.strip_prefix("pack:")
            && let Some((pack, name)) = rest.split_once('/')
        {
            return SoundRef::Pack {
                pack: pack.to_owned(),
                name: name.to_owned(),
            };
        }
        SoundRef::Name(setting.to_owned())
    }

    /// The sound's name, whichever sound of it is picked.
    pub fn name(&self) -> &str {
        match self {
            SoundRef::Name(n) | SoundRef::BuiltIn(n) | SoundRef::Pack { name: n, .. } => n,
        }
    }

    /// The setting's text, as [`SoundRef::parse`] reads it.
    pub fn setting(&self) -> String {
        match self {
            SoundRef::Name(n) => n.clone(),
            SoundRef::Pack { pack, name } => format!("pack:{pack}/{name}"),
            SoundRef::BuiltIn(n) => format!("builtin:{n}"),
        }
    }
}

/// One entry in a sound picker: the setting it saves and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundChoice {
    /// The setting's text ([`SoundRef::setting`]).
    pub setting: String,
    /// What the picker shows: "fire 1", "fire 1 (IT Crowd clips)" or
    /// "fire 1 (built in)".
    pub label: String,
    /// Whether it's one sound of the name above it (a pack's, or the
    /// built-in one), shown indented under it.
    pub under: bool,
}

/// Every sound a picker offers, in order: each built-in sound, then each
/// sound only a pack has (by name). Under any name a pack has, its
/// packs' sounds (newest install first), then the built-in one if there
/// is one; the name on its own plays the newest install's.
///
/// ```
/// use std::path::PathBuf;
/// use raptaboard::sound::{sound_choices, LayeredSound, SoundLayers};
/// let mut layers = SoundLayers::default();
/// let sound = |file: &str| LayeredSound {
///     pack: "fan-clips".into(), pack_name: "IT Crowd clips".into(),
///     path: PathBuf::from(file), installed_at: 1,
/// };
/// layers.add("fire 1", sound("/fire1.mp3"));
/// layers.add("horn", sound("/horn.mp3"));
/// let labels: Vec<String> = sound_choices(&layers).into_iter().map(|c| c.label).collect();
/// let fire = labels.iter().position(|l| l == "fire 1").unwrap();
/// assert_eq!(labels[fire + 1], "fire 1 (IT Crowd clips)");
/// assert_eq!(labels[fire + 2], "fire 1 (built in)");
/// assert_eq!(labels[labels.len() - 2..], ["horn", "horn (IT Crowd clips)"]);
/// ```
pub fn sound_choices(layers: &SoundLayers) -> Vec<SoundChoice> {
    let built_in = Tone::CHOICES.iter().map(|(_, n)| *n);
    let from_packs = layers
        .names()
        .into_iter()
        .filter(|n| Tone::CHOICES.iter().all(|(_, b)| b != n));
    let mut choices = Vec::new();
    for name in built_in.chain(from_packs) {
        choices.push(SoundChoice {
            setting: name.to_owned(),
            label: name.to_owned(),
            under: false,
        });
        let packs = layers.layers(name);
        for s in packs {
            choices.push(SoundChoice {
                setting: SoundRef::Pack {
                    pack: s.pack.clone(),
                    name: name.to_owned(),
                }
                .setting(),
                label: format!("{name} ({})", s.pack_name),
                under: true,
            });
        }
        if !packs.is_empty() && Tone::CHOICES.iter().any(|(_, b)| *b == name) {
            choices.push(SoundChoice {
                setting: SoundRef::BuiltIn(name.to_owned()).setting(),
                label: format!("{name} (built in)"),
                under: true,
            });
        }
    }
    choices
}

/// What a picker shows for the sound `setting` picks, and whether it's
/// there: a pack's sound whose pack isn't installed says so (it plays the
/// built-in sound of its name, or nothing).
///
/// ```
/// use raptaboard::sound::{sound_label, SoundLayers};
/// let none = SoundLayers::default();
/// assert_eq!(sound_label("fire 1", &none), ("fire 1".into(), true));
/// assert_eq!(
///     sound_label("pack:fan-clips/fire 1", &none),
///     ("fire 1 (fan-clips, not installed)".into(), false),
/// );
/// assert_eq!(sound_label("horn", &none), ("horn (not installed)".into(), false));
/// ```
pub fn sound_label(setting: &str, layers: &SoundLayers) -> (String, bool) {
    if let Some(c) = sound_choices(layers)
        .into_iter()
        .find(|c| c.setting == setting)
    {
        return (c.label, true);
    }
    let label = match SoundRef::parse(setting) {
        SoundRef::Pack { pack, name } => format!("{name} ({pack}, not installed)"),
        // A built-in name with no pack: shown plain above, so not here.
        SoundRef::BuiltIn(name) => format!("{name} (not built in)"),
        SoundRef::Name(name) => format!("{name} (not installed)"),
    };
    (label, false)
}

/// The key the app keeps its installed packs' sounds under in egui's
/// memory, for the sound pickers.
fn files_id() -> eframe::egui::Id {
    eframe::egui::Id::unique("raptaboard installed sounds")
}

/// Let the sound pickers see the installed packs' sounds (`files`).
pub fn share_files(ctx: &eframe::egui::Context, files: &SoundFiles) {
    ctx.data_mut(|d| d.insert_temp(files_id(), files.clone()));
}

/// The installed packs' sounds, as [`share_files`] shared them (none if
/// it wasn't called).
pub fn installed(ctx: &eframe::egui::Context) -> SoundLayers {
    ctx.data(|d| d.get_temp::<SoundFiles>(files_id()))
        .and_then(|f| f.read().ok().map(|l| l.clone()))
        .unwrap_or_default()
}

/// A sound to play: a built-in tone or clip, a sound file the team
/// picked, or a sound by name from the installed packs.
#[derive(Debug, Clone, PartialEq)]
pub enum Clip {
    /// One of raptaboard's own, or the newest installed pack's sound of
    /// its name.
    Tone(Tone),
    /// An MP3 or WAV file on this computer. One that's missing or doesn't
    /// decode plays nothing.
    File(PathBuf),
    /// A sound by name ([`SoundRef`]): one only packs have, or one pinned
    /// to a pack or to the built-in sound.
    Sound(SoundRef),
}

/// What a [`Clip`] plays, with the packs installed now.
#[derive(Debug, Clone, PartialEq)]
pub enum Playable {
    /// A sound file: the team's, or a pack's.
    File(PathBuf),
    /// One of raptaboard's own.
    BuiltIn(Tone),
    /// Nothing: a pack's sound that isn't installed, and no built-in sound
    /// has its name.
    Nothing,
}

impl Clip {
    /// The clip a sound setting names (as a sound picker saves it), or
    /// `None` for an empty one. A built-in sound's plain name is its
    /// [`Clip::Tone`].
    ///
    /// ```
    /// use raptaboard::sound::{Clip, SoundRef, Tone};
    /// assert_eq!(Clip::named("chirp"), Some(Clip::Tone(Tone::Chirp)));
    /// assert_eq!(Clip::named("horn"), Some(Clip::Sound(SoundRef::Name("horn".into()))));
    /// assert_eq!(
    ///     Clip::named("builtin:fire 1"),
    ///     Some(Clip::Sound(SoundRef::BuiltIn("fire 1".into()))),
    /// );
    /// assert_eq!(Clip::named(" "), None);
    /// ```
    pub fn named(setting: &str) -> Option<Clip> {
        let setting = setting.trim();
        if setting.is_empty() {
            return None;
        }
        let sound = SoundRef::parse(setting);
        if let SoundRef::Name(name) = &sound
            && let Some((tone, _)) = Tone::CHOICES.iter().find(|(_, n)| n == name)
        {
            return Some(Clip::Tone(*tone));
        }
        Some(Clip::Sound(sound))
    }

    /// The built-in sound this falls back to: the one of its name.
    fn built_in(&self) -> Option<Tone> {
        let name = match self {
            Clip::Tone(tone) => return Some(*tone),
            Clip::File(_) => return None,
            Clip::Sound(sound) => sound.name(),
        };
        Tone::CHOICES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(t, _)| *t)
    }

    /// What this plays with `layers` installed: an installed pack's file
    /// ([`SoundLayers::resolve`]), else the built-in sound of its name,
    /// else nothing.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use raptaboard::sound::{Clip, LayeredSound, Playable, SoundLayers, Tone};
    /// let mut layers = SoundLayers::default();
    /// let fire = Clip::named("fire 1").unwrap();
    /// assert_eq!(fire.playable(&layers), Playable::BuiltIn(Tone::Fire1));
    /// layers.add("fire 1", LayeredSound {
    ///     pack: "p".into(), pack_name: "P".into(),
    ///     path: PathBuf::from("/p/fire.mp3"), installed_at: 1,
    /// });
    /// assert_eq!(fire.playable(&layers), Playable::File("/p/fire.mp3".into()));
    /// let pinned = Clip::named("builtin:fire 1").unwrap();
    /// assert_eq!(pinned.playable(&layers), Playable::BuiltIn(Tone::Fire1));
    /// assert_eq!(Clip::named("horn").unwrap().playable(&layers), Playable::Nothing);
    /// ```
    pub fn playable(&self, layers: &SoundLayers) -> Playable {
        let sound = match self {
            Clip::File(path) => return Playable::File(path.clone()),
            Clip::Tone(tone) => match tone.name() {
                Some(name) => SoundRef::Name(name.to_owned()),
                // The failed-send tone: never a pack's.
                None => return Playable::BuiltIn(*tone),
            },
            Clip::Sound(sound) => sound.clone(),
        };
        if let Some(path) = layers.resolve(&sound) {
            return Playable::File(path);
        }
        self.built_in().map_or(Playable::Nothing, Playable::BuiltIn)
    }
}

/// What `clip` plays with the packs in `packs` installed now.
fn playable(clip: &Clip, packs: &SoundFiles) -> Playable {
    match packs.read() {
        Ok(layers) => clip.playable(&layers),
        Err(_) => clip.playable(&SoundLayers::default()),
    }
}

/// The most sound a plugin's stream holds before more is dropped, in
/// seconds: enough to ride out an uneven frame, little enough to stay in
/// step with the picture.
pub const STREAM_HOLDS_S: f64 = 0.5;

/// Samples waiting to be played on one stream, shared between whoever adds
/// them (a plugin's thread) and the sound thread.
#[derive(Debug, Default)]
struct Queue {
    samples: std::collections::VecDeque<f32>,
    /// The sender is gone: play what's left, then end.
    closed: bool,
}

/// A stream of sound a plugin makes as it goes (a game's), from
/// [`AudioOut::stream`]: 16-bit samples added a little at a time, played as
/// they come, silence when there are none. Dropping it ends the stream.
#[derive(Debug)]
pub struct AudioStream {
    queue: Arc<Mutex<Queue>>,
    /// Samples a second, and channels interleaved.
    pub rate: u32,
    pub channels: u8,
    out: AudioOut,
}

impl AudioStream {
    /// Queue `samples` (interleaved), after what's queued. While sound is
    /// off they're dropped; past [`STREAM_HOLDS_S`] queued, the rest are.
    pub fn push(&self, samples: &[i16]) {
        if let Ok(mut heard) = self.out.heard.lock() {
            *heard += samples.len();
        }
        if !self.out.enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let most = (STREAM_HOLDS_S * f64::from(self.rate) * f64::from(self.channels)) as usize;
        if let Ok(mut q) = self.queue.lock() {
            let room = most.saturating_sub(q.samples.len());
            q.samples
                .extend(samples.iter().take(room).map(|s| f32::from(*s) / 32768.0));
        }
    }

    /// How many samples are queued now, not yet played.
    pub fn queued(&self) -> usize {
        self.queue.lock().map_or(0, |q| q.samples.len())
    }
}

impl Drop for AudioStream {
    fn drop(&mut self) {
        if let Ok(mut q) = self.queue.lock() {
            q.closed = true;
        }
    }
}

/// A stream as the sound thread plays it.
struct StreamSource {
    queue: Arc<Mutex<Queue>>,
    rate: rodio::SampleRate,
    channels: rodio::ChannelCount,
}

impl Iterator for StreamSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let mut q = self.queue.lock().ok()?;
        match q.samples.pop_front() {
            Some(s) => Some(s),
            None if q.closed => None,
            None => Some(0.0),
        }
    }
}

impl Source for StreamSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Where plugins' sound streams go: the sound thread, unless silent. On or
/// off with the dashboard's sound (the bell), [`AudioOut::set_enabled`].
#[derive(Debug, Clone)]
pub struct AudioOut {
    send: Option<Sender<Request>>,
    enabled: Arc<std::sync::atomic::AtomicBool>,
    /// Every sample offered so far, played or not (for tests).
    heard: Arc<Mutex<usize>>,
}

impl AudioOut {
    /// A new stream at `rate` samples a second, `channels` (1 or 2)
    /// interleaved.
    pub fn stream(&self, rate: u32, channels: u8) -> AudioStream {
        let queue = Arc::new(Mutex::new(Queue::default()));
        if let (Some(send), Some(r), Some(c)) = (
            &self.send,
            std::num::NonZeroU32::new(rate),
            std::num::NonZeroU16::new(u16::from(channels)),
        ) {
            let _ = send.send(Request::Stream(StreamSource {
                queue: queue.clone(),
                rate: r,
                channels: c,
            }));
        }
        AudioStream {
            queue,
            rate,
            channels,
            out: self.clone(),
        }
    }

    /// Play plugins' streams (true) or drop what they send (false).
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Every sample plugins have offered so far, played or not.
    pub fn heard(&self) -> usize {
        self.heard.lock().map_or(0, |h| *h)
    }

    /// Whether this is the same output as `other`.
    pub fn same_as(&self, other: &AudioOut) -> bool {
        Arc::ptr_eq(&self.heard, &other.heard)
    }
}

/// What the sound thread is asked to play.
enum Request {
    /// A sound on its own (alerts, failed sends): mixed with whatever plays.
    Mix(Clip),
    /// A plugin's stream, mixed in until it ends.
    Stream(StreamSource),
    /// A robot state sound at `volume` (0 to 1), stopping the last one.
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
    /// Every sound asked for with [`Sound::play_sound`], in order (for tests).
    sounds: Vec<Clip>,
    /// Plays robot state sounds; shared with the NetworkTables thread.
    player: SoundPlayer,
    /// Sounds from installed packs, which the sound thread plays.
    files: SoundFiles,
    /// Plugins' sound streams.
    audio: AudioOut,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Request::Mix(c) => write!(f, "Mix({c:?})"),
            Request::Stream(_) => write!(f, "Stream"),
            Request::Event { clip, volume } => write!(f, "Event({clip:?}, {volume})"),
        }
    }
}

/// Plays robot state sounds from any thread ([`Sound::player`]): the
/// NetworkTables thread plays them as values arrive, so they don't wait
/// for the window to draw (a window in the background draws rarely).
#[derive(Debug, Clone)]
pub struct SoundPlayer {
    /// The sound thread's queue; `None` when silent.
    send: Option<Sender<Request>>,
    /// Every sound asked for, with its volume (for tests).
    played: Arc<Mutex<Vec<(Clip, f32)>>>,
    /// The last robot state the state sound task read (for tests: to know
    /// it has seen a state before the next one is sent).
    seen: Arc<Mutex<Option<crate::types::robot_state::RobotState>>>,
}

impl SoundPlayer {
    /// Play `clip` at `volume` (0 to 1), stopping the last robot state
    /// sound if it's still playing. Never blocks.
    pub fn play_event(&self, clip: Clip, volume: f32) {
        if let Ok(mut played) = self.played.lock() {
            played.push((clip.clone(), volume));
        }
        if let Some(send) = &self.send {
            let _ = send.send(Request::Event { clip, volume });
        }
    }

    /// Note the robot state the state sound task just read (`None`: none
    /// to read). Only tests look at it ([`Sound::state_seen`]).
    pub fn saw(&self, state: Option<crate::types::robot_state::RobotState>) {
        if let Ok(mut seen) = self.seen.lock() {
            *seen = state;
        }
    }

    /// Whether this plays through the same sound thread (and records to
    /// the same list) as `other`.
    pub fn same_as(&self, other: &SoundPlayer) -> bool {
        Arc::ptr_eq(&self.played, &other.played)
    }
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
        let files = SoundFiles::default();
        let packs = files.clone();
        let started = std::thread::Builder::new()
            .name("raptaboard-sound".into())
            .spawn(move || {
                // Opened on the first alert, and kept open after that.
                let mut sink = None;
                let mut failed = false;
                // robot state sounds play here, so a new one can stop it.
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
                    let clip = match request {
                        Request::Mix(clip) => clip,
                        Request::Stream(stream) => {
                            sink.mixer().add(stream);
                            continue;
                        }
                        Request::Event { clip, volume } => {
                            if let Some(old) = event_player.take() {
                                old.stop();
                            }
                            let player = rodio::Player::connect_new(sink.mixer());
                            player.set_volume(volume.clamp(0.0, 1.0));
                            append_clip(&player, &clip, &packs);
                            event_player = Some(player);
                            continue;
                        }
                    };
                    let mut tone = None;
                    match playable(&clip, &packs) {
                        Playable::File(path) => match decode_file(&path) {
                            Some(decoded) => sink.mixer().add(decoded.amplify(CLIP_VOLUME)),
                            // A pack's file that doesn't decode: the
                            // built-in sound of its name.
                            None => tone = clip.built_in(),
                        },
                        Playable::BuiltIn(t) => tone = Some(t),
                        Playable::Nothing => {}
                    }
                    let Some(tone) = tone else { continue };
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
        let sound = started.ok().map(|_| send);
        Sound {
            player: SoundPlayer {
                send: sound.clone(),
                played: Arc::default(),
                seen: Arc::default(),
            },
            audio: AudioOut {
                send: sound.clone(),
                enabled: Arc::new(true.into()),
                heard: Arc::default(),
            },
            sound,
            played: 0,
            tones: Vec::new(),
            sounds: Vec::new(),
            files,
        }
    }

    /// An alert that makes no sound, only counts.
    pub fn silent() -> Sound {
        Sound {
            sound: None,
            played: 0,
            tones: Vec::new(),
            sounds: Vec::new(),
            player: SoundPlayer {
                send: None,
                played: Arc::default(),
                seen: Arc::default(),
            },
            files: SoundFiles::default(),
            audio: AudioOut {
                send: None,
                enabled: Arc::new(true.into()),
                heard: Arc::default(),
            },
        }
    }

    /// Where plugins' sound streams go.
    pub fn audio(&self) -> AudioOut {
        self.audio.clone()
    }

    /// The installed packs' sounds this plays, for
    /// [`crate::content::Content`] to fill.
    pub fn files(&self) -> SoundFiles {
        self.files.clone()
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
            let _ = sound.send(Request::Mix(Clip::Tone(tone)));
        }
    }

    /// Play `clip` on its own, mixed with whatever plays (if not silent):
    /// an Alerts widget's sound. A [`Clip::Tone`] plays as
    /// [`play_tone`](Self::play_tone) does. Never blocks.
    pub fn play_sound(&mut self, clip: Clip) {
        if let Clip::Tone(tone) = clip {
            return self.play_tone(tone);
        }
        self.played += 1;
        self.sounds.push(clip.clone());
        if let Some(sound) = &self.sound {
            let _ = sound.send(Request::Mix(clip));
        }
    }

    /// Every sound played with [`play_sound`](Self::play_sound) that isn't
    /// a [`Clip::Tone`], in order.
    pub fn sounds(&self) -> &[Clip] {
        &self.sounds
    }

    /// Play a robot state sound at `volume` (0 to 1), stopping the last one
    /// if it's still playing (if not silent). Never blocks.
    pub fn play_event(&mut self, clip: Clip, volume: f32) {
        self.player.play_event(clip, volume);
    }

    /// Every robot state sound played so far (from any thread), with its
    /// volume, in order.
    pub fn events(&self) -> Vec<(Clip, f32)> {
        self.player
            .played
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    /// The last robot state the state sound task read, if any
    /// ([`SoundPlayer::saw`]).
    pub fn state_seen(&self) -> Option<crate::types::robot_state::RobotState> {
        self.player.seen.lock().ok().and_then(|s| *s)
    }

    /// A handle that plays robot state sounds from another thread.
    pub fn player(&self) -> SoundPlayer {
        self.player.clone()
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

/// The sound file at `path`, decoded; `None` if it's missing or doesn't
/// decode.
fn decode_file(
    path: &std::path::Path,
) -> Option<rodio::Decoder<std::io::BufReader<std::fs::File>>> {
    let file = std::fs::File::open(path).ok()?;
    rodio::Decoder::try_from(file).ok()
}

/// Queue `clip` on `player`, at full volume (the player sets the volume):
/// an installed pack's file for its name, else the built-in sound.
fn append_clip(player: &rodio::Player, clip: &Clip, packs: &SoundFiles) {
    let tone = match playable(clip, packs) {
        Playable::File(path) => match decode_file(&path) {
            Some(decoded) => return player.append(decoded),
            // The team's file plays nothing; a pack's falls back to the
            // built-in sound of its name.
            None => clip.built_in(),
        },
        Playable::BuiltIn(tone) => Some(tone),
        Playable::Nothing => None,
    };
    let Some(tone) = tone else { return };
    if let Some(bytes) = tone.clip() {
        if let Ok(decoded) = rodio::Decoder::new(std::io::Cursor::new(bytes)) {
            player.append(decoded);
        }
        return;
    }
    // Generated notes, one after another (a rest is a silent note), as
    // soft as an alert's beside the clips.
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

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn tones_stand_in_for_the_fan_clips_until_their_pack_is_installed() {
        let files = SoundFiles::default();
        for tone in [Tone::Disabled1, Tone::Disabled2, Tone::Fire1, Tone::Fire2] {
            assert!(tone.clip().is_none(), "{tone:?} isn't built in");
            assert!(!tone.notes().is_empty(), "{tone:?} plays a tone instead");
            assert_eq!(playable(&Clip::Tone(tone), &files), Playable::BuiltIn(tone));
        }
        // Our own clips are built in.
        assert!(Tone::Danger.clip().is_some() && Tone::LostComms.clip().is_some());
        // Installed, the pack's file plays for the sound of that name.
        files.write().unwrap().add(
            "fire 1",
            LayeredSound {
                pack: "fan-clips".into(),
                pack_name: "IT Crowd clips".into(),
                path: PathBuf::from("/packs/fan-clips/fire1.mp3"),
                installed_at: 1,
            },
        );
        let plays = |tone| playable(&Clip::Tone(tone), &files);
        assert_eq!(
            plays(Tone::Fire1),
            Playable::File(PathBuf::from("/packs/fan-clips/fire1.mp3"))
        );
        assert_eq!(plays(Tone::Fire2), Playable::BuiltIn(Tone::Fire2));
        assert_eq!(plays(Tone::Failure), Playable::BuiltIn(Tone::Failure));
    }

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

    #[test]
    fn a_plugins_stream_plays_what_comes_then_silence_and_holds_half_a_second() {
        let out = Sound::silent().audio();
        let stream = out.stream(100, 1);
        let mut source = StreamSource {
            queue: stream.queue.clone(),
            rate: std::num::NonZeroU32::new(100).unwrap(),
            channels: std::num::NonZeroU16::new(1).unwrap(),
        };
        stream.push(&[16384, -16384]);
        assert_eq!(source.next(), Some(0.5));
        assert_eq!(source.next(), Some(-0.5));
        // Nothing queued: silence, not the end.
        assert_eq!(source.next(), Some(0.0));
        // At 100 a second, half a second is 50 samples: the rest dropped.
        stream.push(&[1; 80]);
        assert_eq!(stream.queued(), 50);
        assert_eq!(out.heard(), 82);
        // Sound off: dropped.
        out.set_enabled(false);
        stream.push(&[1; 10]);
        assert_eq!(stream.queued(), 50);
        // The plugin's side gone: what's left plays, then it ends.
        drop(stream);
        assert_eq!(source.by_ref().count(), 50);
        assert_eq!(source.next(), None);
    }
}
