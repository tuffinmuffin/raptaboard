//! A content pack in a folder on this computer: its `pack.toml` and files,
//! as the raptaboard-content repo keeps them (`packs/<id>/`), for
//! `raptaboard packs install --package <folder>` ([`crate::packs_cli`]).
//!
//! The checks match the content repo's `scripts/build_catalog.py`, so a
//! pack that installs from its folder also passes that repo's CI:
//!
//! - `id`, `name`, `version`, `min_app`, `summary`, `license` and
//!   `credits`, each non-empty text; the id lowercase letters, digits and
//!   dashes; the versions like `1.0.0`;
//! - at least one section: `[[sound]]` (a name and a file), `[[image]]`,
//!   `[[field]]`, `[[plugin]]` or `[[layout]]` (a file, and a name if not
//!   the file's), each file of its section's type, no two with one name;
//! - a `[[plugin]]`'s manifest beside its `.wasm` (`alerts.toml` for
//!   `alerts.wasm`), a good `plugin.toml`, whose name is the plugin's and
//!   whose permissions go into the catalog;
//! - every file in the folder listed (by a section, or in `files` for data
//!   the sections read), and every one listed there; files only, named with
//!   letters, digits and `. _ -`.
//!
//! Its [`Pack`]'s file URLs are the files' paths, read by [`read_file`].

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use serde::Deserialize;

use crate::content::{
    self, Catalog, FORMAT, Pack, PackFile, PackItem, PackPlugin, PackSound, is_file_name,
    is_pack_id,
};

/// A pack's `pack.toml`, as written.
#[derive(Debug, Deserialize)]
struct PackToml {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    min_app: Option<String>,
    summary: Option<String>,
    license: Option<String>,
    credits: Option<String>,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    suggested: bool,
    #[serde(default)]
    sound: Vec<PackSound>,
    #[serde(default)]
    image: Vec<PackItem>,
    #[serde(default)]
    field: Vec<PackItem>,
    #[serde(default)]
    plugin: Vec<PackItem>,
    #[serde(default)]
    layout: Vec<PackItem>,
    /// Data files the sections read, besides their own.
    #[serde(default)]
    files: Vec<String>,
}

/// Each section's name in `pack.toml` and the file types it takes.
const SECTIONS: [(&str, &[&str]); 5] = [
    ("sound", &["mp3", "wav", "ogg", "flac"]),
    ("image", &["png", "jpg", "jpeg", "svg"]),
    ("field", &["json"]),
    ("plugin", &["wasm"]),
    ("layout", &["json"]),
];

/// A section's items: the name each is offered under (if given), and its file.
type Items = Vec<(Option<String>, String)>;

/// Whether `v` is a version like `1.0.0`.
fn is_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// The pack in `folder` (its `pack.toml`), checked, with each file's size
/// and checksum, and its path as its URL.
pub fn load(folder: &Path) -> Result<Pack, String> {
    let toml_path = folder.join("pack.toml");
    let text = std::fs::read_to_string(&toml_path)
        .map_err(|e| format!("{}: can't read: {e}", toml_path.display()))?;
    let where_ = toml_path.display().to_string();
    let t: PackToml = toml::from_str(&text).map_err(|e| format!("{where_}: {e}"))?;
    let need = |key: &str, v: &Option<String>| -> Result<String, String> {
        match v {
            Some(v) if !v.trim().is_empty() => Ok(v.clone()),
            _ => Err(format!("{where_}: needs {key}, a non-empty string")),
        }
    };
    let id = need("id", &t.id)?;
    let name = need("name", &t.name)?;
    let version = need("version", &t.version)?;
    let min_app = need("min_app", &t.min_app)?;
    let summary = need("summary", &t.summary)?;
    let license = need("license", &t.license)?;
    let credits = need("credits", &t.credits)?;
    if !is_pack_id(&id) {
        return Err(format!(
            "{where_}: id must be lowercase letters, digits and dashes"
        ));
    }
    for (key, v) in [("version", &version), ("min_app", &min_app)] {
        if !is_version(v) {
            return Err(format!("{where_}: {key} must be a version like 1.0.0"));
        }
    }
    // Each section's items: (name it's offered under, file).
    let items = |list: &[PackItem]| -> Items {
        list.iter()
            .map(|i| (i.name.clone(), i.file.clone()))
            .collect()
    };
    let sounds = t
        .sound
        .iter()
        .map(|s| (Some(s.name.clone()), s.file.clone()))
        .collect();
    let sections: [(&str, Items); 5] = [
        ("sound", sounds),
        ("image", items(&t.image)),
        ("field", items(&t.field)),
        ("plugin", items(&t.plugin)),
        ("layout", items(&t.layout)),
    ];
    if sections.iter().all(|(_, list)| list.is_empty()) {
        return Err(format!(
            "{where_}: needs at least one of [[sound]], [[image]], [[field]], [[plugin]], [[layout]]"
        ));
    }
    let mut listed: BTreeSet<String> = t.files.iter().cloned().collect();
    let plugins = plugins(folder, &where_, &t.plugin)?;
    let sections = sections.map(|(section, list)| {
        if section != "plugin" {
            return (section, list);
        }
        // Named by their manifests.
        let named = plugins
            .iter()
            .map(|p| (p.name.clone(), p.file.clone()))
            .collect();
        (section, named)
    });
    listed.extend(plugins.iter().map(PackPlugin::manifest));
    for ((section, list), (_, types)) in sections.iter().zip(SECTIONS) {
        let mut names = HashSet::new();
        for (name, file) in list {
            if *section == "sound" && name.as_deref().is_none_or(|n| n.trim().is_empty()) {
                return Err(format!("{where_}: each [[sound]] needs a name"));
            }
            let label = name.clone().unwrap_or_else(|| file.clone());
            if !names.insert(label.clone()) {
                return Err(format!("{where_}: two {section}s are named {label:?}"));
            }
            let ext = Path::new(file)
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase());
            if !ext.is_some_and(|e| types.contains(&e.as_str())) {
                return Err(format!(
                    "{where_}: {file} isn't a {section} file ({})",
                    types.join(", ")
                ));
            }
            listed.insert(file.clone());
        }
    }
    let entries =
        std::fs::read_dir(folder).map_err(|e| format!("{}: can't read: {e}", folder.display()))?;
    let mut present = BTreeSet::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name == "pack.toml" {
            continue;
        }
        if !entry.path().is_file() {
            return Err(format!("{file_name}: packs hold files only, no folders"));
        }
        present.insert(file_name);
    }
    let missing: Vec<&String> = listed.difference(&present).collect();
    if !missing.is_empty() {
        return Err(format!(
            "{where_}: lists files that aren't there: {missing:?}"
        ));
    }
    let extra: Vec<&String> = present.difference(&listed).collect();
    if !extra.is_empty() {
        return Err(format!(
            "{where_}: has files it doesn't list (add them to files): {extra:?}"
        ));
    }
    let mut files = Vec::new();
    for path in &listed {
        if !is_file_name(path) || path == content::Installed::FILE {
            return Err(format!(
                "{where_}: {path:?}: file names are letters, digits, . _ -"
            ));
        }
        let full = folder.join(path);
        let bytes =
            std::fs::read(&full).map_err(|e| format!("{}: can't read: {e}", full.display()))?;
        files.push(PackFile {
            path: path.clone(),
            url: std::path::absolute(&full)
                .unwrap_or(full)
                .display()
                .to_string(),
            size: bytes.len() as u64,
            sha256: content::sha256_hex(&bytes),
        });
    }
    Ok(Pack {
        id,
        name,
        version,
        kind: t.kind,
        min_app,
        summary,
        license,
        credits,
        suggested: t.suggested,
        sounds: t.sound,
        images: t.image,
        fields: t.field,
        plugins,
        layouts: t.layout,
        files,
    })
}

/// The pack's plugins, each named and given its permissions and needs by its manifest
/// (`alerts.toml` beside `alerts.wasm`), which must be there and good.
fn plugins(folder: &Path, where_: &str, items: &[PackItem]) -> Result<Vec<PackPlugin>, String> {
    let mut out = Vec::new();
    for item in items {
        let mut plugin = PackPlugin {
            name: None,
            file: item.file.clone(),
            permissions: Vec::new(),
            needs: None,
        };
        if !item.file.ends_with(".wasm") {
            // Its section's check says what's wrong.
            out.push(plugin);
            continue;
        }
        let manifest = plugin.manifest();
        let text = std::fs::read_to_string(folder.join(&manifest)).map_err(|_| {
            format!(
                "{where_}: plugin {} needs its manifest beside it, {manifest}",
                item.file
            )
        })?;
        let m = raptaboard_plugin_host::Manifest::from_toml(&text)
            .map_err(|e| format!("{}: {e}", folder.join(&manifest).display()))?;
        if item.name.as_ref().is_some_and(|n| *n != m.name) {
            return Err(format!(
                "{where_}: plugin {} is named {:?} in {manifest}",
                item.file, m.name
            ));
        }
        plugin.needs = Some(crate::content::PluginNeeds::of(&m));
        plugin.name = Some(m.name);
        plugin.permissions = m
            .permissions
            .names()
            .iter()
            .map(|p| p.to_string())
            .collect();
        out.push(plugin);
    }
    Ok(out)
}

/// A catalog holding only the pack in `folder`.
pub fn catalog(folder: &Path) -> Result<Catalog, String> {
    Ok(Catalog {
        format: FORMAT,
        release: "local".into(),
        packs: vec![load(folder)?],
    })
}

/// Read the file at `path` (a local pack's file URL), at most `limit`
/// bytes: a [`content::Fetch`] for a pack [`load`]ed from a folder.
pub fn read_file(path: &str, limit: u64) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: can't read: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{path} is larger than {limit} bytes"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    /// A folder holding `pack.toml` (with `toml` as its text) and `files`.
    fn pack_folder(name: &str, toml: &str, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        let dir = content::tests::folder(name);
        std::fs::write(dir.join("pack.toml"), toml).unwrap();
        for (f, bytes) in files {
            std::fs::write(dir.join(f), bytes).unwrap();
        }
        dir
    }

    const HEAD: &str = r#"
id = "horns"
name = "Horns"
version = "1.2.0"
min_app = "0.1.0"
summary = "Some horns."
license = "CC0"
credits = "Made by us."
"#;

    #[test]
    fn a_good_pack_loads_with_its_files_sizes_and_checksums() {
        let toml = format!(
            "{HEAD}\n[[sound]]\nname = \"horn\"\nfile = \"horn.mp3\"\n\n[[image]]\nfile = \"horn.png\"\n"
        );
        let dir = pack_folder(
            "pack-good",
            &toml,
            &[("horn.mp3", b"toot"), ("horn.png", b"png")],
        );
        let pack = load(&dir).unwrap();
        assert_eq!(
            (pack.id.as_str(), pack.version.as_str()),
            ("horns", "1.2.0")
        );
        assert_eq!(pack.contents(), "1 sound, 1 image");
        let horn = pack.files.iter().find(|f| f.path == "horn.mp3").unwrap();
        assert_eq!(horn.size, 4);
        assert_eq!(horn.sha256, content::sha256_hex(b"toot"));
        assert_eq!(read_file(&horn.url, 100).unwrap(), b"toot");
        assert!(read_file(&horn.url, 2).is_err(), "over the limit");
    }

    /// A plugin manifest named `name`, asking to play sounds.
    fn manifest(name: &str) -> String {
        format!(
            "name = \"{name}\"\nversion = \"0.1.0\"\napi = \"0.3\"\n[match]\ntype = \"Arm\"\n\
             [permissions]\nsound = true\n"
        )
    }

    #[test]
    fn a_plugin_is_named_by_its_manifest_beside_it() {
        let toml = format!("{HEAD}\n[[plugin]]\nfile = \"arm.wasm\"\n");
        let arm = manifest("arm");
        let files: &[(&str, &[u8])] = &[("arm.wasm", b"wasm"), ("arm.toml", arm.as_bytes())];
        let pack = load(&pack_folder("pack-plugin", &toml, files)).unwrap();
        assert_eq!(pack.plugins[0].name.as_deref(), Some("arm"));
        assert_eq!(pack.plugins[0].permissions, ["sound"]);
        assert_eq!(pack.plugin_lines(), ["Plugin arm: plays sounds"]);
        // The manifest is one of its files, so it downloads.
        assert!(pack.files.iter().any(|f| f.path == "arm.toml"));

        let error = |toml: &str, files: &[(&str, &[u8])]| {
            load(&pack_folder("pack-plugin-bad", toml, files)).unwrap_err()
        };
        assert!(error(&toml, &[("arm.wasm", b"wasm")]).contains("needs its manifest beside it"));
        let named = toml.replace("file = ", "name = \"leg\"\nfile = ");
        assert!(error(&named, files).contains("is named \"arm\""));
        let no_api = arm.replace("api = \"0.3\"\n", "");
        assert!(
            error(
                &toml,
                &[("arm.wasm", b"wasm"), ("arm.toml", no_api.as_bytes())]
            )
            .contains("arm.toml")
        );
    }

    #[test]
    fn a_wrong_pack_says_whats_wrong() {
        let sound = "\n[[sound]]\nname = \"horn\"\nfile = \"horn.mp3\"\n";
        let error = |toml: String, files: &[(&str, &[u8])]| {
            let dir = pack_folder("pack-bad", &toml, files);
            load(&dir).unwrap_err()
        };
        let horn: &[(&str, &[u8])] = &[("horn.mp3", b"toot")];
        assert!(error(HEAD.replace("license = \"CC0\"", ""), horn).contains("needs license"));
        assert!(
            error(format!("{}{sound}", HEAD.replace("horns", "Horns!")), horn)
                .contains("id must be")
        );
        assert!(
            error(format!("{}{sound}", HEAD.replace("1.2.0", "1.2")), horn).contains("version")
        );
        assert!(error(HEAD.to_owned(), &[]).contains("needs at least one"));
        assert!(error(format!("{HEAD}{sound}"), &[]).contains("aren't there"));
        assert!(
            error(
                format!("{HEAD}{sound}"),
                &[("horn.mp3", b"t"), ("notes.txt", b"n")]
            )
            .contains("doesn't list")
        );
        assert!(
            error(
                format!("{HEAD}{}", sound.replace("horn.mp3", "horn.txt")),
                &[("horn.txt", b"t")]
            )
            .contains("isn't a sound file")
        );
        assert!(error(format!("{HEAD}{sound}{sound}"), horn).contains("two sounds are named"));
    }
}
