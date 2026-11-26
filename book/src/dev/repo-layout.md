# Repo layout

| Path | What it is |
| --- | --- |
| `crates/app` | The eframe/egui desktop app (binaries `raptaboard` and `raptaboard-cli`) |
| `crates/layout` | Layout model; reads and writes Elastic `version 1.0` JSON without losing fields |
| `crates/plugin-host` | Plugin manifests (`plugin.toml`), matching plugins to NT tables, and the plugin runtime |
| `crates/plugin-test` | `raptaboard-plugin-test`: test a plugin's `.wasm` from `cargo test` in the real runtime |
| `crates/win-console` | raptaboard's one unsafe call: a Windows window program joining its terminal |
| `wit/dashboard.wit` | The plugin interface: what a WebAssembly plugin imports and exports |
| `plugins/` | The plugin SDK (`raptaboard-plugin`) and the plugins raptaboard's own tests use, built for WebAssembly in their own workspace |
| `book/` | This book |
| `docs/spec.md` | The spec: Elastic feature list, requirements, plugin design, milestones |
| `packaging/`, `scripts/package.py` | Release packages: the Windows installer, macOS app and Linux install script |
| `media/icon/` | The app icon (`raptaboard.svg`; the other files are made from it) |

NetworkTables comes from the [`nt4`](https://crates.io/crates/nt4) crate: client, typed topics,
structs, time sync and reconnect.

Related repositories:

- [raptaboard-examples](https://github.com/tuffinmuffin/raptaboard-examples): example plugins,
  Number and Alerts.
- [raptaboard-doom](https://github.com/tuffinmuffin/raptaboard-doom): Doom on the 2026 field, a tab
  plugin.
- [raptaboard-content](https://github.com/tuffinmuffin/raptaboard-content): the official content
  packs and their signed catalogs.
