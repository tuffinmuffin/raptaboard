# CLAUDE.md

Guidance for Claude Code working in this repo. Work moves between Claude Code on a PC and claude.ai
project threads, so keep everything a new session needs in the repo.

## Project

raptaboard is an FRC driver dashboard in Rust for Team 3200 (Raptacon): Elastic feature parity,
Elastic layout compatibility, and WebAssembly widget plugins bound to NetworkTables tables.
The spec is `docs/spec.md` (also a Claude Doc: https://claude.ai/artifact/ExMFgLUirHFxwjPBW5wEdb).
Read its "Goal and decisions" and "Milestones" sections before starting work.

Decisions already made (don't reopen without asking the user): egui/eframe UI; our own NT4.1 client
(the published `nt4` crate, from `tuffinmuffin/nt4-rs`); plugins are WASM components only
(wasmtime + `wit/dashboard.wit`), no dynamic libraries, no scripting tier; Elastic layout JSON v1
must round-trip unchanged.

## Commands

```sh
cargo build --workspace
cargo test --workspace            # unit tests, integration tests and doctests
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p raptaboard -- crates/layout/tests/data/elastic_sample.json
cargo run -p raptaboard -- --address sim crates/layout/tests/data/elastic_sample.json  # robot sim
cargo run -p raptaboard -- --help
# Milestone 1 CPU test (opens a window, ~40 s; keep it visible):
cargo test -p raptaboard --release --test cpu_load -- --ignored --nocapture
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings` and tests on Linux, Windows and
macOS. Run the same locally before pushing.

## Layout

- NetworkTables: the `nt4` crate from crates.io (`nt4::Client`, typed subscribers, `nt4::proto` for
  ports and URLs). Fix protocol bugs upstream in `nt4-rs`, not here.
- `crates/layout`: Elastic v1 model. Unknown fields go in `extra` maps so files round-trip.
- `crates/plugin-host`: manifest parsing and matching; `path` has topic path helpers
  (`.type` tables). wasmtime hosting comes in milestone 4.
- `crates/app`: the eframe app. `ui.rs` holds the window, `connection.rs` the robot address modes,
  `cli.rs` the startup flags (`--team`, `--address`, `--port`; stand-ins until settings persist),
  `nt.rs` the live connection (`NtConnection`: a 1-thread tokio runtime that owns the `nt4::Client`
  and restarts it when the address changes; the UI only makes non-blocking calls), `tree.rs` the
  read-only NT tree (values only for topics in open folders, via `NtConnection::watch_values`),
  `struct_view.rs` struct values decoded with the server's `/.schema/` topics
  (`nt4::proto::structs::StructDatabase` + `DynamicStruct`), `widgets.rs` the grid widgets
  (Text Display, Boolean Box; other types draw as placeholders). Widget values come from a
  second subscription (`NtConnection::watch_widget_values`) at the open tab's shortest `period`.
  `tests/cpu_load.rs` is the ignored 30-widgets-at-50-Hz CPU test (prints % of one core and of
  the machine); its `other_dashboard` test runs the same load against Elastic for comparison
  (set `RAPTABOARD_CPU_OTHER=elastic_dashboard`, Elastic on localhost, port 5810 free).
  Next: run it on a Driver Station laptop, then milestone 2.
- `layouts/robot2027_sim.json`: a layout for trying widgets against the robot-2027 sim
  (`cargo run -p raptaboard -- --address sim layouts/robot2027_sim.json`). Its `/DashTest/`
  topics come from robot-2027's `utils/dashboard_test_topics.py` (sim only).
- `wit/dashboard.wit`: plugin ABI. Bump the package version and `HOST_API` in plugin-host together.
- `plugins/arm`: example plugin; its `plugin.toml` is a test fixture.

## Conventions

- Rust 2024 edition, MSRV in the workspace `Cargo.toml`. `unsafe` is forbidden workspace-wide.
- Readers include students new to programming: every public item gets a doc comment in plain
  language with units, and a short doctest where a student would call it.
- Mark tests written by Claude with a `// Tests written by Claude.` comment.
- NetworkTables is client/server: in code and comments say "server" for the NT side (the robot is
  only the default server; it can be the sim or another host), and keep "robot" for robot code itself.
- Match Elastic names exactly where robot code depends on them (`.type` strings, `/Elastic/` topics,
  layout keys).
- egui 0.37 API: implement `eframe::App::ui(&mut self, ui, frame)`; panels are `egui::Panel::top(..)`
  and `CentralPanel::default().show(ui, ..)`. Older examples online use `update` and
  `TopBottomPanel`.

## Git

- Work on a branch and open a PR into `main`; don't push to `main` directly.
- Keep `docs/spec.md` and this file up to date when a decision or milestone changes, so the next
  session (PC or web) can pick up from the repo alone.
