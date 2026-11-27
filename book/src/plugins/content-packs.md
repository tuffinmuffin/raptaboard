# Content packs

A **content pack** is how extra things reach raptaboard without a new release: sounds, widget
plugins, tab plugins and their data. raptaboard lists packs in ⚙ Settings → Content packs, shows
each one's license and what its plugins may do, and downloads one only when the user chooses it.

The official packs are in [raptaboard-content](https://github.com/tuffinmuffin/raptaboard-content):

| Pack | What it holds |
| --- | --- |
| `fan-clips` | Sound clips for the robot state sounds |
| `example-widgets` | The [example plugins](tutorial.md), Number and Alerts |
| `doom` | Doom on the 2026 field: a [tab plugin](tab-plugins.md) and its game data |

## A pack

A pack is a folder: a `pack.toml` and its files, with no subfolders.

```toml
id = "example-widgets"           # the folder's name
name = "Example widget plugins"  # what raptaboard shows
version = "0.2.0"                # raise it whenever a file changes, so raptaboard offers the update
kind = "plugins"                 # a label only
min_app = "0.4.0"                # the oldest raptaboard it works with (0.4.0 for plugins)
summary = "Number and Alerts widgets built as plugins: examples for writing your own."
license = "MIT"                  # one line
credits = """
The full text raptaboard shows before installing: who made it, where it's from, its license.
"""
suggested = false                # true: ticked in the list offered on the first start

[[plugin]]
file = "number.wasm"             # with its manifest beside it, named for it: number.toml

[[plugin]]
file = "alerts.wasm"
```

What it holds is its sections, mixed as needed:

| Section | Holds |
| --- | --- |
| `[[sound]]` | A sound: the `name` raptaboard shows and its `file` (MP3 or WAV). A built-in sound's name, such as `fire 1`, replaces that sound while the pack is installed. |
| `[[plugin]]` | A plugin: its `.wasm` as `file`, and its `plugin.toml` beside it renamed for it (`alerts.wasm` and `alerts.toml`). |
| `files` | Other files its plugins read, such as a game's data: a plugin sees its pack's files in `/pack`. |
| `[[image]]`, `[[field]]`, `[[layout]]` | Accepted, for what comes next; raptaboard doesn't use them yet. |

## Taking files from a release

A plugin is usually built in its own repository, so a pack can take files from that repository's
GitHub release instead of holding copies:

```toml
[[source]]
repo = "tuffinmuffin/raptaboard-examples"
tag = "v0.1.0"
files = ["number.wasm", "number.toml", "alerts.wasm", "alerts.toml"]
```

Building the catalog downloads each file, checks it against that release's `SHA256SUMS`, and lists
its address in the catalog, so raptaboard downloads it from there. To update a plugin, change the
`tag` and raise the pack's `version`. A plugin's repository should publish its `.wasm`, its manifest
(renamed for it) and `SHA256SUMS` on each release; raptaboard-examples' workflow is one to copy.

## Trying a pack

Install a pack folder straight away, without a catalog, with the command-line tool:

```sh
raptaboard-cli packs install --package path/to/my-pack --unsigned
```

`--unsigned` is needed because the pack isn't signed by raptaboard. The same tool lists, updates and
removes packs (`raptaboard-cli packs --help`). The window picks up a pack installed this way at once,
plugins too.

## Publishing in the official packs

Open a pull request on raptaboard-content with the pack's folder. Its CI checks every pack
(`python3 scripts/build_catalog.py --check`): the files are there, each plugin's manifest reads and
matches what the catalog will say it may do, and sources match their checksums.

When it's merged, a maintainer tags a release (`v6`, or `v6-pre.1` for a pre-release to try first).
The Release workflow publishes every file and `catalog.json`, which lists each pack's files, sizes
and checksums, and signs the catalog. raptaboard reads the newest catalog, checks its signature
against the key it's built with and each file against its checksum, and offers the new or updated
packs. Pre-release catalogs are only read when the **Pre-releases** box in the Content window is
ticked.

## Your own source

A team can host packs of its own: build a catalog in the same form and publish it at an HTTPS
address, then add that address in ⚙ Settings → Content packs → **Add source**. Other sources aren't
signed, so raptaboard says to add only ones you trust; every file is still checked against its
catalog's checksum.

## What raptaboard checks before installing

For each plugin in a pack, the Content window lists what it may do, from its `plugin.toml`:

- send values to the robot (`publish`), play sounds (`sound`), keep running while its tab is closed
  (`background`), save its settings on this computer (`storage`);
- what else it needs: a tab it fills, holding the mouse, more memory, more time a call, or more saved
  space (`[limits]`).

raptaboard then holds the plugin to exactly that: a plugin that asks for nothing can only draw.
