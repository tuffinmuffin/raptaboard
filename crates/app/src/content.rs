//! Content packs: sounds (and later widget plugins) raptaboard downloads
//! when the user asks, from a **source**: a `catalog.json` on the web
//! listing packs, each with its license and credits and its files' URLs,
//! sizes and SHA-256 checksums.
//!
//! The official source is raptaboard-content's latest GitHub release
//! ([`OFFICIAL`]); its catalog is signed (Ed25519, `catalog.json.sig`
//! beside it) and only used if the signature checks with a key built in
//! here ([`OFFICIAL_KEYS`]). A source the user adds is anything else with
//! a catalog in the same format; it isn't signed, so the Content window
//! says to add only sources you trust. Every file is checked against the
//! catalog's size and checksum before it's kept, whatever the source.
//!
//! Installed packs live in `~/.raptaboard/content/<pack id>/`: the files
//! and `pack.json` (the catalog's entry for it, and the source it came
//! from). An install downloads into `<id>.partial` first and moves it into
//! place only once every file checks, so a failed or interrupted one leaves
//! the pack as it was. Nothing here is needed at an event: packs are
//! installed beforehand and used from disk, and raptaboard works the same
//! without them.
//!
//! The catalog format (`format` 1) is made by raptaboard-content's
//! `scripts/build_catalog.py`; change both together.
//!
//! Downloads run on a thread of their own ([`Content`]); the window asks
//! for them and picks up what happened each frame, so a slow or missing
//! network never holds up the dashboard.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sound::SoundFiles;

/// The official source's catalog: raptaboard-content's latest release.
pub const OFFICIAL: &str =
    "https://github.com/tuffinmuffin/raptaboard-content/releases/latest/download/catalog.json";
/// The official source's repository on GitHub.
pub const OFFICIAL_REPO: &str = "tuffinmuffin/raptaboard-content";
/// Its releases, newest first, for when the latest has no catalog (a
/// release that failed or is still publishing): GitHub's API.
pub const OFFICIAL_RELEASES: &str =
    "https://api.github.com/repos/tuffinmuffin/raptaboard-content/releases?per_page=10";
/// How many older releases are tried when the latest has no catalog.
const FALLBACK_RELEASES: usize = 3;

/// The keys the official catalog may be signed with (raw Ed25519 public
/// keys): raptaboard-content's `catalog-key.pub`. When the key changes,
/// keep the old one here too for a release or two.
pub const OFFICIAL_KEYS: &[[u8; 32]] = &[[
    0x25, 0x90, 0x99, 0x96, 0x2c, 0x7a, 0x84, 0xcd, 0x52, 0xe1, 0xbf, 0xdc, 0x67, 0x34, 0xd9, 0x89,
    0x4e, 0x71, 0x00, 0x3d, 0xe6, 0x2b, 0x35, 0xae, 0x2d, 0xbb, 0x34, 0xea, 0x94, 0x20, 0xbf, 0xde,
]];

/// The catalog format this raptaboard reads.
pub const FORMAT: u32 = 1;
/// The folder packs are installed in, inside `~/.raptaboard`.
pub const DIR_NAME: &str = "content";
/// The largest catalog read: 1 MB (a catalog is a few kilobytes).
const CATALOG_LIMIT: u64 = 1 << 20;
/// The largest pack file read: 200 MB.
const FILE_LIMIT: u64 = 200 << 20;
/// How long to wait for a server to answer before giving up: short, so a
/// network with no internet (the robot's, an event's) fails quickly.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a catalog may take altogether.
const CATALOG_TIMEOUT: Duration = Duration::from_secs(15);
/// How long one pack file may take altogether.
const FILE_TIMEOUT: Duration = Duration::from_secs(300);

/// A source's list of packs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    /// The format, [`FORMAT`]; a newer one needs a newer raptaboard.
    pub format: u32,
    /// Which release of the source this is (`v1`).
    #[serde(default)]
    pub release: String,
    /// Its packs.
    pub packs: Vec<Pack>,
}

/// A pack as a catalog lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    /// Its id, also its folder's name: lowercase letters, digits and dashes.
    pub id: String,
    /// Its name, shown in the Content window.
    pub name: String,
    /// Its version (`1.0.0`); a higher one is an update.
    pub version: String,
    /// `sounds` or `plugin`.
    pub kind: String,
    /// The oldest raptaboard it works with.
    pub min_app: String,
    /// One line saying what's in it.
    pub summary: String,
    /// Its license, in a line.
    pub license: String,
    /// Its full credits and license text, shown before installing.
    pub credits: String,
    /// Ticked in the list offered on the first run.
    #[serde(default)]
    pub suggested: bool,
    /// The sounds in a sounds pack.
    #[serde(default)]
    pub sounds: Vec<PackSound>,
    /// Its files.
    pub files: Vec<PackFile>,
}

/// A sound in a sounds pack: offered under `name`, played from `file`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackSound {
    /// The name it's offered under. A built-in sound's name (`disabled 1`)
    /// plays this file in place of the generated tone.
    pub name: String,
    /// One of the pack's files.
    pub file: String,
}

/// A file in a pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackFile {
    /// Its name in the pack's folder: no folders, no `..`.
    pub path: String,
    /// Where to download it.
    pub url: String,
    /// Its size, in bytes.
    pub size: u64,
    /// Its SHA-256 checksum, in lowercase hex.
    pub sha256: String,
}

/// A pack installed on this computer: `content/<id>/pack.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Installed {
    /// The catalog URL it was installed from.
    pub source: String,
    /// The catalog's entry for it, as it was when installed.
    pub pack: Pack,
}

/// What's remembered about the packs between runs: `content/state.json`.
/// A file from before 0.3.0's release (`{"offered": true}`) reads as no
/// version yet, so it counts as an upgrade.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// The raptaboard version that last started, for the first start of a
    /// new one ([`crate::upgrade`]).
    pub last_version: Option<String>,
    /// The list of packs is still to be offered: set on the first start
    /// and after an upgrade, cleared once it's answered (so if it couldn't
    /// be read, offline, it's offered at the next start).
    pub offer_pending: bool,
    /// The packs offered so far, by id: offered again after an upgrade,
    /// they aren't ticked.
    pub shown: Vec<String>,
}

/// Whether `id` can be a pack's folder name.
///
/// ```
/// use raptaboard::content::is_pack_id;
/// assert!(is_pack_id("fan-clips"));
/// assert!(!is_pack_id("../home") && !is_pack_id("Fan") && !is_pack_id(""));
/// ```
pub fn is_pack_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether `name` can be a file in a pack's folder: letters, digits and
/// `. _ -`, not starting with a dot (so no `..` and no hidden files).
///
/// ```
/// use raptaboard::content::is_file_name;
/// assert!(is_file_name("fire1.mp3"));
/// assert!(!is_file_name("../x") && !is_file_name("a/b") && !is_file_name(".."));
/// ```
pub fn is_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A version's numbers, `1.2.3` → `(1, 2, 3)`; anything after a `-` (a
/// prerelease) is ignored, and what doesn't read counts as 0.
///
/// ```
/// use raptaboard::content::version;
/// assert_eq!(version("0.3.0"), (0, 3, 0));
/// assert_eq!(version("1.2.3-rc.1"), (1, 2, 3));
/// assert!(version("1.10.0") > version("1.9.9"));
/// ```
pub fn version(v: &str) -> (u64, u64, u64) {
    let mut parts = v
        .split('-')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.trim().parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// This raptaboard's version.
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

impl Catalog {
    /// Read a catalog, checking what raptaboard relies on: a format it
    /// knows, and pack ids and file names that stay inside the pack's
    /// folder. A pack with a bad id or file is a bad catalog: the source
    /// is broken, so none of it is trusted.
    pub fn parse(bytes: &[u8]) -> Result<Catalog, String> {
        // The format first, so a newer one says so rather than failing on
        // a field it changed.
        #[derive(Deserialize)]
        struct Format {
            format: u32,
        }
        let format: Format =
            serde_json::from_slice(bytes).map_err(|e| format!("not a catalog: {e}"))?;
        if format.format != FORMAT {
            return Err(format!(
                "catalog format {} needs a newer raptaboard (this one reads {FORMAT})",
                format.format
            ));
        }
        let catalog: Catalog =
            serde_json::from_slice(bytes).map_err(|e| format!("not a catalog: {e}"))?;
        let mut ids = HashSet::new();
        for pack in &catalog.packs {
            if !is_pack_id(&pack.id) {
                return Err(format!("pack id {:?} isn't allowed", pack.id));
            }
            if !ids.insert(pack.id.as_str()) {
                return Err(format!("pack {} is listed twice", pack.id));
            }
            for f in &pack.files {
                if !is_file_name(&f.path) || f.path == Installed::FILE {
                    return Err(format!("{}: file name {:?} isn't allowed", pack.id, f.path));
                }
            }
            for s in &pack.sounds {
                if !pack.files.iter().any(|f| f.path == s.file) {
                    return Err(format!(
                        "{}: sound {} has no file {}",
                        pack.id, s.name, s.file
                    ));
                }
            }
        }
        Ok(catalog)
    }
}

/// Check `signature` (raw, 64 bytes) of `bytes` against `keys`.
pub fn verify(bytes: &[u8], signature: &[u8], keys: &[[u8; 32]]) -> Result<(), String> {
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| "the catalog's signature isn't 64 bytes".to_string())?;
    let signature = Signature::from_bytes(&signature);
    let good = keys.iter().any(|key| {
        VerifyingKey::from_bytes(key).is_ok_and(|k| k.verify_strict(bytes, &signature).is_ok())
    });
    if good {
        Ok(())
    } else {
        Err("the catalog's signature doesn't match raptaboard's key".into())
    }
}

/// `bytes`' SHA-256 checksum, in lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Where a pack stands against what's installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Not on this computer.
    Available,
    /// Installed, this version or newer.
    Installed,
    /// An older version is installed.
    Update {
        /// The version installed.
        from: String,
    },
    /// Needs a newer raptaboard than this one.
    NeedsApp,
}

/// Where `pack` stands, given the installed copy (if any).
///
/// ```
/// use raptaboard::content::{status, Status};
/// # let pack = |v: &str, app: &str| serde_json::from_value::<raptaboard::content::Pack>(serde_json::json!({
/// #   "id": "p", "name": "P", "version": v, "kind": "sounds", "min_app": app,
/// #   "summary": "", "license": "", "credits": "", "files": []})).unwrap();
/// assert_eq!(status(&pack("1.0.0", "0.1.0"), None), Status::Available);
/// assert_eq!(status(&pack("1.0.0", "99.0.0"), None), Status::NeedsApp);
/// ```
pub fn status(pack: &Pack, installed: Option<&Installed>) -> Status {
    match installed {
        Some(i) if version(&i.pack.version) >= version(&pack.version) => Status::Installed,
        _ if version(&pack.min_app) > version(app_version()) => Status::NeedsApp,
        Some(i) => Status::Update {
            from: i.pack.version.clone(),
        },
        None => Status::Available,
    }
}

/// Downloads `url`, at most `limit` bytes.
pub type Fetch = dyn Fn(&str, u64) -> Result<Vec<u8>, String> + Send + Sync;

/// The packs folder: what's installed, installing and removing.
#[derive(Debug, Clone)]
pub struct Store {
    /// `~/.raptaboard/content`.
    pub dir: PathBuf,
}

impl Installed {
    /// The file in a pack's folder saying what it is.
    pub const FILE: &'static str = "pack.json";
}

impl Store {
    /// The packs folder `dir` (made when first needed).
    pub fn new(dir: impl Into<PathBuf>) -> Store {
        Store { dir: dir.into() }
    }

    /// The folder pack `id` is installed in.
    pub fn pack_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    /// Every pack installed, in id order. A folder without a readable
    /// `pack.json` (or one for another id) isn't a pack and is skipped.
    pub fn installed(&self) -> Vec<Installed> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<Installed> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                if !is_pack_id(&name) {
                    return None;
                }
                let text = std::fs::read(e.path().join(Installed::FILE)).ok()?;
                let installed: Installed = serde_json::from_slice(&text).ok()?;
                (installed.pack.id == name).then_some(installed)
            })
            .collect();
        out.sort_by(|a, b| a.pack.id.cmp(&b.pack.id));
        out
    }

    /// Download `pack` from `source` with `fetch` and install it in place
    /// of any older copy. Each file must match the catalog's size and
    /// checksum; if one doesn't (or can't be had), nothing changes.
    pub fn install(&self, source: &str, pack: &Pack, fetch: &Fetch) -> Result<(), String> {
        if !is_pack_id(&pack.id) {
            return Err(format!("pack id {:?} isn't allowed", pack.id));
        }
        let partial = self.dir.join(format!("{}.partial", pack.id));
        let _ = std::fs::remove_dir_all(&partial);
        std::fs::create_dir_all(&partial).map_err(|e| format!("can't make {partial:?}: {e}"))?;
        let result = self.fill(&partial, source, pack, fetch);
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&partial);
            return result;
        }
        let target = self.pack_dir(&pack.id);
        let old = self.dir.join(format!("{}.old", pack.id));
        let _ = std::fs::remove_dir_all(&old);
        if target.exists() {
            std::fs::rename(&target, &old)
                .map_err(|e| format!("can't replace {}: {e}", pack.id))?;
        }
        if let Err(e) = std::fs::rename(&partial, &target) {
            // Put the old copy back.
            let _ = std::fs::rename(&old, &target);
            let _ = std::fs::remove_dir_all(&partial);
            return Err(format!("can't install {}: {e}", pack.id));
        }
        let _ = std::fs::remove_dir_all(&old);
        Ok(())
    }

    /// Download `pack`'s files into `folder`, checked, then its `pack.json`.
    fn fill(&self, folder: &Path, source: &str, pack: &Pack, fetch: &Fetch) -> Result<(), String> {
        for f in &pack.files {
            if !is_file_name(&f.path) || f.path == Installed::FILE {
                return Err(format!("file name {:?} isn't allowed", f.path));
            }
            let bytes = fetch(&f.url, f.size.min(FILE_LIMIT))?;
            if bytes.len() as u64 != f.size {
                return Err(format!(
                    "{} is {} bytes, not the {} the catalog says",
                    f.path,
                    bytes.len(),
                    f.size
                ));
            }
            if !sha256_hex(&bytes).eq_ignore_ascii_case(&f.sha256) {
                return Err(format!("{} doesn't match the catalog's checksum", f.path));
            }
            std::fs::write(folder.join(&f.path), &bytes)
                .map_err(|e| format!("can't save {}: {e}", f.path))?;
        }
        let record = Installed {
            source: source.to_owned(),
            pack: pack.clone(),
        };
        let json = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
        std::fs::write(folder.join(Installed::FILE), json)
            .map_err(|e| format!("can't save {}: {e}", Installed::FILE))
    }

    /// Remove pack `id`. Removing one that isn't there is fine.
    pub fn remove(&self, id: &str) -> Result<(), String> {
        if !is_pack_id(id) {
            return Err(format!("pack id {id:?} isn't allowed"));
        }
        match std::fs::remove_dir_all(self.pack_dir(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("can't remove {id}: {e}"))
            }
            _ => Ok(()),
        }
    }

    /// The sounds the installed packs offer: each name and its file.
    pub fn sounds(&self) -> HashMap<String, PathBuf> {
        self.installed()
            .into_iter()
            .flat_map(|i| {
                let dir = self.pack_dir(&i.pack.id);
                i.pack
                    .sounds
                    .into_iter()
                    .map(move |s| (s.name, dir.join(s.file)))
            })
            .collect()
    }

    /// What's remembered between runs; the default if nothing is yet.
    pub fn state(&self) -> State {
        std::fs::read(self.dir.join("state.json"))
            .ok()
            .and_then(|t| serde_json::from_slice(&t).ok())
            .unwrap_or_default()
    }

    /// Remember `state` for the next run.
    pub fn save_state(&self, state: &State) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join("state.json"), json).map_err(|e| e.to_string())
    }
}

/// Download and check the catalog at `url`: signed with one of `keys` if
/// any are given (its signature is at `url` + `.sig`), else unsigned.
pub fn fetch_catalog(url: &str, keys: &[[u8; 32]], fetch: &Fetch) -> Result<Catalog, String> {
    let bytes = fetch(url, CATALOG_LIMIT)?;
    if !keys.is_empty() {
        let signature = fetch(&format!("{url}.sig"), 64)?;
        verify(&bytes, &signature, keys)?;
    }
    Catalog::parse(&bytes)
}

/// The official catalog: the latest release's, or if that has none (its
/// release failed or is still publishing), the newest release before it
/// that has one, with a note saying so. Signed either way. Offline, it
/// stops at the first error.
pub fn fetch_official(
    keys: &[[u8; 32]],
    fetch: &Fetch,
) -> Result<(Catalog, Option<String>), String> {
    let first = match fetch_catalog(OFFICIAL, keys, fetch) {
        Ok(catalog) => return Ok((catalog, None)),
        Err(e) if is_offline(&e) => return Err(e),
        Err(e) => e,
    };
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
        #[serde(default)]
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
    }
    let gave_up = |why: String| {
        format!(
            "The official source's latest release has no usable catalog ({first}). A content \
             release may have failed or still be publishing; try again in a few minutes.{why}"
        )
    };
    let releases: Vec<Release> = match fetch(OFFICIAL_RELEASES, CATALOG_LIMIT)
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(r) => r,
        Err(e) => return Err(gave_up(format!(" (Couldn't list its releases: {e}.)"))),
    };
    let has = |r: &Release, name: &str| r.assets.iter().any(|a| a.name == name);
    let newest = releases
        .iter()
        .find(|r| !r.draft)
        .map(|r| r.tag_name.clone());
    for release in releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter(|r| has(r, "catalog.json") && has(r, "catalog.json.sig"))
        .take(FALLBACK_RELEASES)
    {
        let url = format!(
            "https://github.com/{OFFICIAL_REPO}/releases/download/{}/catalog.json",
            release.tag_name
        );
        if let Ok(catalog) = fetch_catalog(&url, keys, fetch) {
            let newest = newest.unwrap_or_else(|| "the latest".into());
            let note = format!(
                "The newest content release ({newest}) has no usable catalog ({first}), so \
                 these packs are from {} instead.",
                release.tag_name
            );
            return Ok((catalog, Some(note)));
        }
    }
    Err(gave_up(String::new()))
}

/// Whether a download error means the network can't reach the server at
/// all (offline, or a network with no internet), not that the server
/// answered with something wrong.
fn is_offline(e: &str) -> bool {
    e.ends_with("(offline?)")
}

/// Download over HTTPS (or HTTP, for a source on the local network) with
/// short timeouts, checking certificates against the computer's own (so
/// networks that add their own certificate work, as a browser would).
/// `limit` is the most bytes read.
pub fn http_get(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let total = if limit <= CATALOG_LIMIT {
        CATALOG_TIMEOUT
    } else {
        FILE_TIMEOUT
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(total))
        .user_agent(format!("raptaboard/{}", app_version()))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(|e| short_error(url, &e))?;
    // ureq stops at its limit even when the body ends exactly there, so
    // one byte more, and the size checked here.
    let bytes = response
        .body_mut()
        .with_config()
        .limit(limit.saturating_add(1))
        .read_to_vec()
        .map_err(|e| short_error(url, &e))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{url}: the file is larger than expected"));
    }
    Ok(bytes)
}

/// An error in a few words, with the host it was for.
fn short_error(url: &str, e: &ureq::Error) -> String {
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url);
    // The file asked for, without any query.
    let file = url
        .split('?')
        .next()
        .and_then(|u| u.rsplit('/').next())
        .unwrap_or(url);
    match e {
        ureq::Error::StatusCode(404) => format!("{host} has no {file} (404)"),
        ureq::Error::StatusCode(code) => format!("{host}: the server said {code} for {file}"),
        ureq::Error::Timeout(_) => format!("{host}: no answer (offline?)"),
        ureq::Error::HostNotFound => format!("{host}: not found (offline?)"),
        ureq::Error::Io(_) | ureq::Error::ConnectionFailed => {
            format!("{host}: can't connect (offline?)")
        }
        ureq::Error::BodyExceedsLimit(_) => format!("{host}: the file is larger than expected"),
        other => format!("{host}: {other}"),
    }
}

/// A source the packs come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Its catalog's URL.
    pub url: String,
    /// The official one: signed, can't be removed.
    pub official: bool,
}

/// The sources: the official one, then those the user added.
pub fn sources(added: &[String]) -> Vec<Source> {
    std::iter::once(Source {
        url: OFFICIAL.into(),
        official: true,
    })
    .chain(
        added
            .iter()
            .filter(|u| u.as_str() != OFFICIAL)
            .map(|u| Source {
                url: u.clone(),
                official: false,
            }),
    )
    .collect()
}

/// A source as last checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    /// The source.
    pub source: Source,
    /// Its catalog, or why it couldn't be had.
    pub catalog: Result<Catalog, String>,
    /// Something to know about the catalog it did get (an older release's).
    pub note: Option<String>,
}

/// What the download thread is asked to do.
enum Job {
    /// Read these sources' catalogs.
    Check(Vec<Source>),
    /// Install this pack from this source.
    Install(String, Box<Pack>),
    /// Remove this pack.
    Remove(String),
}

/// What the download thread did.
enum Done {
    /// The sources' catalogs, read.
    Checked(Vec<Checked>),
    /// A pack installed, or why not.
    Installed(String, Result<(), String>),
    /// A pack removed, or why not.
    Removed(String, Result<(), String>),
}

/// Installing or removing a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Installing (or updating) it.
    Install,
    /// Removing it.
    Remove,
}

/// What became of an install or remove.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// Which it was.
    pub action: Action,
    /// The pack: the version installed, or the one removed.
    pub pack: Pack,
    /// Whether it worked, or why not.
    pub result: Result<(), String>,
}

impl Outcome {
    /// What happened, in a sentence or two for the user.
    ///
    /// ```
    /// use raptaboard::content::{Action, Outcome, Pack};
    /// # let pack: Pack = serde_json::from_value(serde_json::json!({
    /// #   "id": "fan-clips", "name": "IT Crowd clips", "version": "1.0.0", "kind": "sounds",
    /// #   "min_app": "0.1.0", "summary": "", "license": "", "credits": "",
    /// #   "sounds": [{"name": "fire 1", "file": "fire1.mp3"}],
    /// #   "files": [{"path": "fire1.mp3", "url": "https://x/f", "size": 1, "sha256": ""}]})).unwrap();
    /// let done = Outcome { action: Action::Install, pack, result: Ok(()) };
    /// assert_eq!(done.describe(), "Installed IT Crowd clips 1.0.0. Its sounds: fire 1.");
    /// ```
    pub fn describe(&self) -> String {
        let name = &self.pack.name;
        match (&self.action, &self.result) {
            (Action::Install, Ok(())) => {
                let sounds: Vec<&str> = self.pack.sounds.iter().map(|s| s.name.as_str()).collect();
                let mut text = format!("Installed {name} {}.", self.pack.version);
                if !sounds.is_empty() {
                    text.push_str(&format!(" Its sounds: {}.", sounds.join(", ")));
                }
                text
            }
            (Action::Install, Err(e)) => {
                format!("Couldn't install {name}: {e}. Nothing was changed.")
            }
            (Action::Remove, Ok(())) => format!("Removed {name}."),
            (Action::Remove, Err(e)) => format!("Couldn't remove {name}: {e}."),
        }
    }
}

/// The packs as the app knows them: what's installed, the sources as last
/// checked, and a thread that does the downloading.
pub struct Content {
    /// The packs folder.
    store: Store,
    /// Asks the download thread to do something.
    jobs: Sender<Job>,
    /// What it did.
    done: Receiver<Done>,
    /// What's installed.
    pub installed: Vec<Installed>,
    /// The sources as last checked, once they have been.
    pub checked: Option<Vec<Checked>>,
    /// A check is under way.
    pub checking: bool,
    /// Sources to check again once the check under way ends (they changed
    /// while it ran).
    again: Option<Vec<Source>>,
    /// Packs being installed or removed, by id.
    pub busy: HashSet<String>,
    /// What's being done to each pack in `busy`, and to which version.
    requested: HashMap<String, (Action, Pack)>,
    /// Every install and remove finished, oldest first.
    pub outcomes: Vec<Outcome>,
    /// The installed sounds, shared with the sound thread.
    sounds: SoundFiles,
}

impl Content {
    /// Packs in `dir`, downloaded over the internet; the sounds they offer
    /// go to `sounds`.
    pub fn new(dir: impl Into<PathBuf>, sounds: SoundFiles) -> Content {
        Content::with_fetch(dir, sounds, OFFICIAL_KEYS.to_vec(), Box::new(http_get))
    }

    /// Packs in `dir`, downloaded with `fetch`, the official catalog
    /// checked against `keys` (tests use their own of both).
    pub fn with_fetch(
        dir: impl Into<PathBuf>,
        sounds: SoundFiles,
        keys: Vec<[u8; 32]>,
        fetch: Box<Fetch>,
    ) -> Content {
        let store = Store::new(dir);
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (outbox, done) = mpsc::channel::<Done>();
        let worker = store.clone();
        // If the thread can't start, nothing downloads: jobs just wait.
        let _ = std::thread::Builder::new()
            .name("raptaboard-content".into())
            .spawn(move || {
                while let Ok(job) = inbox.recv() {
                    let result = match job {
                        Job::Check(sources) => Done::Checked(
                            sources
                                .into_iter()
                                .map(|source| {
                                    let (catalog, note) = if source.official {
                                        match fetch_official(&keys, &*fetch) {
                                            Ok((c, note)) => (Ok(c), note),
                                            Err(e) => (Err(e), None),
                                        }
                                    } else {
                                        (fetch_catalog(&source.url, &[], &*fetch), None)
                                    };
                                    Checked {
                                        catalog,
                                        note,
                                        source,
                                    }
                                })
                                .collect(),
                        ),
                        Job::Install(source, pack) => {
                            let r = worker.install(&source, &pack, &*fetch);
                            Done::Installed(pack.id, r)
                        }
                        Job::Remove(id) => {
                            let r = worker.remove(&id);
                            Done::Removed(id, r)
                        }
                    };
                    if outbox.send(result).is_err() {
                        break;
                    }
                }
            });
        let mut content = Content {
            store,
            jobs,
            done,
            installed: Vec::new(),
            checked: None,
            checking: false,
            again: None,
            busy: HashSet::new(),
            requested: HashMap::new(),
            outcomes: Vec::new(),
            sounds,
        };
        content.refresh();
        content
    }

    /// The packs folder.
    pub fn dir(&self) -> &Path {
        &self.store.dir
    }

    /// Read what's installed again, and share its sounds.
    fn refresh(&mut self) {
        self.installed = self.store.installed();
        if let Ok(mut sounds) = self.sounds.write() {
            *sounds = self.store.sounds();
        }
    }

    /// Read `sources`' catalogs: now, or once the check under way ends.
    pub fn check(&mut self, sources: Vec<Source>) {
        if self.checking {
            self.again = Some(sources);
        } else if self.jobs.send(Job::Check(sources)).is_ok() {
            self.checking = true;
        }
    }

    /// Install (or update) `pack` from the catalog at `source`.
    pub fn install(&mut self, source: &str, pack: &Pack) {
        if self.busy.insert(pack.id.clone()) {
            self.requested
                .insert(pack.id.clone(), (Action::Install, pack.clone()));
            let _ = self
                .jobs
                .send(Job::Install(source.into(), Box::new(pack.clone())));
        }
    }

    /// Remove pack `id` (if it's installed).
    pub fn remove(&mut self, id: &str) {
        let Some(installed) = self.installed(id).map(|i| i.pack.clone()) else {
            return;
        };
        if self.busy.insert(id.to_owned()) {
            self.requested
                .insert(id.to_owned(), (Action::Remove, installed));
            let _ = self.jobs.send(Job::Remove(id.to_owned()));
        }
    }

    /// The newest finished install or remove of pack `id`, if any.
    pub fn outcome(&self, id: &str) -> Option<&Outcome> {
        self.outcomes.iter().rev().find(|o| o.pack.id == id)
    }

    /// Pick up what the download thread has done. Returns whether anything
    /// changed (to draw again).
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(done) = self.done.try_recv() {
            changed = true;
            match done {
                Done::Checked(checked) => {
                    self.checked = Some(checked);
                    self.checking = false;
                    if let Some(sources) = self.again.take() {
                        self.check(sources);
                    }
                }
                Done::Installed(id, result) | Done::Removed(id, result) => {
                    self.busy.remove(&id);
                    if let Some((action, pack)) = self.requested.remove(&id) {
                        self.outcomes.push(Outcome {
                            action,
                            pack,
                            result,
                        });
                    }
                    self.refresh();
                }
            }
        }
        changed
    }

    /// Whether the download thread has nothing left to do.
    pub fn idle(&self) -> bool {
        !self.checking && self.busy.is_empty()
    }

    /// The installed copy of pack `id`, if any.
    pub fn installed(&self, id: &str) -> Option<&Installed> {
        self.installed.iter().find(|i| i.pack.id == id)
    }

    /// Every pack the checked sources offer, with the source it's from,
    /// the first source listing an id winning.
    pub fn offered(&self) -> Vec<(&Source, &Pack)> {
        let mut seen = HashSet::new();
        self.checked
            .iter()
            .flatten()
            .filter_map(|c| c.catalog.as_ref().ok().map(|cat| (&c.source, cat)))
            .flat_map(|(s, cat)| cat.packs.iter().map(move |p| (s, p)))
            .filter(|(_, p)| seen.insert(p.id.clone()))
            .collect()
    }

    /// What's remembered between runs.
    pub fn state(&self) -> State {
        self.store.state()
    }

    /// Remember `state` for the next run.
    pub fn save_state(&self, state: &State) -> Result<(), String> {
        self.store.save_state(state)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    // Tests written by Claude.
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::sync::{Arc, Mutex};

    /// A signing key for tests, and its public key.
    pub fn test_key() -> (SigningKey, [u8; 32]) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let public = key.verifying_key().to_bytes();
        (key, public)
    }

    /// A pack of sounds, `files` each holding its own name as bytes,
    /// served from `base`.
    pub fn sound_pack(id: &str, version: &str, files: &[(&str, &str)], base: &str) -> Pack {
        Pack {
            id: id.into(),
            name: format!("Pack {id}"),
            version: version.into(),
            kind: "sounds".into(),
            min_app: "0.1.0".into(),
            summary: "Some sounds.".into(),
            license: "CC0".into(),
            credits: "Made by us.".into(),
            suggested: true,
            sounds: files
                .iter()
                .map(|(name, file)| PackSound {
                    name: (*name).into(),
                    file: (*file).into(),
                })
                .collect(),
            files: files
                .iter()
                .map(|(_, file)| PackFile {
                    path: (*file).into(),
                    url: format!("{base}/{id}.{file}"),
                    size: file.len() as u64,
                    sha256: sha256_hex(file.as_bytes()),
                })
                .collect(),
        }
    }

    /// A pretend web: URL → bytes, recording what was asked for.
    #[derive(Clone, Default)]
    pub struct Web {
        pub files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        pub asked: Arc<Mutex<Vec<String>>>,
    }

    impl Web {
        /// Serve `catalog` at `url`, signed with `key` (if given), and
        /// every file in it (each holds its own name).
        pub fn serve(&self, url: &str, catalog: &Catalog, key: Option<&SigningKey>) {
            let bytes = serde_json::to_vec(catalog).unwrap();
            let mut files = self.files.lock().unwrap();
            if let Some(key) = key {
                let sig = key.sign(&bytes).to_bytes().to_vec();
                files.insert(format!("{url}.sig"), sig);
            }
            files.insert(url.into(), bytes);
            for pack in &catalog.packs {
                for f in &pack.files {
                    files.insert(f.url.clone(), f.path.clone().into_bytes());
                }
            }
        }

        pub fn fetch(&self) -> Box<Fetch> {
            let web = self.clone();
            Box::new(move |url: &str, limit: u64| {
                web.asked.lock().unwrap().push(url.into());
                match web.files.lock().unwrap().get(url) {
                    Some(b) if b.len() as u64 > limit => Err("too big".into()),
                    Some(b) => Ok(b.clone()),
                    None => Err(format!("{url}: not found (404)")),
                }
            })
        }
    }

    /// An empty folder for a test.
    pub fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("raptaboard-content-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn catalog(packs: Vec<Pack>) -> Catalog {
        Catalog {
            format: FORMAT,
            release: "v1".into(),
            packs,
        }
    }

    /// The real official source, over the internet: its catalog is signed
    /// with the built-in key and its files match. Needs the internet, so
    /// ignored: `cargo test -p raptaboard --lib official_catalog -- --ignored`.
    #[test]
    #[ignore]
    fn the_official_catalog_downloads_and_checks() {
        let catalog = fetch_catalog(OFFICIAL, OFFICIAL_KEYS, &http_get).unwrap();
        assert!(!catalog.packs.is_empty());
        let store = Store::new(folder("official"));
        for pack in &catalog.packs {
            store.install(OFFICIAL, pack, &http_get).unwrap();
        }
        assert_eq!(store.installed().len(), catalog.packs.len());
        // With the latest release broken, GitHub's real list of releases
        // finds one with a catalog.
        let broken = |url: &str, limit: u64| {
            if url == OFFICIAL {
                Err("github.com has no catalog.json (404)".to_string())
            } else {
                http_get(url, limit)
            }
        };
        let (older, note) = fetch_official(OFFICIAL_KEYS, &broken).unwrap();
        assert!(!older.packs.is_empty() && note.is_some(), "{note:?}");
        // Somewhere with no server: a quick error, not a hang.
        let start = std::time::Instant::now();
        assert!(http_get("https://10.255.255.1/catalog.json", 1000).is_err());
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn the_official_key_is_the_content_repos() {
        // raptaboard-content's catalog-key.pub, base64.
        let b64 = "JZCZlix6hM1S4b/cZzTZiU5xAD3mKzWuLbs06pQgv94=";
        let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = 0u32;
        let mut n = 0;
        let mut out = Vec::new();
        for c in b64.trim_end_matches('=').chars() {
            bits = (bits << 6) | alphabet.find(c).unwrap() as u32;
            n += 6;
            if n >= 8 {
                n -= 8;
                out.push((bits >> n) as u8);
            }
        }
        assert_eq!(OFFICIAL_KEYS, &[<[u8; 32]>::try_from(out).unwrap()]);
        assert!(VerifyingKey::from_bytes(&OFFICIAL_KEYS[0]).is_ok());
    }

    #[test]
    fn a_signed_catalog_checks_and_a_changed_one_doesnt() {
        let (key, public) = test_key();
        let web = Web::default();
        let cat = catalog(vec![sound_pack(
            "beeps",
            "1.0.0",
            &[("beep", "beep.mp3")],
            "https://x",
        )]);
        web.serve("https://x/catalog.json", &cat, Some(&key));
        let fetch = web.fetch();
        assert_eq!(
            fetch_catalog("https://x/catalog.json", &[public], &*fetch),
            Ok(cat)
        );
        // Changed after signing: refused.
        let mut files = web.files.lock().unwrap();
        let body = files.get_mut("https://x/catalog.json").unwrap();
        *body = String::from_utf8(body.clone())
            .unwrap()
            .replace("CC0", "CC1")
            .into_bytes();
        drop(files);
        let err = fetch_catalog("https://x/catalog.json", &[public], &*fetch).unwrap_err();
        assert!(err.contains("signature"), "{err}");
        // Signed by someone else: refused.
        let other = SigningKey::from_bytes(&[9u8; 32])
            .verifying_key()
            .to_bytes();
        web.serve("https://x/catalog.json", &catalog(vec![]), Some(&key));
        assert!(fetch_catalog("https://x/catalog.json", &[other], &*fetch).is_err());
        // An unsigned source (no keys) isn't asked for a signature.
        web.asked.lock().unwrap().clear();
        assert!(fetch_catalog("https://x/catalog.json", &[], &*fetch).is_ok());
        assert_eq!(*web.asked.lock().unwrap(), ["https://x/catalog.json"]);
    }

    #[test]
    fn a_catalog_that_would_leave_its_folder_is_refused() {
        let mut pack = sound_pack("beeps", "1.0.0", &[("beep", "beep.mp3")], "https://x");
        let ok = serde_json::to_vec(&catalog(vec![pack.clone()])).unwrap();
        assert!(Catalog::parse(&ok).is_ok());
        pack.files[0].path = "../beep.mp3".into();
        let bad = serde_json::to_vec(&catalog(vec![pack.clone()])).unwrap();
        assert!(Catalog::parse(&bad).unwrap_err().contains("isn't allowed"));
        pack.files[0].path = "beep.mp3".into();
        pack.id = "../up".into();
        let bad = serde_json::to_vec(&catalog(vec![pack])).unwrap();
        assert!(Catalog::parse(&bad).unwrap_err().contains("isn't allowed"));
        let newer = br#"{"format": 2, "packs": [{"whatever": true}]}"#;
        assert!(
            Catalog::parse(newer)
                .unwrap_err()
                .contains("newer raptaboard")
        );
    }

    /// GitHub's list of releases, newest first: each tag and whether it
    /// has the catalog's two files.
    fn releases(tags: &[(&str, bool)]) -> Vec<u8> {
        let list: Vec<serde_json::Value> = tags
            .iter()
            .map(|(tag, whole)| {
                let assets: Vec<serde_json::Value> = if *whole {
                    vec![
                        serde_json::json!({"name": "catalog.json"}),
                        serde_json::json!({"name": "catalog.json.sig"}),
                    ]
                } else {
                    vec![]
                };
                serde_json::json!({"tag_name": tag, "draft": false, "prerelease": false, "assets": assets})
            })
            .collect();
        serde_json::to_vec(&list).unwrap()
    }

    fn release_url(tag: &str) -> String {
        format!("https://github.com/{OFFICIAL_REPO}/releases/download/{tag}/catalog.json")
    }

    #[test]
    fn the_official_urls_name_the_same_repo() {
        assert!(OFFICIAL.contains(OFFICIAL_REPO) && OFFICIAL_RELEASES.contains(OFFICIAL_REPO));
    }

    #[test]
    fn a_latest_release_without_a_catalog_falls_back_to_the_one_before() {
        let (key, public) = test_key();
        let web = Web::default();
        // v2's release failed: latest points at it, and it has no catalog.
        let v1 = catalog(vec![sound_pack(
            "beeps",
            "1.0.0",
            &[("beep", "beep.mp3")],
            "https://x",
        )]);
        web.serve(&release_url("v1"), &v1, Some(&key));
        web.files.lock().unwrap().insert(
            OFFICIAL_RELEASES.into(),
            releases(&[("v2", false), ("v1", true)]),
        );
        let (got, note) = fetch_official(&[public], &*web.fetch()).unwrap();
        assert_eq!(got, v1);
        let note = note.unwrap();
        assert!(note.contains("v2") && note.contains("from v1"), "{note}");
        // The latest working: no note, no list asked for.
        web.serve(OFFICIAL, &v1, Some(&key));
        web.asked.lock().unwrap().clear();
        assert_eq!(fetch_official(&[public], &*web.fetch()).unwrap().1, None);
        assert!(
            !web.asked
                .lock()
                .unwrap()
                .iter()
                .any(|u| u == OFFICIAL_RELEASES)
        );
    }

    #[test]
    fn no_usable_release_says_why_clearly() {
        let (_, public) = test_key();
        let web = Web::default();
        web.files
            .lock()
            .unwrap()
            .insert(OFFICIAL_RELEASES.into(), releases(&[("v2", false)]));
        let err = fetch_official(&[public], &*web.fetch()).unwrap_err();
        assert!(
            err.contains("may have failed or still be publishing"),
            "{err}"
        );
        // Can't list them either: still says what happened.
        let empty = Web::default();
        let err = fetch_official(&[public], &*empty.fetch()).unwrap_err();
        assert!(err.contains("Couldn't list its releases"), "{err}");
    }

    #[test]
    fn offline_doesnt_try_the_release_list() {
        let (_, public) = test_key();
        let asked = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = asked.clone();
        let offline: Box<Fetch> = Box::new(move |url: &str, _| {
            log.lock().unwrap().push(url.into());
            Err("github.com: can't connect (offline?)".into())
        });
        let err = fetch_official(&[public], &*offline).unwrap_err();
        assert!(err.contains("offline"), "{err}");
        assert_eq!(*asked.lock().unwrap(), [OFFICIAL]);
    }

    #[test]
    fn install_update_and_remove() {
        let dir = folder("install");
        let store = Store::new(&dir);
        let web = Web::default();
        let v1 = sound_pack("beeps", "1.0.0", &[("beep", "beep.mp3")], "https://x");
        web.serve("https://x/c.json", &catalog(vec![v1.clone()]), None);
        let fetch = web.fetch();
        store.install("https://x/c.json", &v1, &*fetch).unwrap();
        let [installed] = &store.installed()[..] else {
            panic!("one pack")
        };
        assert_eq!(installed.pack, v1);
        assert_eq!(installed.source, "https://x/c.json");
        assert_eq!(
            std::fs::read(dir.join("beeps/beep.mp3")).unwrap(),
            b"beep.mp3"
        );
        assert_eq!(store.sounds()["beep"], dir.join("beeps/beep.mp3"));

        let v2 = sound_pack("beeps", "1.1.0", &[("boop", "boop.mp3")], "https://x");
        assert_eq!(
            status(&v2, Some(installed)),
            Status::Update {
                from: "1.0.0".into()
            }
        );
        web.serve("https://x/c.json", &catalog(vec![v2.clone()]), None);
        store.install("https://x/c.json", &v2, &*fetch).unwrap();
        assert_eq!(status(&v2, store.installed().first()), Status::Installed);
        // The old version's files went with it.
        assert!(!dir.join("beeps/beep.mp3").exists());
        assert!(dir.join("beeps/boop.mp3").exists());
        assert!(!dir.join("beeps.partial").exists() && !dir.join("beeps.old").exists());

        store.remove("beeps").unwrap();
        assert!(store.installed().is_empty() && !dir.join("beeps").exists());
        store.remove("beeps").unwrap();
    }

    #[test]
    fn a_file_that_doesnt_match_leaves_the_old_copy() {
        let dir = folder("mismatch");
        let store = Store::new(&dir);
        let web = Web::default();
        let v1 = sound_pack("beeps", "1.0.0", &[("beep", "beep.mp3")], "https://x");
        web.serve("https://x/c.json", &catalog(vec![v1.clone()]), None);
        store
            .install("https://x/c.json", &v1, &*web.fetch())
            .unwrap();

        let v2 = sound_pack("beeps", "2.0.0", &[("beep", "beep.mp3")], "https://y");
        web.serve("https://y/c.json", &catalog(vec![v2.clone()]), None);
        // Same size, different bytes: the checksum catches it.
        web.files
            .lock()
            .unwrap()
            .insert("https://y/beeps.beep.mp3".into(), b"BEEP.MP3".to_vec());
        let err = store
            .install("https://y/c.json", &v2, &*web.fetch())
            .unwrap_err();
        assert!(err.contains("checksum"), "{err}");
        // A missing file: the download's error.
        web.files.lock().unwrap().remove("https://y/beeps.beep.mp3");
        let err = store
            .install("https://y/c.json", &v2, &*web.fetch())
            .unwrap_err();
        assert!(err.contains("404"), "{err}");
        // Version 1 is still installed, whole.
        assert_eq!(store.installed()[0].pack.version, "1.0.0");
        assert_eq!(
            std::fs::read(dir.join("beeps/beep.mp3")).unwrap(),
            b"beep.mp3"
        );
        assert!(!dir.join("beeps.partial").exists());
    }

    #[test]
    fn a_pack_for_a_newer_app_isnt_offered_to_install() {
        let mut pack = sound_pack("beeps", "1.0.0", &[("beep", "beep.mp3")], "https://x");
        pack.min_app = "99.0.0".into();
        assert_eq!(status(&pack, None), Status::NeedsApp);
        pack.min_app = app_version().into();
        assert_eq!(status(&pack, None), Status::Available);
    }

    #[test]
    fn the_state_is_remembered_and_an_old_file_reads_as_an_upgrade() {
        let store = Store::new(folder("state"));
        assert_eq!(store.state(), State::default());
        let state = State {
            last_version: Some("0.3.0".into()),
            offer_pending: false,
            shown: vec!["fan-clips".into()],
        };
        store.save_state(&state).unwrap();
        assert_eq!(store.state(), state);
        // Written by a 0.3.0 development build.
        std::fs::write(store.dir.join("state.json"), br#"{"offered": true}"#).unwrap();
        let mut old = store.state();
        assert_eq!(old.last_version, None);
        assert_eq!(crate::upgrade::start(&mut old), Some(None));
        assert!(old.offer_pending);
        assert_eq!(old.last_version.as_deref(), Some(app_version()));
    }

    #[test]
    fn the_download_thread_checks_installs_and_shares_sounds() {
        let (key, public) = test_key();
        let web = Web::default();
        let pack = sound_pack("beeps", "1.0.0", &[("beep", "beep.mp3")], "https://x");
        web.serve(OFFICIAL, &catalog(vec![pack.clone()]), Some(&key));
        let other = sound_pack("more", "1.0.0", &[("boop", "boop.mp3")], "https://y");
        web.serve("https://y/c.json", &catalog(vec![other]), None);
        let sounds = SoundFiles::default();
        let dir = folder("thread");
        let mut content = Content::with_fetch(&dir, sounds.clone(), vec![public], web.fetch());
        let wait = |content: &mut Content| {
            for _ in 0..500 {
                content.poll();
                if content.idle() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("the download thread didn't finish");
        };
        content.check(sources(&[
            "https://y/c.json".into(),
            "https://z/none.json".into(),
        ]));
        wait(&mut content);
        let checked = content.checked.clone().unwrap();
        assert_eq!(checked.len(), 3);
        assert!(checked[0].catalog.is_ok() && checked[1].catalog.is_ok());
        assert!(checked[2].catalog.as_ref().unwrap_err().contains("404"));
        let offered: Vec<&str> = content
            .offered()
            .iter()
            .map(|(_, p)| p.id.as_str())
            .collect();
        assert_eq!(offered, ["beeps", "more"]);

        content.install(OFFICIAL, &pack);
        wait(&mut content);
        let done = content.outcome("beeps").unwrap();
        assert_eq!((done.action, &done.result), (Action::Install, &Ok(())));
        assert!(done.describe().contains("Its sounds: beep"));
        assert!(content.installed("beeps").is_some());
        assert_eq!(sounds.read().unwrap()["beep"], dir.join("beeps/beep.mp3"));

        content.remove("beeps");
        wait(&mut content);
        assert!(content.installed.is_empty() && sounds.read().unwrap().is_empty());
    }
}
