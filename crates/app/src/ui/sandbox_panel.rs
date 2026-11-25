//! The plugin sandbox's side panel (`raptaboard plugin dev`): the widget's
//! size, the theme, the connection and the lock; the plugin's overlay (its
//! regions outlined, the time each kind of call takes and its memory against
//! the limits, its log, Restart); and the values the sandbox's own server
//! serves, to edit, add and remove ([`crate::sandbox`]).

use eframe::egui;
use nt4::{NtType, Value};

use raptaboard_plugin_host::runtime::{CallTime, Usage};

use super::DashboardApp;
use crate::connection::AddressMode;
use crate::sandbox::{self, Sandbox};

/// A port nothing listens on, for "not connected".
const NOWHERE_PORT: u16 = 9;

/// The sandbox and the panel's own state.
pub(super) struct SandboxView {
    pub sandbox: Sandbox,
    /// The name being typed for a new topic.
    new_name: String,
    /// The type picked for it ([`sandbox::TYPES`]).
    new_type: usize,
    /// What went wrong last, if anything.
    problem: String,
    /// Whether the dashboard is connected to the sandbox's server.
    pub connected: bool,
}

impl DashboardApp {
    /// Show `sandbox`: its layout (one tab, the plugin's widget), connected
    /// to its own server if it has one, with its panel beside the grid.
    /// Nothing is saved: the layout isn't a file, and the settings aren't
    /// written (the caller starts the app with nowhere to save them).
    pub fn open_sandbox(&mut self, sandbox: Sandbox) {
        self.layout = sandbox.layout();
        self.file.saved = self.layout.clone();
        self.history = crate::edit::History::new(&self.layout);
        self.current_tab = 0;
        if let Some(port) = sandbox.port {
            self.address.mode = AddressMode::Custom;
            self.address.custom = "127.0.0.1".into();
            self.address.port = port;
        }
        self.sandbox = Some(SandboxView {
            sandbox,
            new_name: String::new(),
            new_type: 0,
            problem: String::new(),
            connected: true,
        });
    }

    /// The sandbox's panel, if the app is a sandbox.
    pub(super) fn sandbox_panel(&mut self, ui: &mut egui::Ui) {
        let Some(view) = &mut self.sandbox else {
            return;
        };
        egui::Panel::right("plugin_sandbox")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Plugin sandbox");
                    ui.label(egui::RichText::new(&view.sandbox.type_name).strong());
                    let server = match view.sandbox.port {
                        Some(port) => format!("Its own server: 127.0.0.1:{port}"),
                        None => "Showing the robot or sim (--address)".into(),
                    };
                    ui.label(egui::RichText::new(server).weak());
                    ui.label(
                        egui::RichText::new(format!("Bound to {}", view.sandbox.bound)).weak(),
                    );
                    ui.separator();
                    widget_section(ui, view, &mut self.layout, &mut self.locked);
                    if let Some(widget) = self
                        .layout
                        .tabs
                        .first()
                        .and_then(|t| t.grid_layout.containers.first())
                    {
                        ui.separator();
                        plugin_section(ui, widget);
                    }
                    if view.sandbox.serves() {
                        ui.separator();
                        values_section(ui, view);
                    }
                    if !view.problem.is_empty() {
                        ui.colored_label(ui.visuals().error_fg_color, &view.problem);
                    }
                });
            });
        // The connection switch: pointed away from the server and back.
        if let Some(port) = view.sandbox.port {
            let wanted = if view.connected { port } else { NOWHERE_PORT };
            if self.address.port != wanted {
                self.address.port = wanted;
            }
        }
        // The sandbox's layout isn't a file: nothing to save on closing.
        self.file.saved = self.layout.clone();
    }
}

/// The widget's size, the theme, the connection and the lock.
fn widget_section(
    ui: &mut egui::Ui,
    view: &mut SandboxView,
    layout: &mut raptaboard_layout::ElasticLayout,
    locked: &mut bool,
) {
    egui::Grid::new("sandbox_widget")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Size");
            let (w, h) = view.sandbox.cells;
            let shown = format!("{w} × {h} cells");
            egui::ComboBox::from_id_salt("sandbox_size")
                .selected_text(&shown)
                .show_ui(ui, |ui| {
                    for (cw, ch) in sandbox::SIZES {
                        ui.selectable_value(
                            &mut view.sandbox.cells,
                            (cw, ch),
                            format!("{cw} × {ch} cells"),
                        );
                    }
                })
                .response
                .widget_info(|| combo_info("Size", &shown));
            ui.end_row();
            ui.label("Theme");
            let mut dark = ui.visuals().dark_mode;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut dark, true, "Dark");
                ui.selectable_value(&mut dark, false, "Light");
            });
            if dark != ui.visuals().dark_mode {
                ui.ctx().set_theme(if dark {
                    egui::Theme::Dark
                } else {
                    egui::Theme::Light
                });
            }
            ui.end_row();
            ui.label("");
            ui.add_enabled(
                view.sandbox.port.is_some(),
                egui::Checkbox::new(&mut view.connected, "Connected"),
            )
            .on_disabled_hover_text("Only with the sandbox's own server");
            ui.end_row();
            ui.label("");
            ui.checkbox(locked, "Locked");
            ui.end_row();
        });
    // The size picked: the widget's rect, kept where it is.
    let grid = f64::from(layout.grid_size());
    if let Some(widget) = layout
        .tabs
        .first_mut()
        .and_then(|t| t.grid_layout.containers.first_mut())
    {
        let (w, h) = view.sandbox.cells;
        widget.rect.width = f64::from(w) * grid;
        widget.rect.height = f64::from(h) * grid;
    }
}

/// One kind of call's time for people: "0.42 ms, most 1.10, of 100".
pub(super) fn call_text(t: &CallTime, limit_ms: u64) -> String {
    if t.calls == 0 {
        return "not called yet".into();
    }
    format!("{:.2} ms, most {:.2}, of {limit_ms}", t.last_ms, t.max_ms)
}

/// Memory for people: "1.3 MiB of 64".
pub(super) fn memory_text(bytes: usize, limit: usize) -> String {
    let mib = |b: usize| b as f64 / (1 << 20) as f64;
    format!("{:.1} MiB of {:.0}", mib(bytes), mib(limit))
}

/// Whether `used` is over half of `limit`: shown in the warning color.
fn near(used: f64, limit: f64) -> bool {
    limit > 0.0 && used > limit / 2.0
}

/// The plugin's overlay: outline its regions, what it has used against its
/// limits, why it stopped, Restart, and its log.
fn plugin_section(ui: &mut egui::Ui, widget: &raptaboard_layout::WidgetContainer) {
    ui.strong("Plugin");
    let mut outline = crate::plugins::outline_regions(ui.ctx());
    if ui.checkbox(&mut outline, "Outline its regions").changed() {
        crate::plugins::set_outline_regions(ui.ctx(), outline);
    }
    let Some(dev) = crate::plugins::dev_view(ui.ctx(), widget) else {
        ui.label(egui::RichText::new("Not running").weak());
        return;
    };
    if let Some(why) = &dev.stopped {
        ui.colored_label(ui.visuals().error_fg_color, format!("Stopped: {why}"));
    }
    if ui
        .button("Restart")
        .on_hover_text("Start it again from scratch")
        .clicked()
    {
        crate::plugins::restart_widget(ui.ctx(), widget);
    }
    if let Some(u) = dev.usage {
        usage_grid(ui, &u);
    }
    ui.label("Log");
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .inner_margin(4.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("sandbox_log")
                .max_height(150.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if dev.log.is_empty() {
                        ui.label(egui::RichText::new("Nothing logged yet").weak().small());
                    }
                    for line in &dev.log {
                        ui.label(egui::RichText::new(line).monospace().small());
                    }
                });
        });
}

/// Each kind of call's time and the memory, against the limits.
fn usage_grid(ui: &mut egui::Ui, u: &Usage) {
    let warn = ui.visuals().warn_fg_color;
    let limit_ms = u.limits.call_ms;
    egui::Grid::new("sandbox_usage")
        .num_columns(2)
        .show(ui, |ui| {
            for (name, t) in [
                ("Render", &u.render),
                ("Update", &u.update),
                ("Input", &u.input),
                ("Tab mark", &u.tab_mark),
            ] {
                ui.label(name);
                let text = egui::RichText::new(call_text(t, limit_ms)).small();
                if near(t.max_ms, limit_ms as f64) {
                    ui.colored_label(warn, text);
                } else {
                    ui.label(text);
                }
                ui.end_row();
            }
            ui.label("Memory");
            let text =
                egui::RichText::new(memory_text(u.memory_bytes, u.limits.memory_bytes)).small();
            if near(u.memory_bytes as f64, u.limits.memory_bytes as f64) {
                ui.colored_label(warn, text);
            } else {
                ui.label(text);
            }
            ui.end_row();
        });
}

/// The values the sandbox serves: each with an editor for its type and a
/// remove button, then a row to add one.
fn values_section(ui: &mut egui::Ui, view: &mut SandboxView) {
    ui.strong("Values");
    let mut set: Vec<(String, Value)> = Vec::new();
    let mut remove: Option<String> = None;
    for topic in &mut view.sandbox.topics {
        let short = if topic.name == view.sandbox.bound {
            "the widget's topic".to_string()
        } else {
            topic
                .name
                .strip_prefix(&format!("{}/", view.sandbox.bound))
                .unwrap_or(&topic.name)
                .to_string()
        };
        ui.horizontal(|ui| {
            ui.label(&short)
                .on_hover_text(format!("{} ({})", topic.name, topic.ty));
            ui.label(egui::RichText::new(topic.ty.to_string()).weak().small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = ui.small_button("✕").on_hover_text("Stop serving it");
                button.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::accesskit::Role::Button,
                        true,
                        format!("Remove {short}"),
                    )
                });
                if button.clicked() {
                    remove = Some(topic.name.clone());
                }
            });
        });
        if let Some(v) = edit_value(ui, &topic.name, &topic.ty, &topic.value, &mut topic.text) {
            set.push((topic.name.clone(), v));
        }
        ui.add_space(4.0);
    }
    for (name, value) in set {
        if let Err(e) = view.sandbox.set(&name, value) {
            view.problem = e;
        }
    }
    if let Some(name) = remove {
        view.sandbox.remove(&name);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut view.new_name)
                .hint_text("new key")
                .desired_width(120.0),
        )
        .widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::TextInput, true, "New key")
        });
        let shown = sandbox::TYPES[view.new_type].to_string();
        egui::ComboBox::from_id_salt("sandbox_new_type")
            .selected_text(&shown)
            .show_ui(ui, |ui| {
                for (i, ty) in sandbox::TYPES.iter().enumerate() {
                    ui.selectable_value(&mut view.new_type, i, ty.to_string());
                }
            })
            .response
            .widget_info(|| combo_info("New key's type", &shown));
        if ui.button("Add").clicked() {
            match view
                .sandbox
                .add(&view.new_name, sandbox::TYPES[view.new_type].clone())
            {
                Ok(_) => {
                    view.new_name.clear();
                    view.problem.clear();
                }
                Err(e) => view.problem = e,
            }
        }
    });
    ui.label(
        egui::RichText::new("A key goes under the widget's table; start with / for anywhere.")
            .weak()
            .small(),
    );
}

/// A dropdown's name for screen readers and tests, and what it shows.
fn combo_info(name: &str, shown: &str) -> egui::WidgetInfo {
    let mut info = egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, name);
    info.current_text_value = Some(shown.to_owned());
    info
}

/// An editor for a value of type `ty`, named `name` for screen readers and
/// tests. Returns the new value when it's changed.
fn edit_value(
    ui: &mut egui::Ui,
    name: &str,
    ty: &NtType,
    value: &Value,
    text: &mut String,
) -> Option<Value> {
    match value {
        Value::Double(d) => {
            let mut d = *d;
            let r = ui.add(egui::DragValue::new(&mut d).speed(0.01));
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::SpinButton, true, name)
            });
            r.changed().then_some(Value::Double(d))
        }
        Value::Int(i) => {
            let mut i = *i;
            let r = ui.add(egui::DragValue::new(&mut i));
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::SpinButton, true, name)
            });
            r.changed().then_some(Value::Int(i))
        }
        Value::Boolean(b) => {
            let mut b = *b;
            let r = ui.checkbox(&mut b, "");
            r.widget_info(|| {
                egui::WidgetInfo::selected(egui::accesskit::Role::CheckBox, true, b, name)
            });
            r.changed().then_some(Value::Boolean(b))
        }
        _ => {
            let edit = if matches!(ty, NtType::StringArray) {
                egui::TextEdit::multiline(text).desired_rows(2)
            } else {
                egui::TextEdit::singleline(text)
            };
            let r = ui.add(edit.desired_width(f32::INFINITY).hint_text(match ty {
                NtType::StringArray => "one a line",
                NtType::String => "",
                _ => "1, 2, 3",
            }));
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::TextInput, true, name)
            });
            if !r.changed() {
                return None;
            }
            let parsed = sandbox::from_text(ty, text);
            parsed.filter(|v| v != value)
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn usage_reads_against_the_limits() {
        let t = CallTime {
            calls: 2,
            last_ms: 0.42,
            max_ms: 1.1,
        };
        assert_eq!(call_text(&t, 100), "0.42 ms, most 1.10, of 100");
        assert_eq!(call_text(&CallTime::default(), 100), "not called yet");
        assert_eq!(memory_text(3 << 19, 64 << 20), "1.5 MiB of 64");
        assert!(near(60.0, 100.0) && !near(40.0, 100.0));
    }
}
