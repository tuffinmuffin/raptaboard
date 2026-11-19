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
(wasmtime + `wit/dashboard.wit`), no dynamic libraries, no scripting tier.

Elastic is the starting point, not the limit (decided 2026-11-18): the goal is a richer dashboard
that fixes Elastic's shortcomings. raptaboard doesn't have to keep Elastic's file format: teams aren't
expected to switch back and forth, so opening Elastic layouts and saving files Elastic can read is
nice to have, kept when it's cheap, never a reason to block a feature. Layout files are Elastic-shaped
today (`crates/layout`), and that can change; if raptaboard gets its own format, an import tool
converts Elastic layouts into it (one way). raptaboard-only widget settings, widgets and behavior are
welcome. Keep Elastic's names (`.type` strings, `/Elastic/` topics) where
robot code depends on them.

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
macOS. Run the same locally before pushing. Its `coverage` job (cargo-llvm-cov, as in nt4-rs) posts a
coverage report on each PR, compared with the base branch, via `scripts/coverage_summary.py`
(one comment, edited each push; its first line links the commit measured and gives the time).
Locally: `cargo llvm-cov --workspace --ignore-filename-regex '(tests|examples|benches)/' --html`.

## Layout

- NetworkTables: the `nt4` crate from crates.io (`nt4::Client`, typed subscribers, `nt4::proto` for
  ports and URLs). Fix protocol bugs upstream in `nt4-rs`, not here.
- `crates/layout`: Elastic v1 model. Unknown fields go in `extra` maps so files round-trip.
  `ElasticLayout::sanitize` fixes values the app can't show (run on every opened file) and leaves
  valid layouts unchanged (`tests/sanitize.rs`).
- `crates/plugin-host`: manifest parsing and matching; `path` has topic path helpers
  (`.type` tables). wasmtime hosting comes in milestone 4.
- `crates/app`: the eframe app. `ui.rs` holds the window, `connection.rs` the robot address modes,
  `cli.rs` the startup flags (`clap` derive, `cli::Args`: `--team`, `--address`, `--port`,
  `--hide-tree`; this run only),
  `settings.rs` settings kept between runs (command line, then `./.raptaboard/settings.toml`, then
  `~/.raptaboard/settings.toml`, made on the first run, then defaults; changes saved to the local file
  if there is one, else the home file, by `settings::Tracker`; `ui.rs` only says what the current
  values are),
  `nt.rs` the live connection (`NtConnection`: a 1-thread tokio runtime that owns the `nt4::Client`
  and restarts it when the address changes; the UI only makes non-blocking calls), `tree.rs` the (row picking in `TreeSelection`, drag payload `DraggedTopics`)
  read-only NT tree (values only for topics in open folders, via `NtConnection::watch_values`),
  `struct_view.rs` struct values decoded with the server's `/.schema/` topics
  (`nt4::proto::structs::StructDatabase` + `DynamicStruct`), `widgets/` the grid widgets,
  one file per type implementing `widgets::WidgetType` and listed in `widgets::TYPES` (Text Display,
  Boolean Box, Large Text Display, Number Bar, Voltage View, Radial Gauge, Match Time, Single and
  Multi Color View, Graph (`egui_plot`; widgets needing a full `Ui` override `WidgetType::show`); other types draw as placeholders), widget files in `widgets/builtin/`, `widgets/list_layout.rs` (List Layout drawing
  and its settings dialog), helpers in `widgets/shared/`:
  `card.rs`, `format.rs` (number and
  enum text), `props.rs` (period, font size, colors) and `settings.rs` (the settings dialog's shared
  rows; the dialog window is in `ui.rs`); setting keys are constants in the file that owns them,
  listed in `docs/spec.md` under "raptaboard widget settings" (keep that table in step); adding a
  widget is a new file in `builtin/` (listed in `builtin/mod.rs`) plus one line in `TYPES`, `layout_file.rs` opening and saving layout files (atomic
  writes, Ctrl+O/S/Shift+S), `grid.rs` the snap-grid rules
  (snapping, overlap) and `tabs.rs` tab actions and their shortcuts, `edit.rs` undo/redo (`History`, which
  notices any layout change once a frame) and the widget editing shortcuts; the drag handling and lock live
  in `ui.rs`. Widget values come from a
  second set of subscriptions (`NtConnection::watch_widget_values`), one per `period` on the open
  tab, and `traces.rs` (`Traces`) records the values over time that graphs show (every value, from the NT client's event stream, fed by
  `ui.rs::record_traces`; a growable ring buffer per series), `sampling.rs` (`Sampler`) has each widget take a new value only once per its period, so
  fast redraws (mouse moves) or the tree watching the same topic don't make it jump ahead.
  `tests/cpu_load.rs` is the ignored 30-widgets-at-50-Hz CPU test (runs with the tree shown and
  hidden; prints % of one core and of the machine); its `other_dashboard` test runs the same load against Elastic for comparison
  (set `RAPTABOARD_CPU_OTHER=elastic_dashboard`, Elastic on localhost, port 5810 free).
  Results are in `docs/spec.md`; VMs without hardware DX12 render in software, so their numbers
  don't count. Milestone 2 has tabs, the snap grid, the lock and layout open/save
  (`layout_file.rs`, native dialogs via `rfd`); topics and struct fields drag from the tree onto the grid (`tree::DraggedTopic`,
  `widgets::for_topic`; struct fields use Elastic's `struct_meta`, read with
  `struct_view::field_value`); widgets can be selected, duplicated, deleted and edited (settings dialog),
  with undo; next is List Layout editing.
- `layouts/robot2027_sim.json`: a layout for trying widgets against the robot-2027 sim
  (`cargo run -p raptaboard -- --address sim layouts/robot2027_sim.json`). Its `/DashTest/`
  topics come from robot-2027's `utils/dashboard_test_topics.py` (sim only).
  It also keeps widgets for things that don't exist, on purpose, to test that handling: "Never
  published" and "Missing text" (topics never sent), "Missing field" (a field `OrbitPose` doesn't
  have), and the "Missing child" row in Module CAN. Keep them when editing the layout.
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
