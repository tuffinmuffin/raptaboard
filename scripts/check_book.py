#!/usr/bin/env python3
"""Check the book (book/src) and the README: every chapter SUMMARY.md links
exists, every page is in SUMMARY.md, every relative link and image in
them resolves, and every `{{#include file:anchor}}` names a file with that
anchor (mdBook includes nothing, without a word, when the anchor is
gone). Web links aren't fetched. CI runs it before building the book.

    python3 scripts/check_book.py [repo]
"""

import re
import sys
from pathlib import Path

# [text](target) and ![alt](target); the target ends at a space (a title
# follows) or the closing parenthesis.
LINK = re.compile(r"!?\[[^\]]*\]\(\s*<?([^)\s>]*)>?(?:\s+\"[^\"]*\")?\s*\)")
FENCE = re.compile(r"^\s*(```|~~~)")


def links(text: str) -> list[tuple[int, str]]:
    """Each link's line number and target, outside code (fenced blocks and
    `inline code`)."""
    out = []
    fenced = False
    for n, line in enumerate(text.splitlines(), 1):
        if FENCE.match(line):
            fenced = not fenced
            continue
        if fenced:
            continue
        line = re.sub(r"`[^`]*`", "", line)
        out.extend((n, m.group(1)) for m in LINK.finditer(line))
    return out


def is_local(target: str) -> bool:
    """Whether a link is to a file here: not the web, mail or this page."""
    return bool(target) and not re.match(r"^[a-z][a-z0-9+.-]*:", target, re.I) and not target.startswith("#")


def broken(page: Path, text: str, root: Path) -> list[str]:
    """The relative links in `page` whose file isn't there (a book page's
    `.html` link stands for its `.md`)."""
    out = []
    for n, target in links(text):
        if not is_local(target):
            continue
        path = target.split("#")[0].split("?")[0]
        if not path:
            continue
        to = (page.parent / path).resolve()
        if not to.exists() and not (to.suffix == ".html" and to.with_suffix(".md").exists()):
            out.append(f"{page.relative_to(root)}:{n}: {target} isn't there")
    return out


# {{#include path}} or {{#include path:anchor}} (line ranges, path:3:9, are
# only checked for the file).
INCLUDE = re.compile(r"\{\{#include\s+([^}\s:]+)(?::([A-Za-z_][\w-]*))?[^}]*\}\}")


def bad_includes(page: Path, text: str, root: Path) -> list[str]:
    """The `{{#include}}`s in `page` whose file or anchor isn't there, or
    whose anchor holds nothing."""
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        for m in INCLUDE.finditer(line):
            path, anchor = m.group(1), m.group(2)
            where = f"{page.relative_to(root)}:{n}"
            source = (page.parent / path).resolve()
            if not source.is_file():
                out.append(f"{where}: {path} isn't there")
                continue
            if anchor is None:
                continue
            lines = source.read_text(encoding="utf-8").splitlines()
            start = [i for i, l in enumerate(lines) if re.search(rf"ANCHOR:\s*{re.escape(anchor)}\b", l)]
            end = [i for i, l in enumerate(lines) if re.search(rf"ANCHOR_END:\s*{re.escape(anchor)}\b", l)]
            if not start or not end or end[0] <= start[0]:
                out.append(f"{where}: {path} has no anchor {anchor!r}")
            elif not any(l.strip() for l in lines[start[0] + 1 : end[0]]):
                out.append(f"{where}: anchor {anchor!r} in {path} is empty")
    return out


def check(repo: Path) -> list[str]:
    """Every problem in the book and README, as `file:line: what`."""
    src = repo / "book" / "src"
    summary = src / "SUMMARY.md"
    if not summary.is_file():
        return ["book/src/SUMMARY.md isn't there"]
    problems = []
    chapters = set()
    for _, target in links(summary.read_text(encoding="utf-8")):
        if target:  # () is a draft: a page still to write
            chapters.add((src / target).resolve())
    for page in sorted(src.rglob("*.md")):
        if page != summary and page.resolve() not in chapters:
            problems.append(f"{page.relative_to(repo)}: not in SUMMARY.md")
    for page in [repo / "README.md", *sorted(src.rglob("*.md"))]:
        if page.is_file():
            text = page.read_text(encoding="utf-8")
            problems += broken(page, text, repo)
            problems += bad_includes(page, text, repo)
    return problems


def main() -> int:
    repo = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
    problems = check(repo)
    for p in problems:
        print(p, file=sys.stderr)
    if problems:
        return 1
    print("book: every chapter, link, image and include is there")
    return 0


if __name__ == "__main__":
    sys.exit(main())
