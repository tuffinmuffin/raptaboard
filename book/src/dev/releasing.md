# Releasing

A release is a version number, a changelog section, and a tag. GitHub Actions does the rest.

## Make a release

1. **Bump the version** in the workspace `Cargo.toml` (`[workspace.package] version`), and run
   `cargo build` so `Cargo.lock` follows.
2. **Write its changelog section** in `CHANGELOG.md`: a `## x.y.z` heading with what's in it for
   users, short, not every pull request. During development it's `## x.y.z (not released yet)`;
   take that off.
3. **Merge** both in a pull request.
4. **Tag `main`** and push the tag:

   ```sh
   git checkout main && git pull
   git tag v0.5.0
   git push origin v0.5.0
   ```

The **Release** workflow (`.github/workflows/release.yml`) then:

- builds the packages on Windows, macOS and Linux (`scripts/package.py`);
- checks the tag matches the version in `Cargo.toml`, and stops if it doesn't;
- writes the release notes from the changelog section and a table of downloads
  (`scripts/release_notes.py`), and a `SHA256SUMS.txt` of every file;
- publishes the GitHub Release, which becomes the README's download.

| File | What it is |
| --- | --- |
| `raptaboard-x.y.z-windows-x64-setup.exe` | Windows installer, per user, no admin needed |
| `raptaboard-x.y.z-windows-x64.zip` | Windows portable: unzip anywhere, keeps its settings beside it |
| `raptaboard-x.y.z-macos-universal.zip` | macOS app, Apple Silicon and Intel |
| `raptaboard-x.y.z-linux-x64.tar.gz` | Linux, with `install.sh` to add it to the menu |
| `…-debug.zip` | Debug info for decoding crash reports; not needed to run raptaboard |

Packages aren't code-signed, except through the Microsoft Store (below). The release notes say how
to get past each system's warning.

## Trying a release first

A tag with a suffix, such as `v0.5.0-rc.1`, publishes a **prerelease**: the same packages, not
marked as the latest, so the README's download skips it. Use one to try the packages or the release
process, then delete it:

```sh
gh release delete v0.5.0-rc.1 --cleanup-tag
```

You don't need a tag just to try the newest code: every push to `main` builds the packages too,
kept as workflow artifacts for 14 days (Actions → Release → the run → Artifacts). Pull requests
that change packaging files build them as well.

## The Microsoft Store

The Windows build also makes an **MSIX** for the Microsoft Store, in its own `store-msix` artifact on
the workflow run (never on the GitHub Release). The Store signs it. Uploading is done by hand in
Partner Center:

1. Download `store-msix` from the tag's Release workflow run.
2. In Partner Center, start a new submission for raptaboard and upload the `.msix` under Packages.
3. Partner Center asks why raptaboard needs the restricted capability **runFullTrust**. The answer
   that was accepted: raptaboard is a Win32 desktop app packaged with the Desktop Bridge and needs
   runFullTrust to run; it connects to the robot on the local network, opens and saves files the
   user picks, and keeps settings in `~/.raptaboard`; no drivers, services or elevation; open source
   on GitHub.
4. Update the listing's screenshots if they changed (`media/store/`, made by
   `record_store_screenshots`), and submit.

The Store package holds `raptaboard.exe` only. A second, hidden program in the package
(`raptaboard-cli.exe`) was refused as a "headless app", so Store users get the command line from
GitHub. The listing's privacy policy is `PRIVACY.md` on `main`: update it in the same pull request
as anything that makes raptaboard connect somewhere new or store something new.

## Content packs

The official content packs (sounds, plugins) are released from their own repository,
[raptaboard-content](https://github.com/tuffinmuffin/raptaboard-content), by pushing a tag `vN`
there. Its workflow builds the packs and a signed catalog. raptaboard picks up the newest catalog
by itself, so packs can be updated without a raptaboard release.
