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
Color View); every viewable type also gets Text Display and Large Text Display. In raptaboard, Large
Text Display is a Text Display with "Fit text" on (decided with the user 2026-11-21: one text widget,
with all its settings; 2026-11-22: no separate type). Opening a layout turns each Large Text Display
into a Text Display with "Fit text" on (`widgets::upgrade`, before it counts as saved, so it isn't a
fix or an unsaved change), and saving writes it as a Text Display; Elastic then shows it at a normal
text size. `Large Text Display` is also a name `find` knows for Text Display.

### Widget catalog (parity checklist)

| Widget | `.type` / aliases | Binds | Writes |
| --- | --- | --- | --- |
| Boolean Box | Boolean Box | boolean | no |
| Toggle Switch | Toggle Switch | boolean | yes |
| Toggle Button | Toggle Button | boolean | yes |
| Text Display | Text Display / Text View | number, string, arrays | yes |
| Large Text Display | Large Text Display | number, string | yes (opens as a Text Display with Fit text) |
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
| `true_color`, `false_color` (Elastic) | `builtin/boolean_box.rs` | Boolean Box | ARGB number; the settings' Clear makes it see-through (0), so the card shows and icons and text use the theme's text color | green, red |
| `show_text` | `builtin/boolean_box.rs` | Boolean Box | write the text below over the color | false |
| `true_text`, `false_text` | `builtin/boolean_box.rs` | Boolean Box | text when true / false; empty for none | none |
| `label_position` (Elastic) | `list_layout.rs` | List Layout | `TOP`, `LEFT`, `RIGHT`, `BOTTOM`, `HIDDEN` (any case read) | `TOP`; always written |
| `min_value`, `max_value` (Elastic) | `builtin/number_bar.rs`, `radial_gauge.rs` | Number Bar, Voltage View, Radial Gauge, Number Slider | range | -1 to 1; Voltage 4 to 13; Gauge 0 to 100 |
| `divisions` (Elastic) | `builtin/number_bar.rs` | Number Bar, Voltage View, Number Slider | labels along the bar, ends included | 5 |
| `editable` | `shared/props.rs` | Text Display; Dropdown and Split Button Chooser; Enum Dropdown and Toggle | the value can be changed from the widget; off, it only shows it (choosers then show the robot's `active`). Text Display: typed in, with a blue dot; not for arrays or struct fields | Text Display false; the others true |
| `all_groups` | `builtin/alerts.rs` | Alerts | every alert group on the server, not just the widget's `topic` | false |
| `show_text` | `builtin/alerts.rs` | Alerts | list the alerts' text by the circle | true |
| `text_position` | `builtin/alerts.rs` | Alerts | `auto` (beside a wide widget, below a tall one), `right`, `below` | `auto` |
| `min_level` | `builtin/alerts.rs` | Alerts | least urgent shown: `info`, `warning`, `error` | `info` |
| `pulse`, `pulse_seconds` | `builtin/alerts.rs` | Alerts | pulse when a new error arrives, for how long (s), stopping sooner once no error is active | true, 5 |
| `sound`, `tone` | `builtin/alerts.rs` | Alerts | a tone when a new error arrives (at most every 3 s; the bell button mutes it): `beep`, `double beep`, `chirp`, or a built-in clip: `disabled 1`, `disabled 2`, `fire 1`, `fire 2`, `danger danger`, `auto ghosts`, `go go go`, `fms down`, `lost comms` | false, `beep` |
| `field` | `builtin/field/mod.rs` | Field | the field shown: an id from the fields folders (`2026-rebuilt-welded`) or a field JSON's path; not set uses Elastic's `field_game` (by game name), else 2026 REBUILT (welded) | not set |
| `origin` | `builtin/field/mod.rs` | Field | where the poses' (0, 0) is: `center` (WPILib 2027) or `blue_corner` (before 2027) | `center` |
| `robot_length`, `robot_width` (Elastic) | `builtin/field/mod.rs` | Field | the robot's size, meters | 0.85, 0.85 |
| `robot_color` (Elastic) | `builtin/field/mod.rs` | Field | the robot's color | red |
| `field_rotation` (Elastic) | `builtin/field/mod.rs` | Field | the field turned clockwise, degrees (0, 90, 180, 270; 180 is the red side's view) | 0 |
| `layers` | `builtin/field/layers.rs` | Field | layers drawn over the field, each `{"kind", "topic", "data_type", "color"}`: `robots` (poses as robot outlines), `path` (poses joined by a line), `seen_tags` (AprilTag IDs as `int[]`, or seen tags' `Pose3d[]`, lit on the field), `objects` (game pieces: poses or translations, drawn as `marker` `circle`, `ring`, `x` or `square`, `size` meters across, on the field or with `robot_relative` from the robot's pose, as a camera finds them); a topic dropped on the field adds one (objects for a name with a game piece's word in it) | none |
| `attachments` | `builtin/field/attachments.rs` | Field | things fixed to the robot, each placed `x` m forward and `y` m left of its center, facing `yaw` degrees: `{"kind": "camera", "fov", "bands"}` (its field of view, shaded in three range bands, meters, default 4, 6, 8) or `{"kind": "part", "length", "width", "shown_by", "length_topic", "scale", "color"}` (a rectangle reaching out from its base: drawn while the `shown_by` boolean is true, its length from `length_topic` times `scale` meters) | none |
| `click_target`, `target_topic` | `builtin/field/target.rs` | Field | a click on the field publishes a `Pose2d` target (in the widget's `origin`; a drag sets its heading, right-click or a tap on the target clears it) to `target_topic`, with a boolean `<topic>_set` beside it; drawn as a green X from the topic | false, `/raptaboard/target` |
| `show_tags` | `builtin/field/layers.rs` | Field | draw the field's AprilTags | true |
| `show_other_objects`, `show_trajectories` (Elastic) | `builtin/field/layers.rs` | Field | a `Field2d` table's other objects: as robots, and as paths (more than 8 poses, or "traj" or "path" in the name) | true, true |
| `enum_source` | `builtin/enums.rs` | Enum Dropdown, Enum Toggle | where the state names come from: `list`, `topic` (a string[] topic) or `chooser` (the widget's topic is a chooser table); guessed when a widget becomes one | `list` |
| `enum_values` | `builtin/enums.rs` | Enum Dropdown, Enum Toggle | the listed states, one string per line: a value for a string topic; `3 = Climb` or a name (the next number, from 0) for an int topic | none |
| `switches_between` | `builtin/enums.rs` | Enum Toggle | two state names to switch between, like a Toggle Button's off and on (lit on the second; from any other state a click goes to the first); not set, a click steps through every state | not set |
| `options_topic` | `builtin/enums.rs` | Enum Dropdown, Enum Toggle | the string[] topic of names: values of a string topic, or names of 0, 1, 2 ... for an int topic | none |
| `sort_options` (Elastic) | `builtin/chooser.rs` | Dropdown Chooser | list the options sorted by name | false |
| `font_size` | `builtin/chooser.rs` | Dropdown Chooser, Split Button Chooser, Enum Dropdown, Enum Toggle | text size in points; not set fits the text to the widget (drop-down: a third of its height, no wider than the longest option, 12 to 36 pt) | not set |
| `publish_all` (Elastic) | `builtin/number_slider.rs` | Number Slider | send every value during a drag, not only where it's let go | false |
| `step` | `builtin/number_slider.rs` | Number Slider | values snap to multiples of it from the minimum; 0 snaps to a thousandth of the range (rounded down to a power of ten); int topics use at least 1 | 0 |
| `inverted` (Elastic) | `builtin/number_bar.rs` | Number Bar, Voltage View | fill from the far end | false |
| `orientation` (Elastic) | `builtin/number_bar.rs` | Number Bar, Voltage View | `horizontal`, `vertical` | `horizontal` |
| `start_angle`, `end_angle` (Elastic) | `builtin/radial_gauge.rs` | Radial Gauge | degrees clockwise from straight up | -140, 140 |
| `number_of_labels` (Elastic) | `builtin/radial_gauge.rs` | Radial Gauge | steps between labels | 8 |
| `wrap_value`, `show_pointer`, `show_ticks` (Elastic) | `builtin/radial_gauge.rs` | Radial Gauge | wrap out-of-range values; needle; tick marks | false, true, true |
| `time_display_mode` (Elastic) | `builtin/match_time.rs` | Match Time | `Minutes and Seconds`, `Seconds Only` | `Minutes and Seconds` |
| `red_start_time`, `yellow_start_time` (Elastic) | `builtin/match_time.rs` | Match Time | seconds left when the time turns red / yellow | 20, 30 |
| `fill_color` | `shared/scale.rs` | Number Bar, Voltage View, Radial Gauge | the fill, ARGB | blue (Voltage: yellow) |
| `gradient`, `low_color`, `mid_color`, `mid_value`, `high_color` | `shared/scale.rs` | the same | blend the fill by value: low → middle (optional, at `mid_value`, else the range's middle) → high | off; red, none, green |
| `unit` | `shared/scale.rs` | the same | text after the value (`V`, `m/s`) | none (Voltage: `V`) |
| `thickness` | `shared/scale.rs` | the same | bar or track thickness, points (1–200) | 8 bars, 6 gauges |
| `show_value` | `shared/scale.rs` | the same | show the current value and unit | true |
| `show_labels` | `shared/scale.rs` | the same | show the numbers along the scale | true |
| `time_displayed` (Elastic; older `visibleTime`) | `builtin/graph.rs` | Graph | seconds shown | 5 |
| `min_value`, `max_value` (Elastic) | `builtin/graph.rs` | Graph | a fixed range; both unset fits the values | unset |
| `counter_clockwise_positive` (Elastic) | `builtin/gyro.rs` | Gyro | `true`: a number (or a `Gyro` table's `Value`) counts counterclockwise; a `Rotation2d` always does | off (clockwise) |
| `show_robot_rotation` (Elastic) | `builtin/swerve.rs` | SwerveDrive | turn the robot with its heading (the table's `Robot Angle`, or `heading_topic`); off: front up | on |
| `rotation_unit` (Elastic) | `builtin/swerve.rs` | SwerveDrive (Elastic's table) | the table's angles: `Radians`, `Degrees` or `Rotations` | `Degrees` (as Elastic reads a file without it) |
| `setpoint_topic` | `builtin/swerve.rs` | SwerveDrive, Chassis Speeds | the requested states (a `SwerveModuleState[]` topic) or speeds (a `ChassisSpeeds`), drawn dashed against the measured ones | none |
| `heading_topic` | `builtin/swerve.rs` | SwerveDrive, Chassis Speeds | the robot's heading for a struct-bound widget: a `Rotation2d`, `Rotation3d`, `Pose2d` or `Pose3d` topic, or degrees counterclockwise | none |
| `max_speed` | `builtin/swerve.rs` | SwerveDrive, Chassis Speeds | the speed of a full-length arrow (the module spacing; Chassis Speeds: most of the card), m/s | 4.5 |
| `speeds_topic`, `speeds_setpoint_topic` | `builtin/swerve.rs` | SwerveDrive | the whole robot's measured and requested motion, `ChassisSpeeds` topics, drawn from the robot's center as Chassis Speeds draws them | none |
| `max_spin` | `builtin/chassis_speeds.rs` | Chassis Speeds, SwerveDrive | the spin drawn as a three-quarter turn, degrees per second | 360 |
| `fit` | `builtin/mechanism.rs` | Mechanism2d | what the view fits: `mechanism` (the part of the canvas it has used, plus a margin, growing only; a double-click starts it over) or `canvas` (all of `dims`) | `mechanism` |
| `color_by`, `color_below`, `color_above` | `shared/value_colors.rs` | Text Display, Number Bar, Voltage View, Radial Gauge | color by value: `thresholds` or `gradient` between the two numbers, with `low_color`/`mid_color`/`high_color` | off; 10, 20 |
| `fit_text` | `builtin/text_display.rs` | Text Display | the text as large as fits the card, instead of `font_size` | off; on for an opened Large Text Display |
| `true_icon`, `false_icon` (Elastic) | `builtin/boolean_box.rs` | Boolean Box | `None`, `Checkmark`, `X` or `Exclamation Point`, drawn on the state's color in black or white | `None` |
| `true_image`, `false_image` | `builtin/boolean_box.rs` | Boolean Box | an image's file name in the images folders, in place of an icon, drawn on the state's color (for a plain background, set the color to black or white) | none |
| `text_backing`, `true_text_color`, `false_text_color` | `builtin/boolean_box.rs` | Boolean Box | a see-through box behind the text; the text's color per state | off; automatic |
| `warn_below`, `low_below` | `builtin/battery.rs` | Battery | volts below which it's yellow, red; `min_value`/`max_value` (Elastic's keys) are empty and full | 12, 11.5; 11 to 13 |
| `color`, `line_width` (Elastic) | `builtin/graph.rs` | Graph | the first line's color; line width in points | cyan, 2 |
| `series` | `builtin/graph.rs` | Graph | the lines after the main one: `[{"topic", "data_type", "struct_meta"?, "color"?, "name"?}]` | none |
| `show_legend` | `builtin/graph.rs` | Graph | show the legend | when there's more than one line |
| `show_pause` | `builtin/graph.rs` | Graph | show the pause button (and measuring) | true |
| `show_hover` | `builtin/graph.rs` | Graph | show values where the pointer is | true |
| `label_inside` | `list_layout.rs` | List Layout | `true`: each label drawn over its value (on a see-through backing) instead of beside it | off |
| `dividers` | `list_layout.rs` | List Layout | `true`: a thin line under the list's title and below each child | off |
| `hidden` | `list_layout.rs` | a List Layout's child | `true`: kept in the list but not shown or subscribed | shown |

Adding a setting: a key constant and a reader in the file that owns it, its row in that widget's
`WidgetType::settings` (shared rows: `props::text_size_row`, `props::color_row`, `format::rows`),
the key in the widget's `setting_keys` so "Reset settings" clears it, and a row here.

### Grid size

`grid_size` is the cell size in pixels (Elastic's default 128); widget positions and sizes are
saved in pixels at that size. The grid rules (`crates/app/src/grid.rs`, `sanitize`) all take the
cell size, and their tests run at 16, 64, 128 and 256. Drawing sizes (card title 24 pt, gaps,
outlines, corner radii; named constants in `ui/style.rs` and `widgets/shared/card.rs`) are screen
points and don't change with the grid size. There's no setting to change the grid size yet. When
there is one, it must:
- Rescale every widget's position and size (pixels at the old size become pixels at the new),
  or the layout scrambles.
- Shrink the title strip and gaps on small grids: at 16 px a 24 pt title is taller than a cell.
  Likely the title and gaps scale down below some cell size (around 64 px), or the title hides.

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
   hotkeys. **Done** apart from the polish items under Next. Settings are kept between runs: team,
   address, port, tree shown, locked, last layout (however it was opened, the command line
   included) and its last tab (by name and position, so tabs that share a name are told apart and moved
   or renamed tabs are still found), and window size and place, found in order from the
   command line (this run only, never saved), `./.raptaboard/settings.toml`,
   `~/.raptaboard/settings.toml` (made with the defaults on the first run), then defaults; each
   file may set only some keys. Changes made in the app are saved (a second after the last change,
   and on close) to the local file if it exists, else the home file; a file that can't be read is
   noted and never overwritten. Also: tabs (add, close with a confirmation if it has widgets, rename, reorder;
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
   Elastic's `struct_meta` (`path`, `schema_name`, `type`). Fields inside struct arrays drag too
   (`Fleet[2].translation.x`, saved with `[2]` path steps; raptaboard only, Elastic can't bind them). Whole structs wait for their widgets. Dragging folders
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
   - NT tree rows (topics and struct fields that can be widgets) can be picked: click one,
     Ctrl+click (Cmd on macOS) to add or remove, Shift+click for a range. Picked rows are shaded;
     widgets showing them get an amber outline (list rows too), and other tabs with one get a ●.
     The other way round: clicking a widget (or a list child) opens the tree to its topic,
     field or chooser table, scrolls to it and picks it, locked or not and wherever on the widget
     (a toggle or chooser button still does its own thing too; such a click says nothing if the
     topic isn't in the tree, so a failed send's message stays).
     Right-click → "Add to grid" (or "Add N to grid") puts them in the free spots nearest the top
     left, "Add as ▸" makes them as any widget type that can show them all (the default first: a
     Pose2d as a Struct, Field or 3D Pose, a number as a Text Display, Graph, Radial Gauge, ...; set up
     as the settings dialog sets up a type picked there, with that type's update period), and "Add to
     list ▸" adds them to a new list (sized to fit them; always offered) or to the end of
     one of the tab's lists; dragging a picked row takes them all, onto the grid (around the drop) or into a list.
   - Right-clicking a list child (unlocked) opens its own menu, Edit… (its settings) and Edit list…:
     the child sits over the list, so the list's menu never opened there.
   - List Layout editing. Done: lists draw like Elastic's (children compact with no card each:
     a label placed by `label_position` and the child's body, each row its saved height less 48
     and at least 32); Edit… or double-click on a list opens its settings (title, label position and an Inside box that
     draws labels over the values to save space, Dividers for a thin line between children,
     children in order with a Shown checkbox (unticked: kept but not drawn or subscribed), ↑/↓,
     Edit…, Delete); double-clicking a child opens its own settings. Dragging: a grid widget or a
     tree topic dropped over a list goes into it, where the green line shows; a child dragged
     within its list reorders, into another list moves there, and onto free grid becomes a widget
     again (its saved size, in whole cells). New lists: right-click empty grid → "New List
     Layout" (an empty 2 x 2 list, titled "List Layout", nearest the click; new lists start with
     labels inside at the top and dividers on, raptaboard's more compact look, while lists from
     files keep their own settings), or right-click a
     widget → "Put in a new list" (a list in its place, at least 2 x 2 and a cell taller than the
     widget, holding it).
   - Color by value for number widgets (**done**, `shared/value_colors.rs`): `color_by` `thresholds`
     (low color below `color_below`, high above `color_above`, middle between; 10 and 20, red, yellow
     and green if not set) or `gradient` (blended low → middle → high between the same two numbers).
     Text Display fills its body with it, the text black or white to read on
     it; Number Bar, Voltage View and Radial Gauge use it for their fill unless their own gradient (over
     their scale) is on. The colors are the scale's `low_color`, `mid_color`, `high_color`.
   - Text style: a font dialog (from the widget dialog's Text size row) for the value text and the
     title: font family, size, color, bold, italic. Saved as raptaboard keys (`font_family`,
     `font_color`, `font_bold`, ...; `font_size` exists), with a layout-wide default style later.
   - Copy settings to the rest of a list (**done**, `list_layout::copy_settings`): in the list settings,
     set up one child as wanted, then its "Copy to same type" gives the others of the same widget type
     its display settings so they match it exactly: every display setting either has (Elastic's too,
     such as a Boolean Box's colors, and the period) set to its value, or removed where it has none, so
     copying from a child on the defaults puts the others back on the defaults.
     Never what a child shows (topic, `struct_meta`, `data_type`, `*_topic` settings, graph lines,
     field layers and attachments, enum names), its title, or whether it's hidden. Picking which
     children could come later.
   - Scrolling the grid (**done**, asked for by the user 2026-11-22, `ui/grid_view.rs`): widgets
     past the window's edge (a layout made on a bigger screen, or dragged there) were hard to get
     back. The grid scrolls: it's as big as the window or as far as its widgets reach, with solid
     scroll bars when anything is off screen, scrolled by the bars or the mouse wheel (not by
     dragging, which moves widgets). Dragging a widget, a list child or topics from the tree near
     an edge scrolls that way, faster closer to the edge, and the grid grows a cell at a time
     ahead of it, so a widget can be dragged past the window. Each tab keeps its own scroll.
   - Reflow while dragging: widgets in the way slide aside (to the nearest free spot) as a widget is
     dragged or resized over them, instead of the drop being refused.
   - Allow partial overlap: a per-widget setting (this widget may partly overlap others, such as a
     label over a corner of a camera view) and a layout-wide setting allowing it everywhere.
     Partial only: no widget may be completely covered, so every widget keeps some part visible to
     see and grab. A 1x1 on top of a 1x2 is fine (part of the 1x2 still shows); two 1x1s on the same
     cell, or a big widget over a smaller one, are not. Needs a draw order (front/back), and the
     overlap fix when opening a layout must allow the same partial overlaps.
   - Boolean Box text (**done**): `text_backing`, a see-through box behind the text so it reads over
     any color, and `true_text_color` / `false_text_color` (automatic black or white if not set; white
     on the backing).
   - Group widget: a card holding its own small grid, so widgets inside can sit side by side and
     be any size (a List Layout only stacks them). Same dragging in and out as lists; saved as a
     raptaboard-only container type (Elastic would show a placeholder). Its settings can reach the
     widgets inside: group-level settings (font size, period, colors) that every child uses
     unless it sets its own, and resizing the group scales its children's sizes (and text) to
     match.
   - Autoscale for bars and gauges: an option where the scale follows the lowest and highest values
     seen instead of fixed `min_value`/`max_value` (widening as new extremes arrive; optionally only
     growing from the set range, and a reset in the right-click menu). Not saved as the range;
     starts fresh each run.
   - Graph history review: while paused, scroll back and forward through time (drag or the scroll
     wheel along the time axis, maybe zoom), beyond the window shown, for looking back at what led
     up to an event. A per-graph "history kept" setting (expected use is up to 3–5 minutes; memory
     is fine at that, about 1 MB per line at 100 Hz for 10 minutes; make the `traces.rs` cap follow
     the setting instead of a fixed point count) sets how far back is recorded; the cursors measure
     anywhere in it. Zoomed out over minutes, reduce each line to about one point per pixel column
     before drawing, so a frame doesn't copy tens of thousands of points per line. Applies to X/Y and polar graphs
     too.
   - Graph sample statistics: an option (off by default) to show each line's min, max and average
     over the samples in view: the time window shown, or between the A and B cursors when both
     are placed (so a measurement gives them for just that stretch), and the frozen window while
     paused. Shown with the legend or the readout, in each line's color; reduced points for
     drawing (see history review) must not change them, so they're worked out from the recorded
     samples.
   - Min, max and average on number widgets (Text Display, Number Bar, Radial
     Gauge): an option (off by default) to show them next to the value, over a set time window
     (such as the last 10 s) or since the last reset (a Reset in the widget's right-click menu,
     and when the robot reconnects). Taken from every value received, as graphs record them
     (`traces.rs`), not from the widget's sampled period, so short peaks count. Bars and gauges
     could also mark the min and max on their scale.
   - Widget type choices that fit (**done**): the settings dialog's Widget list offers only the types
     that can show what the widget is bound to (`widgets::types_showing`, as the tree's "Add as"): for
     a table its `.type` is the value type (`DashboardApp::value_type`), so a chooser offers the
     choosers and the enum widgets (which now list the chooser `.type` in `table_types`), and a
     number topic offers number widgets, not choosers. Unknown (not connected): every type.
   - Alerts (**done**, `sign_places` in `builtin/alerts.rs`): the most urgent active sign in the circle
     grows large in the middle (an error's ✕ while there are errors, else the ! while there are
     warnings), the other two shrinking along the bottom, so the worst one reads at a glance from
     across the driver station; back to equal sizes when nothing above info is active.
   - Alert history "Not recorded" entries as regular expressions too (a per-entry choice beside
     today's partial text match, which stays the default), if more robot code turns up whose
     alerts need it. Not needed yet: the one noisy alert (robot-2027's Frame Timing) is fixed at
     the source and its numbers are already folded into one event.
   - Drag tabs to reorder them (**done**: `TabAction::Move`, `tabs::drop_place`): unlocked, drag a
     tab along the tab bar, with a marker where it will land, and drop it to move it (one step for
     undo, like Move left and Move right in the tab's right-click menu, which stay).
   - FMSInfo widget with robot state sounds (**done** except the match-timing rows below:
     `builtin/fms_info.rs`, read by `types/robot_state.rs`, sounds in `state_sounds.rs`; first
     release, asked for by the user 2026-11-22; replaces the earlier "sound clips on robot state
     changes" item). First built as a "Robot State" widget, then made general purpose the same day
     (asked for by the user): Elastic's `FMSInfo` type, so Elastic layouts' FMSInfo widgets open as
     it. It binds the Driver Station's table, and dragging that table from the tree makes one
     (`table_types`). Parts, each on or off in the settings (`show_*`): the robot state, large in its
     color with a built-in icon (CC0, `images/`: a stop sign for E-Stop, a robot with an X for
     Disabled, the robot thinking of a brain for Autonomous, a game controller for Teleop, a
     clipboard and pencil for Test; an image of the same name in an images folder replaces it), a
     red or blue alliance square, the match ("Qualification 12"), station, replay (off by default),
     event name, game data (off by default; only used one year so far) and the DS and FMS marks.
     Later: layout options inside the widget.
     - Sounds play from the NetworkTables thread (`state_sounds.rs`), not from drawing: a window in
       the background (or covered) gets few or no frames, so sounds checked while drawing were late
       or missed (seen 2026-11-22). Each frame the app hands the first FMSInfo's sound plan to
       `NtConnection::set_state_sounds`; a task on the client's runtime subscribes to the state and
       plays the changes as values arrive, keeping the last plan meanwhile. Only the first FMSInfo
       in the layout plays; the others only show.
     - Reads the state (checked against WPILib's source 2026-11-22): 2027 publishes it in
       `/DriverStation` (`.type` `DriverStation`, not `/FMSInfo`) as a `ControlWord` struct, 64 bits
       little-endian: `opModeHash:56`, `robotMode:2` (unknown 0, autonomous 1, teleoperated 2,
       utility 3, shown as Test), then `enabled`, `eStop`, `fmsAttached`, `dsAttached` (bits 58-61);
       raptaboard reads the bits, as WPILib's Glass does. 2026 publishes `/FMSInfo` (`.type`
       `FMSInfo`) with an integer `FMSControlData` (enabled 0x01, auto 0x02, test 0x04, e-stop
       0x08, FMS 0x10, DS 0x20). Either table works; the widget reads `ControlWord` first.
     - Shows one large state: Disabled, Autonomous, Teleop, Test, E-Stop, or No robot (not connected
       or no DS), each with its own color (E-Stop red and blinking, Test striped orange), plus small
       DS and FMS marks.
     - Sounds: one row per event (disabled, autonomous, teleop, test, e-stop, and robot connected and
       lost), each Off, a built-in clip, or a file the team picks (MP3 or WAV, played with `rodio`),
       plus a widget volume. Saved in the widget's properties, so they travel with the layout.
       Defaults: disabled plays "disabled 1", autonomous plays "auto ghosts", teleop plays "go go
       go", test plays "danger danger", the rest off.
     - More rows (added 2026-11-22), all off by default: match timing (**not done yet**: it needs
       the match time, which WPILib doesn't publish in this table) from the table and the
       match time (end of auto, endgame at a set time left, 30 s by default, and a 10 s
       countdown; the field already sounds these, so they're for practice or a quiet venue), and
       DS lost or FMS lost while the robot is still connected (a different problem from the robot
       lost; **done**). Exceptions to off (asked for 2026-11-22): FMS lost plays "fms down" and robot lost
       (the NT server gone) plays "lost comms", two text-to-speech clips (first made with Flite, replaced the same day with ones from Free TTS Online, whose terms let its audio be used for personal and commercial purposes). Later ideas, not planned: FMS attached, brownout or low battery.
     - Plays only on a change seen while connected: never for the state at connect, after a
       reconnect, or when a layout opens. A new event cuts off a clip still playing. The bell button
       in the settings bar mutes these too.
     - Plays while the widget is anywhere in the open layout, not only on the shown tab, so switching
       tabs never silences it (`WidgetType::state_sounds`). With two FMSInfo widgets, only the first
       (tab order, then position) plays. Each sound row has ▶ to hear it.
     - Built-in clips (`media/audio/`): "disabled 1" and "disabled 2" (the IT Crowd's "I'm
       disabled"), "fire 1", "fire 2" (the IT Crowd's "Fire!", S1E2), "danger danger" (added 2026-11-22: xpoki's
       "danger_normal_lp.wav" from Freesound, CC BY 4.0, played twice) and "auto ghosts" (added
       2026-11-22: dal_kas's "MustphaMondeRemixes (3).aif" from Freesound, a remix of
       deleted_user_44129's "Ghosts In The Machine.wav", both CC BY 3.0) and "go go go" (added 2026-11-22: theuncertainman's "Go Go
       Go - (Version 2) Northern British Soldier" from Freesound, CC BY 4.0). Every clip's
       source and license is in `media/audio/README.md` and `credits.rs`. More clips later.
       The clips that aren't ours (the IT Crowd's "disabled" and "fire") stay in the releases, 0.1.0 included (decided with the user 2026-11-22):
       non-commercial, educational use by Team 3200 is low risk, and any clip is removed at once
       if its owner asks. Revisit when teams outside Raptacon use raptaboard.
   - Later, after 0.1.0 (asked for by the user 2026-11-22), from the same robot state:
     - Window border by state: a colored border around the whole window, red while disabled, and
       flashing red for a few seconds right after the robot is disabled, so the state shows at a
       glance from any tab. Colors per state set in the Settings window; off by default.
     - E-stop pop-up: a large modal over the whole window when the robot enters e-stop, saying so
       plainly. It clears on a click anywhere or when the robot reconnects (an e-stopped robot
       stays e-stopped until it reboots).
   - Connection settings behind a gear (**done**, `ui/settings_window.rs`; asked for by the user
     2026-11-22): a ⚙ button in the settings bar opens a Settings window with the team number,
     address mode, custom address and port, and sound on or off (later: volume). The bar keeps the
     raptaboard name, the connection status, Tree, Lock, the bell, the alert history, undo and redo,
     so it stays short on a DS laptop.
   - About and licenses (**done**, asked for 2026-11-22): a button in the Settings window opens a
     window with pages for raptaboard (version, repository, MIT license), the built-in sounds,
     images and fields (each one's source, license and credit, from `credits.rs`; CC BY credits
     here meet the license's attribution), the Rust crates (all of them from `Cargo.lock` with their
     licenses and a filter, from `licenses/rust-crates.tsv`) and their full license texts
     (`licenses/THIRD_PARTY_LICENSES.txt`, with copy). `scripts/crate_licenses.py` makes both
     files; a test fails when they don't match `Cargo.lock`.
   - X/Y and polar graphs (`egui_plot`): pair two values recorded at the same time (x with y, or
     angle with distance) and plot one against the other, for a joystick's position, an arm
     joint's path or a turret's aim. Modes: a trail that fades out over a set time (time decay),
     or just the current point (tracking). Several pairs per graph, like Graph's lines; pairs come
     from two topics, two struct fields (Pose2d x/y, Translation2d), or a dropped struct with a
     known pair. Angle units for polar (degrees or radians, zero direction, clockwise or not).
     Shares Graph's recording (`traces.rs`), pause and measuring.
   - Battery widget (**done**, `builtin/battery.rs`): a battery icon filled by charge (the voltage
     between `min_value` and `max_value`, 11 to 13 V), green, yellow below `warn_below` (12 V) and red
     below `low_below` (11.5 V), with the volts and charge on a see-through backing; on its side on a
     wide card, upright on a tall one. A number's "Add as" or Widget list choice (a number still makes
     a Text Display). The sim layout's More widgets tab shows robot-2027's scripted `BatteryVoltage`.
   - Boolean Box images: show an image for true and for false instead of (or over) the color.
     **Done**: Elastic's `true_icon`/`false_icon` set (Checkmark, X, Exclamation Point, any for either
     state in raptaboard), drawn on the state's color in black or white (Elastic draws them alone in the color; decided
     with the user 2026-11-21 that the box is always filled, so there's no option to explain: for a
     plain background, the color rows' Clear makes the color see-through, and icons and text then
     use the theme's text color, dark or light); and the team's own images (decided with the user 2026-11-21), `true_image` /
     `false_image`, chosen from a list of the images in the images folders (`images.rs`: `images/`
     where raptaboard starts, beside the program, and `~/.raptaboard/images/`, as fields are found),
     with an Import… button that copies a picked PNG, JPEG or SVG into `~/.raptaboard/images/` (a
     different image with the same name gets `-2`, ...). Layouts save the file name, so another
     computer needs the same image in its images folder. SVGs are drawn with `resvg` (0.49, no text
     features; about 0.7 MB). Built in (decided with the user 2026-11-21), examples to show the feature
     and a starting place: `checkmark.svg`, `motor.svg`, `motor-on.svg` and `motor-off.svg` (the motor
     in a green or red border, a Boolean Box's true and false images) and `armed.svg` (a target
     reticle), drawn for raptaboard and dedicated CC0, in the repo's `images/`
     with a README saying so and where any outside image's credit goes; a folder's file of the same
     name takes their place. The settings choose a state's picture from one list (the user's ask,
     2026-11-21): (none), the built-in group (Elastic's drawn icons, then the built-in images), a
     line, then the images in the folders, with Import… beside it; an icon is saved as Elastic's
     `true_icon`/`false_icon`, an image as `true_image`/`false_image`, and picking one clears the
     other. The sim layout's Widgets tab has a Boolean Box on robot-2027's
     `IntakeDeployed` with `motor-on.svg` and `motor-off.svg`. Later: embedding images in the layout.
3. Core widgets: the 13 single-topic widgets, Field, Alerts, choosers, generic struct decoding.
   **Started**: the read-only single-topic widgets: Large Text Display, Number Bar, Voltage View,
   Radial Gauge (also Simple Dial), Match Time, Single Color View, Multi Color View, with Elastic's
   settings, plus raptaboard's scale settings for bars and gauges (fill color, solid or blended by
   value low → middle → high; a unit; the value and the scale's labels each on or off; an amber dot past an end when the value is
   beyond the scale). Voltage View is kept for Elastic layouts but is a Number Bar preset (4 to 13,
   yellow, unit V). A dropped topic still becomes a Text Display or Boolean Box; the settings dialog's
   Widget choice switches to any type that can show the value (`WidgetType::shows`), keeping the
   other types' settings. The settings dialogs (widget and list) are windows that can be dragged
   by their title bar out of the way of the widget being changed; they reopen where they were left
   (for the run), and the grid behind stays blocked but only lightly dimmed. Graph (with `egui_plot`): numbers and booleans over time, Elastic's
   settings, and raptaboard's multiple lines on one graph (drop more topics or struct fields from
   the tree onto it; a legend shows when there's more than one). All lines are equal: the
   settings list them with ↑/↓ and Remove, the first being the main line, saved as the widget's own
   `topic`/`color` as Elastic saves it. A pause button freezes the picture while values keep
   recording; while paused, two clicks place measuring cursors A and B (scope style; with only A, clicking A takes it away; with both, any click clears them) and a readout
   gives Δt and each line's values and Δ there. Values are recorded in
   `traces.rs` for the series the open tab's graphs show, each for the longest window wanted:
   every value received (an `all` subscription, read from the client's event stream), not one
   per frame. Graphs default to a 0.016 s period (about 60 a second; other widgets keep 0.06), and
   the window redraws at most about 60 times a second; graphs aren't meant for faster signals. A
   plot only senses hover, so it never takes the drags that move the widget.
   Writing values (decided 2026-11-19): Toggle Switch and Toggle Button are built. Widgets return
   the value to send from `WidgetType::show`, and the grid sends it with `NtConnection::write`.
   As in Elastic, only a topic the server has announced is written, with the topic's own type
   (numbers convert, ints round, text is parsed), published once and kept until the connection
   restarts, without changing its properties; struct topics and fields aren't written. Unlike
   Elastic, a write that can't happen says so: the widget's border flashes red three times,
   an alert tone plays (`sound.rs`, `rodio`; the `alert_sound` setting and a bell button in the
   settings bar turn it off), and the status line gives the reason ("the robot hasn't published
   it", "struct values can't be changed", "it's a boolean", "not connected"). Input: the lock
   only stops layout edits, controls always work; locked, input starts anywhere on the widget;
   unlocked, the title bar moves the widget and the rest is input; a drag that starts in a
   widget keeps adjusting it when the pointer leaves it (egui keeps the drag). A widget shows its
   own write at once and the robot's (or another dashboard's) later change wins, which needs nt4
   0.3 (a client sees its own values, as in ntcore). They work inside List Layouts too: each
   child's body gets a `Ui` (`WidgetType::show_body`), and a child that can't send flashes its own
   row. Unlocked, a child's label is its handle (like a widget's title bar): with labels inside,
   input starts only beside the label (`Live::input`), so the label still selects and drags the
   child; locked, the whole row is input. A failed send's message clears on the next send that works (unless something else has
   said something since).
   Enums (decided): an Enum Dropdown and an Enum Toggle (two named states instead of true and
   false), bound to a SendableChooser table (`options`, `default`, `active`; writes `selected`),
   or a string or int topic with the values listed in its settings; on creation the widget
   guesses the source, and its settings can change it. The robot publishes the full list at the
   start; if the list changes, the pick is dropped and the robot's value is used, and any change
   of the value (robot or another dashboard) is shown. Uses: auto mode, a test in test mode.
   Struct enum fields (schema `enum {..}`) stay display-only: writing one would mean rewriting the
   robot's struct.
   Number Slider (`builtin/number_slider.rs`, done): pressing anywhere in its input area moves the
   knob there and a drag follows the pointer, also outside the widget; it sends where it's let go,
   or every value with `publish_all`; the knob stays at the sent value until the server has it (or
   1 s), so it doesn't jump back while the value is on its way. Escape during a drag cancels it:
   nothing is sent, or with `publish_all` the value from before the drag is sent back (egui ends a
   drag on Escape, and Escape doesn't deselect while a button is held). Clicking the written value
   opens a box with the value selected: Enter sends what's typed (a unit may be left on; kept in
   the range, whole for int topics, not snapped to `step`); a bad number turns red and stays open;
   Escape or a click elsewhere closes it unsent. Horizontal only for now; vertical, keyboard and
   scroll-wheel nudges are later.
   Typing a value (`widgets/shared/entry.rs`, shared by the slider and the Text Display): the box
   opens with the value selected, Enter sends text the widget accepts (bad text turns red and the
   box stays open), Escape or a click elsewhere closes it unsent. A Text Display with `editable`
   on (raptaboard's, off by default; Elastic's is always editable) has a blue dot in its title
   bar's corner (a list child's: its row's corner) and opens the box on a click (locked anywhere,
   unlocked the body); it sends the text and the topic's type decides what it becomes; number
   topics need a number and boolean topics `true` or `false`. A List Layout child's widget also
   gets `Live::free`, its row less an inside label, locked or not, so a slider writes its number
   below the label rather than under it.
   Choosers (`builtin/chooser.rs`, done): Dropdown Chooser and Split Button Chooser, bound to a
   SendableChooser table (`topic` is the table; saved without `data_type`, as Elastic does). They
   read `options`, `default`, `active` and `selected` (`WidgetType::extra_topics`, values in
   `Live::others`) and send to `selected` (`WidgetType::sends_to`; `NtConnection::write_or_publish`
   publishes it as a string, since robot code only reads it). Shown as picked: `selected` if it's an
   option (from any dashboard), else `active`, else `default`; while `active` isn't the pick, an
   orange "Robot has: X" note (and outline on the button). When the robot's option list changes (not
   to empty, which is a restart), a pick is dropped: the robot's `active` (or `default`) is sent.
   In the tree, a table whose `.type` has a widget (`WidgetType::table_types`, "String Chooser") is
   one draggable row (its `.type` values are always watched) with an open arrow of its own that lists
   its parts and their values (`options`, `active`, ...; with metadata shown, `.type` and the other
   hidden ones too), so it's found and dragged the same either way.
   Enums (`builtin/enums.rs`, done): Enum Dropdown and Enum Toggle for a plain string or int topic
   that robot code and dashboards both change (the newest value wins), or a chooser table. Names
   from a list in the settings, a string[] topic, or the chooser (`enum_source`). When a widget is
   changed to one (`WidgetType::chosen`, with every server topic in `ValueInfo::topics`) it guesses:
   a chooser table if the topic is one, else a string[] topic beside it named `<name>Options`,
   `<name>_options`, `<name>Names`, `<name>_names` or `<name>s` (any case), else a list; "Guess
   again" redoes it. The toggle steps to the next state each click (two states switch), or with `switches_between`
   switches between two chosen states like a boolean Toggle Button. A value
   that isn't a state shows `unnamed (7)` or `text (not listed)`. `editable` off (also on the
   choosers) makes them show only: a state display for a subsystem, or a chooser watched from the
   pit. Struct enum fields stay display-only (Text Display shows their names). The tree scrolls
   sideways, so a wide row (opened by selecting a widget) doesn't widen it and move the grid.
   Alerts (`builtin/alerts.rs`, `types/alerts.rs`; decided with the user): WPILib's alert groups are
   tables with `.type` "Alerts" and three string[] lists, `errors`, `warnings`, `infos` (the active
   alerts' text, newest first; text is all an alert carries; 2027 calls the levels HIGH, MEDIUM,
   LOW and Glass still reads these lists). The widget is a circle with a red ✕, yellow ! and blue i
   (painted, not glyphs), each lit with a count while that level has alerts, the ring in the most
   urgent color; optionally the text beside or below. One group, or all groups. A new error (not
   warnings) pulses it for N seconds (or until no error is active, if sooner) and can play a tone (`sound.rs` `Tone`, generated, a few to
   pick; widgets ask through `Live::effects`). When the connection drops, or the topics go before
   it does, the last alerts stay, greyed. A tab with an Alerts widget showing an error or a warning
   gets that sign after its name, open or not (alert topics of other tabs' Alerts widgets are
   subscribed too). An alert group in the tree is a draggable row like a chooser.
   Alert history (`alert_history.rs`, `ui/alert_history_panel.rs`, done): every alert group is recorded
   from connect, whether or not an Alerts widget shows it (every table's `.type` and the alert
   groups' lists are subscribed every 0.02 s). An event per alert, known by group, level and text
   with its numbers left out (`alert_history::pattern`), and which copy, so robot code that puts a
   reading in its text (`Loop at 37%`, `38%`) is one event whose newest text is kept and whose
   changes are counted; the `alert_history_ignore` setting (edited in the panel's "Not recorded"
   list, or a row's right-click "Don't record alerts like this", which adds its text up to the
   first number) leaves out alerts whose text contains an entry. Events: when it was seen, and when and how it ended (cleared; robot disconnected;
   group not published). Count-based: the last `alert_history` events (setting, default 1,000,
   also set in the panel), oldest dropped; nothing goes by age, so a match's alerts are there
   afterwards; memory only. A top-bar "Alerts" button with the number active and the worst one's
   sign shows and hides a right-side panel: newest first, with level, group, text, seen, how long,
   and the ending; filters by level, group and text; Clear forgets ended events (active ones
   stay). Match time isn't recorded yet (no standard topic for it).
   Next: Field, struct widgets.

   Field widget (decided with the user 2026-11-20). A 2D top-down field (3D may come later).
   - Coordinates: WPILib 2027's frame is the default: origin at the field's center on the floor,
     +X toward the blue alliance wall, +Y left facing +X, heading 0 facing +X, counterclockwise
     positive. An `origin` setting also reads the pre-2027 frame (origin at the blue alliance
     corner, +X toward red), converted as WPILib's `fields/README.md` says: x = L/2 - old_x,
     y = W/2 - old_y, heading + 180°. robot-2027 is on RobotPy 2026 today, so its poses are the
     old frame until it moves to the 2027 libraries.
   - Fields: WPILib's `fields` library format (BSD; FIRST and WPILib contributors; notices kept
     with the files): a JSON per field (`name`, `season`, `game`, `field-dimensions` in meters,
     `field-image` with the field's pixel bounds `top`/`left`/`bottom`/`right`, and `field-tags`, the
     official AprilTag layout, already in the 2027 frame) and its image, blue wall on the right.
     They live in a `fields/` folder (the repo's, next to the program, or `~/.raptaboard/fields/`);
     2026 REBUILT ships in the repo and is built into the program so a field always shows. The
     settings pick one from those found, or a JSON chosen with a file dialog. A `raptaboard` object
     in the JSON may hold mapping details WPILib's format doesn't (reserved; none yet).
   - Bound to a `Pose2d` (or `Pose3d`, drawn flat) struct topic, or a WPILib `Field2d` table
     (`.type` "Field2d", `Robot` and other objects as `double[]` x, y, degrees), as Elastic reads.
     The robot is an outline with a heading arrow, sized in meters (`robot_length`, `robot_width`).
     A red-side view turns the field 180°.
   - Overlays, each a layer fed by its own topic: other poses (`Pose2d`, `Pose2d[]`, `Field2d`
     objects) as ghost robots; trajectories (`Pose2d[]`) as lines; AprilTags from the field JSON,
     with detected ones highlighted (an `int[]` of IDs or a `Pose3d[]`); detected objects
     (`Translation2d[]` / `Pose2d[]`, field- or robot-relative); and robot-attached shapes set in the
     settings relative to the robot's center: a camera's FOV wedge (its robot-to-camera transform,
     FOV and range bands) and deployed parts (a rectangle shown while a boolean is true, or
     lengthened by a number). Demo data: Robot-2026's two cameras (`constants/robot_geometry.py`)
     and a 2D port of its camera visualizer's tag rules (in FOV, within 6 m, within 60° of head-on;
     0 to 4 m reliable, 4 to 6 marginal, 6 to 8 unlikely) in robot-2027's sim test topics.
   - Order: (1) fields, the Field widget and its robot (**done**: `fields.rs`, `types/field.rs`,
     `builtin/field.rs`; a Pose2d or Pose3d dragged from the tree makes a Field, and its row still
     opens into its fields; the image decodes on its own thread, at most 2048 pixels wide); (2) pose,
     trajectory, AprilTag and object overlays (**done**: `builtin/field/layers.rs`; a pose array as the widget's
     own topic is the robot and others; robot-2027's sim publishes `OrbitPath` and `SeenTags`; objects layers, decided with the user: a marker per
     layer (circle, ring, X, square) in its color and size, on the field or relative to the robot, fed by
     poses or translations; the sim's `Fuel` and robot-relative `SeenFuel`); (3) robot-attached shapes (**done**: `builtin/field/attachments.rs`; the sim layout has Robot-2026's
     two cameras and an intake on robot-2027's `IntakeDeployed` and `IntakeExtensionIn`).
   - Click to set a target (**done**, `builtin/field/target.rs`; asked for by the user 2026-11-20). Off unless the Field's setting turns it on. A click on the
     field publishes a `Pose2d` target (in the widget's `origin`) to a topic raptaboard publishes
     (`/raptaboard/target` unless set; a widget's settings can't see its title, so one fixed default),
     with `<topic>_set` true beside it; the dashboard publishes the Pose2d's schemas if the server has none
     (`NtConnection::publish_struct`, which only publishes a struct topic that's new or already that type); dragging sets
     its heading; right-click or "Clear target" clears it. Drivers can use it to aim or send the robot
     somewhere, and robot code is free to ignore it. The target shows as a green X, drawn from the
     topic's value, so a target robot code or another dashboard set shows too. Works while locked
     (driver input). Free points and one target for now; still open: snapping to tags or game pieces,
     and a list of targets for a route. robot-2027's `raptaboard-test` test topics log the target.

   Struct widgets after the Field, each its own widget: a Gyro (**done**, `builtin/gyro.rs`, read by
   `types/angle.rs`: a `Rotation2d`, a `Rotation3d`'s yaw, a number of degrees or a WPILib `Gyro`
   table, on a compass dial with 0° up and a robot outline, the angle 0 to 360°; rotations count
   counterclockwise, numbers clockwise unless Elastic's `counter_clockwise_positive` is on;
   robot-2027's test topics publish `OrbitHeading` and a `Gyro` table, both shown on the sim layout's
   Field tab), motion
   vectors (**done** as Chassis Speeds, `builtin/chassis_speeds.rs`, read by `types/motion.rs`: a
   `ChassisSpeeds` (2027: `ChassisVelocities`) or a `Translation2d`, robot-relative: an arrow for the
   velocity and a curved arrow round the robot for its spin, requested speeds dashed against measured,
   turned by a heading topic, as the swerve view; the robot drawing is `widgets/shared/robot_view.rs`;
   the measured and requested numbers are written below the robot when the card has room; the sim layout
   shows robot-2027's `/Drive/MeasuredSpeeds` against `/Drive/CommandedSpeeds`, and its scripted
   `/DashTest/` pair, whose measured speeds lag and fall short, since the keyboard joystick can't spin
   the sim drivetrain), and a swerve view (**done**, `builtin/swerve.rs`, read by `types/swerve.rs`; Elastic's
   `SwerveDrive` name and table (`Front Left Angle`, `... Velocity`, `Robot Angle`), or a
   `SwerveModuleState[]` (2027: `SwerveModuleVelocity[]`) in FL, FR, BL, BR order: each wheel turned
   the way it points with an arrow for its speed, the requested states (`setpoint_topic`) dashed
   against the measured ones, the robot turned with its heading (`heading_topic`); states or a
   rotation dropped on it from the tree set those; the whole robot's motion from `ChassisSpeeds`
   topics (`speeds_topic`, `speeds_setpoint_topic`, suggested by the user 2026-11-20) is drawn from
   its center on the wheels' scale, with the spin arc behind the way it's going; the sim layout shows
   robot-2027's `/Drive/MeasuredStates` against `/Drive/SetpointStates`, with `/Drive/MeasuredSpeeds`
   against `/Drive/CommandedSpeeds`). Mechanisms: WPILib's Mechanism2d first (**done**: `builtin/mechanism.rs`, read by
   `types/mechanism.rs`; bound to the `Mechanism2d` table, whose topics it all asks for so ligaments added
   later show; y up at one scale for x and y, the canvas's color filling the card, zoomed to the part of
   the canvas the mechanism has used (roots and ligament ends, a margin round them; it only grows, so the
   view doesn't jump, and a double-click starts it over; the user found the whole-canvas fit left tall or
   thin cards mostly empty, 2026-11-21) or, with `fit` `canvas`, all of `dims`; each ligament in its color
   with its weight scaled to the card; the sim layout's Widgets tab shows robot-2027's `DashTest/Lift`, an
   elevator with a swinging arm); it only chains rigid ligaments, so
   later raptaboard may get its own mechanism data model and tools to build and preview one.
   3D Pose (**done**, asked for by the user 2026-11-20; `builtin/pose3d.rs`, read by `types/pose3d.rs`):
   a `Pose3d`, `Transform3d`, `Rotation3d` or `Quaternion` (or a `Pose2d`, flat) as X, Y and Z axis
   arrows (red, green, blue) turned by its rotation over faint world axes, with a box turning with
   them, and its place and roll, pitch and yaw below (hidden on a small card). Dragging the card
   orbits the view (from where the press began), a double-click resets it; the view is kept per
   widget while the program runs. A Rotation3d, Transform3d or Quaternion dragged from the tree makes
   one (a Pose3d still makes a Field; a Gyro still shows a Rotation3d's yaw). robot-2027's
   `TumblePose` (pitching and rolling) is on the sim layout's Field tab, below the Gyros.
   Whole structs dragged from the tree: a generic struct widget (**done**: Struct, `builtin/struct_view.rs`,
   last in `TYPES` so a struct another widget is made for makes that one; its fields like the tree shows
   them, from `types::structs::decode`: nested structs indented under their kind, struct arrays as
   `[0]`, `[1]`, ..., enum names; it scrolls; "waiting for the schema" until robot code sends one; the
   sim layout's Widgets tab shows robot-2027's `CyclingMode`) for any struct. Decided with the user
   2026-11-21: every whole struct dragged from the tree makes a Struct (structs are as often debugging
   or telemetry as something to draw, and a Field is usually one per dashboard); the Field, Gyro, 3D
   Pose, swerve view and Chassis Speeds come from the tree's right-click "Add as ▸" (they `show` their
   types but don't `accept` them; tables such as `Field2d`, `Gyro`, `SwerveDrive` and `Mechanism2d`
   still make their widget). A struct inside a struct (a Pose2d's `translation`), an item of a struct
   array (`[1]`) and an array field of structs drag out on their own too (the user's ask, 2026-11-21),
   saved as a `struct_meta` path with the inner struct's `data_type`; the widget gets the inner struct's
   bytes (`types::structs::nested_struct`), and widgets that read their topic's newest value use
   `Live::newest`, which gives the part for a `struct_meta` widget.
   Milestone 4 plugins may also match a struct type (a `struct = "ArmState"` rule).
   First release (decided with the user 2026-11-21): after milestone 3, a polish phase ending in
   release 0.1.0, "basics done". In it: drag tabs to reorder (done), a bigger worst-alert sign, a
   Battery widget, Boolean Box text and images, color by value, autoscale for bars and gauges,
   min/max/average on number widgets and graphs, graph history review, widget type choices that
   fit, copy settings within a list, and (added 2026-11-22) the connection settings behind a gear and
   the Robot State widget with its sounds. Then the release: version 0.1.0, a changelog, a tag, and a
   CI workflow building Windows, macOS and Linux binaries for a GitHub Release. After it: the window border by robot state and the
   e-stop pop-up, the Group widget, X/Y and polar graphs, partial overlap, reflow while dragging, the text style
   dialog, regex alert filters, and plugins.
4. Plugins v1: wasmtime host, limits, hot reload, the arm plugin, plugin template and SDK crate.
5. Robot hooks and the rest: ElasticLib hooks, layout download, remaining widgets, units, MJPEG
   cameras (Mechanism2d was done in milestone 3).
   Low priority, after the other features (asked for by the user 2026-11-20), both for the Field:
   - Path files as layers: load a PathWeaver `.wpilib.json` trajectory (or a PathPlanner `.path`)
     straight into a path layer, so planned paths show without robot code publishing them. Today
     a trajectory shows only once robot code publishes it (`Field2d.getObject(..).setTrajectory`, or
     a `Pose2d[]` such as PathPlanner's `/PathPlanner/activePath`).
   - Limelight's `botpose` arrays: a layer reading for its `double[]` (`botpose_wpiblue` and the
     like: x, y, z, roll, pitch, yaw, latency, ...). Today a `double[]` is read as x, y, degrees
     triples, which draws a botpose wrong; until then robot code republishes it as a `Pose2d` or
     `Pose3d`.
6. Competition hardening: DS docking, WHEP cameras, crash reports, Windows installer, plugins from
   the robot.
