# raptaboard spec

This is the repo copy of the spec. The working copy with diagrams and comments is the Claude Doc
"Rust FRC dashboard: Elastic parity and plugin spec" (https://claude.ai/artifact/ExMFgLUirHFxwjPBW5wEdb).
When the two differ, update both.

## Goal and decisions

raptaboard is an FRC driver dashboard in Rust for Team 3200 (Raptacon). It started from
[Elastic](https://github.com/Gold872/elastic-dashboard) and aims past it: what Elastic does, fixes for
its limits, and widget plugins that load at runtime and bind to a NetworkTables table. Opening
Elastic layouts is a convenience, not a requirement.

| Decision | Choice | Date |
| --- | --- | --- |
| UI framework | egui / eframe | 2026-11-14 |
| NT client | our own NT4.1 client, published as the [`nt4`](https://crates.io/crates/nt4) crate (`tuffinmuffin/nt4-rs`) | 2026-11-17 |
| Plugins | WebAssembly components only, run by wasmtime, interface in `wit/dashboard.wit` | 2026-11-14 |
| Dynamic-library plugins | rejected: a crash would take down the dashboard during a match | 2026-11-14 |
| Layout files | ~~read and write Elastic `version 1.0` JSON unchanged~~ (2026-11-14); replaced 2026-11-18: no need to keep Elastic's file format. Teams aren't expected to switch back and forth; working with Elastic files is nice to have when it's cheap. Files are Elastic-shaped today and may become raptaboard's own format; if they do, an import tool converts Elastic layouts into it (one way) | 2026-11-18 |
| Elastic's role | the starting point, not the limit: add features beyond it; keep Elastic's names where robot code depends on them (`.type`, `/Elastic/` topics) | 2026-11-18 |
| Repo | `tuffinmuffin/raptaboard` | 2026-11-14 |

Open: should the robot serve plugins from port 5800 along with layouts? Stream Deck hooks are a later interest.

## Part 1: what Elastic does (source: main at 7c548c1, 2027.0.0-alpha10)

### Connection

- `ws://<host>:<port>/nt/Elastic`, subprotocol `v4.1.networktables.first.wpi.edu`, falling back to
  `networktables.first.wpi.edu` (v4.0). A second websocket sends RTT time pings.
- Port: robot code 5810, SystemCore internal 6810.
- Address modes: Driver Station (asks the DS at `ws://<host>:6768/ipws`), team number (10.TE.AM.2),
  SystemCore mDNS (`robot.local`), SystemCore WiFi (`172.30.0.1`), localhost, custom.
- Subscriptions are shared between widgets on the same topic. Default period 0.06 s, graphs 0.033 s.
  The open tab gets one subscription per period, and each widget shows a new value once per its
  period on a steady beat, however often the window redraws.

### Structs

Generic decoding of any struct from `/.schema/struct:<Name>`. A widget can bind to one field of a
struct topic (saved as `struct_meta`). Hand-written decoders only for Pose2d and SwerveModuleState.

### Layout, tabs, grid

Named tabs, each a snap grid (default cell 128 px). Widgets and List Layouts (vertical stack) sit on
the grid. Layout lock, auto-resize above the Driver Station. Hotkeys: Ctrl+O import, Ctrl+S save,
Ctrl+Shift+S export, Ctrl+D download from robot, Ctrl+1..9 tab, Ctrl+Tab / Ctrl+Shift+Tab cycle,
Ctrl+Left/Right move tab, Ctrl+T new tab, Ctrl+W close tab, Ctrl+, settings, Ctrl+K robot,
Ctrl+Shift+K sim.

### How a topic becomes a widget

1. Table with a `.type` child: build the widget registered under that string or alias.
2. Table without `.type`: Camera Stream if it has `mode`, `modes`, `source`, `streams` and
   `description` or `connected`; YAGSL Swerve Drive if it has YAGSL's keys.
3. Single topic: boolean is Boolean Box; other numbers and strings are Text Display.
4. Otherwise a List Layout of the children.

"Show As" options: boolean (Boolean Box, Toggle Switch, Toggle Button); number (Number Bar, Number
Slider, Voltage View, Radial Gauge, Graph, Match Time); string (Single Color View), string[] (Multi
Color View); every viewable type also gets Text Display and Large Text Display.

### Widget catalog (parity checklist)

| Widget | `.type` / aliases | Binds | Writes |
| --- | --- | --- | --- |
| Boolean Box | Boolean Box | boolean | no |
| Toggle Switch | Toggle Switch | boolean | yes |
| Toggle Button | Toggle Button | boolean | yes |
| Text Display | Text Display / Text View | number, string, arrays | yes |
| Large Text Display | Large Text Display | number, string | no |
| Number Bar | Number Bar | number | no |
| Number Slider | Number Slider | number | yes |
| Voltage View | Voltage View | number | no |
| Radial Gauge | Radial Gauge / Simple Dial | number | no |
| Graph | Graph | number | no |
| Match Time | Match Time | number | no |
| Single Color View | Single Color View | string | no |
| Multi Color View | Multi Color View | string[] | no |
| Dropdown Chooser | Dropdown Chooser / Selectable, ComboBox Chooser, String Chooser | options, default, selected/tune, selected/value | yes |
| Split Button Chooser | Split Button Chooser | same | yes |
| Alerts | Alerts | errors, warnings, infos | no |
| Field | Field / Field2d | Pose2d topics | no |
| Command | Command | running, name | yes |
| Scheduler | Scheduler | Names, Ids, Cancel | yes |
| Subsystem | Subsystem | .default, .command | no |
| PIDController | PIDController / PID Controller | p, i, d, setpoint | yes |
| ProfiledPIDController | ProfiledPIDController | p, i, d, goal | yes |
| SwerveDrive | SwerveDrive | per-corner Angle and Velocity, robot angle | no |
| YAGSL Swerve Drive | YAGSL Swerve Drive | YAGSL table | no |
| DifferentialDrive | DifferentialDrive / Differential Drivebase | Left/Right Motor Speed | yes |
| Gyro | Gyro | Value | no |
| Accelerometer | Accelerometer | Value | no |
| 3-Axis Accelerometer | 3-Axis Accelerometer / 3AxisAccelerometer | X, Y, Z | no |
| Encoder | Encoder / Quadrature Encoder | Distance, Speed | no |
| Ultrasonic | Ultrasonic | Value | no |
| Motor Controller | Motor Controller / Nidec Brushless | Value | yes |
| Relay | Relay | Value | yes |
| PowerDistribution | PowerDistribution / PDP | Chan0..23, Voltage, TotalCurrent | no |
| RobotPreferences | RobotPreferences | /Preferences | yes |
| DSInfo | DSInfo / DriverStation, FMSInfo | ControlWord, MatchState/* | no |
| Camera Stream | Camera Stream | CameraPublisher table (MJPEG, WHEP) | no |

Not in Elastic: Mechanism2d, any display of the 2027 `unit` topic property.

### ElasticLib hooks

- `/Elastic/RobotNotifications`: JSON string with `title`, `description`, `level` (INFO, WARNING,
  ERROR), `displayTime` ms (default 3000), `width` (350), `height`. Skip one older than 3 s on connect.
- `/Elastic/SelectedTab`: tab name or index.

### Layout JSON (v1)

```json
{"version": 1.0, "grid_size": 128, "tabs": [{"name": "Teleop", "grid_layout": {
  "layouts": [{"title": "...", "x": 0, "y": 0, "width": 256, "height": 384, "type": "List Layout",
               "properties": {"label_position": "TOP"}, "children": [ ...widgets... ]}],
  "containers": [{"title": "...", "x": 256, "y": 0, "width": 512, "height": 256, "type": "Field",
                  "properties": {"topic": "/Drive/Field", "period": 0.06}}]}}]}
```

Pixels at the saved grid size. `properties` always has `topic` and `period`; single-topic widgets add
`data_type` and `struct_meta`. Unknown types load as empty placeholders.

### raptaboard widget settings

raptaboard adds its own keys to a widget's `properties`, next to Elastic's. Elastic ignores keys it
doesn't know, so these files still open there (without these settings). A key at its default is
left out of the file. Each key is a constant in the file that owns it under
`crates/app/src/widgets/` (shared ones in `shared/props.rs` and `shared/format.rs`, the rest in the
widget's own file in `builtin/`); readers there clamp out-of-range values and ignore wrong types.

| Key | Defined in | Widgets | Value | Default |
| --- | --- | --- | --- | --- |
| `period` (Elastic) | `shared/props.rs` | all | update period, seconds (0.01–5) | 0.06 |
| `font_size` | `shared/props.rs` | Text Display; Boolean Box text | points, 8–96 | 16 |
| `decimal_places` | `shared/format.rs` | Text Display (double, float) | fixed places, 0–10 | up to 4, trailing zeros cut |
| `int_format` | `shared/format.rs` | Text Display (int, enum numbers) | `decimal`, `hex` (`0x1A`), `binary` (`0b11010`) | `decimal` |
| `int_digits` | `shared/format.rs` | Text Display (int, enum numbers) | at least this many digits, zero-padded, 1–64 | no padding |
| `enum_display` | `shared/format.rs` | Text Display (enum struct field) | `name`, `number`, `both` | `name` |
| `true_color`, `false_color` (Elastic) | `builtin/boolean_box.rs` | Boolean Box | ARGB number | green, red |
| `show_text` | `builtin/boolean_box.rs` | Boolean Box | write the text below over the color | false |
| `true_text`, `false_text` | `builtin/boolean_box.rs` | Boolean Box | text when true / false; empty for none | none |

Adding a setting: a key constant and a reader in the file that owns it, its row in that widget's
`WidgetType::settings` (shared rows: `props::text_size_row`, `props::color_row`, `format::rows`),
the key in the widget's `setting_keys` so "Reset settings" clears it, and a row here.

### Widget code

Each widget type is one file in `crates/app/src/widgets/builtin/` implementing the `WidgetType` trait
(`names` it's saved under, which dropped topics it `accepts`, `draw_body`, its `settings` rows,
its `setting_keys`), listed in `widgets::TYPES`. The grid and the settings dialog only use that
trait. The trait's default `draw` puts `draw_body` inside the standard card (`shared/card.rs`: rounded
box, title on top); a widget that needs another frame or none (a camera view edge to edge)
overrides `draw`, using `shared/card.rs`'s pieces as it likes. `shared/settings.rs` has the dialog's shared rows (title,
period) and "Reset settings". Milestone 4 plugin widgets implement the same trait through an
adapter: WIT `render` shapes drawn with egui, settings rows from `plugin.toml`.

### Remote layouts

`GET http://<robot>:5800/?format=json` lists `files`; then `GET http://<robot>:5800/<name>.json`.
Merge into or overwrite the open layout.

## Part 2: raptaboard requirements

Parity: everything above, with the same `.type` strings, hotkeys and robot hooks, so robot code
written for Elastic works unchanged. Windows matters most (DS laptop); macOS and Linux too.

Beyond Elastic: plugins, Mechanism2d, `unit` display and conversion, Tunables-aware editing (write
`tune`, show `value`, mark when they differ), native layout v2 with plugin versions, and a target of
under 5% CPU on a DS laptop with 30 widgets at 50 Hz.

## WPILib 2027 and robot-2027

- `/Telemetry/` (Telemetry API) and `/Tunables/` (robust tunables are `<name>/tune` and
  `<name>/value`). Tables set a `.type` string; units travel as the `unit` topic property.
- robot-2027 today publishes `/Drive/Pose` (`struct:Pose2d`), `/Drive/MeasuredStates` and
  `/Drive/SetpointStates` (`SwerveModuleVelocity[]`), `/Drive/*Speeds` (`ChassisVelocities`),
  `/Drive/Module/<corner>/<field>` and `/Drive/Gyro/<field>` (inputs dataclasses, unit in the name),
  `/Drive/Health/allModulesHealthy`, `/FrameTiming/*`, `/inputs/actions/...`, `/robot/name`.
- Asks for the robot side: a `.type` on each subsystem table, and units as the `unit` property once
  we move to `Telemetry.log`.

## Plugin system

A plugin is a folder with `plugin.toml` and `widget.wasm` (a WASM component exporting the `widget`
world in `wit/dashboard.wit`).

1. Manifest: name, version, host API version, match rules, size, properties schema, permissions.
2. Match rules: `type` (the table's `.type`), `path` (glob), `has` (required child keys). All given
   rules must pass; a `.type` match is most specific. Implemented in `crates/plugin-host`.
3. Binding: the plugin reads keys relative to the table it was dropped on.
4. Render: called when a bound value changes; returns shapes (line, polyline, arc, rect, circle,
   text, image) in widget pixels. Only the host draws.
5. Input: click, drag, key go to `on-input`. Publishing needs `publish = true` and stays inside the
   bound table.
6. Properties: the schema becomes the edit dialog and is saved in the layout.
7. Limits: wasmtime fuel and memory caps; a trapped plugin shows a "plugin stopped" tile and the rest
   of the dashboard keeps running.

Example: `plugins/arm/plugin.toml` matches `.type = "Arm"` and draws `angle_rad`, `setpoint_rad`,
`extension_m`, `at_goal`. The planned Rust source is in the Claude Doc.

## Milestones

1. Spike: egui window, NT4.1 client connected to the robot-2027 sim (via the `nt4` crate), NT
   tree, Text Display and Boolean Box, 30-widget CPU test. **Started**: workspace, Elastic layout
   import/export, plugin manifests, WIT file, layout viewer window, CI, live NT connection with
   status and topic count, read-only NT tree with live values for open folders and generic struct decoding from
   `/.schema/`, Text Display and Boolean Box widgets (read-only) with List Layout children, and the CPU test
   (`crates/app/tests/cpu_load.rs`). First results, 2026-11-17 on an 18-core Mac, 30 widgets at
   50 Hz with 280 topics on the server: raptaboard 9 to 17% of one core (0.5 to 0.95% of the
   machine); Elastic 2027.0.0-alpha10 on the same load and layout 36.5 to 37.1% of one core (2.0%),
   about 2 to 4 times as much. Hiding raptaboard's NT tree saves about 1.3 points of one core
   (17.0 to 15.7%); most of the rest is redrawing the window 50 times a second. Windows, 2026-11-17 on a
   20-thread i7-13800H laptop (not a DS laptop), same test: 15.6% of one core with the tree shown and 15.1%
   hidden (0.78% and 0.75% of the machine); Elastic 2027.0.0-alpha10 61.5% of one core (3.1%), about 4
   times as much. A Windows 11 ARM VM under Parallels drew with wgpu's software fallback (no
   hardware DX12) and used about 330% of one core, so VM numbers measure the software renderer, not
   raptaboard. Milestone 1 is done; the OpenGL (glow) fallback for laptops without DX12 is an option
   for milestone 2 if a team laptop needs it.
2. Grid and layouts: tabs, snap grid, drag from tree, List Layout, lock, layout save, settings,
   hotkeys. **Started**: tabs (add, close with a confirmation if it has widgets, rename, reorder;
   Ctrl+1..9 and Ctrl+Left/Right switch tabs; unlike Elastic, adding, closing and moving tabs have
   no shortcuts, since they're rare and hard to remember), moving and resizing
   widgets and List Layouts on the snap grid (no overlaps; a widget's top-left cell must show, but
   the rest may run off the right or bottom of the window, up to the 100-cell limit, so a big
   widget can show just its first row or column), and the layout lock. With no layout
   file the app starts with Elastic's empty "Teleoperated" and "Autonomous" tabs. Layout open and
   save: Open…, Save, Save as… (Ctrl+O, Ctrl+S, Ctrl+Shift+S) with native file dialogs, an
   unsaved-changes dot and window title, and a "save changes?" question before opening another
   layout or quitting. Saves write a temporary file and rename it, so a failed save keeps the old
   file. Opened layouts are checked: a grid size outside 16 to 1024, no tabs, sizes of zero or less,
   negative positions, widgets past 100 cells and overlapping widgets (moved to the nearest free spot) are fixed and listed ("fixed N problems"), and
   files over 10 MB are refused; normal Elastic layouts come through unchanged. Drag from tree:
   dragging a topic onto the grid makes a one-cell widget there (Boolean Box for booleans, Text
   Display for the rest; saved with `topic`, `period` and `data_type` like Elastic). Fields inside a
   struct topic (Pose2d → translation → x) drag too: the widget keeps the struct topic and saves
   Elastic's `struct_meta` (`path`, `schema_name`, `type`); fields inside struct arrays don't, as in
   Elastic. Whole structs wait for their widgets. Dragging folders
   (tables with `.type`) waits for the widgets that draw tables in milestone 3. Editing widgets
   (unlocked): click to select (outlined), right-click for Duplicate and Delete; Delete or
   Backspace deletes, Ctrl+D duplicates into the nearest free spot, Esc deselects. Undo and redo
   (Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y, and buttons in the settings bar) cover every layout edit,
   tabs included, up to 100 steps, so deleting needs no confirmation; opening a file starts a new
   history. Widget settings (Edit… in the widget menu, double-click, or Enter): title (rename, or
   reset to the name from the source, which is shown), update period, Boolean Box true and false
   colors (Elastic's `true_color`/`false_color`), and raptaboard-only settings for Text Display,
   offered by the value's type (`data_type`, else the struct field's or the topic's type): fixed
   `decimal_places` for double/float, `int_format` (`decimal`, `hex` as `0x1A`, `binary` as
   `0b11010`) and `int_digits` (zero padding, like `%08X`) for int and enum numbers, `enum_display` (`name`, `number` or `both`) for enum
   struct fields, and `font_size` for any value. Changes show on the grid as they're made; Cancel or Esc undoes
   them, and one visit to the dialog is one undo step. Boolean Box can also write text over its
   color (`show_text`, `true_text`, `false_text`; black or white, whichever reads better). The
   setting keys are listed under "raptaboard widget settings". "Reset settings" puts every setting back
   to its default (period 0.06 s, default colors, no raptaboard settings) but keeps the title and
   the topic or field. Settings at their default are left out of
   the file. Next:
   - List Layout editing (drop into a list, create one).
   - Color by value for number widgets (ints and floats): a background color chosen by thresholds
     (below 10 red, 10 to 20 yellow, above green), or a gradient between colors across a range.
     raptaboard-only settings in the widget dialog.
   - Text style: a font dialog (from the widget dialog's Text size row) for the value text and the
     title: font family, size, color, bold, italic. Saved as raptaboard keys (`font_family`,
     `font_color`, `font_bold`, ...; `font_size` exists), with a layout-wide default style later.
   - Boolean Box images: show an image for true and for false instead of (or over) the color.
     A built-in library of standard images (Elastic's `true_icon`/`false_icon` set: Checkmark, X,
     Exclamation Point, so its layouts keep their icons, plus more), and any PNG/SVG the team
     picks. Open (decide when built), likely both: embed the image in the layout, scaled down to a
     small fixed size so the file stays well under its 10 MB limit and is fully portable, and/or
     keep image files next to the layout or served by the robot.
   - Find a topic's widgets: selecting a topic (or struct field) in the NT tree highlights every
     widget on the open tab bound to it, and marks other tabs that have one, so it's easy to see
     where a value is already shown before adding another.
   - Add from the tree without dragging: right-click a topic or struct field → "Add to grid" puts
     its widget in the first free spot on the open tab.
   - Select several topics in the tree (Ctrl+click, Shift+click for a range) and add them all at
     once, by dragging them together or with "Add to grid"; each gets its own widget, placed in free
     spots starting where they're dropped.
3. Core widgets: the 13 single-topic widgets, Field, Alerts, choosers, generic struct decoding.
   Whole structs dragged from the tree: a generic struct widget (fields like the tree shows them,
   enum names, struct arrays as items; raptaboard-only type) for any struct, and defaults for common
   WPILib ones: Pose2d/Pose3d → Field, Rotation2d → Gyro, SwerveModuleState[] → swerve view.
   Milestone 4 plugins may also match a struct type (a `struct = "ArmState"` rule).
4. Plugins v1: wasmtime host, limits, hot reload, the arm plugin, plugin template and SDK crate.
5. Robot hooks and the rest: ElasticLib hooks, layout download, remaining widgets, Mechanism2d,
   units, MJPEG cameras.
6. Competition hardening: DS docking, WHEP cameras, crash reports, Windows installer, plugins from
   the robot.
