# raptaboard

An FRC driver dashboard written in Rust, from FRC Team 3200 (Raptacon). It started from
[Elastic](https://github.com/Gold872/elastic-dashboard) and aims past it: everything Elastic does,
a richer set of widget settings, and **widget plugins**: small WebAssembly modules that load without rebuilding the dashboard and draw
a NetworkTables table, such as an arm subsystem.

Status: **0.1.0, the first release**: the basics are done (see [CHANGELOG.md](CHANGELOG.md)).
It opens and saves Elastic layouts, with tabs, a snap grid, List Layouts, undo and a lock for
driving. It connects to the robot over NetworkTables (by team number, the Driver Station,
SystemCore or the sim) and shows a live tree of every topic, WPILib structs included; drag a topic
onto the grid to make a widget. Widgets: Text Display, Boolean Box, Number Bar, Voltage View,
Radial Gauge, Battery, Graph, Match Time, Single and Multi Color View, Toggle Switch and Button,
Number Slider, Dropdown and Split Button Chooser, Enum Dropdown and Toggle, Field, Gyro, Swerve,
Chassis Speeds, 3D Pose, Mechanism2d, Struct, Alerts (with an alert history) and FMSInfo (the
robot's state and the match, with a sound for each change of state); other Elastic
types show as placeholders for now. Plugins come next.

## Install

Download the file for your computer from the
[latest release](https://github.com/tuffinmuffin/raptaboard/releases/latest):

| System | File |
| --- | --- |
| Windows, installed (Start menu, uninstaller; no admin needed) | `raptaboard-<version>-windows-x64-setup.exe` |
| Windows, portable (unzip and run; settings stay in its folder) | `raptaboard-<version>-windows-x64.zip` |
| macOS 11+, Apple Silicon and Intel | `raptaboard-<version>-macos-universal.zip` |
| Linux x64 (`./install.sh` adds it to your menu, for you only) | `raptaboard-<version>-linux-x64.tar.gz` |

The builds aren't signed yet, so the first start asks first. **Windows:** at "Windows protected
your PC", click "More info", then "Run anyway". **macOS:** right-click raptaboard.app, choose
Open, then Open again. Each package has a README.txt with first steps.

![raptaboard connected to the robot sim, showing Text Display and Boolean Box widgets](docs/images/m1-widgets.png)

### Field

The Field widget draws the robot on the field from a Pose2d (or a Field2d table), with layers from
other topics: other robots, paths, the AprilTags the cameras see, and game pieces on the field or
seen from the robot. Cameras' views and parts such as an intake ride on the robot. The clip below is
the app itself, rendered headless against a scripted robot (Robot-2026's cameras and size, on the
2026 field); remake it with
`cargo test -p raptaboard --lib record_ -- --ignored` (it needs a GPU).

![The Field widget: a robot orbiting the 2026 field with its cameras' views, other robots, its path, balls and the AprilTags it sees](docs/images/field.gif)

## Building from source

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
| `packaging/`, `scripts/package.py` | Release packages: the Windows installer, macOS app and Linux install script |
| `media/icon/` | The app icon (`raptaboard.svg`; the other files are made from it) |

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

MIT, see `LICENSE`. The app icon (`media/icon/`) is Raptacon's raptor mark, from Raptacon's
[brand kit](https://github.com/Raptacon/brand-kit), in a gear; our own built-in images (`images/`) are CC0.
raptaboard includes sounds, images and fields from others (credits in `media/audio/README.md`,
`images/README.md` and `fields/README.md`; the built-in 2026 field image is WPILib's,
`fields/LICENSE-WPILib.md`) and is built from open-source Rust crates (`licenses/`). The app lists
them all under ⚙ Settings, "About and licenses", and release packages include the crates' license
texts (`THIRD-PARTY-LICENSES.txt`). After changing dependencies, run
`python3 scripts/crate_licenses.py`.
