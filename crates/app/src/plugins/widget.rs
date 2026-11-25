//! A plugin as a widget type: [`PluginType`] implements
//! [`WidgetType`](crate::widgets::WidgetType), so the grid, the settings
//! dialog and tab marks treat a plugin like a built-in widget.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Rect};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use raptaboard_plugin_host::PropertySpec;
use raptaboard_plugin_host::runtime::{
    FrameInfo, Input, Key, Modifiers, MouseButton, Point, PointerButton, Role, Sample, Size,
    Subscription,
};
use serde_json::{Map, Value as Json};

use super::worker::{Drawn, Request, Worker};
use super::{PluginType, convert, draw};
use crate::types::alerts::Level;
use crate::widgets::shared::{card, props, sound_pick};
use crate::widgets::{Effect, Live, Server, ValueInfo, WidgetType};

/// The topic a plugin's sample names (`""`, a key, or a full path), in full.
fn full_topic(widget: &WidgetContainer, topic: &str) -> Option<String> {
    if topic.starts_with('/') {
        return Some(topic.to_string());
    }
    let own = widget.topic()?;
    if topic.is_empty() {
        Some(own.to_string())
    } else {
        Some(format!("{}/{topic}", own.trim_end_matches('/')))
    }
}

/// The topics a widget's subscriptions need, in full, given the server.
fn needed(widget: &WidgetContainer, subs: &[Subscription], server: Server<'_>) -> Vec<String> {
    let mut topics = Vec::new();
    for sub in subs {
        match sub {
            Subscription::Bound => topics.extend(widget.topic().map(str::to_string)),
            Subscription::Key(k) => topics.extend(full_topic(widget, k)),
            Subscription::AllKeys => {
                if let Some(table) = widget.topic() {
                    let prefix = format!("{}/", table.trim_end_matches('/'));
                    topics.extend(
                        server
                            .topics
                            .iter()
                            .filter(|t| t.name.starts_with(&prefix))
                            .map(|t| t.name.to_string()),
                    );
                }
            }
            // Every table's topics: which tables have the `.type` is only
            // known from their values, so the choice is made when they're sent.
            Subscription::TablesOfType(_) => {
                for table in server.tables {
                    let prefix = format!("{}/", table.trim_end_matches('/'));
                    topics.extend(
                        server
                            .topics
                            .iter()
                            .filter(|t| t.name.starts_with(&prefix))
                            .map(|t| t.name.to_string()),
                    );
                }
            }
        }
    }
    topics.sort();
    topics.dedup();
    topics
}

/// The values a widget's subscriptions see now, as the plugin names them:
/// `""` for the bound topic, keys under the table, full paths for tables of
/// a type.
fn seen(
    widget: &WidgetContainer,
    subs: &[Subscription],
    own: Option<&Value>,
    others: &HashMap<&str, &Value>,
) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let table = widget.topic().map(|t| t.trim_end_matches('/').to_string());
    for sub in subs {
        match sub {
            Subscription::Bound => {
                if let Some(v) = own.or_else(|| widget.topic().and_then(|t| others.get(t).copied()))
                {
                    out.insert(String::new(), v.clone());
                }
            }
            Subscription::Key(k) => {
                if let Some(full) = full_topic(widget, k)
                    && let Some(v) = others.get(full.as_str())
                {
                    out.insert(k.clone(), (*v).clone());
                }
            }
            Subscription::AllKeys => {
                if let Some(table) = &table {
                    let prefix = format!("{table}/");
                    for (name, v) in others {
                        if let Some(key) = name.strip_prefix(&prefix) {
                            out.insert(key.to_string(), (*v).clone());
                        }
                    }
                }
            }
            Subscription::TablesOfType(ty) => {
                let tables: Vec<&str> = others
                    .iter()
                    .filter(|(_, v)| matches!(v, Value::String(t) if t == ty))
                    .filter_map(|(name, _)| name.strip_suffix("/.type"))
                    .collect();
                for (name, v) in others {
                    if tables.iter().any(|t| {
                        name.len() > t.len()
                            && name.starts_with(t)
                            && name.as_bytes()[t.len()] == b'/'
                    }) {
                        out.insert(name.to_string(), (*v).clone());
                    }
                }
            }
        }
    }
    out
}

/// Send `worker` what changed since the values it was last sent.
fn feed(worker: &Worker, now: HashMap<String, Value>, time_us: i64) {
    let mut sent = worker.sent.lock().unwrap();
    let mut samples = Vec::new();
    for (topic, value) in &now {
        if sent.get(topic) != Some(value) {
            samples.push(Sample {
                topic: topic.clone(),
                value: convert::to_plugin(value),
                time_us,
            });
        }
    }
    for topic in sent.keys() {
        if !now.contains_key(topic) {
            samples.push(Sample {
                topic: topic.clone(),
                value: None,
                time_us,
            });
        }
    }
    samples.sort_by(|a, b| a.topic.cmp(&b.topic));
    *sent = now;
    drop(sent);
    worker.update(samples);
}

/// A setting's name as a label: `decimal_places` → "Decimal places".
fn label(name: &str) -> String {
    let words = name.replace(['_', '-'], " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The id of the area a tab plugin fills on the tab named `tab_name`: it
/// has the keyboard while this has focus.
pub fn tab_focus_id(tab_name: &str) -> egui::Id {
    egui::Id::unique(("raptaboard plugin tab", tab_name.trim().to_lowercase()))
}

/// The keys that give the keyboard back from a tab plugin: Shift+Escape
/// (a game takes Escape alone for its menu).
pub const LEAVE_KEYS: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Escape);

/// How long the hint saying so shows after a tab plugin takes the keyboard.
const LEAVE_HINT_S: f64 = 3.0;

/// Which modifier keys are held, as the plugin interface's flags.
fn modifiers(m: egui::Modifiers) -> Modifiers {
    let mut out = Modifiers::empty();
    for (on, flag) in [
        (m.shift, Modifiers::SHIFT),
        (m.ctrl, Modifiers::CTRL),
        (m.alt, Modifiers::ALT),
        (m.mac_cmd, Modifiers::COMMAND),
    ] {
        if on {
            out |= flag;
        }
    }
    out
}

/// A tab plugin's input this frame from egui's `events`, for an area at
/// `rect` that has the keyboard if `focused`: keys down and up (not
/// repeats), typed text and modifier keys pressed or let go (as keys named
/// "Shift", "Ctrl", "Alt" and "Command": a game fires with Ctrl) while
/// focused; and the pointer moving, pressing and scrolling over it, in
/// pixels from its top left. `held` is the modifiers the plugin was told
/// are held, `now` those held now; losing the keyboard lets them all go.
/// While it `holds_mouse` (and is focused), the mouse's movement comes as
/// `mouse-motion` and every button as pressed or let go wherever it is (the
/// cursor is hidden and kept still).
pub fn tab_inputs(
    events: &[egui::Event],
    rect: Rect,
    focused: bool,
    holds_mouse: bool,
    held: egui::Modifiers,
    now: egui::Modifiers,
) -> Vec<Input> {
    let holds_mouse = holds_mouse && focused;
    let button = |b: &egui::PointerButton| match b {
        egui::PointerButton::Primary => MouseButton::Primary,
        egui::PointerButton::Secondary => MouseButton::Secondary,
        egui::PointerButton::Middle => MouseButton::Middle,
        egui::PointerButton::Extra1 => MouseButton::Extra1,
        egui::PointerButton::Extra2 => MouseButton::Extra2,
    };
    let local = |p: egui::Pos2| Point {
        x: p.x - rect.min.x,
        y: p.y - rect.min.y,
    };
    let mut out = Vec::new();
    // Without the keyboard nothing is held, as far as the plugin knows.
    let now = if focused { now } else { egui::Modifiers::NONE };
    {
        for (name, was, is) in [
            ("Shift", held.shift, now.shift),
            ("Ctrl", held.ctrl, now.ctrl),
            ("Alt", held.alt, now.alt),
            ("Command", held.mac_cmd, now.mac_cmd),
        ] {
            let key = Key {
                name: name.into(),
                modifiers: modifiers(now),
            };
            match (was, is) {
                (false, true) => out.push(Input::KeyDown(key)),
                (true, false) => out.push(Input::KeyUp(key)),
                _ => {}
            }
        }
    }
    for event in events {
        match event {
            egui::Event::Key {
                key,
                pressed,
                repeat: false,
                modifiers: m,
                ..
            } if focused => {
                let key = Key {
                    name: key.name().into(),
                    modifiers: modifiers(*m),
                };
                out.push(if *pressed {
                    Input::KeyDown(key)
                } else {
                    Input::KeyUp(key)
                });
            }
            egui::Event::Text(text) if focused => out.push(Input::Text(text.clone())),
            egui::Event::MouseMoved(delta) if holds_mouse => {
                out.push(Input::MouseMotion(Point {
                    x: delta.x,
                    y: delta.y,
                }));
            }
            egui::Event::PointerMoved(p) if !holds_mouse && rect.contains(*p) => {
                out.push(Input::PointerMove(local(*p)));
            }
            egui::Event::PointerButton {
                pos,
                button: b,
                pressed,
                ..
            } if holds_mouse || rect.contains(*pos) => {
                let at = PointerButton {
                    pos: local(*pos),
                    button: button(b),
                };
                out.push(if *pressed {
                    Input::PointerDown(at)
                } else {
                    Input::PointerUp(at)
                });
            }
            egui::Event::MouseWheel { delta, .. } if focused => {
                out.push(Input::Wheel(Point {
                    x: delta.x,
                    y: delta.y,
                }));
            }
            _ => {}
        }
    }
    out
}

/// Whether the tab plugin on the tab named `tab_name` holds the mouse now
/// in the window `ctx` (for tests).
#[cfg(test)]
pub(crate) fn holds_mouse(ctx: &egui::Context, tab_name: &str) -> bool {
    ctx.data(|d| d.get_temp(tab_focus_id(tab_name).with("holds mouse")))
        .unwrap_or(false)
}

impl PluginType {
    /// Show a tab plugin filling `rect` (the tab, `widget` its stand-in
    /// from [`super::tab_widget`]): clicked, it takes the keyboard, keeping
    /// Tab, the arrows and Escape, and gets keys, text, the pointer and
    /// focus changes as input.
    pub fn show_tab(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) {
        let id = tab_focus_id(&widget.title);
        let response = ui.interact(rect, id, egui::Sense::click());
        if response.clicked() {
            response.request_focus();
        }
        let mut focused = response.has_focus();
        // Shift+Escape gives the keyboard (and mouse) back, and isn't the
        // plugin's; so does the plugin asking (`release-focus`: a game's Quit).
        let asked = self
            .worker(widget, ui.ctx())
            .is_some_and(|w| std::mem::take(&mut w.latest.lock().unwrap().release_focus));
        if focused && (asked || ui.input_mut(|i| i.consume_shortcut(&LEAVE_KEYS))) {
            ui.memory_mut(|m| m.surrender_focus(id));
            focused = false;
        }
        // Holding the mouse while it has the keyboard. While the window is in
        // the background (another app), egui counts no widget focused, so the
        // mouse is let go, and taken again when the window comes back.
        let capture = self.loaded().plugin.manifest.capture_mouse;
        let holds_mouse = focused && capture;
        let holding = id.with("holds mouse");
        let was_holding: bool = ui.data(|d| d.get_temp(holding)).unwrap_or(false);
        if holds_mouse != was_holding {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CursorGrab(if holds_mouse {
                    egui::CursorGrab::Locked
                } else {
                    egui::CursorGrab::None
                }));
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CursorVisible(!holds_mouse));
            ui.data_mut(|d| d.insert_temp(holding, holds_mouse));
        }
        if focused {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
        }
        if let Some(worker) = self.worker(widget, ui.ctx()) {
            let state = id.with("input state");
            let (was_focused, held): (bool, egui::Modifiers) =
                ui.data(|d| d.get_temp(state)).unwrap_or_default();
            let (events, now) = ui.input(|i| (i.events.clone(), i.modifiers));
            if focused != was_focused {
                worker.input(Input::Focus(focused));
            }
            // Just took the keyboard: say how to give it back, for a while.
            let gained = id.with("gained");
            let time = ui.input(|i| i.time);
            if focused && !was_focused {
                ui.data_mut(|d| d.insert_temp(gained, time));
            }
            let since: Option<f64> = ui.data(|d| d.get_temp(gained));
            if focused && since.is_some_and(|t| time - t < LEAVE_HINT_S) {
                let hint = format!(
                    "{} gives the {} back",
                    ui.ctx().format_shortcut(&LEAVE_KEYS),
                    if capture {
                        "keyboard and mouse"
                    } else {
                        "keyboard"
                    }
                );
                // Drawn over the plugin's picture, after it.
                let layer = egui::LayerId::new(egui::Order::Foreground, id.with("hint"));
                let over = ui.ctx().layer_painter(layer).with_clip_rect(rect);
                let at = rect.right_top() + egui::vec2(-8.0, 8.0);
                let galley = over.layout_no_wrap(
                    hint,
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
                let back =
                    egui::Rect::from_min_size(at - egui::vec2(galley.size().x, 0.0), galley.size())
                        .expand(4.0);
                // Asked again until the hint's time is up, so it goes.
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
                over.rect_filled(back, 4.0, egui::Color32::from_black_alpha(180));
                over.galley(
                    back.min + egui::vec2(4.0, 4.0),
                    galley,
                    egui::Color32::WHITE,
                );
            }
            for input in tab_inputs(&events, rect, focused, holds_mouse, held, now) {
                worker.input(input);
            }
            ui.data_mut(|d| {
                d.insert_temp(
                    state,
                    (focused, if focused { now } else { egui::Modifiers::NONE }),
                )
            });
        }
        let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        inside.set_clip_rect(rect.intersect(ui.clip_rect()));
        self.show_body(&mut inside, rect, widget, live);
    }

    /// Carry out what the plugin asked for since the last frame.
    fn carry_out(&self, worker: &Worker, widget: &WidgetContainer, live: &Live<'_>) {
        let requests = std::mem::take(&mut worker.latest.lock().unwrap().requests);
        for request in requests {
            match request {
                Request::Sound(s) => {
                    if let Some(clip) = crate::sound::Clip::named(&s) {
                        live.frame.effects.borrow_mut().push(Effect::Sound(clip));
                    }
                }
                Request::Publish(key, value) => {
                    if let (Some(topic), Some((value, ty))) =
                        (full_topic(widget, &key), convert::from_plugin(&value))
                    {
                        live.frame
                            .effects
                            .borrow_mut()
                            .push(Effect::Write { topic, value, ty });
                    }
                }
                Request::Log(line) => eprintln!("plugin {}: {line}", self.name),
            }
        }
    }

    fn worker(&self, widget: &WidgetContainer, ctx: &egui::Context) -> Option<Arc<Worker>> {
        super::worker_for(self, widget, Some(ctx))
    }
}

impl WidgetType for PluginType {
    fn names(&self) -> &'static [&'static str] {
        self.names
    }

    /// A plugin is never the default for a dropped topic: built-in widgets
    /// stay that, and plugins are picked with Add as or the type choice.
    fn accepts(&self, _nt_type: &str) -> bool {
        false
    }

    fn shows(&self, nt_type: &str) -> bool {
        self.loaded().plugin.manifest.matches_topic(nt_type)
    }

    fn table_types(&self) -> &'static [&'static str] {
        self.loaded().table_types
    }

    fn extra_topics(&self, p: &Map<String, Json>, server: Server<'_>) -> Vec<String> {
        let widget = for_props(self.names[0], p);
        match super::worker_for(self, &widget, None) {
            Some(w) => match &w.latest.lock().unwrap().subscriptions {
                Some(subs) => needed(&widget, subs, server),
                None => Vec::new(),
            },
            None => Vec::new(),
        }
    }

    fn background_topics(&self, p: &Map<String, Json>, server: Server<'_>) -> Vec<String> {
        if self.loaded().plugin.manifest.permissions.background {
            self.extra_topics(p, server)
        } else {
            Vec::new()
        }
    }

    /// The plugin's mark. With `background` it keeps getting values while its
    /// tab is closed, so the mark stays current.
    fn tab_mark(&self, widget: &WidgetContainer, values: &HashMap<&str, &Value>) -> Option<Level> {
        let background = self.loaded().plugin.manifest.permissions.background;
        let worker = super::worker_for(self, widget, None)?;
        if background {
            let subs = worker.latest.lock().unwrap().subscriptions.clone();
            if let Some(subs) = subs {
                feed(&worker, seen(widget, &subs, None, values), 0);
            }
        }
        let mark = worker.latest.lock().unwrap().tab_mark;
        mark.map(convert::level)
    }

    fn show(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let body = card::draw_card(ui.painter(), ui.visuals(), rect, &widget.title);
        let clip = body.intersect(ui.clip_rect());
        let mut inside = ui.new_child(egui::UiBuilder::new().max_rect(body));
        inside.set_clip_rect(clip);
        self.show_body(&mut inside, body.shrink(card::BODY_MARGIN), widget, live)
    }

    fn show_body(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        widget: &WidgetContainer,
        live: &Live<'_>,
    ) -> Option<Value> {
        let Some(worker) = self.worker(widget, ui.ctx()) else {
            card::draw_message(ui.painter(), ui.visuals(), rect, "Plugin not loaded");
            return None;
        };
        self.carry_out(&worker, widget, live);
        let (subs, stopped) = {
            let latest = worker.latest.lock().unwrap();
            (latest.subscriptions.clone(), latest.stopped.clone())
        };
        if let Some(stopped) = stopped {
            let text = format!("Plugin stopped: {}", stopped.reason);
            card::draw_message(ui.painter(), ui.visuals(), rect, &text);
            let button = Rect::from_center_size(
                rect.center_bottom() - egui::vec2(0.0, 14.0),
                egui::vec2(70.0, 22.0),
            );
            if ui.put(button, egui::Button::new("Restart")).clicked() {
                super::restart(self, widget, ui.ctx());
                ui.ctx().request_repaint();
            }
            return None;
        }
        let subs = subs?;
        feed(
            &worker,
            seen(widget, &subs, live.value, live.frame.others),
            live.frame.now_us,
        );
        let visuals = ui.visuals().clone();
        let drawn = Drawn {
            size: (rect.width(), rect.height()),
            connected: live.frame.connected,
            locked: live.place.locked,
            dark: visuals.dark_mode,
        };
        let next = worker.render_if_needed(drawn, |time_s| FrameInfo {
            size: Size {
                width: rect.width(),
                height: rect.height(),
            },
            time_s,
            connected: live.frame.connected,
            locked: live.place.locked,
            theme: convert::theme(&visuals),
        });
        // An animation: wake when its next frame is due.
        if let Some(at) = next {
            ui.ctx()
                .request_repaint_after(at.saturating_duration_since(std::time::Instant::now()));
        }

        let latest = worker.latest.lock().unwrap();
        let Some(picture) = &latest.picture else {
            return None;
        };
        let pixels = (ui.scope_id().with("plugin pixels"), latest.picture_number);
        draw::shapes(ui.painter(), rect.min, &picture.shapes, pixels);
        let summary = picture.summary.clone();
        let regions = picture.regions.clone();
        drop(latest);

        if let Some(summary) = summary {
            let text = format!("{}: {summary}", widget.title);
            ui.interact(
                rect,
                ui.scope_id().with("plugin summary"),
                egui::Sense::hover(),
            )
            .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, &text));
        }
        let outline = super::outline_regions(ui.ctx());
        for region in regions {
            let area = draw::region_rect(rect.min, &region.min, &region.max);
            if outline {
                draw::outline(ui.painter(), area, &region.label);
            }
            let response = ui.interact(area, ui.scope_id().with(&region.id), egui::Sense::click());
            let role = match region.role {
                Role::Button => egui::accesskit::Role::Button,
                Role::Toggle => egui::accesskit::Role::CheckBox,
                Role::Label => egui::accesskit::Role::Label,
            };
            let checked = region.checked;
            response.widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(role, true, &region.label);
                info.selected = checked;
                info
            });
            if response.clicked() {
                worker.input(Input::Activate(region.id.clone()));
                ui.ctx().request_repaint();
            }
        }
        None
    }

    /// Not used: plugins draw through [`show`](Self::show).
    fn draw_body(
        &self,
        painter: &egui::Painter,
        visuals: &egui::Visuals,
        rect: Rect,
        _props: &Map<String, Json>,
        _value: Option<&Value>,
    ) {
        card::draw_message(painter, visuals, rect, &self.name);
    }

    fn settings(&self, ui: &mut egui::Ui, p: &mut Map<String, Json>, _value: ValueInfo<'_>) {
        let loaded = self.loaded();
        for (name, spec) in &loaded.plugin.manifest.properties {
            let text = label(name);
            match spec {
                PropertySpec::Number { default } => {
                    props::number_row(ui, p, &text, name, *default, f64::MIN..=f64::MAX)
                }
                PropertySpec::Bool { default } => props::flag_row(ui, p, &text, "", name, *default),
                PropertySpec::Enum { default, options } => {
                    let choices: Vec<&str> = options.iter().map(String::as_str).collect();
                    props::choice_row(ui, p, &text, name, default, &choices);
                }
                PropertySpec::Color { default } => {
                    let fallback = props::color_property(
                        &[(name.clone(), Json::from(default.as_str()))]
                            .into_iter()
                            .collect(),
                        name,
                    )
                    .unwrap_or(egui::Color32::GRAY);
                    props::color_row(ui, p, &text, name, fallback);
                }
                PropertySpec::String { default } => {
                    ui.label(&text);
                    let mut value = props::text(p, name, default).to_owned();
                    if ui.text_edit_singleline(&mut value).changed() {
                        props::set_property(p, name, (value != *default).then(|| value.into()));
                    }
                    ui.end_row();
                }
                PropertySpec::Sound { default } => {
                    ui.label(&text);
                    let layers = sound_pick::installed(ui);
                    let current = props::text(p, name, default).to_owned();
                    let mut picked = current.clone();
                    let (shown, found) = crate::sound::sound_label(&current, &layers);
                    ui.horizontal(|ui| {
                        sound_pick::sound_combo(ui, name, &text, &shown, |ui| {
                            sound_pick::sound_menu(ui, &mut picked, &layers)
                        });
                        if !found {
                            sound_pick::missing_note(ui, &current);
                        }
                    });
                    ui.end_row();
                    if picked != current {
                        props::set_property(p, name, (picked != *default).then(|| picked.into()));
                    }
                }
            }
        }
    }

    fn setting_keys(&self) -> &'static [&'static str] {
        self.loaded().setting_keys
    }
}

/// A stand-in widget with these settings, for the trait's calls that get only
/// a widget's settings (its topic is in them).
fn for_props(type_name: &str, p: &Map<String, Json>) -> WidgetContainer {
    WidgetContainer {
        title: String::new(),
        rect: raptaboard_layout::Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        },
        widget_type: type_name.to_string(),
        properties: p.clone(),
        extra: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn widget(topic: &str) -> WidgetContainer {
        let mut p = Map::new();
        p.insert("topic".into(), topic.into());
        for_props("plugin:x", &p)
    }

    #[test]
    fn topics_are_named_as_the_plugin_names_them() {
        let w = widget("/Arm/");
        assert_eq!(full_topic(&w, "").as_deref(), Some("/Arm/"));
        assert_eq!(full_topic(&w, "angle").as_deref(), Some("/Arm/angle"));
        assert_eq!(full_topic(&w, "/Other/x").as_deref(), Some("/Other/x"));
    }

    #[test]
    fn tables_of_a_type_are_picked_by_their_type_value() {
        let w = widget("/A");
        let alerts = Value::String("Alerts".into());
        let other = Value::String("Arm".into());
        let errors = Value::StringArray(vec!["bad".into()]);
        let others: HashMap<&str, &Value> = [
            ("/A/.type", &alerts),
            ("/A/errors", &errors),
            ("/AB/errors", &errors),
            ("/B/.type", &other),
            ("/B/errors", &errors),
        ]
        .into_iter()
        .collect();
        let got = seen(
            &w,
            &[Subscription::TablesOfType("Alerts".into())],
            None,
            &others,
        );
        let mut keys: Vec<&str> = got.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["/A/.type", "/A/errors"]);
    }

    #[test]
    fn a_tab_plugin_gets_keys_while_focused_and_the_pointer_over_it() {
        let rect = Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 100.0));
        let key = |k, pressed, repeat| egui::Event::Key {
            key: k,
            physical_key: None,
            pressed,
            repeat,
            modifiers: egui::Modifiers::NONE,
        };
        let events = [
            key(egui::Key::W, true, false),
            key(egui::Key::W, true, true),
            key(egui::Key::W, false, false),
            egui::Event::Text("w".into()),
            egui::Event::PointerMoved(egui::pos2(110.0, 60.0)),
            egui::Event::PointerMoved(egui::pos2(10.0, 10.0)),
        ];
        let none = egui::Modifiers::NONE;
        let got = tab_inputs(&events, rect, true, false, none, egui::Modifiers::CTRL);
        let names: Vec<String> = got
            .iter()
            .map(|i| match i {
                Input::KeyDown(k) => format!("down {}", k.name),
                Input::KeyUp(k) => format!("up {}", k.name),
                Input::Text(t) => format!("text {t}"),
                Input::PointerMove(p) => format!("move {},{}", p.x, p.y),
                other => format!("{other:?}"),
            })
            .collect();
        // Ctrl pressed since last frame is a key; a repeat isn't; the
        // pointer is in the area's own pixels, and only over it.
        assert_eq!(
            names,
            ["down Ctrl", "down W", "up W", "text w", "move 10,10"]
        );
        // Not focused: the pointer only.
        let got = tab_inputs(&events, rect, false, false, none, egui::Modifiers::CTRL);
        assert!(matches!(got.as_slice(), [Input::PointerMove(_)]));
        // Ctrl let go.
        let got = tab_inputs(&[], rect, true, false, egui::Modifiers::CTRL, none);
        assert!(matches!(got.as_slice(), [Input::KeyUp(k)] if k.name == "Ctrl"));
        // The keyboard lost with Ctrl still held: let go, as far as the
        // plugin knows.
        let got = tab_inputs(
            &[],
            rect,
            false,
            false,
            egui::Modifiers::CTRL,
            egui::Modifiers::CTRL,
        );
        assert!(matches!(got.as_slice(), [Input::KeyUp(k)] if k.name == "Ctrl"));
    }

    #[test]
    fn a_tab_plugin_holding_the_mouse_gets_its_motion_and_every_button() {
        let rect = Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(200.0, 100.0));
        let events = [
            egui::Event::MouseMoved(egui::vec2(3.0, -2.0)),
            egui::Event::PointerMoved(egui::pos2(110.0, 60.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(5.0, 5.0),
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        let none = egui::Modifiers::NONE;
        // Holding it: the motion, and a button even off the area; no pointer
        // positions (the cursor is held still).
        let got = tab_inputs(&events, rect, true, true, none, none);
        assert!(
            matches!(
                got.as_slice(),
                [
                    Input::MouseMotion(Point { x: 3.0, y: -2.0 }),
                    Input::PointerDown(PointerButton {
                        button: MouseButton::Secondary,
                        ..
                    })
                ]
            ),
            "{got:?}"
        );
        // Not holding it: no motion, the pointer over the area only.
        let got = tab_inputs(&events, rect, true, false, none, none);
        assert!(matches!(got.as_slice(), [Input::PointerMove(_)]), "{got:?}");
        // Holding it needs the keyboard too.
        let got = tab_inputs(&events, rect, false, true, none, none);
        assert!(matches!(got.as_slice(), [Input::PointerMove(_)]), "{got:?}");
    }

    #[test]
    fn labels_come_from_setting_names() {
        assert_eq!(label("decimal_places"), "Decimal places");
        assert_eq!(label("x"), "X");
    }
}
