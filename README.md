# raptaboard

An FRC driver dashboard written in Rust, from FRC Team 3200 (Raptacon). It started from
[Elastic](https://github.com/Gold872/elastic-dashboard) and aims past it: everything Elastic does,
a richer set of widget settings, and **widget plugins**: small WebAssembly modules that load without rebuilding the dashboard and draw
a NetworkTables table, such as an arm subsystem.

Status: **milestone 3 in progress.** The app opens an Elastic layout and shows its tabs and widgets
on the grid, where tabs can be added, renamed and reordered and widgets moved and resized (the
"🔓 Unlocked" button locks them), and layouts are opened and saved with Open…, Save and Save as…
(Ctrl+O, Ctrl+S, Ctrl+Shift+S; Cmd on macOS). Drag a topic from the NetworkTables tree onto the
grid to make a widget for it. It connects to the robot over NetworkTables (status and topic count in the settings
bar) with a NetworkTables tree of live values, including WPILib structs such as Pose2d. Widgets so far:
Text Display, Boolean Box, Large Text Display, Number Bar, Voltage View, Radial Gauge, Match Time,
Single and Multi Color View, Graph, Toggle Switch and Button, Number Slider, Dropdown and Split
Button Chooser, Enum Dropdown and Toggle, Alerts (with an alert history), and Field; other Elastic
types show as placeholders for now.

![raptaboard connected to the robot sim, showing Text Display and Boolean Box widgets](docs/images/m1-widgets.png)

### Field

The Field widget draws the robot on the field from a Pose2d (or a Field2d table), with layers from
other topics: other robots, paths, the AprilTags the cameras see, and game pieces on the field or
seen from the robot. Cameras' views and parts such as an intake ride on the robot. The clip below is
the app itself, rendered headless against a scripted robot (Robot-2026's cameras and size, on the
2026 field); remake it with
`cargo test -p raptaboard --lib record_ -- --ignored` (it needs a GPU).

![The Field widget: a robot orbiting the 2026 field with its cameras' views, other robots, its path, balls and the AprilTags it sees](docs/images/field.gif)

## Quick start

Install Rust with [rustup](https://rustup.rs) (stable, 1.95 or newer), then:

```sh
cargo run -p raptaboard -- crates/layout/tests/data/elastic_sample.json
cargo test --workspace
```

To connect to robot code running in simulation on this computer, add `--address sim`
(`layouts/robot2027_sim.json` is a layout made for the robot-2027 sim). Use
`--team N` for another team's robot, `--port N` for a server on another port, `--hide-tree` to start
with the NT tree hidden (the "◀ Tree" button in the settings bar toggles it), and `--help` for every option.

On Linux you also need the windowing libraries:
`sudo apt install libxkbcommon-x11-0 libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev`.

## Repo layout

| Path | What it is |
| --- | --- |
| `crates/layout` | Layout model; reads and writes Elastic `version 1.0` JSON without losing fields |
| `crates/plugin-host` | Plugin manifests (`plugin.toml`), matching plugins to NT tables, topic path helpers |
| `crates/app` | The eframe/egui desktop app (binary `raptaboard`) |
| `wit/dashboard.wit` | The plugin interface: what a WASM widget imports and exports |
| `plugins/arm` | Example plugin manifest for an arm table tagged `.type = "Arm"` |
| `docs/spec.md` | The spec: Elastic feature list, requirements, plugin design, milestones |

NetworkTables comes from the [`nt4`](https://crates.io/crates/nt4) crate: client, typed topics,
structs, time sync and reconnect.

## Writing a plugin (preview)

A plugin is a folder with `plugin.toml` and `widget.wasm`. The manifest says which tables it can
draw:

```toml
name = "arm"
version = "0.1.0"
api = "0.1"

[match]
type = "Arm"        # the table's .type topic
```

The WASM module implements the `widget` world from `wit/dashboard.wit`: it reads values with
`get("angle_rad")` and returns shapes to draw. Running plugins arrives in milestone 4.

## Contributing

Work happens on branches with pull requests into `main`. CI runs `cargo fmt --check`,
`cargo clippy -D warnings` and `cargo test` on Linux, Windows and macOS. Read `CLAUDE.md` if you are
using Claude Code; it has the same rules.

## License

MIT, see `LICENSE`. raptaboard includes sounds, images and fields from others (credits in
`media/audio/README.md`, `images/README.md` and `fields/README.md`) and is built from open-source
Rust crates (`licenses/`). The app lists them all under ⚙ Settings, "About and licenses". After
changing dependencies, run `python3 scripts/crate_licenses.py`.
