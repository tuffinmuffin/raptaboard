//! The ⚙ Settings window (robot connection, sound, the robot state's
//! border and e-stop pop-up) and the "About and
//! licenses" window it opens: raptaboard's version and license, where the
//! built-in sounds, images and fields came from, and every Rust crate it's
//! built from with its license (from [`crate::credits`]).

use std::sync::LazyLock;

use eframe::egui;

use crate::connection::AddressMode;
use crate::credits;

use super::DashboardApp;

/// Which part of the About window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum AboutPage {
    /// The version and raptaboard's own license.
    #[default]
    Raptaboard,
    /// The built-in sounds, images and fields.
    Media,
    /// The privacy policy.
    Privacy,
    /// The Rust crates, with their licenses.
    Crates,
    /// The crates' full license texts.
    Texts,
}

/// The gear's windows: which are open, and what's picked in them.
#[derive(Default)]
pub(super) struct GearWindows {
    /// The Settings window is open.
    pub settings: bool,
    /// The About and licenses window is open.
    pub about: bool,
    /// The About window's page.
    pub page: AboutPage,
    /// The filter typed above the crate list.
    pub crate_filter: String,
}

/// How far down the Settings window opens, in points: below the settings
/// bar and the tab bar.
const SETTINGS_TOP: f32 = 96.0;

/// The full license texts, a line at a time, for drawing only the lines shown.
static TEXT_LINES: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| credits::THIRD_PARTY_LICENSES.lines().collect());

impl DashboardApp {
    /// The ⚙ button that opens the Settings window.
    pub(super) fn gear_button(&mut self, ui: &mut egui::Ui) {
        if ui
            .selectable_label(self.gear.settings, "⚙")
            .on_hover_text("Settings: robot connection, sound, about and licenses")
            .clicked()
        {
            self.gear.settings = !self.gear.settings;
        }
    }

    /// The Settings and About windows, while open.
    pub(super) fn gear_windows(&mut self, ctx: &egui::Context) {
        let mut open = self.gear.settings;
        // Top right, below the bars, so it doesn't cover the ⚙ that
        // closes it or the bar's other buttons.
        let corner = egui::pos2(ctx.content_rect().max.x - 16.0, SETTINGS_TOP);
        egui::Window::new("Settings")
            .id(egui::Id::unique("settings_window"))
            .default_pos(corner)
            .pivot(egui::Align2::RIGHT_TOP)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| self.settings_contents(ui));
        self.gear.settings &= open;
        let mut open = self.gear.about;
        egui::Window::new("About and licenses")
            .id(egui::Id::unique("about_window"))
            .open(&mut open)
            .default_size([620.0, 480.0])
            .collapsible(false)
            .show(ctx, |ui| self.about_contents(ui));
        self.gear.about &= open;
    }

    fn settings_contents(&mut self, ui: &mut egui::Ui) {
        ui.strong("Robot connection");
        egui::Grid::new("connection_settings")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Team");
                // 0 is "no team yet", shown blank.
                ui.add(
                    egui::DragValue::new(&mut self.address.team)
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
                ui.end_row();

                ui.label("Server");
                // Named for screen readers; on screen the choices say what it is.
                egui::ComboBox::from_id_salt("address_mode")
                    .selected_text(self.address.mode.label())
                    .show_ui(ui, |ui| {
                        for mode in AddressMode::ALL {
                            ui.selectable_value(&mut self.address.mode, mode, mode.label());
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
                ui.end_row();

                if self.address.mode == AddressMode::Custom {
                    ui.label("Address");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.address.custom)
                            .hint_text("Host name or IP address"),
                    )
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::accesskit::Role::TextInput,
                            true,
                            "Server address",
                        )
                    });
                    ui.end_row();
                }

                ui.label("Port");
                ui.add(egui::DragValue::new(&mut self.address.port).range(1..=65535))
                    .on_hover_text(format!(
                        "The NetworkTables server's port; robots use {}",
                        nt4::proto::ROBOT_CODE_PORT
                    ));
                ui.end_row();
            });
        ui.separator();
        ui.strong("Sound");
        ui.checkbox(&mut self.alert_sound, "Play tones and clips")
            .on_hover_text(
                "A failed send's tone and the Alerts widget's sounds. The bell in the bar does the same.",
            );
        ui.separator();
        ui.strong("Robot state");
        // Kept in the layout file, so saved with it, but a setting, not an
        // edit: undo and redo leave it alone.
        let kept = super::robot_state_view::Look::of(&self.layout);
        let mut look = kept.clone();
        super::robot_state_view::settings_rows(ui, &mut look);
        if look != kept {
            self.history
                .set_everywhere(&mut self.layout, |l| look.write_to(l));
            self.robot.look = look;
        }
        ui.separator();
        ui.strong("Content packs");
        ui.horizontal(|ui| {
            ui.label("Check for updates every");
            ui.add(
                egui::DragValue::new(&mut self.content_check_minutes)
                    .range(0..=24 * 60)
                    .suffix(" min"),
            )
            .on_hover_text(
                "How often to look for new content packs and updates, only while the robot \
                 isn't enabled. 0 never checks.",
            )
            .widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::accesskit::Role::SpinButton,
                    true,
                    "Content update check, minutes",
                )
            });
            if self.content_check_minutes == 0 {
                ui.weak("(never)");
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("Content packs…")
                .on_hover_text("Download sound packs, such as the IT Crowd clips")
                .clicked()
            {
                self.packs.open = true;
            }
            if ui.button("About and licenses…").clicked() {
                self.gear.about = true;
            }
        });
    }

    fn about_contents(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (page, label) in [
                (AboutPage::Raptaboard, "raptaboard"),
                (AboutPage::Media, "Sounds, images, fields"),
                (AboutPage::Privacy, "Privacy"),
                (AboutPage::Crates, "Rust crates"),
                (AboutPage::Texts, "License texts"),
            ] {
                ui.selectable_value(&mut self.gear.page, page, label);
            }
        });
        ui.separator();
        match self.gear.page {
            AboutPage::Raptaboard => about_raptaboard(ui),
            AboutPage::Media => about_media(ui),
            AboutPage::Privacy => about_privacy(ui),
            AboutPage::Crates => about_crates(ui, &mut self.gear.crate_filter),
            AboutPage::Texts => about_texts(ui),
        }
    }
}

fn about_raptaboard(ui: &mut egui::Ui) {
    ui.heading(format!("raptaboard {}", env!("CARGO_PKG_VERSION")));
    ui.label("An FRC driver dashboard by Team 3200, Raptacon.");
    ui.hyperlink(env!("CARGO_PKG_REPOSITORY"));
    ui.add_space(6.0);
    ui.label(
        "raptaboard is under the MIT license below. It includes sounds, images and fields \
         from others, and is built from open-source Rust crates; the other pages say where \
         each came from and its license.",
    );
    ui.add_space(6.0);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.monospace(credits::LICENSE);
    });
}

fn about_privacy(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("Also online:");
        ui.hyperlink(credits::PRIVACY_URL);
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for block in credits::policy_blocks(credits::PRIVACY) {
            match block {
                credits::PolicyBlock::Title(t) => {
                    ui.heading(t);
                }
                credits::PolicyBlock::Heading(h) => {
                    ui.add_space(6.0);
                    ui.strong(h);
                }
                credits::PolicyBlock::Paragraph(p) => {
                    ui.label(p);
                }
                credits::PolicyBlock::Item(i) => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("•");
                        ui.label(i);
                    });
                }
            }
        }
    });
}

fn about_media(ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        for m in credits::MEDIA {
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.strong(format!("{}: {}", m.kind, m.what));
                ui.weak(m.files.join(", "));
                ui.label(format!("From {}", m.source));
                ui.horizontal(|ui| {
                    ui.label("License:");
                    match m.link {
                        Some(link) => {
                            ui.hyperlink_to(m.license, link);
                        }
                        None => {
                            ui.label(m.license);
                        }
                    }
                });
                if !m.note.is_empty() {
                    ui.label(m.note);
                }
            });
        }
        ui.collapsing("WPILib's license (the field)", |ui| {
            ui.monospace(credits::WPILIB_LICENSE);
        });
    });
}

fn about_crates(ui: &mut egui::Ui, filter: &mut String) {
    let crates = credits::crates();
    ui.label(format!(
        "raptaboard is built from {} Rust crates. Their licenses:",
        crates.len()
    ));
    ui.horizontal_wrapped(|ui| {
        for (license, n) in credits::license_counts() {
            ui.weak(format!("{license} ({n})"));
        }
    });
    ui.add(
        egui::TextEdit::singleline(filter)
            .hint_text("Find a crate or license")
            .desired_width(f32::INFINITY),
    );
    let needle = filter.to_lowercase();
    let shown: Vec<_> = crates
        .iter()
        .filter(|c| {
            needle.is_empty()
                || c.name.to_lowercase().contains(&needle)
                || c.license.to_lowercase().contains(&needle)
        })
        .collect();
    let row = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show_rows(ui, row, shown.len(), |ui, range| {
            for c in &shown[range] {
                ui.horizontal(|ui| {
                    let name = format!("{} {}", c.name, c.version);
                    if c.repository.is_empty() {
                        ui.label(name);
                    } else {
                        ui.hyperlink_to(name, c.repository);
                    }
                    ui.weak(c.license);
                });
            }
        });
}

fn about_texts(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("Every license text the crates ship, and the standard texts for the rest.");
        if ui.button("Copy all").clicked() {
            ui.ctx().copy_text(credits::THIRD_PARTY_LICENSES.to_owned());
        }
    });
    let lines = &*TEXT_LINES;
    let row = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::both()
        .auto_shrink(false)
        .show_rows(ui, row, lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for line in &lines[range] {
                ui.monospace(*line);
            }
        });
}
