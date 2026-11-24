#!/usr/bin/env python3
"""Build raptaboard's release packages for the system this runs on.

    python scripts/package.py                # build, then package into dist/
    python scripts/package.py --skip-build   # package what's already built
    python scripts/package.py --msix         # Windows: also the Microsoft Store package

Windows: a portable zip (with its own .raptaboard settings folder) and a
per-user installer (Inno Setup's iscc must be installed). Every package has
both programs: raptaboard (the dashboard) and raptaboard-cli (its commands,
such as `packs`, as a terminal program; on Windows, where raptaboard.exe is
a window program, the only way a terminal waits for them). macOS: a zipped
raptaboard.app, one program for Apple Silicon and Intel when both targets are
given (--target aarch64-apple-darwin --target x86_64-apple-darwin). Linux: a
.tar.gz with install.sh. Each also gets a -debug archive holding the debug
info (keep it to decode crash reports).

With --msix (Windows), also an MSIX for the Microsoft Store in dist-store/,
not dist/: the same raptaboard.exe as the other Windows packages (not
raptaboard-cli, which the Store package leaves out), unsigned
(the Store signs it), from packaging/windows/msix/
(the manifest, and store.json with the identity from Partner Center), with
makeappx from the Windows SDK. It's uploaded to Partner Center, never
attached to a GitHub release (unsigned, it won't install).

The crates' license texts come from licenses/THIRD_PARTY_LICENSES.txt (made by
scripts/crate_licenses.py, the same list the About window shows). Run from
anywhere; paths are from the repo's top folder. Only Python's standard
library is used.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKAGING = ROOT / "packaging"
ICONS = ROOT / "media" / "icon"

# A fresh settings file for the Windows zip: raptaboard uses a .raptaboard
# folder in the folder it starts in before the one in the home folder, and
# Explorer starts it in its own folder, so the zip's settings stay with it.
PORTABLE_SETTINGS = """\
# raptaboard's settings for this folder (the portable zip). raptaboard
# fills this in as you change settings; delete this .raptaboard folder to
# use the settings in your home folder instead.
"""


def run(*args: str | Path, cwd: Path = ROOT) -> None:
    print("+", " ".join(str(a) for a in args), flush=True)
    subprocess.run([str(a) for a in args], cwd=cwd, check=True)


def version() -> str:
    with open(ROOT / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["workspace"]["package"]["version"]


def system() -> str:
    if sys.platform == "win32":
        return "windows"
    if sys.platform == "darwin":
        return "macos"
    return "linux"


def build_dir(target: str | None) -> Path:
    base = ROOT / "target"
    return (base / target if target else base) / "release"


# The programs every package holds: the dashboard, and raptaboard-cli, its
# commands (`raptaboard-cli packs list`) as a terminal program. Windows needs
# it (raptaboard.exe is a window program there); macOS and Linux ship it too,
# so the command is the same everywhere.
PROGRAMS = ["raptaboard", "raptaboard-cli"]


def build(targets: list[str | None]) -> None:
    bins = [arg for name in PROGRAMS for arg in ("--bin", name)]
    for target in targets:
        extra = ["--target", target] if target else []
        run("cargo", "build", "--release", "--locked", "-p", "raptaboard", *bins, *extra)


def built(path: Path) -> Path:
    """`path`, a program that must have been built: stops if it's missing (a
    package without raptaboard-cli would build fine and be wrong)."""
    if not path.is_file():
        sys.exit(f"{path} wasn't built")
    return path


def debug_file(folder: Path, program: str, suffix: str) -> Path:
    """Where a program's debug info is in `folder` (`.pdb`, `.dSYM`, `.dwp`):
    named after the program, or with its dashes as underscores, as Cargo
    names it on some systems (raptaboard_cli.pdb). If neither is there, the
    first name, which the archive notes as missing and skips."""
    names = [folder / f"{n}{suffix}" for n in (program, program.replace("-", "_"))]
    return next((p for p in names if p.exists()), names[0])


def add_windows_programs(release: Path, stage: Path) -> None:
    """Copy each program (.exe) into `stage`."""
    for name in PROGRAMS:
        shutil.copy(built(release / f"{name}.exe"), stage)


def fresh(path: Path) -> Path:
    if path.exists():
        shutil.rmtree(path)
    path.mkdir(parents=True)
    return path


def add_docs(into: Path, ver: str) -> None:
    """The files every package has: the user README (with the version
    filled in), the changelog and the licenses."""
    readme = (PACKAGING / "README.txt").read_text(encoding="utf-8")
    (into / "README.txt").write_text(readme.replace("{VERSION}", ver), encoding="utf-8")
    shutil.copy(ROOT / "CHANGELOG.md", into / "CHANGELOG.md")
    shutil.copy(ROOT / "LICENSE", into / "LICENSE.txt")
    # The built-in 2026 field image is WPILib's; its license asks for this.
    shutil.copy(ROOT / "fields" / "LICENSE-WPILib.md", into / "LICENSE-WPILib.txt")
    # Every crate built in, with its license text (also in the About window).
    shutil.copy(
        ROOT / "licenses" / "THIRD_PARTY_LICENSES.txt", into / "THIRD-PARTY-LICENSES.txt"
    )


def zip_folder(folder: Path, out: Path) -> None:
    """Zip `folder` into `out`, with the folder itself at the top."""
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for path in sorted(folder.rglob("*")):
            z.write(path, path.relative_to(folder.parent))
    print("wrote", out)


def debug_archive(files: list[Path], out: Path) -> None:
    """Zip the debug info files and folders in `files` (missing ones are
    noted and skipped)."""
    present = [f for f in files if f.exists()]
    for f in files:
        if not f.exists():
            print("no debug info at", f)
    if not present:
        return
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for f in present:
            items = sorted(f.rglob("*")) if f.is_dir() else [f]
            for item in items:
                z.write(item, item.relative_to(f.parent))
    print("wrote", out)


def msix_version(ver: str) -> str:
    """The Store's four-part version: 0.2.0 is 0.2.0.0 (the last part must
    be 0). A prerelease suffix is dropped.

    >>> msix_version("0.2.0")
    '0.2.0.0'
    >>> msix_version("1.4.2-rc.1")
    '1.4.2.0'
    """
    return ver.split("-")[0] + ".0"


def find_makeappx() -> str | None:
    """makeappx.exe from the newest Windows SDK installed."""
    found = shutil.which("makeappx")
    if found:
        return found
    base = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
    tools = sorted((base / "Windows Kits" / "10" / "bin").glob("10.*/x64/makeappx.exe"))
    return str(tools[-1]) if tools else None


def package_msix(ver: str, out: Path) -> None:
    """The Microsoft Store's MSIX, into `out`, from the program already built."""
    identity = json.loads((PACKAGING / "windows" / "msix" / "store.json").read_text(encoding="utf-8"))
    stage = fresh(out / "stage" / "msix")
    # Only the dashboard: raptaboard-cli is in the installer and zip.
    shutil.copy(built(build_dir(None) / "raptaboard.exe"), stage)
    add_docs(stage, ver)
    shutil.copytree(ICONS / "msix", stage / "Assets")
    manifest = (PACKAGING / "windows" / "msix" / "AppxManifest.xml").read_text(encoding="utf-8")
    for key, value in {
        "IDENTITY_NAME": identity["identity_name"],
        "PUBLISHER": identity["publisher"],
        "PUBLISHER_DISPLAY_NAME": identity["publisher_display_name"],
        "DISPLAY_NAME": identity["display_name"],
        "VERSION": msix_version(ver),
    }.items():
        manifest = manifest.replace("{" + key + "}", value)
    assert "{" not in manifest.split("-->", 1)[1], "a placeholder left in the manifest"
    (stage / "AppxManifest.xml").write_text(manifest, encoding="utf-8")
    makeappx = find_makeappx()
    if not makeappx:
        sys.exit("makeappx wasn't found; install the Windows SDK")
    msix = out / f"raptaboard-{ver}-store.msix"
    run(makeappx, "pack", "/o", "/d", stage, "/p", msix)
    print("wrote", msix)


def find_iscc() -> str | None:
    found = shutil.which("iscc")
    if found:
        return found
    for base in (os.environ.get("ProgramFiles(x86)"), os.environ.get("ProgramFiles")):
        if base:
            path = Path(base) / "Inno Setup 6" / "ISCC.exe"
            if path.exists():
                return str(path)
    return None


def package_windows(ver: str, out: Path, installer: bool) -> None:
    name = f"raptaboard-{ver}-windows-x64"
    release = build_dir(None)
    stage = fresh(out / "stage" / name)
    add_windows_programs(release, stage)
    add_docs(stage, ver)
    # The installer takes the files it lists, so the portable settings
    # folder only goes in the zip.
    if installer:
        iscc = find_iscc()
        if not iscc:
            sys.exit("Inno Setup's iscc wasn't found; install Inno Setup 6 or pass --no-installer")
        run(
            iscc,
            f"/DAppVersion={ver}",
            f"/DSourceDir={stage}",
            f"/DOutputDir={out}",
            PACKAGING / "windows" / "raptaboard.iss",
        )
    settings = stage / ".raptaboard"
    settings.mkdir()
    (settings / "settings.toml").write_text(PORTABLE_SETTINGS, encoding="utf-8")
    zip_folder(stage, out / f"{name}.zip")
    debug_archive([debug_file(release, p, ".pdb") for p in PROGRAMS], out / f"{name}-debug.zip")


def package_macos(ver: str, out: Path, targets: list[str | None]) -> None:
    if len(targets) > 1:
        arch = "universal"
    else:
        arch = {"arm64": "arm64", "x86_64": "x64"}.get(platform.machine(), platform.machine())
        if targets[0]:
            arch = "arm64" if targets[0].startswith("aarch64") else "x64"
    name = f"raptaboard-{ver}-macos-{arch}"
    stage = fresh(out / "stage" / name)
    app = stage / "raptaboard.app" / "Contents"
    (app / "MacOS").mkdir(parents=True)
    (app / "Resources").mkdir()
    # Both programs inside the app (Contents/MacOS), each one program for
    # every target given (lipo makes Apple Silicon and Intel into one).
    for program_name in PROGRAMS:
        binaries = [built(build_dir(t) / program_name) for t in targets]
        program = app / "MacOS" / program_name
        if len(binaries) > 1:
            run("lipo", "-create", "-output", program, *binaries)
        else:
            shutil.copy(binaries[0], program)
        program.chmod(0o755)
    plist = (PACKAGING / "macos" / "Info.plist").read_text(encoding="utf-8")
    (app / "Info.plist").write_text(plist.replace("{VERSION}", ver), encoding="utf-8")
    shutil.copy(ICONS / "raptaboard.icns", app / "Resources" / "raptaboard.icns")
    # An ad-hoc signature (no Apple account): Apple Silicon needs one to run
    # the program at all; Gatekeeper still asks on the first open. --deep
    # signs raptaboard-cli too.
    run("codesign", "--force", "--deep", "--sign", "-", stage / "raptaboard.app")
    add_docs(stage, ver)
    # ditto keeps the app bundle's permissions in the zip; no resource forks
    # or extended attributes, which would show as ._ files elsewhere.
    run(
        "ditto", "-c", "-k", "--norsrc", "--noextattr", "--keepParent",
        stage, out / f"{name}.zip",
    )
    debug_each(targets, out / f"{name}-debug.zip")


def debug_each(targets: list[str | None], out: Path) -> None:
    """The macOS debug info: each program's .dSYM for each target, in a
    folder named for the target when there are two (both have the same
    names)."""
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for t in targets:
            for program in PROGRAMS:
                dsym = debug_file(build_dir(t), program, ".dSYM")
                if not dsym.exists():
                    print("no debug info at", dsym)
                    continue
                top = Path(t) if t and len(targets) > 1 else Path()
                for item in sorted(dsym.rglob("*")):
                    z.write(item, top / item.relative_to(dsym.parent))
    print("wrote", out)


def package_linux(ver: str, out: Path) -> None:
    name = f"raptaboard-{ver}-linux-x64"
    release = build_dir(None)
    stage = fresh(out / "stage" / name)
    for program in PROGRAMS:
        shutil.copy(built(release / program), stage)
        (stage / program).chmod(0o755)
    shutil.copy(ICONS / "raptaboard.png", stage)
    shutil.copy(PACKAGING / "linux" / "raptaboard.desktop", stage)
    shutil.copy(PACKAGING / "linux" / "install.sh", stage)
    (stage / "install.sh").chmod(0o755)
    add_docs(stage, ver)
    tar = out / f"{name}.tar.gz"
    with tarfile.open(tar, "w:gz") as t:
        t.add(stage, arcname=name)
    print("wrote", tar)
    debug_archive([debug_file(release, p, ".dwp") for p in PROGRAMS], out / f"{name}-debug.zip")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--target", action="append", help="a Rust target to build (macOS: give two for a universal app)")
    parser.add_argument("--out", type=Path, default=ROOT / "dist", help="where the packages go (default dist/)")
    parser.add_argument("--skip-build", action="store_true", help="package what's already built")
    parser.add_argument("--no-installer", action="store_true", help="Windows: the zip only")
    parser.add_argument("--msix", action="store_true", help="Windows: also the Microsoft Store MSIX, in dist-store/")
    args = parser.parse_args()

    ver = version()
    targets: list[str | None] = list(args.target) if args.target else [None]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if not args.skip_build:
        build(targets)
    which = system()
    if which == "windows":
        package_windows(ver, out, not args.no_installer)
        if args.msix:
            store_out = (out.parent / "dist-store").resolve()
            store_out.mkdir(parents=True, exist_ok=True)
            package_msix(ver, store_out)
            shutil.rmtree(store_out / "stage", ignore_errors=True)
    elif which == "macos":
        package_macos(ver, out, targets)
    else:
        package_linux(ver, out)
    shutil.rmtree(out / "stage", ignore_errors=True)


if __name__ == "__main__":
    main()
