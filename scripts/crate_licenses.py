#!/usr/bin/env python3
"""List the Rust crates built into raptaboard, with their licenses.

Usage (from the repo root): python3 scripts/crate_licenses.py

Reads `cargo metadata --locked` and writes two files in `licenses/`, both
built into the program and shown in its About window (gear, then "About
and licenses"):

- `rust-crates.tsv`: one tab-separated line per crate (name, version,
  license, repository), sorted by name, without the workspace's own crates.
- `THIRD_PARTY_LICENSES.txt`: the license texts themselves. Each license
  file the crates ship (LICENSE, COPYING, NOTICE and the like) appears
  once, with the crates that ship it listed above it. Crates that ship no
  license file are listed by their license, and the standard texts of
  those licenses (from `licenses/spdx/`, copied from SPDX's
  license-list-data) are at the end.

Run it again after changing dependencies: a test checks both files still
match Cargo.lock.

Written by Claude.
"""

import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "licenses"
# File names (upper-cased) that hold a crate's license or copyright notice.
LICENSE_NAMES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE")


def license_files(crate_dir: Path) -> list[Path]:
    """The license files a crate ships: at its top, and fonts' licenses."""
    found = [
        p
        for p in crate_dir.iterdir()
        if p.is_file() and p.name.upper().startswith(LICENSE_NAMES)
    ]
    fonts = crate_dir / "fonts"
    if fonts.is_dir():
        found += [p for p in fonts.iterdir() if p.suffix == ".txt"]
    return sorted(found)


def main() -> None:
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        check=True,
        capture_output=True,
        text=True,
        cwd=ROOT,
    ).stdout
    meta = json.loads(out)
    members = set(meta["workspace_members"])
    packages = sorted(
        (p for p in meta["packages"] if p["id"] not in members),
        key=lambda p: (p["name"], p["version"]),
    )

    rows = []
    # Each distinct license text, the crates that ship it, in first-seen order.
    texts: dict[str, list[str]] = {}
    # Crates with no license file, by license expression.
    bare: dict[str, list[str]] = {}
    for pkg in packages:
        license = (pkg.get("license") or "").strip()
        if not license and pkg.get("license_file"):
            license = "see license file"
        license = license or "unknown"
        repo = (pkg.get("repository") or "").strip()
        rows.append((pkg["name"], pkg["version"], license, repo))

        crate = f"{pkg['name']} {pkg['version']}"
        files = license_files(Path(pkg["manifest_path"]).parent)
        if not files:
            authors = ", ".join(pkg.get("authors") or []) or "its authors"
            bare.setdefault(license, []).append(f"{crate} (copyright {authors})")
        for f in files:
            text = f.read_text(errors="replace").strip()
            texts.setdefault(text, []).append(f"{crate} ({f.name})")

    with open(OUT / "rust-crates.tsv", "w", newline="\n") as w:
        w.write("# name\tversion\tlicense\trepository\n")
        for row in rows:
            w.write("\t".join(row) + "\n")

    rule = "=" * 78
    with open(OUT / "THIRD_PARTY_LICENSES.txt", "w", newline="\n") as w:
        w.write(
            "Licenses of the Rust crates built into raptaboard\n\n"
            "raptaboard itself is under the MIT license (LICENSE in its repository).\n"
            "It is built from the open-source crates below. Each license text they\n"
            "ship is given once, after the crates that ship it. Where a crate offers\n"
            "a choice of licenses (\"MIT OR Apache-2.0\"), raptaboard uses it under\n"
            "the first that is not a GPL or LGPL.\n"
            "Made by scripts/crate_licenses.py from Cargo.lock.\n"
        )
        for text, crates in texts.items():
            w.write(f"\n{rule}\nUsed by:\n")
            for c in crates:
                w.write(f"  {c}\n")
            w.write(f"{rule}\n\n{text}\n")
        w.write(f"\n{rule}\nCrates that ship no license file\n{rule}\n")
        for license, crates in sorted(bare.items()):
            w.write(f"\nUnder {license}:\n")
            for c in crates:
                w.write(f"  {c}\n")
        used = set()
        for license in bare:
            used.update(re.findall(r"[A-Za-z0-9.-]+", license))
        for spdx in sorted((OUT / "spdx").glob("*.txt")):
            if spdx.stem in used:
                text = spdx.read_text().strip()
                w.write(f"\n{rule}\nStandard text: {spdx.stem}\n{rule}\n\n{text}\n")
    missing = sorted(
        # GPL and LGPL are only ever an alternative here, never the one used.
        {i for i in used if i not in {"OR", "AND", "WITH"} and "GPL" not in i}
        - {p.stem for p in (OUT / "spdx").glob("*.txt")}
    )
    if missing:
        print("No standard text in licenses/spdx/ for:", ", ".join(missing))


if __name__ == "__main__":
    os.chdir(ROOT)
    main()
