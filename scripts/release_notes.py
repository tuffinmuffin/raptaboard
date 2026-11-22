#!/usr/bin/env python3
"""Print the GitHub Release notes for a version: its CHANGELOG.md section
and which file to download. The release workflow runs it.

    python scripts/release_notes.py 0.1.0
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

DOWNLOADS = """\
## Downloads

| System | File |
| --- | --- |
| Windows, installed for you (Start menu, uninstaller; no admin needed) | `raptaboard-{v}-windows-x64-setup.exe` |
| Windows, portable (unzip anywhere, a USB stick is fine) | `raptaboard-{v}-windows-x64.zip` |
| macOS 11 or newer, Apple Silicon and Intel | `raptaboard-{v}-macos-universal.zip` |
| Linux x64 (`./install.sh` adds it to your menu) | `raptaboard-{v}-linux-x64.tar.gz` |

These builds aren't signed yet. **Windows:** at "Windows protected your PC", click "More info",
then "Run anyway". **macOS:** right-click raptaboard.app, choose Open, then Open again.

The `-debug` files hold debug info for decoding crash reports; you don't need them to run
raptaboard. `SHA256SUMS.txt` lists each file's checksum.
"""


def section(changelog: str, version: str) -> str:
    """The text under `## {version}` in the changelog, up to the next `## `."""
    lines = changelog.splitlines()
    start = next(
        (i for i, l in enumerate(lines) if l.strip() == f"## {version}"), None
    )
    if start is None:
        sys.exit(f"CHANGELOG.md has no '## {version}' section")
    body = []
    for line in lines[start + 1 :]:
        if line.startswith("## "):
            break
        body.append(line)
    return "\n".join(body).strip()


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    version = sys.argv[1].removeprefix("v")
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    print(section(changelog, version))
    print()
    print(DOWNLOADS.format(v=version))


if __name__ == "__main__":
    main()
