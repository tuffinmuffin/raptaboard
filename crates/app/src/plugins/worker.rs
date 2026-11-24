//! One plugin widget running on its own thread, so the window never waits on
//! a plugin: the window sends it values, frames to draw and input, and draws
//! the newest picture it has finished.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use raptaboard_plugin_host::runtime::{
    FrameInfo, Host, Input, Level, Limits, NtValue, Picture, Plugin, Runtime, Sample, Size,
    Stopped, Subscription,
};

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
    /// The mark for the tab, from after the last update.
    pub tab_mark: Option<Level>,
    /// Why the plugin stopped, if it has.
    pub stopped: Option<Stopped>,
    /// What it asked for since the window last looked.
    pub requests: Vec<Request>,
    /// When the plugin asked to be drawn again (`redraw-in-s`).
    pub redraw_at: Option<Instant>,
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
    /// measuring text and asking for repaints when a picture is ready.
    pub fn start(
        runtime: Arc<Runtime>,
        plugin: Arc<Plugin>,
        properties: HashMap<String, NtValue>,
        ctx: egui::Context,
    ) -> Worker {
        let (sender, receiver) = mpsc::channel();
        let latest = Arc::new(Mutex::new(Latest::default()));
        let shared = latest.clone();
        let name = format!("plugin {}", plugin.manifest.name);
        let started = std::thread::Builder::new()
            .name(name)
            .spawn(move || run(runtime, plugin, properties, ctx, receiver, shared));
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
    /// to be drawn again has come. `frame` makes the frame's details.
    pub fn render_if_needed(&self, drawn: Drawn, frame: impl FnOnce(f64) -> FrameInfo) {
        let due = {
            let mut latest = self.latest.lock().unwrap();
            let due = latest.redraw_at.is_some_and(|at| at <= Instant::now());
            if due {
                latest.redraw_at = None;
            }
            due
        };
        let mut state = self.drawn.lock().unwrap();
        if state.1 || state.0 != Some(drawn) || due {
            *state = (Some(drawn), false);
            let time_s = self.started.elapsed().as_secs_f64();
            let _ = self.sender.send(Message::Render(frame(time_s)));
        }
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

    fn play_sound(&mut self, sound: &str) -> Result<(), String> {
        if crate::sound::Clip::named(sound).is_none() {
            return Err(format!("no sound {sound:?}"));
        }
        self.request(Request::Sound(sound.to_string()));
        Ok(())
    }

    fn log(&mut self, message: &str) {
        self.request(Request::Log(message.to_string()));
    }
}

fn run(
    runtime: Arc<Runtime>,
    plugin: Arc<Plugin>,
    properties: HashMap<String, NtValue>,
    ctx: egui::Context,
    receiver: Receiver<Message>,
    latest: Arc<Mutex<Latest>>,
) {
    let host = AppHost {
        ctx: ctx.clone(),
        latest: latest.clone(),
    };
    let stop = |stopped: Stopped| {
        latest.lock().unwrap().stopped = Some(stopped);
        ctx.request_repaint();
    };
    let mut instance =
        match runtime.instantiate(&plugin, Box::new(host), properties, Limits::default()) {
            Ok(i) => i,
            Err(stopped) => return stop(stopped),
        };
    match instance.subscriptions() {
        Ok(s) => latest.lock().unwrap().subscriptions = Some(s),
        Err(stopped) => return stop(stopped),
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
            return stop(stopped);
        }
        if updated {
            match instance.tab_mark() {
                Ok(mark) => latest.lock().unwrap().tab_mark = mark,
                Err(stopped) => return stop(stopped),
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
                        latest.redraw_at = again.map(|d| Instant::now() + d);
                    }
                    ctx.request_repaint();
                    if let Some(d) = again {
                        ctx.request_repaint_after(d);
                    }
                }
                Err(stopped) => return stop(stopped),
            }
        }
    }
}
