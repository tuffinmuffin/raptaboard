//! A tab a plugin fills (plugin.toml's `tab = "Doom"`): an empty tab of that
//! name shows the plugin at the tab's size instead of the grid. Clicked, it
//! takes the keyboard, and the dashboard's own shortcuts are off meanwhile
//! (a game uses Ctrl, the numbers and the arrows).

use std::cell::RefCell;
use std::collections::HashMap;

use eframe::egui;
use nt4::Value;

use super::DashboardApp;
use crate::plugins::PluginType;
use crate::widgets;

impl DashboardApp {
    /// The plugin that fills the open tab, if one does.
    pub(super) fn tab_plugin(&self) -> Option<&'static PluginType> {
        crate::plugins::tab_plugin(self.layout.tabs.get(self.current_tab)?)
    }

    /// Whether a tab plugin has the keyboard: the dashboard's shortcuts are
    /// off until it doesn't (a click elsewhere, or another tab).
    pub(super) fn plugin_has_keys(&self, ctx: &egui::Context) -> bool {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return false;
        };
        self.tab_plugin().is_some()
            && ctx.memory(|m| m.has_focus(crate::plugins::widget::tab_focus_id(&tab.name)))
    }

    /// Show the open tab's plugin, filling the panel.
    pub(super) fn plugin_tab(
        &mut self,
        ui: &mut egui::Ui,
        ty: &'static PluginType,
        values: &HashMap<&str, &Value>,
    ) {
        let Some(tab) = self.layout.tabs.get(self.current_tab) else {
            return;
        };
        let widget = crate::plugins::tab_widget(ty, &tab.name);
        let rect = ui.max_rect();
        let effects = RefCell::new(Vec::new());
        let frame = widgets::Frame {
            traces: &self.traces,
            now_us: self.nt.client().map_or(0, |c| c.now_us()),
            others: values,
            connected: matches!(
                self.nt.status(),
                crate::nt::ConnectionStatus::Connected { .. }
            ),
            effects: &effects,
            structs: self.nt.structs(),
        };
        let live = frame.live(
            None,
            widgets::Place {
                locked: true,
                ..Default::default()
            },
        );
        ty.show_tab(ui, rect, &widget, &live);
        for effect in effects.into_inner() {
            self.apply_effect(effect);
        }
    }
}
