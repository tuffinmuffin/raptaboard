//! Content packs in the window ([`crate::content`]): the Content window
//! (⚙ Settings → Content packs…), where packs are installed, updated and
//! removed and sources added, and the list offered once on the first run.
//!
//! Nothing downloads unless the user asks: opening the Content window or
//! pressing "Check for packs" reads the sources' catalogs, and a pack's
//! files download only from its Install or Update button (or the first-run
//! list's Download). Each pack's license and credits show before installing.

use std::collections::{HashMap, HashSet};

use eframe::egui;

use crate::content::{self, Content, Pack, Status};
use crate::types::robot_state::Mode;

use super::DashboardApp;

/// The Content window and the first-run list: what's open and picked.
#[derive(Default)]
pub(super) struct ContentWindows {
    /// The Content window is open.
    pub open: bool,
    /// A source URL being typed.
    pub new_source: String,
    /// Packs whose credits are shown, by id.
    pub credits: HashSet<String>,
    /// The first-run list, while it's being offered.
    pub offer: Option<Offer>,
}

/// The first-run list of packs.
#[derive(Debug, Default)]
pub(super) struct Offer {
    /// Packs ticked, by id: once the official catalog is read, the
    /// suggested ones.
    pub picked: Option<HashMap<String, bool>>,
}

/// The official source's packs this raptaboard can install, from the
/// last check, if it worked.
fn official_packs(content: &Content) -> Option<Vec<Pack>> {
    let checked = content.checked.as_ref()?;
    let official = checked.iter().find(|c| c.source.official)?;
    let catalog = official.catalog.as_ref().ok()?;
    Some(
        catalog
            .packs
            .iter()
            .filter(|p| content::status(p, content.installed(&p.id)) == Status::Available)
            .cloned()
            .collect(),
    )
}

impl DashboardApp {
    /// Whether the robot is enabled (autonomous, teleop or test): no
    /// pop-ups about content then.
    pub(super) fn robot_running(&self) -> bool {
        matches!(
            self.robot.mode,
            Some(Mode::Autonomous | Mode::Teleop | Mode::Test)
        )
    }

    /// The sources: the official one and those added.
    fn content_sources(&self) -> Vec<content::Source> {
        content::sources(&self.content_sources)
    }

    /// Start the first run's offer, if this is one: read the official
    /// catalog, to list its packs once it arrives. Called once at start.
    pub(super) fn start_content(&mut self) {
        let Some(content) = &mut self.content else {
            return;
        };
        if !content.state().offered {
            content.check(content::sources(&self.content_sources));
            self.packs.offer = Some(Offer::default());
        }
    }

    /// Each frame: pick up what the download thread did, and show the
    /// Content window and the first-run list.
    pub(super) fn content_windows(&mut self, ctx: &egui::Context) {
        let Some(content) = &mut self.content else {
            self.packs.offer = None;
            if self.packs.open {
                self.content_window(ctx);
            }
            return;
        };
        if content.poll() {
            ctx.request_repaint();
        }
        if !content.idle() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        self.first_run_list(ctx);
        if self.packs.open {
            self.content_window(ctx);
        }
    }

    /// The list offered on the first run, once the official catalog is in.
    /// If it can't be read (offline), it's offered again next start.
    fn first_run_list(&mut self, ctx: &egui::Context) {
        let running = self.robot_running();
        let (Some(content), Some(offer)) = (&self.content, &mut self.packs.offer) else {
            return;
        };
        if content.checking {
            return;
        }
        let Some(packs) = official_packs(content) else {
            // Offline, or the catalog is broken: try again next start.
            self.packs.offer = None;
            return;
        };
        if packs.is_empty() {
            self.packs.offer = None;
            let _ = content.save_state(&content::State { offered: true });
            return;
        }
        if running {
            return;
        }
        let picked = offer
            .picked
            .get_or_insert_with(|| packs.iter().map(|p| (p.id.clone(), p.suggested)).collect());
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("content_offer")).show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.heading("Extra content for raptaboard");
            ui.label(
                "These packs can be downloaded from raptaboard-content on GitHub. Nothing \
                 downloads unless you choose it, and you can add or remove packs later in \
                 ⚙ Settings → Content packs.",
            );
            ui.separator();
            for pack in &packs {
                let on = picked.entry(pack.id.clone()).or_insert(false);
                ui.checkbox(on, egui::RichText::new(&pack.name).strong());
                ui.indent(&pack.id, |ui| {
                    ui.label(&pack.summary);
                    ui.label(egui::RichText::new(format!("License: {}", pack.license)).weak());
                    ui.collapsing("Credits and license", |ui| ui.label(&pack.credits));
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                let any = picked.values().any(|on| *on);
                if ui.add_enabled(any, egui::Button::new("Download")).clicked() {
                    answer = Some(true);
                }
                if ui.button("Not now").clicked() {
                    answer = Some(false);
                }
            });
        });
        let Some(download) = answer else {
            return;
        };
        let picked = offer.picked.take().unwrap_or_default();
        self.packs.offer = None;
        let Some(content) = &mut self.content else {
            return;
        };
        if download {
            for pack in packs.iter().filter(|p| picked.get(&p.id) == Some(&true)) {
                content.install(content::OFFICIAL, pack);
            }
        }
        self.status = match content.save_state(&content::State { offered: true }) {
            Ok(()) if download => "Downloading content packs…".into(),
            Ok(()) => String::new(),
            Err(e) => format!("Couldn't save the content packs' state: {e}"),
        };
    }

    /// The Content window: sources, and each pack with its button.
    fn content_window(&mut self, ctx: &egui::Context) {
        let mut open = self.packs.open;
        egui::Window::new("Content packs")
            .id(egui::Id::unique("content_window"))
            .open(&mut open)
            .default_size([560.0, 420.0])
            .collapsible(false)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.content_contents(ui));
            });
        self.packs.open &= open;
    }

    fn content_contents(&mut self, ui: &mut egui::Ui) {
        let sources = self.content_sources();
        let Some(content) = &mut self.content else {
            ui.label("No home folder was found, so packs can't be installed.");
            return;
        };
        // Opening the window is asking: read the catalogs if not yet.
        if content.checked.is_none() {
            content.check(sources.clone());
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!content.checking, egui::Button::new("Check for packs"))
                .on_hover_text("Read each source's list of packs again")
                .clicked()
            {
                content.check(sources.clone());
            }
            if content.checking {
                ui.spinner();
                ui.label("Checking…");
            }
        });
        ui.label(
            egui::RichText::new(format!("Installed in {}", content.dir().display()))
                .weak()
                .small(),
        );
        if let Some(problem) = &content.problem {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
        ui.separator();

        // The packs: what the sources offer, then any installed that none
        // of them lists any more.
        let offered: Vec<(String, Pack)> = content
            .offered()
            .into_iter()
            .map(|(s, p)| (s.url.clone(), p.clone()))
            .collect();
        let listed: HashSet<String> = offered.iter().map(|(_, p)| p.id.clone()).collect();
        let orphans: Vec<content::Installed> = content
            .installed
            .iter()
            .filter(|i| !listed.contains(&i.pack.id))
            .cloned()
            .collect();
        if offered.is_empty() && orphans.is_empty() && !content.checking {
            ui.label("No packs yet: check for packs (needs the internet).");
        }
        for (source, pack) in &offered {
            let status = content::status(pack, content.installed(&pack.id));
            let busy = content.busy.contains(&pack.id);
            ui.horizontal(|ui| {
                ui.strong(&pack.name);
                ui.label(egui::RichText::new(format!("{} · {}", pack.version, pack.kind)).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if busy {
                        ui.spinner();
                        return;
                    }
                    match &status {
                        Status::Available => {
                            if ui.button("Install").clicked() {
                                content.install(source, pack);
                            }
                        }
                        Status::Update { from } => {
                            if ui
                                .button("Update")
                                .on_hover_text(format!("Installed: {from}"))
                                .clicked()
                            {
                                content.install(source, pack);
                            }
                        }
                        Status::Installed => {
                            if ui.button("Remove").clicked() {
                                content.remove(&pack.id);
                            }
                            ui.label("Installed");
                        }
                        Status::NeedsApp => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Needs raptaboard {} or newer",
                                    pack.min_app
                                ))
                                .weak(),
                            );
                        }
                    }
                });
            });
            ui.label(&pack.summary);
            ui.label(egui::RichText::new(format!("License: {}", pack.license)).weak());
            let mut shown = self.packs.credits.contains(&pack.id);
            if ui.toggle_value(&mut shown, "Credits and license").clicked() {
                if shown {
                    self.packs.credits.insert(pack.id.clone());
                } else {
                    self.packs.credits.remove(&pack.id);
                }
            }
            if shown {
                ui.indent(("credits", &pack.id), |ui| ui.label(&pack.credits));
            }
            ui.separator();
        }
        for installed in &orphans {
            let id = &installed.pack.id;
            ui.horizontal(|ui| {
                ui.strong(&installed.pack.name);
                ui.label(egui::RichText::new(&installed.pack.version).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if content.busy.contains(id) {
                        ui.spinner();
                    } else if ui.button("Remove").clicked() {
                        content.remove(id);
                    }
                    ui.label("Installed");
                });
            });
            ui.label(
                egui::RichText::new(format!(
                    "From {}, which doesn't list it now",
                    installed.source
                ))
                .weak(),
            );
            ui.separator();
        }

        // The sources.
        ui.strong("Sources");
        let checked = content.checked.clone().unwrap_or_default();
        let mut remove = None;
        for source in &sources {
            ui.horizontal(|ui| {
                if source.official {
                    ui.label("raptaboard-content (official, signed)")
                        .on_hover_text(&source.url);
                } else {
                    ui.label(&source.url);
                    ui.label(egui::RichText::new("not signed").weak());
                    if ui.small_button("Remove").clicked() {
                        remove = Some(source.url.clone());
                    }
                }
            });
            if let Some(c) = checked.iter().find(|c| c.source == *source) {
                if let Err(e) = &c.catalog {
                    ui.colored_label(ui.visuals().warn_fg_color, e);
                }
                if let Some(note) = &c.note {
                    ui.colored_label(ui.visuals().warn_fg_color, note);
                }
            }
        }
        ui.horizontal(|ui| {
            let typed = ui.add(
                egui::TextEdit::singleline(&mut self.packs.new_source)
                    .hint_text("https://…/catalog.json")
                    .desired_width(320.0),
            );
            typed.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::TextInput, true, "Source URL")
            });
            let url = self.packs.new_source.trim().to_owned();
            let good = (url.starts_with("https://") || url.starts_with("http://"))
                && !sources.iter().any(|s| s.url == url);
            if ui
                .add_enabled(good, egui::Button::new("Add source"))
                .clicked()
            {
                self.content_sources.push(url);
                self.packs.new_source.clear();
                content.check(content::sources(&self.content_sources));
            }
        });
        ui.label(
            egui::RichText::new(
                "Add only sources you trust: their packs aren't signed by raptaboard \
                 (each file is still checked against its catalog).",
            )
            .weak()
            .small(),
        );
        if let Some(url) = remove {
            self.content_sources.retain(|u| *u != url);
            content.check(content::sources(&self.content_sources));
        }
    }
}
