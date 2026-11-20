//! The alert history: recording every alert group's alerts each frame, the
//! top-bar button that shows how bad things are and opens the history, and
//! the history panel itself (see [`crate::alert_history`]).

use std::collections::BTreeSet;

use chrono::Local;
use eframe::egui::{self, Sense};
use nt4::Topic;

use crate::alert_history::{self, AlertEvent};
use crate::types::alerts::{self, Level};
use crate::nt::ConnectionStatus;
use crate::widgets::builtin::alerts as alerts_widget;

use super::DashboardApp;

/// How often every alert group's lists are sent for the history, at most
/// (seconds): often enough to catch an alert that's only on for a moment.
pub(super) const HISTORY_PERIOD_S: f64 = 0.02;
/// Height of one history row, in points.
const ROW_HEIGHT: f32 = 20.0;

/// What the history panel shows: levels, a group (all if `None`) and text
/// to look for.
#[derive(Debug, Clone)]
pub(super) struct HistoryFilter {
    levels: [bool; 3],
    group: Option<String>,
    text: String,
    /// What's typed in the "Not recorded" list's box.
    new_ignore: String,
}

impl Default for HistoryFilter {
    fn default() -> Self {
        HistoryFilter {
            levels: [true; 3],
            group: None,
            text: String::new(),
            new_ignore: String::new(),
        }
    }
}

impl HistoryFilter {
    /// Whether `e` passes.
    fn shows(&self, e: &AlertEvent) -> bool {
        self.levels[e.level as usize]
            && self.group.as_ref().is_none_or(|g| *g == e.group)
            && (self.text.is_empty() || e.text.to_lowercase().contains(&self.text.to_lowercase()))
    }
}

/// `d` as `12 s`, `3 m 05 s` (185 s) or `1 h 02 m`.
fn short_duration(d: chrono::TimeDelta) -> String {
    let s = d.num_seconds().max(0);
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} m {:02} s", s / 60, s % 60),
        _ => format!("{} h {:02} m", s / 3600, s % 3600 / 60),
    }
}

impl DashboardApp {
    /// The topics the history needs: every table's `.type` (to find the
    /// alert groups) and every alert group's three lists.
    pub(super) fn history_topics(&self, topics: &[Topic]) -> Vec<String> {
        let mut names = Vec::new();
        for t in topics {
            let Some(table) = t.name.strip_suffix("/.type") else {
                continue;
            };
            names.push(t.name.to_string());
            if matches!(&t.value, Some(v) if matches!(&v.value, nt4::Value::String(s) if s == alerts::ALERTS_TYPE))
            {
                names.extend(alerts::table_topics(table).into_iter().skip(1));
            }
        }
        names
    }

    /// Record this frame's alerts from every group in the history.
    pub(super) fn record_alerts(&mut self, values: &std::collections::HashMap<&str, &nt4::Value>) {
        let connected = matches!(self.nt.status(), ConnectionStatus::Connected { .. });
        let groups: Option<Vec<(String, Vec<alerts::ActiveAlert>)>> = connected.then(|| {
            alerts::alert_tables(values)
                .iter()
                .filter_map(|table| {
                    let alerts = alerts::read_group(table, values)?;
                    let group = table.rsplit('/').next().unwrap_or(table).to_owned();
                    Some((group, alerts))
                })
                .collect()
        });
        self.alert_history.update(Local::now(), groups.as_deref());
    }

    /// The top-bar button that shows and hides the history: "Alerts" with
    /// how many are active and the most urgent one's sign.
    pub(super) fn alert_history_button(&mut self, ui: &mut egui::Ui) {
        let active = self.alert_history.active_count();
        let highest = self.alert_history.highest_active();
        let text = match (active, highest) {
            (0, _) => "Alerts".to_owned(),
            // Room after the count for the sign.
            (n, _) => format!("Alerts {n}     "),
        };
        let hint = match highest {
            Some(level) => format!(
                "{active} active, the worst an {}. Click to {} the alert history.",
                level.name(1),
                if self.show_alert_history {
                    "hide"
                } else {
                    "show"
                }
            ),
            None => format!(
                "No active alerts. Click to {} the alert history.",
                if self.show_alert_history {
                    "hide"
                } else {
                    "show"
                }
            ),
        };
        let button = ui
            .add(egui::Button::new(text).selected(self.show_alert_history))
            .on_hover_text(hint);
        if let Some(level) = highest {
            let r = (button.rect.height() * 0.32).min(7.0);
            let c = egui::pos2(button.rect.right() - r - 6.0, button.rect.center().y);
            ui.painter().circle_filled(c, r, level.color());
            alerts_widget::draw_sign(ui.painter(), level, c, r * 1.2, egui::Color32::BLACK);
        }
        if button.clicked() {
            self.show_alert_history = !self.show_alert_history;
        }
    }

    /// The history panel: what to show, Clear, how many to keep, and the
    /// events, newest first.
    pub(super) fn alert_history_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Alert history");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("✕")
                    .on_hover_text("Hide the alert history")
                    .clicked()
                {
                    self.show_alert_history = false;
                }
            });
        });
        let total = self.alert_history.events().len();
        ui.horizontal(|ui| {
            let mut keep = self.alert_history.capacity();
            ui.label(format!("{total} events; keep"));
            if ui
                .add(
                    egui::DragValue::new(&mut keep)
                        .range(alert_history::MIN_CAPACITY..=alert_history::MAX_CAPACITY)
                        .speed(10),
                )
                .on_hover_text("How many events are kept; the oldest go first when it's full")
                .changed()
            {
                self.alert_history.set_capacity(keep);
            }
            if ui
                .button("Clear")
                .on_hover_text("Forget the events that have ended (active alerts stay)")
                .clicked()
            {
                self.alert_history.clear();
            }
        });
        // What to show.
        let groups: BTreeSet<String> = self
            .alert_history
            .events()
            .map(|e| e.group.clone())
            .collect();
        let filter = &mut self.history_filter;
        ui.horizontal(|ui| {
            for level in Level::ALL {
                ui.checkbox(&mut filter.levels[level as usize], level.name(2));
            }
            egui::ComboBox::from_id_salt("history_group")
                .selected_text(filter.group.as_deref().unwrap_or("every group"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut filter.group, None, "every group");
                    for g in &groups {
                        ui.selectable_value(&mut filter.group, Some(g.clone()), g);
                    }
                });
        });
        ui.add(egui::TextEdit::singleline(&mut filter.text).hint_text("Text to look for"));
        self.ignore_list(ui);
        ui.separator();
        let now = Local::now();
        let shown: Vec<&AlertEvent> = self
            .alert_history
            .events()
            .rev()
            .filter(|e| self.history_filter.shows(e))
            .collect();
        if shown.is_empty() {
            ui.weak(if total == 0 {
                "No alerts seen since connecting"
            } else {
                "No alerts match"
            });
            return;
        }
        let mut ignore = None;
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_HEIGHT, shown.len(), |ui, rows| {
                for e in &shown[rows] {
                    ui.horizontal(|ui| {
                        ui.set_height(ROW_HEIGHT);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, ROW_HEIGHT), Sense::hover());
                        let color = if e.ended.is_some() {
                            e.level.color().gamma_multiply(0.6)
                        } else {
                            e.level.color()
                        };
                        alerts_widget::draw_sign(ui.painter(), e.level, rect.center(), 11.0, color);
                        ui.monospace(e.seen.format("%H:%M:%S").to_string());
                        let ended = match e.ended {
                            Some((at, how)) => format!("{} {}", how.label(), at.format("%H:%M:%S")),
                            None => "active".to_owned(),
                        };
                        let changed = match e.changes {
                            0 => String::new(),
                            1 => " · changed once".to_owned(),
                            n => format!(" · changed {n} times"),
                        };
                        ui.weak(format!(
                            "{} · {}{changed}",
                            short_duration(e.duration(now)),
                            ended
                        ));
                        let text = format!("{}: {}", e.group, e.text);
                        let label = if e.ended.is_some() {
                            egui::RichText::new(&text).weak()
                        } else {
                            egui::RichText::new(&text)
                        };
                        let row = ui
                            .add(egui::Label::new(label).truncate().sense(Sense::click()))
                            .on_hover_text(&text);
                        row.context_menu(|ui| {
                            let entry = alert_history::ignore_entry(&e.text);
                            if ui
                                .button("Don't record alerts like this")
                                .on_hover_text(format!(
                                    "Add \"{entry}\" to the list of alerts not recorded, and \
                                     forget the ones recorded"
                                ))
                                .clicked()
                            {
                                ignore = Some(entry);
                                ui.close();
                            }
                        });
                    });
                }
            });
        if let Some(entry) = ignore {
            let mut list = self.alert_history.ignored().to_vec();
            list.push(entry);
            self.alert_history.set_ignored(list);
        }
    }

    /// The "Not recorded" list: alerts the history skips, by text they
    /// contain, with a box to add one and a ✕ to take one off.
    fn ignore_list(&mut self, ui: &mut egui::Ui) {
        let list = self.alert_history.ignored().to_vec();
        let title = if list.is_empty() {
            "Not recorded: nothing".to_owned()
        } else {
            format!("Not recorded: {}", list.len())
        };
        egui::CollapsingHeader::new(title)
            .id_salt("history_ignore")
            .show(ui, |ui| {
                ui.weak("Alerts whose text contains one of these aren't recorded (any case).");
                let mut list = list.clone();
                let mut changed = false;
                list.retain(|entry| {
                    let mut keep = true;
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("✕")
                            .on_hover_text("Record these again")
                            .clicked()
                        {
                            keep = false;
                            changed = true;
                        }
                        ui.monospace(entry);
                    });
                    keep
                });
                ui.horizontal(|ui| {
                    let add = ui.add(
                        egui::TextEdit::singleline(&mut self.history_filter.new_ignore)
                            .hint_text("Text, such as Frame Timing:"),
                    );
                    let enter = add.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("Add").clicked() || enter)
                        && !self.history_filter.new_ignore.trim().is_empty()
                    {
                        list.push(std::mem::take(&mut self.history_filter.new_ignore));
                        changed = true;
                    }
                });
                if changed {
                    self.alert_history.set_ignored(list);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn durations_read_short() {
        assert_eq!(short_duration(chrono::TimeDelta::seconds(12)), "12 s");
        assert_eq!(short_duration(chrono::TimeDelta::seconds(185)), "3 m 05 s");
        assert_eq!(short_duration(chrono::TimeDelta::seconds(3720)), "1 h 02 m");
        assert_eq!(short_duration(chrono::TimeDelta::seconds(-3)), "0 s");
    }
}
