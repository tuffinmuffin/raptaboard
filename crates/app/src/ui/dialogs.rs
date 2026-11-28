//! The settings dialogs for widgets and List Layouts.

use eframe::egui::{self, Color32, Sense};
use raptaboard_layout::{LayoutContainer, WidgetContainer};

use crate::grid;
use crate::tabs::TabAction;
use crate::types::structs::{self, FieldInfo};
use crate::widgets::shared::props;
use crate::widgets::shared::settings as widget_settings;
use crate::widgets::{group, list_layout};

use super::DashboardApp;

/// A widget whose settings dialog is open.
#[derive(Debug, Clone)]
pub(super) enum Editing {
    /// A widget on the grid: its tab, its index in the tab's `containers`,
    /// and the widget as it was when the dialog opened, for Cancel.
    Widget {
        tab: usize,
        index: usize,
        original: WidgetContainer,
    },
    /// A List Layout: its tab, its index in the tab's `layouts`, and the
    /// list (with its children) as it was, for Cancel. `child` is the child
    /// whose settings are open inside the list's dialog, with that child as
    /// it was when they opened. `child_only` is set when they were opened
    /// by double-clicking the child: closing them then closes the dialog,
    /// rather than going back to the list's settings.
    List {
        tab: usize,
        index: usize,
        original: Box<LayoutContainer>,
        child: Option<(usize, WidgetContainer)>,
        child_only: bool,
    },
}

/// How dark the dim behind a settings dialog is, from 0 (none) to 255;
/// light, so the widget being changed can be seen.
pub(super) const SETTINGS_BACKDROP_ALPHA: u8 = 40;

/// Show a settings dialog titled `title`, which can be moved out of the
/// way of the widget it's changing by dragging its title bar. It starts in
/// the middle of the window and reopens where it was last left (for this
/// run). The grid behind is blocked while it's open, as for any dialog, but
/// only lightly dimmed, so changes can be seen. `name` tells dialogs apart.
pub(super) fn show_settings(
    ctx: &egui::Context,
    name: &str,
    title: &str,
    contents: impl FnOnce(&mut egui::Ui),
) {
    // A backdrop over everything but the dialog, catching clicks and drags
    // so the grid can't be changed underneath.
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique((name, "backdrop")))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
            ui.painter().rect_filled(
                rect,
                0.0,
                Color32::from_black_alpha(SETTINGS_BACKDROP_ALPHA),
            );
        });
    egui::Window::new(title)
        .id(egui::Id::unique(name))
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .constrain(true)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .show(ctx, |ui| {
            ui.add_space(2.0);
            contents(ui);
        });
}

/// Where a widget whose settings are open is.
pub(super) enum WidgetIn<'a> {
    /// On the grid: it can be pinned in place.
    Grid,
    /// In a List Layout or Group; with the layout's shared settings, if it
    /// has any: a checkbox to use them, a note, and the rows they grey out.
    Layout(Option<&'a Shared>),
}

/// A List Layout's or Group's shared settings, as a widget inside's dialog
/// shows them.
pub(super) struct Shared {
    /// What they set: "update period 0.50 s, text size 24 pt".
    pub note: String,
    /// The settings they set ([`group::controlled_keys`]).
    pub keys: Vec<&'static str>,
}

/// One widget's settings in a dialog titled `heading`, with Done, Cancel
/// and Reset settings; `within` adds "Pin in place" (on the grid) or a
/// note above the settings (in a group). Returns `Some(true)` for Done, `Some(false)` for
/// Cancel or Esc, `None` while it stays open. Changes are made to `widget`
/// as they happen; on Cancel the caller puts the original back.
pub(super) fn widget_window(
    ctx: &egui::Context,
    heading: &str,
    widget: &mut WidgetContainer,
    nt_type: Option<&str>,
    field: Option<&FieldInfo>,
    topics: &[(String, String)],
    within: WidgetIn<'_>,
) -> Option<bool> {
    let mut close = None;
    show_settings(ctx, "widget_settings", heading, |ui| {
        // What the widget's group or list sets is greyed out while it uses
        // them.
        let using = props::flag(&widget.properties, group::USE_SHARED, true);
        let controlled = match &within {
            WidgetIn::Layout(Some(shared)) if using => shared.keys.as_slice(),
            _ => &[],
        };
        props::set_controlled(ui.ctx(), controlled);
        if let WidgetIn::Layout(Some(shared)) = &within {
            let note = &shared.note;
            let mut using = props::flag(&widget.properties, group::USE_SHARED, true);
            if ui
                .checkbox(&mut using, "Use the shared settings")
                .on_hover_text(
                    "Off, this widget uses only its own settings, not its group's or list's",
                )
                .changed()
            {
                let value = (!using).then_some(false.into());
                props::set_property(&mut widget.properties, group::USE_SHARED, value);
            }
            ui.add_enabled_ui(using, |ui| ui.weak(note));
            ui.add_space(4.0);
        }
        widget_settings::show(ui, widget, nt_type, field, topics);
        props::set_controlled(ui.ctx(), &[]);
        if matches!(within, WidgetIn::Grid) {
            pin_checkbox(ui, &mut widget.properties);
        }
        ui.add_space(4.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Done").clicked() {
                close = Some(true);
            }
            if ui.button("Cancel").on_hover_text("Esc").clicked() {
                close = Some(false);
            }
            ui.separator();
            let custom = widget_settings::has_custom_settings(widget);
            if ui
                .add_enabled(custom, egui::Button::new("Reset settings"))
                .on_hover_text(
                    "Put every setting back to its default. The title is kept; \
                     Cancel still undoes this.",
                )
                .on_disabled_hover_text("All settings are at their defaults")
                .clicked()
            {
                widget_settings::reset_settings(widget);
            }
        });
    });
    // Esc cancels. A click outside does nothing, so a stray click doesn't
    // throw away the changes.
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = Some(false);
    }
    close
}

/// "Pin in place" for a widget or List Layout on the grid
/// ([`grid::PINNED`]): never pushed aside or packed.
pub(super) fn pin_checkbox(
    ui: &mut egui::Ui,
    props: &mut serde_json::Map<String, serde_json::Value>,
) {
    let mut pinned = props.get(grid::PINNED).and_then(|v| v.as_bool()) == Some(true);
    if ui
        .checkbox(&mut pinned, "Pin in place")
        .on_hover_text(
            "Never pushed aside by another widget or moved by Pack widgets; \
             it still moves when you drag it",
        )
        .changed()
    {
        if pinned {
            props.insert(grid::PINNED.into(), true.into());
        } else {
            props.remove(grid::PINNED);
        }
    }
}

impl DashboardApp {
    /// The NT type of the value `widget` shows: its saved `data_type`, or
    /// for a struct field the type in its `struct_meta`, or else the type
    /// the server announced for its topic, or for a table (a chooser) its
    /// `.type` (`String Chooser`). `None` if none of those is known.
    pub(super) fn value_type(&self, widget: &WidgetContainer) -> Option<String> {
        let saved = |v: Option<&serde_json::Value>| v?.as_str().map(str::to_owned);
        if let Some(ty) = saved(widget.properties.get(props::DATA_TYPE)) {
            return Some(ty);
        }
        if widget.struct_meta().is_some() {
            return saved(
                widget
                    .properties
                    .get("struct_meta")
                    .and_then(|m| m.get("type")),
            );
        }
        let name = widget.topic()?;
        if let Some(topic) = self.nt.client()?.topic(name) {
            return Some(topic.ty.to_string());
        }
        // A table: its `.type`.
        let table_type = format!("{}/.type", name.trim_end_matches('/'));
        self.nt
            .topics()
            .into_iter()
            .find(|t| *t.name == *table_type)
            .and_then(|t| match t.value?.value {
                nt4::Value::String(ty) => Some(ty),
                _ => None,
            })
    }

    /// The settings dialog for a widget or List Layout, while one is open.
    /// Changes show on the grid as they're made; Cancel or Esc puts things
    /// back as they were, and the whole visit is one step for undo.
    pub(super) fn widget_dialog(&mut self, ctx: &egui::Context) {
        match self.editing {
            Some(Editing::Widget { .. }) => self.single_widget_dialog(ctx),
            Some(Editing::List { .. }) => self.list_dialog(ctx),
            None => {}
        }
    }

    /// Every topic on the server, as (full name, NT type), for settings
    /// that refer to other topics.
    pub(super) fn topic_list(&self) -> Vec<(String, String)> {
        self.nt
            .topics()
            .into_iter()
            .map(|t| (t.name.to_string(), t.ty.to_string()))
            .collect()
    }

    /// The type and struct field info for `widget`'s settings.
    pub(super) fn value_facts(
        &self,
        widget: &WidgetContainer,
    ) -> (Option<String>, Option<FieldInfo>) {
        let field = widget.struct_meta().and_then(|meta| {
            structs::field_info(self.nt.structs()?, &meta.schema_name, &meta.path)
        });
        (self.value_type(widget), field)
    }

    /// The settings of a widget on the grid.
    pub(super) fn single_widget_dialog(&mut self, ctx: &egui::Context) {
        let Some(Editing::Widget { tab, index, .. }) = self.editing else {
            return;
        };
        let Some(widget) = self.widget_at(tab, index) else {
            self.editing = None;
            return;
        };
        let (nt_type, field) = self.value_facts(widget);
        let topics = self.topic_list();
        let Some(widget) = self
            .layout
            .tabs
            .get_mut(tab)
            .and_then(|t| t.grid_layout.containers.get_mut(index))
        else {
            return;
        };
        let close = widget_window(
            ctx,
            "Widget settings",
            widget,
            nt_type.as_deref(),
            field.as_ref(),
            &topics,
            WidgetIn::Grid,
        );
        match close {
            Some(true) => self.editing = None,
            Some(false) => {
                if let Some(Editing::Widget { original, .. }) = self.editing.take() {
                    *widget = original;
                }
            }
            None => {}
        }
    }

    /// The widget at `index` in tab `tab`'s `containers`.
    pub(super) fn widget_at(&self, tab: usize, index: usize) -> Option<&WidgetContainer> {
        self.layout.tabs.get(tab)?.grid_layout.containers.get(index)
    }

    /// The settings of a List Layout: its title, label position and
    /// children. A child's Edit… shows that child's widget settings in the
    /// same dialog; its Done or Cancel comes back to the list.
    pub(super) fn list_dialog(&mut self, ctx: &egui::Context) {
        let Some(Editing::List {
            tab, index, child, ..
        }) = &self.editing
        else {
            return;
        };
        let (tab, index, child) = (*tab, *index, child.as_ref().map(|(c, _)| *c));
        let Some(list) = self
            .layout
            .tabs
            .get(tab)
            .and_then(|t| t.grid_layout.layouts.get(index))
        else {
            self.editing = None;
            return;
        };

        // A child's settings, inside the list's dialog.
        if let Some(c) = child {
            let Some(widget) = list.children.get(c) else {
                self.close_child();
                return;
            };
            let (nt_type, field) = self.value_facts(widget);
            let topics = self.topic_list();
            let heading = format!("{}: {}", list.title, widget.title);
            let shared = group::shared_note(list, widget).map(|got| Shared {
                note: format!(
                    "Set by the {}: {got}.",
                    if group::is_group(list) {
                        "group"
                    } else {
                        "list"
                    }
                ),
                keys: group::controlled_keys(list),
            });
            let Some(widget) = self
                .layout
                .tabs
                .get_mut(tab)
                .and_then(|t| t.grid_layout.layouts.get_mut(index))
                .and_then(|l| l.children.get_mut(c))
            else {
                return;
            };
            match widget_window(
                ctx,
                &heading,
                widget,
                nt_type.as_deref(),
                field.as_ref(),
                &topics,
                WidgetIn::Layout(shared.as_ref()),
            ) {
                Some(true) => self.close_child(),
                Some(false) => {
                    if let Some(Editing::List {
                        child: Some((_, original)),
                        ..
                    }) = &self.editing
                    {
                        *widget = original.clone();
                    }
                    self.close_child();
                }
                None => {}
            }
            return;
        }

        let Some(list) = self
            .layout
            .tabs
            .get_mut(tab)
            .and_then(|t| t.grid_layout.layouts.get_mut(index))
        else {
            return;
        };
        let mut close = None;
        let mut action = None;
        let heading = if crate::widgets::group::is_group(list) {
            "Group settings"
        } else {
            "List settings"
        };
        show_settings(ctx, "list_settings", heading, |ui| {
            if crate::widgets::group::is_group(list) {
                crate::widgets::group::settings(ui, list);
            } else {
                action = list_layout::settings(ui, list);
            }
            // Apart from the rows for the widgets inside: it's the layout's own.
            ui.separator();
            pin_checkbox(ui, &mut list.properties);
            ui.add_space(4.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Done").clicked() {
                    close = Some(true);
                }
                if ui.button("Cancel").on_hover_text("Esc").clicked() {
                    close = Some(false);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = Some(false);
        }
        if let Some(list_layout::ListAction::EditChild(c)) = action
            && let Some(widget) = list.children.get(c)
            && let Some(Editing::List { child, .. }) = &mut self.editing
        {
            *child = Some((c, widget.clone()));
        }
        match close {
            Some(true) => self.editing = None,
            Some(false) => {
                if let Some(Editing::List { original, .. }) = self.editing.take() {
                    *list = *original;
                }
            }
            None => {}
        }
    }

    /// Names for the open tab's List Layouts, in order: their titles, with
    /// a number added where two share one ("List Layout (2)").
    pub(super) fn list_names(&self) -> Vec<String> {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return Vec::new();
        };
        list_layout::menu_names(&tab.grid_layout.layouts)
    }

    /// Leave a child's settings and go back to its list's.
    /// Close a list child's settings: back to the list's, or (opened by
    /// double-clicking the child) close the dialog.
    pub(super) fn close_child(&mut self) {
        match &mut self.editing {
            Some(Editing::List {
                child_only: true, ..
            }) => self.editing = None,
            Some(Editing::List { child, .. }) => *child = None,
            _ => {}
        }
    }

    /// The "close this tab?" question, while one is open.
    pub(super) fn close_dialog(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_close else { return };
        let Some(tab) = self.layout.tabs.get(i) else {
            self.confirm_close = None;
            return;
        };
        let name = tab.name.clone();
        let widgets = tab.grid_layout.containers.len() + tab.grid_layout.layouts.len();
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm_close_tab")).show(ctx, |ui| {
            ui.heading(format!("Close \"{name}\"?"));
            ui.label(format!(
                "Its {widgets} widget{} will be removed.",
                if widgets == 1 { "" } else { "s" }
            ));
            ui.horizontal(|ui| {
                if ui.button("Close tab").clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => self.run_tab_action(TabAction::Close(i)),
            Some(false) => self.confirm_close = None,
            None if modal.should_close() => self.confirm_close = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use eframe::egui::{Event, PointerButton, Pos2, RawInput, Rect};

    /// One frame showing a settings dialog, with `events`; returns where
    /// the dialog is.
    fn frame(ctx: &egui::Context, events: Vec<Event>) -> Rect {
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            show_settings(&ctx, "test_settings", "Settings", |ui| {
                ui.label("A setting");
            });
        });
        output.textures_delta.clear();
        ctx.memory(|m| m.area_rect(egui::Id::unique("test_settings")))
            .expect("the dialog was drawn")
    }

    #[test]
    fn settings_dialogs_can_be_dragged_out_of_the_way() {
        let ctx = egui::Context::default();
        let start = frame(&ctx, vec![]);
        frame(&ctx, vec![]);
        // It starts in the middle of the window.
        assert!(
            (start.center() - Pos2::new(400.0, 300.0)).length() < 1.0,
            "{start:?}"
        );
        // Drag its title bar 150 points right.
        let grab = Pos2::new(start.center().x, start.min.y + 8.0);
        let press = |down| Event::PointerButton {
            pos: grab,
            button: PointerButton::Primary,
            pressed: down,
            modifiers: Default::default(),
        };
        frame(&ctx, vec![Event::PointerMoved(grab), press(true)]);
        for step in 1..=10 {
            let at = grab + egui::vec2(15.0 * step as f32, 0.0);
            frame(&ctx, vec![Event::PointerMoved(at)]);
        }
        frame(&ctx, vec![press(false)]);
        let moved = frame(&ctx, vec![]);
        assert!(
            (moved.min.x - start.min.x - 150.0).abs() < 2.0,
            "{start:?} -> {moved:?}"
        );
        // Only sideways (egui rounds to half points).
        assert!((moved.min.y - start.min.y).abs() <= 1.0);
    }
}
