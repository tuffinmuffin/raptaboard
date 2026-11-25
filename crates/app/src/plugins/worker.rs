//! One plugin widget running on its own thread, so the window never waits on
//! a plugin: the window sends it values, frames to draw and input, and draws
//! the newest picture it has finished.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use raptaboard_plugin_host::runtime::{
    FrameInfo, Host, Input, Level, Limits, NtValue, Picture, Plugin, Runtime, Sample, Size,
    Stopped, Subscription, Usage,
};

/// How many of a plugin's log lines are kept, for the sandbox's overlay.
pub const LOG_LINES: usize = 200;

/// What the plugin asked the dashboard to do, for the window to carry out on
/// its next frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Play this sound setting (`fire 1`, `pack:…`, `builtin:…`).
    Sound(String),
    /// Publish a value under the widget's table: (key, value).
    Publish(String, NtValue),
    /// A line for the log.
    Log(String),
}

/// What the window reads back from the worker.
#[derive(Default)]
pub struct Latest {
    /// What the plugin wants values for, once it has said.
    pub subscriptions: Option<Vec<Subscription>>,
    /// The newest picture finished.
    pub picture: Option<Picture>,
    /// Counts pictures, so pixel images are sent to the GPU only when new.
    pub picture_number: u64,
    /// The mark for the tab, from after the last update.
    pub tab_mark: Option<Level>,
    /// Why the plugin stopped, if it has.
    pub stopped: Option<Stopped>,
    /// What it asked for since the window last looked.
    pub requests: Vec<Request>,
    /// When the plugin asked to be drawn again (`redraw-in-s`).
    pub redraw_at: Option<Instant>,
    /// When the frame being drawn now was due, if it was drawn because its
    /// time came: the next is due `redraw-in-s` after that, not after it
    /// finished, so an animation keeps the rate it asked for.
    pub drawing_due: Option<Instant>,
    /// What it has used (call times, memory), as of its last call.
    pub usage: Option<Usage>,
    /// Its last [`LOG_LINES`] log lines (and printed lines), oldest first.
    pub log: VecDeque<String>,
}

/// What the last picture was drawn for, so the window asks for a new one
/// only when something changed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawn {
    /// Width and height in pixels.
    pub size: (f32, f32),
    pub connected: bool,
    pub locked: bool,
    pub dark: bool,
}

enum Message {
    Update(Vec<Sample>),
    Render(FrameInfo),
    Input(Input),
}

/// A running plugin widget. Dropping it stops the thread.
pub struct Worker {
    sender: Sender<Message>,
    /// What the worker has made, shared with its thread.
    pub latest: Arc<Mutex<Latest>>,
    /// The values last sent, by topic as the plugin names them, so only
    /// changes are sent.
    pub sent: Mutex<HashMap<String, nt4::Value>>,
    /// What the last frame asked for was drawn for, and whether values or
    /// input came since.
    drawn: Mutex<(Option<Drawn>, bool)>,
    started: Instant,
}

impl Worker {
    /// Start `plugin` on a thread of its own with `properties`. `ctx` is for
    /// measuring text and asking for repaints when a picture is ready; its
    /// sound (`play-audio`) goes to `audio`.
    pub fn start(
        runtime: Arc<Runtime>,
        plugin: Arc<Plugin>,
        properties: HashMap<String, NtValue>,
        ctx: egui::Context,
        audio: crate::sound::AudioOut,
    ) -> Worker {
        let (sender, receiver) = mpsc::channel();
        let latest = Arc::new(Mutex::new(Latest::default()));
        let shared = latest.clone();
        let name = format!("plugin {}", plugin.manifest.name);
        let started = std::thread::Builder::new()
            .name(name)
            .spawn(move || run(runtime, plugin, properties, ctx, audio, receiver, shared));
        if let Err(e) = started {
            latest.lock().unwrap().stopped = Some(Stopped {
                reason: format!("couldn't start a thread: {e}"),
            });
        }
        Worker {
            sender,
            latest,
            sent: Mutex::new(HashMap::new()),
            drawn: Mutex::new((None, true)),
            started: Instant::now(),
        }
    }

    /// Send changed values.
    pub fn update(&self, samples: Vec<Sample>) {
        if !samples.is_empty() {
            self.drawn.lock().unwrap().1 = true;
            let _ = self.sender.send(Message::Update(samples));
        }
    }

    /// Ask for a new picture if anything changed since the last: values or
    /// input sent since, a different `drawn`, or the time the plugin asked
    /// to be drawn again has come. `frame` makes the frame's details from
    /// its time: when it was due, for one drawn because its time came (so an
    /// animation moves evenly however late the window woke), else now.
    ///
    /// Returns when the plugin wants drawing next, for the window to wake
    /// then: asked from the window's frame, so it isn't lost (a wake asked
    /// from the worker's thread right after "show this now" is dropped).
    pub fn render_if_needed(
        &self,
        drawn: Drawn,
        frame: impl FnOnce(f64) -> FrameInfo,
    ) -> Option<Instant> {
        let (due, pending) = {
            let mut latest = self.latest.lock().unwrap();
            let due = latest.redraw_at.filter(|at| *at <= Instant::now());
            if due.is_some() {
                latest.redraw_at = None;
                latest.drawing_due = due;
            }
            (due, latest.redraw_at)
        };
        let mut state = self.drawn.lock().unwrap();
        if state.1 || state.0 != Some(drawn) || due.is_some() {
            *state = (Some(drawn), false);
            let at = due.unwrap_or_else(Instant::now);
            let time_s = at.saturating_duration_since(self.started).as_secs_f64();
            let _ = self.sender.send(Message::Render(frame(time_s)));
        }
        pending
    }

    /// Pass on input.
    pub fn input(&self, input: Input) {
        self.drawn.lock().unwrap().1 = true;
        let _ = self.sender.send(Message::Input(input));
    }
}

/// The dashboard's side of a plugin's calls, on the worker thread.
struct AppHost {
    ctx: egui::Context,
    latest: Arc<Mutex<Latest>>,
    audio: crate::sound::AudioOut,
    /// Its sound stream, once it has sent sound (a new one if the rate or
    /// channels change).
    stream: Option<crate::sound::AudioStream>,
}

impl AppHost {
    fn request(&self, request: Request) {
        self.latest.lock().unwrap().requests.push(request);
        self.ctx.request_repaint();
    }
}

impl Host for AppHost {
    fn publish(&mut self, key: &str, value: NtValue) -> Result<(), String> {
        if super::convert::from_plugin(&value).is_none() {
            return Err("structs can't be published yet".into());
        }
        self.request(Request::Publish(key.to_string(), value));
        Ok(())
    }

    fn measure_text(&self, text: &str, size: f32, max_width: Option<f32>, monospace: bool) -> Size {
        let font = if monospace {
            egui::FontId::monospace(size)
        } else {
            egui::FontId::proportional(size)
        };
        let wrap = max_width.unwrap_or(f32::INFINITY);
        let galley = self
            .ctx
            .fonts_mut(|f| f.layout(text.to_string(), font, egui::Color32::WHITE, wrap));
        Size {
            width: galley.size().x,
            height: galley.size().y,
        }
    }

    fn play_audio(&mut self, samples: &[i16], rate: u32, channels: u8) -> Result<(), String> {
        let fits = self
            .stream
            .as_ref()
            .is_some_and(|s| s.rate == rate && s.channels == channels);
        if !fits {
            self.stream = Some(self.audio.stream(rate, channels));
        }
        if let Some(stream) = &self.stream {
            stream.push(samples);
        }
        Ok(())
    }

    fn play_sound(&mut self, sound: &str) -> Result<(), String> {
        if crate::sound::Clip::named(sound).is_none() {
            return Err(format!("no sound {sound:?}"));
        }
        self.request(Request::Sound(sound.to_string()));
        Ok(())
    }

    fn log(&mut self, message: &str) {
        {
            let mut latest = self.latest.lock().unwrap();
            if latest.log.len() == LOG_LINES {
                latest.log.pop_front();
            }
            latest.log.push_back(message.to_string());
        }
        self.request(Request::Log(message.to_string()));
    }
}

/// When the next frame of an animation is due: `period` after the one just
/// drawn was due (`was_due`), keeping the rate, or `period` from `now` if it
/// wasn't drawn because its time came, or that's already gone by (a slow
/// plugin or a busy window skips frames rather than racing to catch up).
///
/// ```
/// use std::time::{Duration, Instant};
/// use raptaboard::plugins::worker::next_due;
/// let now = Instant::now();
/// let ms = Duration::from_millis;
/// // Drawn 5 ms late: the next is 28 ms after it was due, not after now.
/// assert_eq!(next_due(Some(now - ms(5)), ms(28), now), now + ms(23));
/// assert_eq!(next_due(None, ms(28), now), now + ms(28));
/// assert_eq!(next_due(Some(now - ms(40)), ms(28), now), now + ms(28));
/// ```
pub fn next_due(was_due: Option<Instant>, period: Duration, now: Instant) -> Instant {
    match was_due.map(|d| d + period) {
        Some(next) if next > now => next,
        _ => now + period,
    }
}

fn run(
    runtime: Arc<Runtime>,
    plugin: Arc<Plugin>,
    properties: HashMap<String, NtValue>,
    ctx: egui::Context,
    audio: crate::sound::AudioOut,
    receiver: Receiver<Message>,
    latest: Arc<Mutex<Latest>>,
) {
    let host = AppHost {
        ctx: ctx.clone(),
        latest: latest.clone(),
        audio,
        stream: None,
    };
    let stop = |stopped: Stopped, usage: Option<Usage>| {
        let mut latest = latest.lock().unwrap();
        latest.stopped = Some(stopped);
        latest.usage = usage.or(latest.usage);
        ctx.request_repaint();
    };
    let mut instance = match runtime.instantiate(
        &plugin,
        Box::new(host),
        properties,
        Limits::for_manifest(&plugin.manifest),
    ) {
        Ok(i) => i,
        Err(stopped) => return stop(stopped, None),
    };
    match instance.subscriptions() {
        Ok(s) => latest.lock().unwrap().subscriptions = Some(s),
        Err(stopped) => return stop(stopped, Some(instance.usage())),
    }
    ctx.request_repaint();
    // Each turn: everything that's waiting, in order, but only the newest
    // frame drawn, so a slow plugin skips frames instead of falling behind.
    while let Ok(first) = receiver.recv() {
        let mut frame = None;
        let mut updated = false;
        let mut result = Ok(());
        for message in std::iter::once(first).chain(receiver.try_iter()) {
            result = match message {
                Message::Update(samples) => {
                    updated = true;
                    instance.update(&samples)
                }
                Message::Input(input) => instance.on_input(&input),
                Message::Render(f) => {
                    frame = Some(f);
                    Ok(())
                }
            };
            if result.is_err() {
                break;
            }
        }
        if let Err(stopped) = result {
            return stop(stopped, Some(instance.usage()));
        }
        if updated {
            match instance.tab_mark() {
                Ok(mark) => latest.lock().unwrap().tab_mark = mark,
                Err(stopped) => return stop(stopped, Some(instance.usage())),
            }
        }
        if let Some(frame) = frame {
            match instance.render(frame) {
                Ok(picture) => {
                    let again = picture
                        .redraw_in_s
                        .filter(|s| s.is_finite() && *s >= 0.0)
                        .map(|s| Duration::from_secs_f64(s.min(60.0)));
                    {
                        let mut latest = latest.lock().unwrap();
                        latest.picture = Some(picture);
                        latest.picture_number += 1;
                        let now = Instant::now();
                        let was_due = latest.drawing_due.take();
                        latest.redraw_at = again.map(|d| next_due(was_due, d, now));
                        latest.usage = Some(instance.usage());
                    }
                    // Show it. The window wakes for the next one itself
                    // (`render_if_needed`): a delayed wake asked here, after
                    // this one, would be dropped.
                    ctx.request_repaint();
                }
                Err(stopped) => return stop(stopped, Some(instance.usage())),
            }
        } else {
            latest.lock().unwrap().usage = Some(instance.usage());
        }
    }
}
