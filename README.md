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
| `plugins/` | The plugin SDK (`raptaboard-plugin`) and the plugins raptaboard's own tests use, built for WebAssembly in their own workspace |
| `docs/spec.md` | The spec: Elastic feature list, requirements, plugin design, milestones |
| `packaging/`, `scripts/package.py` | Release packages: the Windows installer, macOS app and Linux install script |
| `media/icon/` | The app icon (`raptaboard.svg`; the other files are made from it) |

NetworkTables comes from the [`nt4`](https://crates.io/crates/nt4) crate: client, typed topics,
structs, time sync and reconnect.

## Writing a plugin (preview)

A plugin is a folder with `plugin.toml` and `widget.wasm`. The manifest says what it can show,
one topic or a table:

```toml
name = "number"
version = "0.1.0"
api = "0.3"

[match]
topic_types = ["double", "float", "int"]   # or, for a table: type = "Arm" (its .type topic)

[properties]
decimal_places = { type = "number", default = -1 }
```

The WASM component implements the `widget` world from `wit/dashboard.wit`: the dashboard pushes it
the values it asked for, and it returns shapes to draw. In Rust, the SDK in `plugins/sdk` does the
plumbing: implement its `Widget` trait and pass the type to `widget!`. The example plugins are in
[raptaboard-examples](https://github.com/tuffinmuffin/raptaboard-examples): Number, a small one,
and Alerts, the built-in Alerts widget remade, a bigger one. The plugins in `plugins/` are the
ones raptaboard's own tests use. To try one, build it and start the dashboard with it:

```sh
rustup target add wasm32-wasip2
(cd plugins && cargo build --release --target wasm32-wasip2)
cargo run -p raptaboard -- --plugin plugins/topic-test --plugin plugins/table-test --address sim
```

Their widget types (`plugin:topic-test`, `plugin:table-test`) are then in "Add as" and the settings dialog's widget choice for
the topics and tables they show, after the built-in ones. Rebuild one while the dashboard runs and its
widgets restart with the new build.

To try one by hand without a robot, open it in the sandbox: the dashboard with only that plugin's
widget and a NetworkTables server of its own, serving the sample values from `[sandbox]` in its
`plugin.toml`. A side panel changes, adds and removes values and sets the widget's size, the
theme, the connection and the lock. It also shows the plugin's clickable regions outlined, how
long each kind of call takes and how much memory it uses against the limits, its log, and why it
stopped, with Restart; the widget restarts whenever the `.wasm` is rebuilt. Nothing
is saved. (`--address sim --topic <path>` shows the sim's values instead.)

```sh
cargo run -p raptaboard -- plugin dev plugins/table-test
```

To test one without the dashboard, `crates/plugin-test` (`raptaboard-plugin-test`) runs its built
`.wasm` in the dashboard's own plugin runtime from `cargo test`: set values, the size and the
connection, click its regions, and check the text it drew, its tab mark and the sounds it asked for.
raptaboard-examples' `number/tests/harness.rs` and `alerts/tests/harness.rs` are examples:

```rust
let mut widget = raptaboard_plugin_test::Harness::build(env!("CARGO_MANIFEST_DIR"));
widget.unit("", "m").set("", 1.5);
assert!(widget.render().has_text("1.5 m"));
```

A plugin can fill a tab instead of being a widget: with `tab = "Doom"` in its `plugin.toml`, an
empty tab named Doom shows it at the tab's size, and clicking it gives it the keyboard (the
dashboard's shortcuts are off until Shift+Esc or a click elsewhere). It can draw its own pixels (a game's
frame), stream sound, read its own folder at `/pack`, and hold the mouse while it has the
keyboard (`capture_mouse = true`: a game turning with it). `plugins/tab-test` is a small example:
`cargo run -p raptaboard -- plugin dev plugins/tab-test`.

To share a plugin, put it in a content pack: its `.wasm` and its `plugin.toml`, renamed for it
(`alerts.wasm` and `alerts.toml`), with a `[[plugin]]` naming the `.wasm` in the pack's `pack.toml`.
Try a pack folder with `raptaboard packs install --package <folder> --unsigned`. Installed packs'
plugins load at once, without a restart; before installing, raptaboard shows what each plugin may
do (send values, play sounds, run while its tab is closed). See `docs/spec.md`, "Plugin system",
for what's coming.

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
