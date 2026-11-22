//! Licenses and credits: raptaboard's own license, the sounds, images and
//! fields built into it, and the Rust crates it's built from.
//!
//! The About window (⚙ Settings, then "About and licenses") shows all of
//! this. No egui here: just the text and lists.
//!
//! When you add a built-in clip, image or field, add a [`MediaCredit`] to
//! [`MEDIA`] and a line to the README beside the file (`media/audio/`,
//! `images/` or `fields/`). After changing dependencies, run
//! `python3 scripts/crate_licenses.py` to remake the crate list and
//! license texts; a test checks they match `Cargo.lock`.
//!
//! ```
//! use raptaboard::credits;
//! // Every built-in sound clip says where it came from.
//! assert!(credits::MEDIA.iter().any(|m| m.files.contains(&"danger.mp3")));
//! // The crates raptaboard is built from, with their licenses.
//! let crates = credits::crates();
//! assert!(crates.iter().any(|c| c.name == "egui"));
//! ```

/// raptaboard's own license (MIT), the repo's `LICENSE` file.
pub const LICENSE: &str = include_str!("../../../LICENSE");

/// The crate list, made by `scripts/crate_licenses.py`: one tab-separated
/// line per crate (name, version, license, repository).
pub const CRATES_TSV: &str = include_str!("../../../licenses/rust-crates.tsv");

/// The full license texts of every crate (about 0.8 MB of text), made by
/// `scripts/crate_licenses.py`.
pub const THIRD_PARTY_LICENSES: &str = include_str!("../../../licenses/THIRD_PARTY_LICENSES.txt");

/// Where a built-in sound, image or field came from, and its license.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaCredit {
    /// What kind of thing: `"Sound"`, `"Image"` or `"Field"`.
    pub kind: &'static str,
    /// The files it covers, by name, in their folder (`media/audio/`,
    /// `images/` or `fields/frc/`).
    pub files: &'static [&'static str],
    /// What it is, in a few words.
    pub what: &'static str,
    /// Who made it and where it came from.
    pub source: &'static str,
    /// The license, as its short name ("CC BY 4.0").
    pub license: &'static str,
    /// A link to the license or the original, if there is one.
    pub link: Option<&'static str>,
    /// Anything else to know: changes we made, or limits on use.
    pub note: &'static str,
}

/// Every sound, image and field built into raptaboard.
pub const MEDIA: &[MediaCredit] = &[
    MediaCredit {
        kind: "Sound",
        files: &["danger.mp3"],
        what: "\"danger danger\" in a robot voice",
        source: "\"danger_normal_lp.wav\" by Sarah-Christin Freytag (xpoki), Freesound, \
                 https://freesound.org/s/436708/",
        license: "CC BY 4.0",
        link: Some("https://creativecommons.org/licenses/by/4.0/"),
        note: "Played twice and converted to MP3.",
    },
    MediaCredit {
        kind: "Sound",
        files: &["auto-ghosts.mp3"],
        what: "\"Auto Ghosts\", for auto starting",
        source: "\"mustphamonderemixes-3\" by dal_kas, Freesound, \
                 https://freesound.org/s/124570/",
        license: "to be confirmed",
        link: Some("https://freesound.org/s/124570/"),
        note: "Converted to MP3. Check the license on its Freesound page and credit it here.",
    },
    MediaCredit {
        kind: "Sound",
        files: &["disabled1.mp3", "disabled2.mp3"],
        what: "\"I'm disabled\"",
        source: "a line from the TV series The IT Crowd",
        license: "not ours",
        link: None,
        note: "A short quote kept for team, non-commercial use.",
    },
    MediaCredit {
        kind: "Sound",
        files: &["fire1.mp3", "fire2.mp3"],
        what: "\"fire\" alarms",
        source: "not recorded yet",
        license: "unknown",
        link: None,
        note: "Source to be found, or the clips replaced.",
    },
    MediaCredit {
        kind: "Image",
        files: &[
            "armed.svg",
            "checkmark.svg",
            "motor.svg",
            "motor-on.svg",
            "motor-off.svg",
        ],
        what: "icons for Boolean Boxes",
        source: "drawn for raptaboard by Team 3200 (Raptacon)",
        license: "CC0 1.0",
        link: Some("https://creativecommons.org/publicdomain/zero/1.0/"),
        note: "Public domain: use them for anything.",
    },
    MediaCredit {
        kind: "Field",
        files: &[
            "2026-rebuilt.png",
            "2026-rebuilt-welded.json",
            "2026-rebuilt-andymark.json",
        ],
        what: "the 2026 REBUILT field and its AprilTags",
        source: "WPILib (allwpilib), copyright FIRST and other WPILib contributors",
        license: "BSD 3-Clause (WPILib)",
        link: Some("https://github.com/wpilibsuite/allwpilib/blob/main/LICENSE.md"),
        note: "",
    },
];

/// WPILib's license, which the built-in field comes under.
pub const WPILIB_LICENSE: &str = include_str!("../../../fields/LICENSE-WPILib.md");

/// One Rust crate raptaboard is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrateLicense<'a> {
    /// The crate's name on crates.io.
    pub name: &'a str,
    /// Its version, as in `Cargo.lock`.
    pub version: &'a str,
    /// Its license, as the crate states it (`"MIT OR Apache-2.0"`).
    pub license: &'a str,
    /// Where its source is, if it says.
    pub repository: &'a str,
}

/// Every Rust crate in [`CRATES_TSV`], sorted by name.
///
/// ```
/// let crates = raptaboard::credits::crates();
/// assert!(crates.windows(2).all(|w| w[0].name <= w[1].name));
/// ```
pub fn crates() -> Vec<CrateLicense<'static>> {
    CRATES_TSV
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut parts = line.split('\t');
            let mut next = || parts.next().unwrap_or("");
            CrateLicense {
                name: next(),
                version: next(),
                license: next(),
                repository: next(),
            }
        })
        .collect()
}

/// How many crates use each license, most used first (ties by name).
///
/// ```
/// let counts = raptaboard::credits::license_counts();
/// assert!(counts.iter().any(|(license, _)| license.contains("MIT")));
/// ```
pub fn license_counts() -> Vec<(&'static str, usize)> {
    let mut counts = std::collections::BTreeMap::new();
    for c in crates() {
        *counts.entry(c.license).or_insert(0) += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    counts
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;
    use std::collections::BTreeSet;

    /// The (name, version) of every crate from crates.io in `Cargo.lock`.
    fn locked() -> BTreeSet<(String, String)> {
        let lock = include_str!("../../../Cargo.lock");
        let mut out = BTreeSet::new();
        for block in lock.split("[[package]]").skip(1) {
            let field = |key: &str| {
                block
                    .lines()
                    .find_map(|l| l.strip_prefix(&format!("{key} = \"")))
                    .map(|v| v.trim_end_matches('"').to_owned())
            };
            // Workspace crates have no source.
            if field("source").is_some() {
                out.insert((field("name").unwrap(), field("version").unwrap()));
            }
        }
        out
    }

    #[test]
    fn crate_list_matches_cargo_lock() {
        let listed: BTreeSet<_> = crates()
            .iter()
            .map(|c| (c.name.to_owned(), c.version.to_owned()))
            .collect();
        assert_eq!(
            listed,
            locked(),
            "licenses/rust-crates.tsv is out of date: run python3 scripts/crate_licenses.py"
        );
        assert!(crates().iter().all(|c| !c.license.is_empty()));
    }

    #[test]
    fn license_texts_cover_every_crate() {
        for (name, version) in locked() {
            assert!(
                THIRD_PARTY_LICENSES.contains(&format!("  {name} {version} ")),
                "{name} {version} is missing from licenses/THIRD_PARTY_LICENSES.txt: \
                 run python3 scripts/crate_licenses.py"
            );
        }
    }

    #[test]
    fn every_built_in_file_has_a_credit() {
        let credited: BTreeSet<&str> = MEDIA.iter().flat_map(|m| m.files.iter().copied()).collect();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/audio");
        for entry in std::fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.ends_with(".mp3") || name.ends_with(".wav") {
                assert!(credited.contains(name.as_str()), "no credit for {name}");
            }
        }
        for (name, _) in crate::images::BUILT_IN {
            assert!(credited.contains(name), "no credit for {name}");
        }
    }
}
