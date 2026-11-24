//! `raptaboard packs …`: list, install, update and remove content packs
//! from the command line, without opening the window. It uses the same
//! sources, checks and packs folder as the Content window
//! ([`crate::content`]).
//!
//! ```text
//! raptaboard packs list
//! raptaboard packs install fan-clips
//! raptaboard packs install --package path/to/my-pack --unsigned
//! raptaboard packs install horns --url https://example.org/catalog.json --unsigned
//! raptaboard packs update            (every installed pack with an update)
//! raptaboard packs remove fan-clips
//! raptaboard packs sources
//! ```
//!
//! The packs come from the sources in the settings (the official one, then
//! those added), or only from `--url` (a catalog) or `--package` (a pack
//! folder, [`crate::pack_folder`]). The official catalog must be signed;
//! any other `--url`, and any `--package`, needs `--unsigned`, so nothing
//! unsigned installs by accident. HTTPS certificates are always checked.
//! Before installing, each pack's license and credits are shown and the
//! user is asked, unless `--yes`.

use std::io::Write;
use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::content::{
    self, Action, Checked, Fetch, OFFICIAL, Outcome, Pack, Source, Status, Store, offers, status,
};
use crate::pack_folder;

/// `raptaboard packs …`.
#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Packs {
    /// List the installed packs, and those the sources have.
    List {
        #[command(flatten)]
        from: From,
    },
    /// Install packs (or install them again), after showing each one's license.
    Install {
        /// The packs' ids; with --package, may be left out.
        #[arg(value_name = "ID")]
        ids: Vec<String>,
        #[command(flatten)]
        from: From,
        /// Accept each pack's license without asking.
        #[arg(long, short)]
        yes: bool,
    },
    /// Update installed packs that a source has a newer version of.
    Update {
        /// The packs' ids; every installed pack if none.
        #[arg(value_name = "ID")]
        ids: Vec<String>,
        #[command(flatten)]
        from: From,
        /// Accept each pack's license without asking.
        #[arg(long, short)]
        yes: bool,
    },
    /// Remove installed packs.
    Remove {
        /// The packs' ids.
        #[arg(value_name = "ID", required = true)]
        ids: Vec<String>,
    },
    /// List the sources packs come from.
    Sources,
}

/// Where the packs come from, if not the sources in the settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct From {
    /// Use only the catalog at this URL.
    #[arg(long, value_name = "URL", conflicts_with = "package")]
    pub url: Option<String>,
    /// Use only the pack in this folder (its pack.toml and files).
    #[arg(long, value_name = "FOLDER")]
    pub package: Option<PathBuf>,
    /// Allow a catalog or pack not signed by raptaboard: needed for --package,
    /// and for a --url other than the official catalog. HTTPS certificates
    /// are still checked.
    #[arg(long)]
    pub unsigned: bool,
}

/// What a command works with: where packs are installed, the settings'
/// sources, how to download, and how to talk to the user.
pub struct Env<'a> {
    /// The packs folder.
    pub store: Store,
    /// The sources added in the settings (`content_sources`).
    pub sources: Vec<String>,
    /// The keys the official catalog may be signed with.
    pub keys: &'a [[u8; 32]],
    /// Downloads a URL.
    pub fetch: &'a Fetch,
    /// Where to print.
    pub out: &'a mut dyn Write,
    /// Asks a yes-or-no question; `false` if there's no answer.
    pub ask: &'a mut dyn FnMut(&str) -> bool,
}

/// The catalogs a command read: each source as the Content window checks
/// it ([`Source::read`]).
struct Read {
    /// Each source, its catalog (or why not) and any note about it.
    checked: Vec<Checked>,
    /// From a pack folder on this computer (`--package`): its files are
    /// read from disk, not downloaded.
    local: bool,
}

/// Run `command`. An error is what to print before exiting with a failure;
/// a command that did part of its work also fails, having printed why.
pub fn run(command: &Packs, env: &mut Env<'_>) -> Result<(), String> {
    match command {
        Packs::List { from } => list(from, env),
        Packs::Install { ids, from, yes } => install(ids, from, *yes, env),
        Packs::Update { ids, from, yes } => update(ids, from, *yes, env),
        Packs::Remove { ids } => remove(ids, env),
        Packs::Sources => {
            for s in content::sources(&env.sources) {
                let note = if s.official {
                    "  (official, signed)"
                } else {
                    "  (added, not signed)"
                };
                say(env, &format!("{}{note}", s.url));
            }
            Ok(())
        }
    }
}

/// Run `command` for a program's `main` (`raptaboard` or `raptaboard-cli`):
/// on the packs folder under `home`, with the settings' added `sources`,
/// printing to the terminal and asking with `ask`. The exit code: 0 if it
/// all worked, else 1, after printing why as `program: …`.
pub fn run_in_home(
    program: &str,
    command: &Packs,
    sources: Vec<String>,
    home: Option<&std::path::Path>,
    ask: &mut dyn FnMut(&str) -> bool,
) -> i32 {
    let Some(home) = home else {
        eprintln!("{program}: no home folder to keep packs in");
        return 1;
    };
    let dir = home.join(crate::settings::DIR_NAME).join(content::DIR_NAME);
    let mut out = std::io::stdout();
    let mut env = Env {
        store: Store::new(dir),
        sources,
        keys: content::OFFICIAL_KEYS,
        fetch: &content::http_get,
        out: &mut out,
        ask,
    };
    let code = match run(command, &mut env) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{program}: {e}");
            1
        }
    };
    let _ = std::io::stdout().flush();
    code
}

/// Ask `question` on the terminal (stdin): whether the answer was yes.
/// No answer (stdin closed) is no.
pub fn ask_terminal(question: &str) -> bool {
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).is_ok_and(|n| n > 0)
        && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Print a line (a closed output is no reason to stop).
fn say(env: &mut Env<'_>, line: &str) {
    let _ = writeln!(env.out, "{line}");
}

/// Read the catalogs `from` names: the settings' sources, or only its
/// `--url` or `--package`. Refuses unsigned ones without `--unsigned`.
/// Prints any note about a catalog (an older release's being used).
fn read(from: &From, env: &mut Env<'_>) -> Result<Read, String> {
    let read = if let Some(folder) = &from.package {
        if !from.unsigned {
            return Err(format!(
                "{} is a pack folder, which isn't signed by raptaboard: add --unsigned to use it",
                folder.display()
            ));
        }
        Read {
            checked: vec![Checked {
                source: Source {
                    url: format!("file:{}", folder.display()),
                    official: false,
                },
                catalog: pack_folder::catalog(folder),
                note: None,
            }],
            local: true,
        }
    } else {
        let sources = match &from.url {
            Some(url) if url != OFFICIAL && !from.unsigned => {
                return Err(format!(
                    "{url} isn't the official catalog, so it isn't signed by raptaboard: add --unsigned to use it"
                ));
            }
            Some(url) => vec![Source {
                url: url.clone(),
                official: url == OFFICIAL,
            }],
            None => content::sources(&env.sources),
        };
        Read {
            checked: sources
                .into_iter()
                .map(|s| s.read(env.keys, env.fetch))
                .collect(),
            local: false,
        }
    };
    for c in &read.checked {
        if let Some(note) = &c.note {
            say(env, &format!("Note: {note}"));
        }
    }
    Ok(read)
}

/// Print each catalog that couldn't be read, and why.
fn report_unread(read: &Read, env: &mut Env<'_>) {
    for c in &read.checked {
        if let Err(e) = &c.catalog {
            say(env, &format!("Couldn't read {}: {e}", c.source.url));
        }
    }
}

/// The pack `id` from the first source that has it, and that source, as
/// the Content window picks it ([`offers`]).
fn find<'r>(read: &'r Read, id: &str) -> Option<(&'r Source, &'r Pack)> {
    offers(&read.checked).into_iter().find(|(_, p)| p.id == id)
}

fn list(from: &From, env: &mut Env<'_>) -> Result<(), String> {
    let installed = env.store.installed();
    let reads = read(from, env)?;
    say(env, "Installed:");
    if installed.is_empty() {
        say(env, "  (none)");
    }
    for i in &installed {
        let newer = find(&reads, &i.pack.id)
            .and_then(|(_, p)| match status(p, Some(i)) {
                Status::Update { .. } => Some(format!("  (update: {})", p.version)),
                _ => None,
            })
            .unwrap_or_default();
        say(
            env,
            &format!(
                "  {} {}  {}: {}{newer}",
                i.pack.id,
                i.pack.version,
                i.pack.name,
                i.pack.contents()
            ),
        );
    }
    say(env, "Available:");
    let mut any = false;
    for (_, p) in offers(&reads.checked) {
        if installed.iter().any(|i| i.pack.id == p.id) {
            continue;
        }
        any = true;
        let needs = match status(p, None) {
            Status::NeedsApp => format!("  (needs raptaboard {})", p.min_app),
            _ => String::new(),
        };
        say(
            env,
            &format!(
                "  {} {}  {}: {} License: {}{needs}",
                p.id, p.version, p.name, p.summary, p.license
            ),
        );
    }
    if !any {
        say(env, "  (none)");
    }
    report_unread(&reads, env);
    Ok(())
}

/// Show `pack`'s license and credits, and ask to install it (yes if
/// `yes`). Prints why not if not.
fn agreed(pack: &Pack, yes: bool, env: &mut Env<'_>) -> bool {
    say(env, "");
    say(
        env,
        &format!("{} {} ({})", pack.name, pack.version, pack.id),
    );
    say(env, &format!("  {}", pack.summary));
    say(env, &format!("  Holds: {}", pack.contents()));
    say(env, &format!("  License: {}", pack.license));
    for line in pack.credits.trim().lines() {
        say(env, &format!("    {line}"));
    }
    if yes || (env.ask)(&format!("Install {} under this license? [y/N] ", pack.name)) {
        return true;
    }
    say(
        env,
        &format!(
            "Didn't install {} (add --yes to accept without asking).",
            pack.id
        ),
    );
    false
}

/// Install `pack` from `source`, saying what happened as the Content
/// window does ([`Outcome::describe`]).
fn put(read: &Read, source: &Source, pack: &Pack, env: &mut Env<'_>) -> bool {
    let fetch: &Fetch = if read.local {
        &pack_folder::read_file
    } else {
        env.fetch
    };
    let outcome = Outcome {
        action: Action::Install,
        pack: pack.clone(),
        result: env.store.install(&source.url, pack, fetch),
    };
    say(env, &outcome.describe());
    outcome.result.is_ok()
}

/// The note after a change: a running raptaboard sees it when it starts again.
const RESTART: &str = "If raptaboard is open, restart it to use the change.";

fn install(ids: &[String], from: &From, yes: bool, env: &mut Env<'_>) -> Result<(), String> {
    let reads = read(from, env)?;
    let mut ids = ids.to_vec();
    if ids.is_empty() {
        // A folder's pack needs no id.
        match (&from.package, reads.checked.first().map(|c| &c.catalog)) {
            (Some(_), Some(Ok(c))) => ids = c.packs.iter().map(|p| p.id.clone()).collect(),
            (Some(_), Some(Err(e))) => return Err(e.clone()),
            _ => return Err("name the packs to install (see raptaboard packs list)".into()),
        }
    }
    let mut ok = true;
    let mut changed = false;
    for id in &ids {
        let Some((source, pack)) = find(&reads, id) else {
            say(env, &format!("No source has a pack {id}."));
            ok = false;
            continue;
        };
        if status(pack, None) == Status::NeedsApp {
            say(
                env,
                &format!(
                    "{id} needs raptaboard {} or newer; this is {}.",
                    pack.min_app,
                    content::app_version()
                ),
            );
            ok = false;
            continue;
        }
        if !agreed(pack, yes, env) {
            ok = false;
            continue;
        }
        let done = put(&reads, source, pack, env);
        changed |= done;
        ok &= done;
    }
    finish(Some(&reads), ok, changed, env)
}

fn update(ids: &[String], from: &From, yes: bool, env: &mut Env<'_>) -> Result<(), String> {
    let installed = env.store.installed();
    for id in ids {
        if !installed.iter().any(|i| i.pack.id == *id) {
            return Err(format!(
                "{id} isn't installed (raptaboard packs install {id})"
            ));
        }
    }
    let reads = read(from, env)?;
    let mut ok = true;
    let mut changed = false;
    let mut any = false;
    for i in installed
        .iter()
        .filter(|i| ids.is_empty() || ids.contains(&i.pack.id))
    {
        let id = &i.pack.id;
        let Some((source, pack)) = find(&reads, id) else {
            if !ids.is_empty() {
                say(env, &format!("No source has {id} now."));
                ok = false;
            }
            continue;
        };
        match status(pack, Some(i)) {
            Status::Installed => {
                if !ids.is_empty() {
                    say(env, &format!("{id} {} is up to date.", i.pack.version));
                }
            }
            Status::NeedsApp => {
                say(
                    env,
                    &format!(
                        "{id} {} needs raptaboard {} or newer.",
                        pack.version, pack.min_app
                    ),
                );
                ok = false;
            }
            // (Available can't be: it's installed.)
            Status::Update { .. } | Status::Available => {
                any = true;
                if agreed(pack, yes, env) {
                    say(
                        env,
                        &format!("Updating {id} from {} to {}.", i.pack.version, pack.version),
                    );
                    let done = put(&reads, source, pack, env);
                    changed |= done;
                    ok &= done;
                } else {
                    ok = false;
                }
            }
        }
    }
    if !any && ok && ids.is_empty() {
        say(env, "Every installed pack is up to date.");
    }
    finish(Some(&reads), ok, changed, env)
}

/// Report unread sources, the restart note after a change, and whether it
/// all worked.
fn finish(reads: Option<&Read>, ok: bool, changed: bool, env: &mut Env<'_>) -> Result<(), String> {
    if !ok && let Some(reads) = reads {
        report_unread(reads, env);
    }
    if changed {
        say(env, RESTART);
    }
    if ok {
        Ok(())
    } else {
        Err("not everything was done (see above)".into())
    }
}

fn remove(ids: &[String], env: &mut Env<'_>) -> Result<(), String> {
    let installed = env.store.installed();
    let mut ok = true;
    let mut changed = false;
    for id in ids {
        let Some(i) = installed.iter().find(|i| i.pack.id == *id) else {
            say(env, &format!("{id} isn't installed."));
            ok = false;
            continue;
        };
        let outcome = Outcome {
            action: Action::Remove,
            pack: i.pack.clone(),
            result: env.store.remove(id),
        };
        say(env, &outcome.describe());
        changed |= outcome.result.is_ok();
        ok &= outcome.result.is_ok();
    }
    finish(None, ok, changed, env)
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use crate::content::Catalog;
    use crate::content::tests::{Web, folder, sound_pack, test_key};

    /// The official catalog (signed): `fan-clips` 1.0.0 and `extra`.
    fn official(web: &Web) -> [u8; 32] {
        let (key, public) = test_key();
        let base = "https://packs";
        let catalog = Catalog {
            format: content::FORMAT,
            release: "v1".into(),
            packs: vec![
                sound_pack("fan-clips", "1.0.0", &[("fire 1", "fire1.mp3")], base),
                sound_pack("extra", "1.0.0", &[("boop", "boop.mp3")], base),
            ],
        };
        web.serve(OFFICIAL, &catalog, Some(&key));
        public
    }

    /// What a run printed, and what it asked.
    struct Ran {
        result: Result<(), String>,
        out: String,
        asked: Vec<String>,
    }

    /// Run `raptaboard packs <args>` on the packs folder `dir`, answering
    /// `answer` to every question.
    fn run_in(
        dir: &std::path::Path,
        web: &Web,
        public: [u8; 32],
        args: &[&str],
        answer: bool,
    ) -> Ran {
        let argv = std::iter::once("packs")
            .chain(args.iter().copied())
            .map(String::from);
        let (_, command) = crate::cli::parse_command(argv).unwrap();
        let Some(crate::cli::Command::Packs(packs)) = command else {
            panic!("not a packs command")
        };
        let mut out = Vec::new();
        let mut asked = Vec::new();
        let mut ask = |q: &str| {
            asked.push(q.to_owned());
            answer
        };
        let keys = [public];
        let fetch = web.fetch();
        let mut env = Env {
            store: Store::new(dir),
            sources: Vec::new(),
            keys: &keys,
            fetch: &*fetch,
            out: &mut out,
            ask: &mut ask,
        };
        let result = run(&packs, &mut env);
        Ran {
            result,
            out: String::from_utf8(out).unwrap(),
            asked,
        }
    }

    fn installed(dir: &std::path::Path) -> Vec<(String, String)> {
        Store::new(dir)
            .installed()
            .into_iter()
            .map(|i| (i.pack.id, i.pack.version))
            .collect()
    }

    #[test]
    fn install_shows_the_license_asks_and_installs() {
        let (web, dir) = (Web::default(), folder("cli-install"));
        let public = official(&web);
        // Not agreed: nothing installed, and a failure.
        let ran = run_in(&dir, &web, public, &["install", "fan-clips"], false);
        assert!(ran.result.is_err());
        assert!(
            ran.out.contains("License: CC0") && ran.out.contains("Made by us."),
            "{}",
            ran.out
        );
        assert_eq!(ran.asked.len(), 1);
        assert!(installed(&dir).is_empty());
        // Agreed.
        let ran = run_in(&dir, &web, public, &["install", "fan-clips"], true);
        assert_eq!(ran.result, Ok(()));
        assert!(
            ran.out
                .contains("Installed Pack fan-clips 1.0.0. Its sounds: fire 1."),
            "{}",
            ran.out
        );
        assert!(ran.out.contains(RESTART));
        assert_eq!(installed(&dir), [("fan-clips".into(), "1.0.0".into())]);
        // --yes doesn't ask; an id no source has fails.
        let ran = run_in(
            &dir,
            &web,
            public,
            &["install", "--yes", "extra", "nope"],
            false,
        );
        assert!(ran.asked.is_empty());
        assert!(ran.out.contains("No source has a pack nope."));
        assert!(ran.result.is_err());
        assert_eq!(installed(&dir).len(), 2);
    }

    #[test]
    fn list_shows_installed_with_updates_and_whats_available() {
        let (web, dir) = (Web::default(), folder("cli-list"));
        let public = official(&web);
        let old = sound_pack(
            "fan-clips",
            "0.9.0",
            &[("fire 1", "fire1.mp3")],
            "https://packs",
        );
        Store::new(&dir)
            .install(OFFICIAL, &old, &*web.fetch())
            .unwrap();
        let ran = run_in(&dir, &web, public, &["list"], false);
        assert_eq!(ran.result, Ok(()));
        let out = ran.out;
        assert!(
            out.contains("fan-clips 0.9.0  Pack fan-clips: 1 sound  (update: 1.0.0)"),
            "{out}"
        );
        assert!(
            out.contains("extra 1.0.0  Pack extra: Some sounds. License: CC0"),
            "{out}"
        );
        assert!(!out.contains("Couldn't read"));
        // Offline: says so, still lists what's installed.
        let ran = run_in(&dir, &Web::default(), public, &["list"], false);
        assert!(
            ran.out.contains("fan-clips 0.9.0") && ran.out.contains("Couldn't read"),
            "{}",
            ran.out
        );
    }

    #[test]
    fn update_installs_only_newer_versions() {
        let (web, dir) = (Web::default(), folder("cli-update"));
        let public = official(&web);
        let old = sound_pack(
            "fan-clips",
            "0.9.0",
            &[("fire 1", "fire1.mp3")],
            "https://packs",
        );
        Store::new(&dir)
            .install(OFFICIAL, &old, &*web.fetch())
            .unwrap();
        let ran = run_in(&dir, &web, public, &["update", "--yes"], false);
        assert_eq!(ran.result, Ok(()));
        assert!(
            ran.out.contains("Updating fan-clips from 0.9.0 to 1.0.0."),
            "{}",
            ran.out
        );
        assert_eq!(installed(&dir), [("fan-clips".into(), "1.0.0".into())]);
        // Again: nothing to do. Not installed: an error.
        let ran = run_in(&dir, &web, public, &["update", "--yes"], false);
        assert!(
            ran.out.contains("Every installed pack is up to date."),
            "{}",
            ran.out
        );
        let ran = run_in(&dir, &web, public, &["update", "fan-clips"], false);
        assert!(ran.out.contains("fan-clips 1.0.0 is up to date."));
        assert!(ran.asked.is_empty());
        let ran = run_in(&dir, &web, public, &["update", "extra"], false);
        assert!(ran.result.unwrap_err().contains("extra isn't installed"));
    }

    #[test]
    fn remove_removes_and_a_pack_not_installed_fails() {
        let (web, dir) = (Web::default(), folder("cli-remove"));
        let public = official(&web);
        run_in(&dir, &web, public, &["install", "-y", "fan-clips"], false)
            .result
            .unwrap();
        let ran = run_in(&dir, &web, public, &["remove", "fan-clips"], false);
        assert_eq!(ran.result, Ok(()));
        assert!(ran.out.contains("Removed Pack fan-clips."));
        assert!(installed(&dir).is_empty());
        let ran = run_in(&dir, &web, public, &["remove", "fan-clips"], false);
        assert!(ran.result.is_err() && ran.out.contains("fan-clips isn't installed."));
    }

    #[test]
    fn another_catalog_needs_unsigned_and_the_official_one_its_signature() {
        let (web, dir) = (Web::default(), folder("cli-url"));
        let public = official(&web);
        let other = "https://example.org/catalog.json";
        let catalog = Catalog {
            format: content::FORMAT,
            release: String::new(),
            packs: vec![sound_pack(
                "horns",
                "1.0.0",
                &[("horn", "horn.mp3")],
                "https://example.org",
            )],
        };
        web.serve(other, &catalog, None);
        let ran = run_in(
            &dir,
            &web,
            public,
            &["install", "-y", "horns", "--url", other],
            false,
        );
        assert!(ran.result.unwrap_err().contains("add --unsigned"));
        assert!(installed(&dir).is_empty());
        let ran = run_in(
            &dir,
            &web,
            public,
            &["install", "-y", "horns", "--url", other, "--unsigned"],
            false,
        );
        assert_eq!(ran.result, Ok(()), "{}", ran.out);
        assert_eq!(Store::new(&dir).installed()[0].source, other);
        // The official catalog as --url is still checked: one signed by
        // another key fails.
        let ran = run_in(
            &dir,
            &web,
            [9; 32],
            &["install", "-y", "extra", "--url", OFFICIAL],
            false,
        );
        assert!(
            ran.result.is_err() && ran.out.contains("Couldn't read"),
            "{}",
            ran.out
        );
        assert_eq!(installed(&dir).len(), 1);
    }

    #[test]
    fn a_pack_folder_installs_with_unsigned() {
        let (web, dir) = (Web::default(), folder("cli-package"));
        let public = official(&web);
        let pack = folder("cli-package-src");
        std::fs::write(
            pack.join("pack.toml"),
            "id = \"horns\"\nname = \"Horns\"\nversion = \"0.1.0\"\nmin_app = \"0.1.0\"\n\
             summary = \"Horns.\"\nlicense = \"CC0\"\ncredits = \"Us.\"\n\
             [[sound]]\nname = \"horn\"\nfile = \"horn.mp3\"\n",
        )
        .unwrap();
        std::fs::write(pack.join("horn.mp3"), b"toot").unwrap();
        let path = pack.display().to_string();
        let ran = run_in(
            &dir,
            &web,
            public,
            &["install", "-y", "--package", &path],
            false,
        );
        assert!(ran.result.unwrap_err().contains("add --unsigned"));
        let ran = run_in(
            &dir,
            &web,
            public,
            &["install", "-y", "--package", &path, "--unsigned"],
            false,
        );
        assert_eq!(ran.result, Ok(()), "{}", ran.out);
        let store = Store::new(&dir);
        assert_eq!(store.installed()[0].source, format!("file:{path}"));
        assert_eq!(
            std::fs::read(store.pack_dir("horns").join("horn.mp3")).unwrap(),
            b"toot"
        );
        assert!(web.asked.lock().unwrap().is_empty(), "nothing downloaded");
        // Changed and installed again, same version: install replaces it.
        std::fs::write(pack.join("horn.mp3"), b"honk").unwrap();
        run_in(
            &dir,
            &web,
            public,
            &["install", "-y", "--package", &path, "--unsigned"],
            false,
        )
        .result
        .unwrap();
        assert_eq!(
            std::fs::read(store.pack_dir("horns").join("horn.mp3")).unwrap(),
            b"honk"
        );
    }

    #[test]
    fn an_older_release_in_use_is_noted_as_the_window_notes_it() {
        use crate::content::tests::{release_url, releases};
        let (key, public) = test_key();
        let (web, dir) = (Web::default(), folder("cli-note"));
        // The latest release (v2) has no catalog; v1's is used.
        let v1 = Catalog {
            format: content::FORMAT,
            release: "v1".into(),
            packs: vec![sound_pack(
                "beeps",
                "1.0.0",
                &[("beep", "beep.mp3")],
                "https://x",
            )],
        };
        web.serve(&release_url("v1"), &v1, Some(&key));
        web.files.lock().unwrap().insert(
            content::OFFICIAL_RELEASES.into(),
            releases(&[("v2", false), ("v1", true)]),
        );
        let ran = run_in(&dir, &web, public, &["list"], false);
        assert_eq!(ran.result, Ok(()));
        let note = Source {
            url: OFFICIAL.into(),
            official: true,
        }
        .read(&[public], &*web.fetch())
        .note
        .unwrap();
        assert!(ran.out.contains(&format!("Note: {note}")), "{}", ran.out);
        assert!(ran.out.contains("beeps 1.0.0"), "{}", ran.out);
    }
}
