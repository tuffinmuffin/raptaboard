//! The sound picker the FMSInfo and Alerts settings share: every built-in
//! sound and every installed pack's ([`crate::sound::sound_choices`]), so
//! "fire 1" can play the newest pack's sound, a pinned pack's, or the
//! built-in one.

use eframe::egui;

use crate::sound::{self, SoundLayers};

/// The installed packs' sounds, for the pickers in this frame.
pub fn installed(ui: &egui::Ui) -> SoundLayers {
    sound::installed(ui.ctx())
}

/// The picker's sound entries: a click sets `picked` to that sound's
/// setting. A pack's and the built-in sound of a name sit indented under it.
pub fn sound_menu(ui: &mut egui::Ui, picked: &mut String, layers: &SoundLayers) {
    for choice in sound::sound_choices(layers) {
        let label = if choice.under {
            format!("    {}", choice.label)
        } else {
            choice.label
        };
        ui.selectable_value(picked, choice.setting, label);
    }
}

/// A sound dropdown showing `shown`, named `name` for screen readers (and
/// tests) with `shown` as its value; `menu` fills it.
pub fn sound_combo(
    ui: &mut egui::Ui,
    id: &str,
    name: &str,
    shown: &str,
    menu: impl FnOnce(&mut egui::Ui),
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(170.0)
        .show_ui(ui, menu)
        .response
        .widget_info(|| {
            let mut info =
                egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, ui.is_enabled(), name);
            info.current_text_value = Some(shown.to_owned());
            info
        });
}

/// A red "not installed" beside a picker whose sound's pack isn't
/// installed (it plays the built-in sound of its name, or nothing).
pub fn missing_note(ui: &mut egui::Ui, setting: &str) {
    ui.colored_label(ui.visuals().error_fg_color, "not installed")
        .on_hover_text(format!(
            "No installed pack has \"{setting}\": the built-in sound of its name plays, if there is one"
        ));
}
