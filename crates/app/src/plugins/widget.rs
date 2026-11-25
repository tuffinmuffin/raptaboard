//! A plugin as a widget type: [`PluginType`] implements
//! [`WidgetType`](crate::widgets::WidgetType), so the grid, the settings
//! dialog and tab marks treat a plugin like a built-in widget.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Rect};
use nt4::Value;
use raptaboard_layout::WidgetContainer;
use raptaboard_plugin_host::PropertySpec;
use raptaboard_plugin_host::runtime::{FrameInfo, Input, Role, Sample, Size, Subscription};
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

impl PluginType {
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
        worker.render_if_needed(drawn, |time_s| FrameInfo {
            size: Size {
                width: rect.width(),
                height: rect.height(),
            },
            time_s,
            connected: live.frame.connected,
            locked: live.place.locked,
            theme: convert::theme(&visuals),
        });

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
    fn labels_come_from_setting_names() {
        assert_eq!(label("decimal_places"), "Decimal places");
        assert_eq!(label("x"), "X");
    }
}
