//! The main window: settings bar, tab bar and the widget grid.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use raptaboard_layout::{ElasticLayout, Rect as LayoutRect};

use crate::cli::StartupOptions;
use crate::connection::{AddressMode, resolve};
use crate::nt::{ConnectionStatus, NtConnection};
use crate::tree::{Folder, TopicEntry};
use nt4::proto::structs::StructDatabase;

/// Top-level app state.
pub struct DashboardApp {
    layout: Option<ElasticLayout>,
    layout_path: String,
    status: String,
    current_tab: usize,
    team: u32,
    mode: AddressMode,
    custom_address: String,
    nt: NtConnection,
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
            nt: NtConnection::new()?,
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
            self.nt
                .set_target(host.as_deref(), nt4::proto::ROBOT_CODE_PORT);
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
    fn tree_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("NetworkTables");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(!self.tree_filter.is_empty(), egui::Button::new("Clear"))
                .clicked()
            {
                self.tree_filter.clear();
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.tree_filter)
                    .hint_text("Filter topics")
                    .desired_width(f32::INFINITY),
            );
        });
        ui.checkbox(&mut self.show_metadata, "Show metadata (.type, /.schema)");
        ui.separator();
        let topics = self.nt.topics();
        if topics.is_empty() {
            ui.weak(match self.nt.status() {
                ConnectionStatus::Connected { .. } => "No topics yet",
                _ => "Not connected",
            });
            self.nt.watch_values(Vec::new());
            return;
        }
        self.nt.update_structs(&topics);
        let total = topics.len();
        let tree = Folder::build(
            topics.into_iter().map(TopicEntry::from),
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
        let visible = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.nt.structs() {
                Some(structs) => tree.show(ui, "", filtering, structs),
                None => tree.show(ui, "", filtering, &StructDatabase::new()),
            })
            .inner;
        self.nt.watch_values(visible);
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

    fn grid(&self, ui: &mut egui::Ui) {
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
        let fill = visuals.extreme_bg_color;
        let text = visuals.text_color();
        let accent = visuals.selection.bg_fill;

        for w in &tab.grid_layout.containers {
            let rect = to_screen(&w.rect).shrink(4.0);
            painter.rect(
                rect,
                CornerRadius::same(12),
                fill,
                Stroke::new(1.0, accent),
                StrokeKind::Inside,
            );
            card_text(&painter, rect, &w.title, &w.widget_type, w.topic(), text);
        }
        for l in &tab.grid_layout.layouts {
            let rect = to_screen(&l.rect).shrink(4.0);
            painter.rect(
                rect,
                CornerRadius::same(12),
                Color32::TRANSPARENT,
                Stroke::new(1.5, accent),
                StrokeKind::Inside,
            );
            let detail = format!("{} widgets", l.children.len());
            card_text(
                &painter,
                rect,
                &l.title,
                &l.layout_type,
                Some(&detail),
                text,
            );
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

fn card_text(
    painter: &egui::Painter,
    rect: Rect,
    title: &str,
    kind: &str,
    detail: Option<&str>,
    color: Color32,
) {
    let left = rect.min.x + 10.0;
    painter.text(
        Pos2::new(left, rect.min.y + 8.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(15.0),
        color,
    );
    painter.text(
        Pos2::new(left, rect.min.y + 28.0),
        egui::Align2::LEFT_TOP,
        kind,
        egui::FontId::proportional(12.0),
        color.gamma_multiply(0.7),
    );
    if let Some(detail) = detail {
        painter.text(
            Pos2::new(left, rect.min.y + 44.0),
            egui::Align2::LEFT_TOP,
            detail,
            egui::FontId::monospace(11.0),
            color.gamma_multiply(0.6),
        );
    }
}

impl eframe::App for DashboardApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("settings").show(ui, |ui| {
            self.settings_bar(ui);
            self.tab_bar(ui);
        });
        egui::Panel::left("nt_tree")
            .resizable(true)
            .default_size(300.0)
            .show(ui, |ui| self.tree_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.grid(ui));
        // The client runs on its own thread; redraw a few times a second so
        // connection changes show without waiting for mouse movement.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));
    }
}
