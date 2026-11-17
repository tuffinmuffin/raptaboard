//! The main window: settings bar, tab bar and the widget grid.

use std::collections::HashMap;
use std::time::Duration;

use eframe::egui::{self, Color32, Rect, Sense, Stroke, Vec2};
use nt4::{Topic, Value};
use raptaboard_layout::{ElasticLayout, Rect as LayoutRect};

use crate::cli::StartupOptions;
use crate::connection::{AddressMode, resolve};
use crate::nt::{ConnectionStatus, NtConnection};
use crate::tree::{Folder, TopicEntry};
use crate::widgets::{self, WidgetKind};
use nt4::proto::structs::StructDatabase;

/// Elastic's default widget update period, in seconds.
const DEFAULT_PERIOD_S: f64 = 0.06;
/// Space between List Layout children, in points.
const LIST_GAP: f32 = 6.0;

/// Top-level app state.
pub struct DashboardApp {
    layout: Option<ElasticLayout>,
    layout_path: String,
    status: String,
    current_tab: usize,
    team: u32,
    mode: AddressMode,
    custom_address: String,
    port: u16,
    nt: NtConnection,
    show_tree: bool,
    tree_filter: String,
    show_metadata: bool,
}

impl DashboardApp {
    /// Create the app from the startup options, loading the layout file if one is given.
    ///
    /// Fails only if the tokio runtime for the NT client cannot start.
    pub fn new(options: StartupOptions) -> std::io::Result<Self> {
        let layout_path = options.layout_path;
        let mut app = DashboardApp {
            layout: None,
            layout_path: layout_path.clone().unwrap_or_default(),
            status: "No layout loaded".into(),
            current_tab: 0,
            team: options.team,
            mode: options.mode,
            custom_address: options.custom_address,
            port: options.port,
            nt: NtConnection::new()?,
            show_tree: options.show_tree,
            tree_filter: String::new(),
            show_metadata: false,
        };
        if layout_path.is_some() {
            app.load_layout();
        }
        Ok(app)
    }

    fn load_layout(&mut self) {
        let result = std::fs::read_to_string(&self.layout_path)
            .map_err(|e| e.to_string())
            .and_then(|text| ElasticLayout::from_json(&text).map_err(|e| e.to_string()));
        match result {
            Ok(layout) => {
                self.status = format!(
                    "Loaded {} tabs, {} widgets",
                    layout.tabs.len(),
                    layout.widgets().count()
                );
                self.layout = Some(layout);
                self.current_tab = 0;
            }
            Err(e) => self.status = format!("Could not load layout: {e}"),
        }
    }

    fn settings_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("raptaboard");
            let (label, hint) = if self.show_tree {
                ("◀ Tree", "Hide the NetworkTables tree")
            } else {
                ("▶ Tree", "Show the NetworkTables tree")
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                self.show_tree = !self.show_tree;
            }
            ui.separator();
            ui.label("Team");
            ui.add(egui::DragValue::new(&mut self.team).range(1..=99999));
            egui::ComboBox::from_id_salt("address_mode")
                .selected_text(self.mode.label())
                .show_ui(ui, |ui| {
                    for mode in AddressMode::ALL {
                        ui.selectable_value(&mut self.mode, mode, mode.label());
                    }
                });
            if self.mode == AddressMode::Custom {
                ui.text_edit_singleline(&mut self.custom_address);
            }
            let host = resolve(self.mode, self.team, &self.custom_address);
            self.nt.set_target(host.as_deref(), self.port);
            ui.separator();
            status_label(ui, &self.nt.status());
        });
        ui.horizontal(|ui| {
            ui.label("Layout");
            ui.text_edit_singleline(&mut self.layout_path);
            if ui.button("Load").clicked() {
                self.load_layout();
            }
            ui.label(&self.status);
        });
    }

    /// The NetworkTables tree in the left panel. Read-only: values are
    /// changed through widgets, not here.
    ///
    /// Returns the topics drawn, whose values the tree needs.
    fn tree_panel(&mut self, ui: &mut egui::Ui, topics: &[Topic]) -> Vec<String> {
        ui.horizontal(|ui| {
            ui.heading("NetworkTables");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("◀")
                    .on_hover_text("Hide the tree")
                    .clicked()
                {
                    self.show_tree = false;
                }
            });
        });
        // A plain row: a right-to-left layout here would fill the rest of the
        // panel's height and push the tree out of view.
        ui.horizontal(|ui| {
            let clear_width = 48.0;
            let filter_width = ui.available_width() - clear_width - ui.spacing().item_spacing.x;
            ui.add(
                egui::TextEdit::singleline(&mut self.tree_filter)
                    .hint_text("Filter topics")
                    .desired_width(filter_width),
            );
            if ui
                .add_enabled(
                    !self.tree_filter.is_empty(),
                    egui::Button::new("Clear").min_size(egui::vec2(clear_width, 0.0)),
                )
                .clicked()
            {
                self.tree_filter.clear();
            }
        });
        ui.checkbox(&mut self.show_metadata, "Show metadata (.type, /.schema)");
        ui.separator();
        if topics.is_empty() {
            ui.weak(match self.nt.status() {
                ConnectionStatus::Connected { .. } => "No topics yet",
                _ => "Not connected",
            });
            return Vec::new();
        }
        let total = topics.len();
        let tree = Folder::build(
            topics.iter().cloned().map(TopicEntry::from),
            self.show_metadata,
            &self.tree_filter,
        );
        let filtering = !self.tree_filter.trim().is_empty();
        // Say when topics are hidden, so a forgotten filter isn't mistaken
        // for missing data.
        let shown = tree.topic_count();
        if shown < total {
            let why = if filtering {
                "filter and hidden metadata"
            } else {
                "hidden metadata"
            };
            let why = if filtering && self.show_metadata {
                "filter"
            } else {
                why
            };
            ui.weak(format!("Showing {shown} of {total} topics ({why})"));
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.nt.structs() {
                Some(structs) => tree.show(ui, "", filtering, structs),
                None => tree.show(ui, "", filtering, &StructDatabase::new()),
            })
            .inner
    }

    /// The topics the widgets on the open tab are bound to, and how often
    /// they want updates (the shortest `period` of them, in seconds).
    /// Only widgets raptaboard can draw are counted.
    fn tab_topics(&self) -> (Vec<String>, f64) {
        let Some(tab) = self
            .layout
            .as_ref()
            .and_then(|l| l.tabs.get(self.current_tab))
        else {
            return (Vec::new(), DEFAULT_PERIOD_S);
        };
        let grid = &tab.grid_layout;
        let drawn = grid
            .containers
            .iter()
            .chain(grid.layouts.iter().flat_map(|l| &l.children))
            .filter(|w| WidgetKind::from_type(&w.widget_type).is_some());
        let mut names = Vec::new();
        let mut period = f64::INFINITY;
        for w in drawn {
            if let Some(topic) = w.topic() {
                names.push(topic.to_owned());
                period = period.min(w.period().unwrap_or(DEFAULT_PERIOD_S));
            }
        }
        // A period of 0 or less would ask for every change; Elastic's floor is
        // a few milliseconds, so keep it sane.
        let period = if period.is_finite() {
            period.max(0.01)
        } else {
            DEFAULT_PERIOD_S
        };
        (names, period)
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let Some(layout) = &self.layout else { return };
        ui.horizontal(|ui| {
            for (i, tab) in layout.tabs.iter().enumerate() {
                if ui
                    .selectable_label(self.current_tab == i, &tab.name)
                    .clicked()
                {
                    self.current_tab = i;
                }
            }
        });
    }

    fn grid(&self, ui: &mut egui::Ui, values: &HashMap<&str, &Value>) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let origin = response.rect.min;
        let grid =
            self.layout
                .as_ref()
                .map_or(raptaboard_layout::DEFAULT_GRID_SIZE, |l| l.grid_size()) as f32;
        let visuals = ui.visuals();
        let line = Stroke::new(1.0, visuals.weak_text_color().gamma_multiply(0.3));

        let mut x = 0.0;
        while x <= response.rect.width() {
            painter.vline(origin.x + x, response.rect.y_range(), line);
            x += grid;
        }
        let mut y = 0.0;
        while y <= response.rect.height() {
            painter.hline(response.rect.x_range(), origin.y + y, line);
            y += grid;
        }

        let Some(tab) = self
            .layout
            .as_ref()
            .and_then(|l| l.tabs.get(self.current_tab))
        else {
            return;
        };
        let to_screen = |r: &LayoutRect| {
            Rect::from_min_size(
                origin + Vec2::new(r.x as f32, r.y as f32),
                Vec2::new(r.width as f32, r.height as f32),
            )
        };
        let value_of = |topic: Option<&str>| topic.and_then(|t| values.get(t).copied());

        for w in &tab.grid_layout.containers {
            let rect = to_screen(&w.rect).shrink(4.0);
            widgets::draw(&painter, visuals, rect, w, value_of(w.topic()));
        }
        // A List Layout stacks its children top to bottom, each at its own
        // height and the layout's full width; children that don't fit are cut off.
        for l in &tab.grid_layout.layouts {
            let rect = to_screen(&l.rect).shrink(4.0);
            let body = widgets::draw_card(&painter, visuals, rect, &l.title).shrink(LIST_GAP);
            let inside = painter.with_clip_rect(body.intersect(painter.clip_rect()));
            let mut top = body.min.y;
            for child in &l.children {
                let height = child.rect.height as f32;
                let child_rect = Rect::from_min_size(
                    egui::pos2(body.min.x, top),
                    Vec2::new(body.width(), height),
                );
                widgets::draw(&inside, visuals, child_rect, child, value_of(child.topic()));
                top += height + LIST_GAP;
            }
        }
    }
}

/// Show the connection state as a colored dot and a line of text.
fn status_label(ui: &mut egui::Ui, status: &ConnectionStatus) {
    let (color, text) = match status {
        ConnectionStatus::NoTarget => (Color32::GRAY, "Waiting for Driver Station".to_owned()),
        ConnectionStatus::Connecting { host, port } => (
            Color32::from_rgb(230, 160, 40),
            format!("Connecting to {host}:{port}..."),
        ),
        ConnectionStatus::Connected { host, port, topics } => (
            Color32::from_rgb(60, 180, 90),
            format!("Connected to {host}:{port}, {topics} topics"),
        ),
    };
    ui.label(egui::RichText::new("●").color(color));
    ui.label(text);
}

impl eframe::App for DashboardApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("settings").show(ui, |ui| {
            self.settings_bar(ui);
            self.tab_bar(ui);
        });
        // One copy of every topic per frame, shared by the tree and the grid.
        let topics = self.nt.topics();
        self.nt.update_structs(&topics);
        let values: HashMap<&str, &Value> = topics
            .iter()
            .filter_map(|t| Some((&*t.name, &t.value.as_ref()?.value)))
            .collect();
        // A hidden tree costs nothing: it isn't built and asks for no values.
        let tree_topics = if self.show_tree {
            egui::Panel::left("nt_tree")
                .resizable(true)
                .default_size(300.0)
                .show(ui, |ui| self.tree_panel(ui, &topics))
                .inner
        } else {
            Vec::new()
        };
        egui::CentralPanel::default().show(ui, |ui| self.grid(ui, &values));

        self.nt.watch_values(tree_topics);
        let (widget_topics, period_s) = self.tab_topics();
        let has_widgets = !widget_topics.is_empty();
        self.nt.watch_widget_values(widget_topics, period_s);

        // The client runs on its own thread, so egui doesn't know when values
        // change. Redraw at the widgets' period while widgets are showing, and
        // a few times a second otherwise so connection changes still show.
        let connected = matches!(self.nt.status(), ConnectionStatus::Connected { .. });
        let wait = if has_widgets && connected {
            Duration::from_secs_f64(period_s)
        } else {
            Duration::from_millis(250)
        };
        ui.ctx().request_repaint_after(wait);
    }
}
