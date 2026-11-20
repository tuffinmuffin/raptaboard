//! The NT tree panel, and adding topics picked there to the grid.

use eframe::egui::{self, Sense};
use nt4::Topic;
use raptaboard_layout::{Rect as LayoutRect, WidgetContainer};

use crate::grid::{self, Item};
use crate::nt::ConnectionStatus;
use crate::tree::{AddTo, Folder, TopicEntry};
use crate::types::structs;
use crate::widgets;
use nt4::proto::structs::StructDatabase;

use super::DashboardApp;

impl DashboardApp {
    /// The NetworkTables tree in the left panel. Read-only: values are
    /// changed through widgets, not here.
    ///
    /// Returns the topics drawn, whose values the tree needs.
    pub(super) fn tree_panel(&mut self, ui: &mut egui::Ui, topics: &[Topic]) -> Vec<String> {
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
        self.tree_sel.begin_frame();
        self.tree_sel.lists = self.list_names();
        let no_structs = StructDatabase::new();
        let structs = self.nt.structs().unwrap_or(&no_structs);
        // Both ways: a wide row scrolls sideways rather than widening the
        // panel, which would push the grid over (selecting a widget opens
        // its folder, so that would move the grid under the pointer).
        let drawn = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Rows are picked, not their text: no text cursor or text
                // highlighting, which would look like picking.
                ui.style_mut().interaction.selectable_labels = false;
                let drawn = tree.show(ui, "", filtering, structs, !self.locked, &mut self.tree_sel);
                // Clicking the empty space below the rows unpicks them.
                let below = ui.allocate_response(ui.available_size(), Sense::click());
                if below.clicked() {
                    self.tree_sel.clear();
                }
                drawn
            })
            .inner;
        self.tree_sel.end_frame();
        // Every table's `.type`, so tables made for a widget (choosers) are
        // known before their folder is opened.
        let mut drawn = drawn;
        drawn.extend(
            topics
                .iter()
                .filter(|t| t.name.ends_with("/.type"))
                .map(|t| t.name.to_string()),
        );
        // Picking in the tree deselects the grid's widget, whose outline
        // would otherwise linger beside the picked topic's.
        if self.tree_sel.clicked {
            self.selected = None;
        }
        if let Some(row) = &self.tree_sel.not_found {
            self.status =
                format!("No row for {row} in the tree: not published, or hidden by the filter");
            self.find_status = true;
        }
        drawn
    }

    /// Make widgets for `topics` on the open tab, one cell each, where `to`
    /// says: free spots nearest the tab's top left, the end of a list, or a
    /// new list. The status line says what happened.
    pub(super) fn add_topics(&mut self, topics: &[crate::tree::DraggedTopic], to: AddTo) {
        let cell = f64::from(self.layout.grid_size());
        let one = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: cell,
            height: cell,
        };
        let made: Vec<WidgetContainer> = topics
            .iter()
            .filter_map(|t| widgets::for_topic(&t.name, &t.ty, t.field.as_ref(), one))
            .collect();
        let wanted = made.len();
        let room = self.grid_room;
        let Some(tab) = self.layout.tabs.get_mut(self.current_tab) else {
            return;
        };
        if to == AddTo::NewList {
            match grid::add_list_holding(&mut tab.grid_layout, (0.0, 0.0), made, cell, room) {
                Some(i) => {
                    self.selected = Some(Item::List(i));
                    self.status = format!(
                        "Made a list of {wanted} widget{}",
                        if wanted == 1 { "" } else { "s" }
                    );
                }
                None => self.status = "No free space for a new list".to_owned(),
            }
            return;
        }
        if let AddTo::List(i) = to
            && let Some(l) = tab.grid_layout.layouts.get_mut(i)
        {
            l.children.extend(made);
            self.selected = Some(Item::List(i));
            self.status = format!(
                "Added {wanted} widget{} to \"{}\"",
                if wanted == 1 { "" } else { "s" },
                l.title
            );
            return;
        }
        let placed = grid::place_near(&mut tab.grid_layout, made, (0.0, 0.0), cell, room);
        self.selected = placed.first().map(|&i| Item::Widget(i));
        self.status = if placed.len() < wanted {
            format!(
                "Added {} of {wanted} widgets; no free space for the rest",
                placed.len()
            )
        } else {
            format!(
                "Added {wanted} widget{}",
                if wanted == 1 { "" } else { "s" }
            )
        };
    }

    /// Open the tree to `widget`'s topic (or struct field) and pick it, if
    /// the server has it; otherwise leave the tree alone, unpick, and say
    /// why in the status line.
    pub(super) fn find_in_tree(&mut self, widget: &WidgetContainer) {
        self.find_widget(widget, false);
    }

    /// Like [`find_in_tree`](Self::find_in_tree), for a click the widget
    /// itself used (a toggle, a chooser's button): the row is picked if
    /// there is one, and nothing is said if not, so the widget's own message
    /// (a failed send's) stays in the status line.
    pub(super) fn find_in_tree_quietly(&mut self, widget: &WidgetContainer) {
        self.find_widget(widget, true);
    }

    fn find_widget(&mut self, widget: &WidgetContainer, quiet: bool) {
        let Some(topic) = widget.topic() else { return };
        let field = widget.struct_meta();
        // A widget bound to a whole table (a chooser) is found if anything
        // is published under the table.
        let table = format!("{}/", topic.trim_end_matches('/'));
        let published = self.nt.client().and_then(|c| c.topic(topic)).is_some()
            || self.nt.topics().iter().any(|t| t.name.starts_with(&table));
        let missing = if !published {
            Some(format!("{topic} isn't published"))
        } else {
            field.as_ref().and_then(|meta| {
                let known = self
                    .nt
                    .structs()
                    .and_then(|db| structs::field_info(db, &meta.schema_name, &meta.path));
                known
                    .is_none()
                    .then(|| format!("{topic} has no field {}", structs::path_text(&meta.path)))
            })
        };
        match missing {
            Some(_) if quiet => {}
            Some(why) => {
                self.tree_sel.clear();
                self.status = why;
                self.find_status = true;
            }
            None => {
                // A finding message from an earlier click is out of date now.
                if self.find_status {
                    self.status.clear();
                    self.find_status = false;
                }
                self.tree_sel.reveal(topic, field.map(|m| m.path));
            }
        }
    }
}
