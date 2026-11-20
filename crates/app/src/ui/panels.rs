//! The settings bar (connection, lock, file) and the tab bar.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Sense};

use crate::connection::{AddressMode, resolve};
use crate::edit::EditAction;
use crate::layout_file::{self, FileAction};
use crate::nt::ConnectionStatus;
use crate::tabs::{self, TabAction};
use crate::types::alerts::Level;
use crate::widgets::builtin::alerts as alerts_widget;

use super::DashboardApp;

/// Show the connection state as a colored dot and a line of text.
/// `no_team` says the address is by team number but none is set.
pub(super) fn status_label(ui: &mut egui::Ui, status: &ConnectionStatus, no_team: bool) {
    let (color, text) = match status {
        ConnectionStatus::NoTarget if no_team => (Color32::GRAY, "Set a team number".to_owned()),
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

impl DashboardApp {
    pub(super) fn settings_bar(&mut self, ui: &mut egui::Ui) {
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
            let (label, hint) = if self.locked {
                (
                    "🔒 Locked",
                    "Unlock to move, resize and edit widgets and tabs",
                )
            } else {
                (
                    "🔓 Unlocked",
                    "Lock the layout so nothing moves by accident",
                )
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                self.locked = !self.locked;
                self.drag = None;
                self.child_drag = None;
                self.renaming = None;
                self.selected = None;
            }
            let (label, hint) = if self.alert_sound {
                (
                    "🔔",
                    "A tone plays when a value can't be sent. Click to turn it off.",
                )
            } else {
                (
                    "🔕",
                    "No tone when a value can't be sent. Click to turn it on.",
                )
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                self.alert_sound = !self.alert_sound;
            }
            self.alert_history_button(ui);
            ui.separator();
            ui.label("Team");
            // 0 is "no team yet", shown blank.
            ui.add(
                egui::DragValue::new(&mut self.team)
                    .range(0..=99999)
                    .custom_formatter(|n, _| {
                        if n == 0.0 {
                            String::new()
                        } else {
                            n.to_string()
                        }
                    })
                    .custom_parser(|text| {
                        let text = text.trim();
                        if text.is_empty() {
                            Some(0.0)
                        } else {
                            text.parse().ok()
                        }
                    })
                    .min_decimals(0)
                    .max_decimals(0),
            )
            .on_hover_text("Your FRC team number, for finding the robot at 10.TE.AM.2");
            // Named for screen readers; on screen the choices say what it is.
            egui::ComboBox::from_id_salt("address_mode")
                .selected_text(self.mode.label())
                .show_ui(ui, |ui| {
                    for mode in AddressMode::ALL {
                        ui.selectable_value(&mut self.mode, mode, mode.label());
                    }
                })
                .response
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::accesskit::Role::ComboBox,
                        true,
                        "Robot address",
                    )
                });
            if self.mode == AddressMode::Custom {
                ui.add(
                    egui::TextEdit::singleline(&mut self.custom_address)
                        .hint_text("Host name or IP address"),
                )
                .widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::accesskit::Role::TextInput,
                        true,
                        "Server address",
                    )
                });
            }
            let host = resolve(self.mode, self.team, &self.custom_address);
            self.nt.set_target(host.as_deref(), self.port);
            ui.separator();
            let no_team = self.mode == AddressMode::TeamNumber && self.team == 0;
            status_label(ui, &self.nt.status(), no_team);
        });
        ui.horizontal(|ui| {
            let ctx = ui.ctx().clone();
            for (label, action) in [
                ("Open…", FileAction::Open),
                ("Save", FileAction::Save),
                ("Save as…", FileAction::SaveAs),
            ] {
                if ui
                    .button(label)
                    .on_hover_text(ctx.format_shortcut(&action.shortcut()))
                    .clicked()
                {
                    self.run_file_action(action);
                }
            }
            ui.separator();
            for (label, action, can) in [
                ("Undo", EditAction::Undo, self.history.can_undo()),
                ("Redo", EditAction::Redo, self.history.can_redo()),
            ] {
                let button = ui.add_enabled(!self.locked && can, egui::Button::new(label));
                if button
                    .on_hover_text(ctx.format_shortcut(&action.shortcut()))
                    .on_disabled_hover_text(if self.locked {
                        "Unlock to change the layout"
                    } else {
                        "Nothing to undo or redo"
                    })
                    .clicked()
                {
                    self.run_edit_action(&ctx, action);
                }
            }
            ui.separator();
            let name = layout_file::display_name(self.layout_path.as_deref());
            let name = ui.strong(name);
            if let Some(path) = &self.layout_path {
                name.on_hover_text(path.display().to_string());
            }
            if self.unsaved() {
                ui.label(egui::RichText::new("●").color(Color32::from_rgb(230, 160, 40)))
                    .on_hover_text("Unsaved changes");
            }
            ui.weak(&self.status);
            if !self.fixes.is_empty() {
                let n = self.fixes.len();
                let list = self.fixes.join("\n");
                let badge = ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "⚠ fixed {n} problem{}",
                                if n == 1 { "" } else { "s" }
                            ))
                            .color(Color32::from_rgb(230, 160, 40)),
                        )
                        .sense(Sense::click()),
                    )
                    .on_hover_text(format!("{list}\n\nRight-click to copy this list"));
                // Copy the list so it can be pasted into a bug report or chat.
                badge.context_menu(|ui| {
                    if ui.button("Copy list").clicked() {
                        ui.ctx().copy_text(list.clone());
                        self.status = format!(
                            "Copied {n} fix{} to the clipboard",
                            if n == 1 { "" } else { "es" }
                        );
                    }
                });
            }
        });
    }

    /// The row of tabs. Click to switch; when unlocked, double-click to
    /// rename, right-click for more, and "+" adds a tab.
    /// Each tab's mark from its Alerts widgets: the most urgent level
    /// above info they show now, open or not. Nothing is read when no tab
    /// has an Alerts widget.
    pub(super) fn tab_alert_marks(&self) -> Vec<Option<Level>> {
        let widgets_of = |t: &raptaboard_layout::Tab| {
            let grid = &t.grid_layout;
            grid.containers
                .iter()
                .chain(grid.layouts.iter().flat_map(|l| &l.children))
                .cloned()
                .collect::<Vec<_>>()
        };
        let any = self
            .layout
            .tabs
            .iter()
            .flat_map(widgets_of)
            .any(|w| w.widget_type == "Alerts");
        if !any {
            return vec![None; self.layout.tabs.len()];
        }
        let topics = self.nt.topics();
        let values: HashMap<&str, &nt4::Value> = topics
            .iter()
            .filter_map(|t| Some((&*t.name, &t.value.as_ref()?.value)))
            .collect();
        self.layout
            .tabs
            .iter()
            .map(|t| {
                widgets_of(t)
                    .iter()
                    .filter_map(|w| alerts_widget::tab_level(w, &values))
                    .min()
            })
            .collect()
    }

    pub(super) fn tab_bar(&mut self, ui: &mut egui::Ui) -> Option<TabAction> {
        let mut action = None;
        let names: Vec<String> = self.layout.tabs.iter().map(|t| t.name.clone()).collect();
        let count = names.len();
        // Tabs with widgets for the topics picked in the tree get a mark.
        let picked = self.tree_sel.selected();
        let has_picked: Vec<bool> = self
            .layout
            .tabs
            .iter()
            .map(|t| {
                let grid = &t.grid_layout;
                grid.containers
                    .iter()
                    .chain(grid.layouts.iter().flat_map(|l| &l.children))
                    .any(|w| picked.iter().any(|p| p.is_shown_by(w)))
            })
            .collect();
        let alert_marks = self.tab_alert_marks();
        ui.horizontal(|ui| {
            for (i, name) in names.iter().enumerate() {
                if let Some((renaming, text, focused)) = &mut self.renaming
                    && *renaming == i
                {
                    let edit = ui.add(egui::TextEdit::singleline(text).desired_width(120.0));
                    // Focus the box once, when renaming starts. Asking every
                    // frame would keep it from ever losing focus, so Enter
                    // and clicking away would do nothing.
                    if !*focused {
                        edit.request_focus();
                        *focused = true;
                    }
                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        self.renaming = None;
                    } else if edit.lost_focus() {
                        action = Some(TabAction::Rename(i, text.clone()));
                        self.renaming = None;
                    }
                    continue;
                }
                let marked = has_picked[i] && self.current_tab != i;
                let alert = alert_marks[i];
                let mut text = if marked {
                    format!("{name} ●")
                } else {
                    name.clone()
                };
                // Room after the name for the alert's sign.
                if alert.is_some() {
                    text.push_str("     ");
                }
                let mut hint = tabs::tab_hint(ui.ctx(), i, self.locked);
                if marked {
                    hint = format!("{hint}\nHas widgets for the topics picked in the tree");
                }
                if let Some(level) = alert {
                    hint = format!("{hint}\nHas an active {}", level.name(1));
                }
                let tab = ui
                    .selectable_label(self.current_tab == i, egui::RichText::new(text))
                    .on_hover_text(hint);
                if let Some(level) = alert {
                    let r = (tab.rect.height() * 0.32).min(7.0);
                    let c = egui::pos2(tab.rect.right() - r - 4.0, tab.rect.center().y);
                    ui.painter().circle_filled(c, r, level.color());
                    alerts_widget::draw_sign(ui.painter(), level, c, r * 1.2, egui::Color32::BLACK);
                }
                if tab.clicked() {
                    action = Some(TabAction::Select(i));
                }
                if self.locked {
                    continue;
                }
                if tab.double_clicked() {
                    self.renaming = Some((i, name.clone(), false));
                }
                tab.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        self.renaming = Some((i, name.clone(), false));
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move left"))
                        .clicked()
                    {
                        // Moves act on the open tab, so open this one first.
                        self.current_tab = i;
                        action = Some(TabAction::MoveLeft);
                    }
                    if ui
                        .add_enabled(i + 1 < count, egui::Button::new("Move right"))
                        .clicked()
                    {
                        self.current_tab = i;
                        action = Some(TabAction::MoveRight);
                    }
                    if ui
                        .add_enabled(count > 1, egui::Button::new("Close"))
                        .clicked()
                    {
                        action = Some(TabAction::Close(i));
                    }
                });
            }
            if !self.locked && ui.small_button("+").on_hover_text("New tab").clicked() {
                action = Some(TabAction::New);
            }
        });
        action
    }
}
