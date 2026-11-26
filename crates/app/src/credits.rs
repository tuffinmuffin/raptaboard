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

/// raptaboard's privacy policy (the repo's `PRIVACY.md`, which the Microsoft
/// Store listing links to), shown on the About window's Privacy page.
pub const PRIVACY: &str = include_str!("../../../PRIVACY.md");

/// Where the privacy policy is online.
pub const PRIVACY_URL: &str = "https://github.com/tuffinmuffin/raptaboard/blob/main/PRIVACY.md";

/// A part of the privacy policy, as the About window draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyBlock {
    /// The title (`# `).
    Title(String),
    /// A section heading (`## `).
    Heading(String),
    /// A smaller heading, inside a section (`### `).
    Subheading(String),
    /// A paragraph, its lines joined.
    Paragraph(String),
    /// A list item (`- `), its lines joined.
    Item(String),
    /// A list item inside another (`  - `), its lines joined.
    SubItem(String),
}

/// The privacy policy's Markdown as blocks of plain text: links become
/// their text with the address after it, and `**` and backticks go.
///
/// ```
/// use raptaboard::credits::{policy_blocks, PolicyBlock};
/// let blocks = policy_blocks(
///     "# Title\n\n### Part\n\nSome **bold**\ntext, [a link](https://x.org).\n\n- one\n  more\n  - inner",
/// );
/// assert_eq!(blocks, [
///     PolicyBlock::Title("Title".into()),
///     PolicyBlock::Subheading("Part".into()),
///     PolicyBlock::Paragraph("Some bold text, a link (https://x.org).".into()),
///     PolicyBlock::Item("one more".into()),
///     PolicyBlock::SubItem("inner".into()),
/// ]);
/// ```
pub fn policy_blocks(markdown: &str) -> Vec<PolicyBlock> {
    let mut blocks = Vec::new();
    for chunk in markdown.split("\n\n") {
        // The item being read, and whether it's inside another.
        let mut item: Option<(String, bool)> = None;
        let mut para: Vec<&str> = Vec::new();
        let flush =
            |blocks: &mut Vec<PolicyBlock>, item: &mut Option<(String, bool)>| match item.take() {
                Some((i, false)) => blocks.push(PolicyBlock::Item(plain(&i))),
                Some((i, true)) => blocks.push(PolicyBlock::SubItem(plain(&i))),
                None => {}
            };
        for raw in chunk.lines().filter(|l| !l.trim().is_empty()) {
            let line = raw.trim();
            let indented = raw.starts_with(' ');
            if let Some(h) = line.strip_prefix("### ") {
                blocks.push(PolicyBlock::Subheading(plain(h)));
            } else if let Some(h) = line.strip_prefix("## ") {
                blocks.push(PolicyBlock::Heading(plain(h)));
            } else if let Some(h) = line.strip_prefix("# ") {
                blocks.push(PolicyBlock::Title(plain(h)));
            } else if let Some(i) = line.strip_prefix("- ") {
                flush(&mut blocks, &mut item);
                item = Some((i.to_owned(), indented));
            } else if let Some((i, _)) = &mut item {
                i.push(' ');
                i.push_str(line);
            } else {
                para.push(line);
            }
        }
        flush(&mut blocks, &mut item);
        if !para.is_empty() {
            blocks.push(PolicyBlock::Paragraph(plain(&para.join(" "))));
        }
    }
    blocks
}

/// Markdown inline text as plain text: `[text](url)` → `text (url)`,
/// `<url>` → `url`, no `**` or backticks.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let Some(mid) = rest[start..].find("](").map(|m| start + m) else {
            break;
        };
        let Some(end) = rest[mid..].find(')').map(|e| mid + e) else {
            break;
        };
        out.push_str(&rest[..start]);
        out.push_str(&rest[start + 1..mid]);
        out.push_str(" (");
        out.push_str(&rest[mid + 2..end]);
        out.push(')');
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.replace("**", "").replace(['`', '<', '>'], "")
}

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
        source: "\"MustphaMondeRemixes (3).aif\" by dal_kas, Freesound, \
                 https://freesound.org/s/124570/, a remix of \"Ghosts In The Machine.wav\" \
                 by deleted_user_44129, https://freesound.org/s/124470/",
        license: "CC BY 3.0",
        link: Some("https://creativecommons.org/licenses/by/3.0/"),
        note: "Converted to MP3.",
    },
    MediaCredit {
        kind: "Sound",
        files: &["go-go-go.mp3"],
        what: "\"go go go\", for teleop starting",
        source: "\"Go Go Go - (Version 2) Northern British Soldier\" by theuncertainman, \
                 Freesound, https://freesound.org/s/402643/",
        license: "CC BY 4.0",
        link: Some("https://creativecommons.org/licenses/by/4.0/"),
        note: "Converted to MP3.",
    },
    MediaCredit {
        kind: "Sound",
        files: &["fms-down.mp3", "lost-comms.mp3"],
        what: "\"FMS lost\" and \"comms lost\", for losing the field or the robot",
        source: "made for raptaboard with Free TTS Online (text to speech), 2026-11-22",
        license: "free to use: Free TTS Online says its audio may be used for personal and \
                  commercial purposes",
        link: None,
        note: "No other license or restriction.",
    },
    MediaCredit {
        kind: "Image",
        files: &[
            "armed.svg",
            "checkmark.svg",
            "motor.svg",
            "motor-on.svg",
            "motor-off.svg",
            "estop.svg",
            "disabled.svg",
            "auto.svg",
            "teleop.svg",
            "test.svg",
        ],
        what: "icons for Boolean Boxes, and the FMSInfo widget's robot states",
        source: "drawn for raptaboard by Team 3200 (Raptacon)",
        license: "CC0 1.0",
        link: Some("https://creativecommons.org/publicdomain/zero/1.0/"),
        note: "Public domain: use them for anything.",
    },
    MediaCredit {
        kind: "Image",
        files: &["raptaboard.svg"],
        what: "raptaboard's app icon: the raptor in a gear",
        source: "Raptacon's raptor mark, from Raptacon's brand kit \
                 (https://github.com/Raptacon/brand-kit), in Raptacon Green inside a \
                 gear drawn for raptaboard",
        license: "Raptacon (Team 3200) brand: free to use and modify for team purposes",
        link: Some("https://github.com/Raptacon/brand-kit"),
        note: "The raptor mark is Raptacon's; ask the team before using it elsewhere.",
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

    #[test]
    fn the_privacy_policy_reads_as_plain_text() {
        let blocks = policy_blocks(PRIVACY);
        assert_eq!(
            blocks.first(),
            Some(&PolicyBlock::Title("raptaboard privacy policy".into()))
        );
        let headings: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                PolicyBlock::Heading(h) => Some(h.as_str()),
                _ => None,
            })
            .collect();
        assert!(headings.contains(&"We collect no data"), "{headings:?}");
        assert!(headings.contains(&"What it connects to"), "{headings:?}");
        // No Markdown left to show as symbols.
        for b in &blocks {
            let text = format!("{b:?}");
            assert!(
                !text.contains("**")
                    && !text.contains("](")
                    && !text.contains('`')
                    && !text.contains("\"#")
                    && !text.contains("\"- "),
                "{text}"
            );
        }
        // Subheadings and lists, nested too, from a sample (the policy has
        // none today).
        let sample =
            policy_blocks("# T\n\n## H\n\n### Packs\n\n- **One** item\n  - a `sub` item\n- Two\n");
        assert!(sample.contains(&PolicyBlock::Subheading("Packs".into())));
        assert!(sample.contains(&PolicyBlock::Item("One item".into())));
        assert!(sample.contains(&PolicyBlock::SubItem("a sub item".into())));
        assert!(sample.contains(&PolicyBlock::Item("Two".into())));
    }

    #[test]
    fn every_credited_sound_is_built_in() {
        // The fan clips moved to a content pack (their credits with them).
        let dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/audio"));
        for m in MEDIA.iter().filter(|m| m.kind == "Sound") {
            for file in m.files {
                assert!(
                    dir.join(file).is_file(),
                    "{file} is credited but not built in"
                );
            }
        }
    }
}
