#!/usr/bin/env python3
"""Turn a cargo-llvm-cov JSON summary into a Markdown coverage report.

Usage: python3 scripts/coverage_summary.py <coverage.json> [<base.json>]

<coverage.json> is `cargo llvm-cov report --json --summary-only` output.
With <base.json> (the same report for the base branch), each crate's line
coverage also shows the change. Prints the report on stdout: a table per
crate, then a collapsed table per file.

Written by Claude.
"""

import json
import pathlib
import sys

# Marks the PR comment so CI updates it instead of adding a new one.
MARKER = "<!-- raptaboard-coverage -->"
KINDS = ("lines", "functions", "regions")


def load(path: str) -> dict:
    """{relative file path: summary} from an llvm-cov JSON export."""
    data = json.loads(pathlib.Path(path).read_text())
    files = {}
    for f in data["data"][0]["files"]:
        name = f["filename"].replace("\\", "/")
        # Paths are absolute; keep them from `crates/` on.
        i = name.find("crates/")
        files[name[i:] if i >= 0 else name] = f["summary"]
    return files


def crate_of(path: str) -> str:
    parts = path.split("/")
    return parts[1] if len(parts) > 2 and parts[0] == "crates" else "(other)"


def add(totals: dict, summary: dict) -> None:
    for k in KINDS:
        t = totals.setdefault(k, [0, 0])
        t[0] += summary[k]["covered"]
        t[1] += summary[k]["count"]


def pct(covered: int, count: int) -> float | None:
    return 100.0 * covered / count if count else None


def fmt(covered: int, count: int) -> str:
    p = pct(covered, count)
    return "–" if p is None else f"{p:.1f}% ({covered}/{count})"


def by_crate(files: dict) -> dict:
    crates: dict = {}
    for path, summary in files.items():
        add(crates.setdefault(crate_of(path), {}), summary)
    return crates


def delta(now: list, base: list | None) -> str:
    if base is None:
        return "new"
    a, b = pct(*now), pct(*base)
    if a is None or b is None:
        return ""
    d = a - b
    if abs(d) < 0.05:
        return "±0.0"
    return f"{d:+.1f}"


def report(files: dict, base: dict | None) -> str:
    crates = by_crate(files)
    base_crates = by_crate(base) if base is not None else None
    total: dict = {}
    for summary in files.values():
        add(total, summary)

    out = [MARKER, "## Coverage", ""]
    line = f"**{fmt(*total['lines'])}** of lines covered"
    if base is not None:
        base_total: dict = {}
        for summary in base.values():
            add(base_total, summary)
        line += f" ({delta(total['lines'], base_total.get('lines'))} against the base branch)"
    out += [line + ".", ""]

    header = "| Crate | Lines | Functions | Regions |"
    if base_crates is not None:
        header += " Lines vs base |"
    out += [header, "|" + " --- |" * (header.count("|") - 1)]
    for crate in sorted(crates):
        t = crates[crate]
        row = f"| `{crate}` | " + " | ".join(fmt(*t[k]) for k in KINDS) + " |"
        if base_crates is not None:
            b = base_crates.get(crate)
            row += f" {delta(t['lines'], b['lines'] if b else None)} |"
        out.append(row)

    out += ["", "<details><summary>By file</summary>", ""]
    out += ["| File | Lines | Functions |", "| --- | --- | --- |"]
    for path in sorted(files):
        s = files[path]
        cells = [fmt(s[k]["covered"], s[k]["count"]) for k in ("lines", "functions")]
        out.append(f"| `{path}` | {cells[0]} | {cells[1]} |")
    out += ["", "</details>", ""]
    out.append(
        "All workspace crates (`layout`, `plugin-host`, `app`); test, example "
        "and bench files are not counted. Drawing code in `app` only runs in a "
        "window, so its coverage stays low."
    )
    return "\n".join(out) + "\n"


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        return 2
    files = load(sys.argv[1])
    base = None
    if len(sys.argv) == 3 and pathlib.Path(sys.argv[2]).is_file():
        base = load(sys.argv[2])
    sys.stdout.write(report(files, base))
    return 0


if __name__ == "__main__":
    sys.exit(main())
