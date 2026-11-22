//! The settings bar (connection status, lock, the gear, file) and the tab bar.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Sense};

use crate::connection::{AddressMode, resolve};
use crate::edit::EditAction;
use crate::layout_file::{self, FileAction};
use crate::nt::ConnectionStatus;
use crate::tabs::{self, TabAction};
use crate::types::alerts::Level;
use crate::widgets;
use crate::widgets::shared::alert_sign;

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
            let (label, hint) = if self.tree.shown {
                ("◀ Tree", "Hide the NetworkTables tree")
            } else {
                ("▶ Tree", "Show the NetworkTables tree")
            };
            if ui.button(label).on_hover_text(hint).clicked() {
                self.tree.shown = !self.tree.shown;
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
                self.grid.drag = None;
                self.grid.child_drag = None;
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
            // The gear before the alert history button, whose width changes
            // as alerts come and go, so the gear stays put.
            self.gear_button(ui);
            self.alert_history_button(ui);
            ui.separator();
            // The address is set in the Settings window; the connection
            // follows it every frame, whether that window is open or not.
            let host = resolve(self.address.mode, self.address.team, &self.address.custom);
            self.nt.set_target(host.as_deref(), self.address.port);
            ui.separator();
            let no_team = self.address.mode == AddressMode::TeamNumber && self.address.team == 0;
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
            let name = layout_file::display_name(self.file.path.as_deref());
            let name = ui.strong(name);
            if let Some(path) = &self.file.path {
                name.on_hover_text(path.display().to_string());
            }
            if self.unsaved() {
                ui.label(egui::RichText::new("●").color(Color32::from_rgb(230, 160, 40)))
                    .on_hover_text("Unsaved changes");
            }
            ui.weak(&self.status);
            if !self.file.fixes.is_empty() {
                let n = self.file.fixes.len();
                let list = self.file.fixes.join("\n");
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

    /// Each tab's mark from its widgets ([`WidgetType::tab_mark`]): the
    /// most urgent alert they show now, open or not, from the server's
    /// `values`.
    ///
    /// [`WidgetType::tab_mark`]: crate::widgets::WidgetType::tab_mark
    pub(super) fn tab_marks(&self, values: &HashMap<&str, &nt4::Value>) -> Vec<Option<Level>> {
        self.layout
            .tabs
            .iter()
            .map(|t| {
                let grid = &t.grid_layout;
                grid.containers
                    .iter()
                    .chain(grid.layouts.iter().flat_map(|l| &l.children))
                    .filter_map(|w| widgets::find(&w.widget_type)?.tab_mark(w, values))
                    .min()
            })
            .collect()
    }

    /// The row of tabs. Click to switch; when unlocked, double-click to
    /// rename, right-click for more, and "+" adds a tab.
    pub(super) fn tab_bar(
        &mut self,
        ui: &mut egui::Ui,
        values: &HashMap<&str, &nt4::Value>,
    ) -> Option<TabAction> {
        let mut action = None;
        let names: Vec<String> = self.layout.tabs.iter().map(|t| t.name.clone()).collect();
        let count = names.len();
        // Tabs with widgets for the topics picked in the tree get a mark.
        let picked = self.tree.selection.selected();
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
        let alert_marks = self.tab_marks(values);
        ui.horizontal(|ui| {
            // Each tab's place, and a tab being dragged along the bar (and
            // whether it was let go this frame), for moving it.
            let mut rects: Vec<egui::Rect> = Vec::with_capacity(count);
            let mut dragging: Option<(usize, bool)> = None;
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
                    rects.push(edit.rect);
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
                rects.push(tab.rect);
                if let Some(level) = alert {
                    alert_sign::draw_badge(ui.painter(), level, tab.rect, 4.0);
                }
                if tab.clicked() {
                    action = Some(TabAction::Select(i));
                }
                if self.locked {
                    continue;
                }
                // Unlocked, a tab drags along the bar to move it.
                let tab = tab.interact(egui::Sense::drag());
                if tab.dragged() || tab.drag_stopped() {
                    dragging = Some((i, tab.drag_stopped()));
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
            if let Some((from, released)) = dragging
                && let Some(x) = ui.input(|i| i.pointer.latest_pos()).map(|p| p.x)
            {
                let centers: Vec<f32> = rects.iter().map(|r| r.center().x).collect();
                let (to, gap) = tabs::drop_place(&centers, from, x);
                if released {
                    if to != from {
                        action = Some(TabAction::Move { from, to });
                    }
                } else {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    // A marker in the gap it will land in.
                    if to != from
                        && let Some(edge) = match gap {
                            0 => rects.first().map(|r| r.left() - 2.0),
                            g => rects.get(g - 1).map(|r| r.right() + 2.0),
                        }
                    {
                        let y = rects[from].y_range();
                        ui.painter().vline(
                            edge,
                            y,
                            egui::Stroke::new(3.0, ui.visuals().selection.stroke.color),
                        );
                    }
                }
            }
            if !self.locked && ui.small_button("+").on_hover_text("New tab").clicked() {
                action = Some(TabAction::New);
            }
        });
        action
    }
}
