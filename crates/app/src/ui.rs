//! The main window: settings bar, tab bar and the widget grid.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use raptaboard_layout::{ElasticLayout, Rect as LayoutRect};

use crate::connection::{AddressMode, resolve};

/// Top-level app state.
pub struct DashboardApp {
    layout: Option<ElasticLayout>,
    layout_path: String,
    status: String,
    current_tab: usize,
    team: u32,
    mode: AddressMode,
    custom_address: String,
}

impl DashboardApp {
    /// Create the app, loading a layout file if a path is given.
    pub fn new(layout_path: Option<String>) -> Self {
        let mut app = DashboardApp {
            layout: None,
            layout_path: layout_path.clone().unwrap_or_default(),
            status: "No layout loaded".into(),
            current_tab: 0,
            team: 3200,
            mode: AddressMode::TeamNumber,
            custom_address: String::new(),
        };
        if layout_path.is_some() {
            app.load_layout();
        }
        app
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
            let target = resolve(self.mode, self.team, &self.custom_address)
                .map(|host| {
                    nt4::proto::client_url(&host, nt4::proto::ROBOT_CODE_PORT, "raptaboard")
                })
                .unwrap_or_else(|| "waiting for Driver Station".into());
            ui.label(format!("Target: {target} (not connected yet)"));
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
        egui::CentralPanel::default().show(ui, |ui| self.grid(ui));
    }
}
