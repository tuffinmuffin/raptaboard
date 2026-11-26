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

/// How long after starting the first background check for updates waits,
/// so it never holds up opening the window.
pub(super) const FIRST_CHECK_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

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

/// The list of packs offered on the first start and after an upgrade.
#[derive(Debug, Default)]
pub(super) struct Offer {
    /// Packs ticked, by id: once the official catalog is read, the
    /// suggested ones not offered before.
    pub picked: Option<HashMap<String, bool>>,
    /// The packs offered before, which start unticked.
    pub shown: Vec<String>,
    /// After Download: the packs downloading, shown until Done.
    pub installing: Vec<Pack>,
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

/// The first-run window after Download: each pack downloading, then
/// installed (with its sounds) or why not, and Done once none is still
/// downloading. Returns whether Done was clicked.
fn installing_window(ctx: &egui::Context, content: &Content, packs: &[Pack]) -> bool {
    let busy = packs.iter().any(|p| content.busy.contains(&p.id));
    let failed = packs
        .iter()
        .any(|p| content.outcome(&p.id).is_some_and(|o| o.result.is_err()));
    let mut done = false;
    egui::Modal::new(egui::Id::unique("content_offer")).show(ctx, |ui| {
        ui.set_max_width(520.0);
        ui.heading(if busy {
            "Downloading content packs…"
        } else if failed {
            "Some packs didn't install"
        } else {
            "Content packs installed"
        });
        ui.separator();
        for pack in packs {
            ui.horizontal_wrapped(|ui| {
                if content.busy.contains(&pack.id) {
                    ui.spinner();
                    ui.label(format!("Downloading {}…", pack.name));
                    return;
                }
                match content.outcome(&pack.id) {
                    Some(o) if o.result.is_ok() => {
                        ui.label("✔");
                        ui.label(o.describe());
                    }
                    Some(o) => {
                        ui.colored_label(ui.visuals().error_fg_color, "✖");
                        ui.colored_label(ui.visuals().error_fg_color, o.describe());
                    }
                    None => {
                        ui.spinner();
                        ui.label(format!("Downloading {}…", pack.name));
                    }
                }
            });
        }
        ui.separator();
        if failed {
            ui.label("You can try again in ⚙ Settings → Content packs.");
        } else {
            ui.label("Add or remove packs any time in ⚙ Settings → Content packs.");
        }
        if ui.add_enabled(!busy, egui::Button::new("Done")).clicked() {
            done = true;
        }
    });
    done
}

/// Remember that the list of `packs` was answered: not pending, and those
/// packs shown.
fn answered(content: &Content, packs: &[Pack]) -> Result<(), String> {
    let mut state = content.state();
    state.offer_pending = false;
    for pack in packs {
        if !state.shown.contains(&pack.id) {
            state.shown.push(pack.id.clone());
        }
    }
    content.save_state(&state)
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
        content::sources(&self.content_sources, self.content_prereleases)
    }

    /// Called once at start: on the first start of a new version (or the
    /// very first), run its one-time tasks ([`crate::upgrade`]); then if
    /// the packs are still to be offered, read the official catalog to
    /// list them once it arrives.
    pub(super) fn start_content(&mut self) {
        let Some(content) = &mut self.content else {
            return;
        };
        let mut state = content.state();
        if crate::upgrade::start(&mut state).is_some()
            && let Err(e) = content.save_state(&state)
        {
            self.status = format!("Couldn't save the content packs' state: {e}");
        }
        if state.offer_pending {
            content.check(content::sources(
                &self.content_sources,
                self.content_prereleases,
            ));
            self.packs.offer = Some(Offer {
                picked: None,
                shown: state.shown,
                installing: Vec::new(),
            });
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
        // Packs installed or removed (or the first frame): their plugins.
        if std::mem::take(&mut content.plugins_changed) {
            let problems = crate::plugins::use_packs(content.dir(), &content.plugins());
            for p in &problems {
                eprintln!("raptaboard: plugin not loaded: {p}");
            }
            if let Some(first) = problems.first() {
                self.status = format!("Plugin not loaded: {first}");
            }
            ctx.request_repaint();
        }
        if !content.idle() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        // The background check for updates: every content_check_minutes,
        // only while the robot isn't enabled and no list is showing. If it
        // can't reach a source, it says nothing (the Content window shows
        // why, if opened) and tries again next time.
        let quiet = !matches!(
            self.robot.mode,
            Some(Mode::Autonomous | Mode::Teleop | Mode::Test)
        ) && self.packs.offer.is_none();
        let every = (self.content_check_minutes > 0)
            .then(|| std::time::Duration::from_secs(u64::from(self.content_check_minutes) * 60));
        if content.idle()
            && self
                .content_timer
                .due(std::time::Instant::now(), every, quiet)
        {
            content.check(content::sources(
                &self.content_sources,
                self.content_prereleases,
            ));
        }
        self.first_run_list(ctx);
        if self.packs.open {
            self.content_window(ctx);
        }
    }

    /// The list offered on the first start and after an upgrade, once the
    /// official catalog is in: the packs not installed. If it can't be read
    /// (offline), it's offered again next start.
    fn first_run_list(&mut self, ctx: &egui::Context) {
        let running = self.robot_running();
        let (Some(content), Some(offer)) = (&self.content, &mut self.packs.offer) else {
            return;
        };
        if !offer.installing.is_empty() {
            if !running && installing_window(ctx, content, &offer.installing) {
                self.packs.offer = None;
            }
            return;
        }
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
            let _ = answered(content, &[]);
            return;
        }
        if running {
            return;
        }
        let shown = &offer.shown;
        let picked = offer.picked.get_or_insert_with(|| {
            packs
                .iter()
                .map(|p| (p.id.clone(), p.suggested && !shown.contains(&p.id)))
                .collect()
        });
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
                    for line in pack.plugin_lines() {
                        ui.label(egui::RichText::new(line).weak());
                    }
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
        let chosen: Vec<Pack> = packs
            .iter()
            .filter(|p| download && picked.get(&p.id) == Some(&true))
            .cloned()
            .collect();
        let Some(content) = &mut self.content else {
            return;
        };
        for pack in &chosen {
            content.install(content::OFFICIAL, pack);
        }
        if let Err(e) = answered(content, &packs) {
            self.status = format!("Couldn't save the content packs' state: {e}");
        }
        content.refresh_news();
        // Downloading: the window stays, showing each pack until Done.
        match &mut self.packs.offer {
            Some(offer) if !chosen.is_empty() => offer.installing = chosen,
            _ => self.packs.offer = None,
        }
    }

    /// The update notice in the bar, when a check found something: "📦 1
    /// update · Review · ✕". Review opens the Content window; ✕ hides it
    /// until something newer. Not shown while the robot is enabled.
    pub(super) fn content_notice(&mut self, ui: &mut egui::Ui) {
        if self.robot_running() || self.packs.offer.is_some() {
            return;
        }
        let Some(content) = &mut self.content else {
            return;
        };
        let words = content::news_text(content.news());
        if words.is_empty() {
            return;
        }
        let names: Vec<String> = content
            .news()
            .iter()
            .map(|n| match n {
                content::News::Update { pack, from } => {
                    format!("{} {} (installed: {from})", pack.name, pack.version)
                }
                content::News::New(pack) => format!("{} (new)", pack.name),
            })
            .collect();
        ui.separator();
        ui.label(format!("📦 Content: {words}"))
            .on_hover_text(names.join("\n"));
        if ui
            .small_button("Review")
            .on_hover_text("Open the Content packs window")
            .clicked()
        {
            self.packs.open = true;
        }
        if ui
            .small_button("✕")
            .on_hover_text("Hide this until something newer comes")
            .clicked()
            && let Err(e) = content.dismiss_news()
        {
            self.status = format!("Couldn't save the content packs' state: {e}");
        }
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
        if let Some(last) = content.outcomes.last() {
            let color = if last.result.is_ok() {
                ui.visuals().text_color()
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(color, last.describe());
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
                ui.label(
                    egui::RichText::new(format!("{} · {}", pack.version, pack.contents())).weak(),
                );
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
            for line in pack.plugin_lines() {
                ui.label(egui::RichText::new(line).weak());
            }
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
        let mut prereleases_changed = false;
        for source in &sources {
            ui.horizontal(|ui| {
                if source.official {
                    ui.label("raptaboard-content (official, signed)")
                        .on_hover_text(&source.url);
                    prereleases_changed = ui
                        .checkbox(&mut self.content_prereleases, "Pre-releases")
                        .on_hover_text(
                            "Also offer packs being tried before they're released: the \
                             newest content release, a pre-release or not. Still signed.",
                        )
                        .changed();
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
                content.check(content::sources(
                    &self.content_sources,
                    self.content_prereleases,
                ));
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
        if prereleases_changed {
            content.check(content::sources(
                &self.content_sources,
                self.content_prereleases,
            ));
        }
        if let Some(url) = remove {
            self.content_sources.retain(|u| *u != url);
            content.check(content::sources(
                &self.content_sources,
                self.content_prereleases,
            ));
        }
    }
}
