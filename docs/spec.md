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
| Plugin design | API 0.3 (0.2 on 2026-11-23): one instance per widget on its own thread, values pushed, shapes and accessible regions out; then tabs, pixels, audio, 3D scenes and UI elements ("Plugin system"). API 0.1 was a placeholder and is dropped | 2026-11-23 |
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
- Address modes: Driver Station, team number (10.TE.AM.2), SystemCore mDNS (`robot.local`),
  SystemCore WiFi (`172.30.0.1`), localhost, custom. Driver Station (`driver_station.rs`, built
  2026-11-26 from Elastic's `ds_interop.dart`; not yet tried with a real Driver Station) listens on
  this computer to both the 2027 Driver Station's websocket, `ws://127.0.0.1:6768/ipws`
  (`{"robotIp": "10.32.0.2", "dockedHeight": …}`, `"0.0.0.0"` while unknown), and the NI Driver
  Station's TCP port 1742 (`{"robotIP": <u32, big-endian>}`, 0 while unknown; what Elastic read
  before May 2026, and WPILib's own client reads), each retried every second, only while the mode
  is chosen; the newest address either gave is used, and kept if the Driver Station closes.
  Resizing above a docked Driver Station (`dockedHeight`) isn't done yet.
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
| `font_family` | `shared/text_style.rs` | Text Display, Boolean Box text, Struct values | `"sans"`, `"mono"`, `"serif"`, `"narrow"` or `"digital"` | the widget's own (Text Display and Struct Mono, Boolean Box Sans) |
| `font_color` | `shared/text_style.rs` | Text Display, Struct values | ARGB color; beats a color-by-value background's black or white | the theme's |
| `font_bold`, `font_italic` | `shared/text_style.rs` | Text Display, Boolean Box text, Struct values | true or false | false |
| `title_font_family`, `title_color`, `title_bold`, `title_italic` | `shared/text_style.rs` | all, List Layouts too | as the value text's | Sans, the theme's color, plain |
| `title_font_size` | `shared/text_style.rs` | all, List Layouts too | points, 8–20 (the title strip's height) | 14 |
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
| `autoscale` | `shared/scale.rs` | the same (not a gauge with `wrap_value`) | the scale runs from the lowest to the highest value seen (every value received, from the app's start or the right-click menu's Reset scale; kept while the tab is closed, nothing recorded then), rounded out to round numbers; never saved as the range | false |
| `show_stats` | `shared/stats.rs` | Text Display (numbers), Number Bar, Voltage View, Radial Gauge | a line under the value with the min, max and average of every value received (short peaks count; each value counts once in the average) | false |
| `stats_window` | `shared/stats.rs` | the same | the statistics cover the last this many seconds (0.5–600); not set: since the last reset (the app's start, the server reconnecting, or Reset min/max in the right-click menu) | not set |
| `autoscale_keep_range` | `shared/scale.rs` | the same | with `autoscale`, `min_value` to `max_value` stays on the scale and only an end that's passed moves out | false |
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
| `show_stats` | `builtin/graph.rs` (key from `shared/stats.rs`) | Graph | each line's min, max and average, bottom left in its color: over the time shown, or between the A and B cursors once both are placed; from every value recorded, not the thinned lines | false |
| `history_kept` | `builtin/graph.rs` | Graph | seconds of values recorded, to look back through while paused (1–600; never less than `time_displayed`) | 60 |
| `label_inside` | `list_layout.rs` | List Layout | `true`: each label drawn over its value (on a see-through backing) instead of beside it | off |
| `dividers` | `list_layout.rs` | List Layout | `true`: a thin line under the list's title and below each child | off |
| `hidden` | `list_layout.rs` | a List Layout's child | `true`: kept in the list but not shown or subscribed | shown |
| `volume` | `builtin/fms_info.rs` | FMSInfo | its state sounds' loudness, 0–1 | 0.8 |
| `pairs` | `builtin/xy_graph.rs` | X/Y Graph, Polar Graph | the pairs shown: `[{"x": source, "y": source, "color"?, "name"?}]`, each source `{"topic", "data_type", "struct_meta"?}` as a Graph's lines; a Polar Graph's x is the angle and y the distance | the widget's own topic as the first pair |
| `trail_time` | `builtin/xy_graph.rs` | X/Y Graph, Polar Graph | seconds of trail behind each pair's newest point (0–60; 0: the point only) | 3 |
| `x_min`, `x_max`, `y_min`, `y_max` | `builtin/xy_graph.rs` | X/Y Graph | an axis' fixed range (both ends set), else it fits the values | fits |
| `same_scale` | `builtin/xy_graph.rs` | X/Y Graph | one unit as long across as up (positions on the floor) | off |
| `max_distance` | `builtin/xy_graph.rs` | Polar Graph | the outer ring's distance | fits the values |
| `angle_unit` | `builtin/xy_graph.rs` | Polar Graph | `"degrees"` or `"radians"` | degrees |
| `zero_at` | `builtin/xy_graph.rs` | Polar Graph | where angle 0 points: `"right"`, `"up"`, `"left"` or `"down"` | right |
| `clockwise` | `builtin/xy_graph.rs` | Polar Graph | angles grow clockwise | off (anticlockwise, as WPILib) |
| `pinned` | `grid.rs` | all, List Layouts and Groups too | `true`: never pushed aside or packed (still dragged by hand) | off |
| `may_overlap` | `crates/layout/src/overlap.rs` | all, List Layouts and Groups too | `true`: may overlap others (set by an Alt drop over them; removed when it overlaps nothing) | off |
| `z` | `crates/layout/src/overlap.rs` | all, List Layouts and Groups too | its place front to back, higher in front (Bring to front, Send to back) | 0 |
| `rows` | `group.rs` | Group | the inner grid's rows, which set its square cells' size (1–24; columns: as many as fit) | 2 |
| `grid_lines` | `group.rs` | Group | `false`: its cells' faint lines hidden | shown |
| `shared` | `group.rs` | Group, List Layout | settings for the widgets inside: any of `period`, `font_size`, the value text's style keys and the title's (`group::SHARED_KEYS`; a List Layout's title keys style its labels) | none |
| `shared_on` | `group.rs` | Group, List Layout | `true`: `shared` replaces those settings of every widget inside (the usual where `shared` leaves one unset) | off |
| `use_shared` | `group.rs` | a Group's or List Layout's child | `false`: the widget keeps its own settings while `shared_on` is set | uses them |

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
period) and "Reset settings". Plugin widgets implement the same trait through an
adapter ("Plugin system"): WIT `render` shapes drawn with egui, settings rows from `plugin.toml`.

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

Planned with the user 2026-11-23, when plugin work started. Until then `wit/dashboard.wit` and
`plugin.toml` were placeholders, so API 0.1 was dropped rather than kept working. Plugins remain WebAssembly
components run by wasmtime (no dynamic libraries, no scripting). The aim is that anyone can write a
widget outside this repo, install it at runtime, and have it look and behave like a built-in one.

### What a plugin is

A folder with `plugin.toml` and `widget.wasm` (a component exporting a world from
`wit/dashboard.wit`), plus any files it reads (images, models, a game's data). It reaches users in a
content pack (`[[plugin]]`, signed like sounds), or during development from a local folder that reloads
when the `.wasm` changes. `plugin.toml` gives:

1. Name, version, the host API version (`api = "0.2"`).
2. Match rules: for a plugin bound to a table, `type` (the table's `.type`), `path` (glob) and `has`
   (required child keys). All the rules given must pass; a `.type` match is the most specific. A plugin
   bound to one topic gives `topic_types` (NT types, like `["double", "int"]`) instead, and subscribes to
   it as `bound`. Implemented in `crates/plugin-host`.
3. Size in grid cells (default and minimum).
4. Properties: the edit dialog is made from them and they're saved in the layout. The types are `number`,
   `bool`, `string`, `color`, `enum`, plus `sound` (the shared sound picker) and, later, `table`
   (bind another table).
5. Permissions: `publish` (write NT values, only under the bound table), `sound` (`play-sound`) and
   `background` (keep getting values, and so set its tab's mark, while its tab is closed). Packs
   show a plugin's permissions before installing, as they show licenses, in the window and in
   `raptaboard packs install`.

A plugin widget is saved in the layout as type `plugin:<name>` with its properties. With the plugin
missing, the widget shows a "missing plugin: <name>" card and keeps its properties. Installing a
plugin pack while the dashboard runs makes it available at once, in Add as and the widget type
choices, without a restart.

### How it runs (API 0.3: grid widgets and tabs)

The interface is `wit/dashboard.wit`. One instance runs per widget, each on its own worker thread.
The window sends it values and input and draws the newest picture it has finished, so it never waits
on a plugin.

- `subscriptions()`: which values to push: keys under the bound table, every key under it, or every
  table on the server with a given `.type` (the Alerts widget's `all_groups`). Asked again when a
  property changes.
- `update(samples)`: every change since the last call, oldest first, with the server's time. A graph
  plugin misses no points between frames.
- `render(frame)`: the size, time, connection state, lock and theme colors in. Out come shapes (line,
  polyline, arc, rect, circle, text that can wrap, images from the plugin's folder) and *regions*. A
  region is a clickable part with a label and role, which the dashboard turns into an accessible
  control (screen readers, and the kittest tests find it by label). It also returns `redraw-in-s` for
  animations and pulses, and a screen-reader summary.
- `tab-mark()`: the alert level for the mark on its tab.
- `on-input(event)`: a region activated, pointer down/up/move, wheel, focus, key down/up and text.
  Keys go to a widget only while it has focus (a click into it; Escape or a click elsewhere ends
  it), and the dashboard's own shortcuts are off meanwhile.
- Host calls: `get`, `unit`, `publish`, `property`, `measure-text`, `play-sound` (through
  raptaboard's own sounds, honoring mute) and `log`.
- Limits (100 ms a call and 64 MiB by default, `runtime::Limits`): wasmtime epoch interruption gives each call a time budget (better than fuel for work that
  must keep a frame rate), and each instance has a memory cap. WASI gives clocks and random only, no
  files or network. A plugin that traps or runs over shows a "plugin stopped" card with Restart, and
  the rest of the dashboard keeps going.
- In the app, an adapter implements `widgets::WidgetType` for every plugin, so the grid, the settings
  dialog, tab marks and `background_topics` treat it like a built-in.
- For authors: a `raptaboard-plugin` Rust crate (wit-bindgen bindings and helpers, target
  `wasm32-wasip2`) and a template. Other languages can build components too, but only Rust is
  supported.

### Developing and testing plugins

Planned with the user 2026-11-23: authors shouldn't need a robot or the sim to see a plugin work.

- **Test harness** (`raptaboard-plugin-test`, `crates/plugin-test`; done 2026-11-24 apart from
  timed scripts and PNG snapshots, which come with the sandbox window's renderer): loads a built `widget.wasm` in the real
  host runtime and limits, for `cargo test`. A test sets values (one at a time or a timed script),
  sizes, the connection state and the theme, and sends input (`activate("bell")`, keys). It then
  checks what came back: shapes, regions found by label, the tab mark, redraw requests, sounds asked
  for and log lines, and optionally a rendered PNG snapshot. The Number and Alerts example plugins
  (raptaboard-examples) are checked against the built-ins with it.
- **Sandbox window** (`raptaboard plugin dev <folder>`; started 2026-11-25: the server, the plugin's
  `[sandbox]` sample values, the values panel (type, add, remove), size presets and the theme,
  connected and locked switches, and `--address` with `--topic`; the dev overlay 2026-11-25: regions
  outlined with their labels, each kind of call's last and longest time and the memory against the
  limits (`runtime::Usage`), the plugin's last 200 log lines, why it stopped, and Restart; still to
  come: sliders and scripted sequences): the dashboard with its own NT server inside
  (the in-process `nt4::Server` the UI tests use) instead of a robot. The plugin from the folder is
  placed on a tab and reloads when the `.wasm` is rebuilt. A values panel types values, drags
  sliders, adds and removes topics, and plays scripted sequences (later, a `.wpilog` replay). A dev
  overlay shows the regions' outlines, time and memory per call against the limits, the plugin's
  log and any trap with Restart, with switches for theme, connected and locked and preset widget
  sizes. `--address` points it at the sim or a robot instead. It doubles as a demo of a plugin
  without a robot.

### Order

1. **Runtime and the two example plugins** (started 2026-11-23: `crates/plugin-host` `runtime.rs` runs
   plugins with the limits, the `plugins/` workspace has the SDK crate and the Number plugin, and
   plugin-host's tests run them; the app runs plugins loaded with `--plugin <folder>`, reloading a
   changed `.wasm`, through `crates/app/src/plugins/`; the Alerts plugin (2026-11-24) draws what the
   built-in does, and `ui/tests/plugins.rs` sets both beside their built-ins; plugins load from
   installed packs (2026-11-24, see "Content packs"); next the `example-widgets` pack in
   raptaboard-content and the test harness (with tests for both plugins in their `tests/`); the
   sandbox window and its dev overlay (2026-11-25); next Doom. Not yet: images in pictures, focus and keys, struct values). The wasmtime host with the limits above, API 0.2,
   the adapter, loading from packs and from a local folder, the SDK crate and the test harness. Then two plugins in
   `plugins/` (built in CI, shipped as an `example-widgets` pack in raptaboard-content), each set beside
   its built-in to show plugins can match them:
   - **Number**: a Text Display for a number (decimals, unit, font size or fit, color by value).
   - **Alerts**: the built-in Alerts widget (`builtin/alerts.rs`): the circle with its three
     signs and counts, the text list beside or below, `all_groups`, the pulse on a new error, its
     tone (a `sound` property, muted by the top bar's bell like the built-in's), greyed when
     disconnected, and the tab mark, also while its tab is closed. It reads the same layout properties.

   The arm plugin (`plugins/arm`) stays as the manifest fixture.

   Then the sandbox window, before Doom (whose keys, ticks and sound need a live window most).
2. **Doom**, the easter egg (doomMod, `tuffinmuffin/raptaboard-doom`): API additions for a plugin
   that fills a tab (a manifest can claim a tab title, `tab = "Doom"`, so a tab of that name
   becomes the game when the pack is installed), an RGBA pixel frame drawn as a scaled texture,
   `tick(dt)` at a set rate (35 Hz), an audio stream (PCM into the mixer, silent while the tab is
   hidden), and read-only files from the plugin's pack (the WAD). Hidden tabs don't tick, so the
   game pauses. Without the pack, a tab named "Doom" stays a plain tab: nothing hints at the easter
   egg or offers the pack (decided with the user 2026-11-25). The engine isn't settled: doomMod's
   room4doom build was a proof of concept for making the WAD, and another engine, library or
   method is fine (the user, 2026-11-25); the API is engine-neutral, so it comes first, tried with a
   small test plugin. API 0.3 (2026-11-25): a `pixels` shape (RGBA, sharp or smooth), `play-audio`
   (16-bit PCM queued, needs `sound`), the plugin's folder read-only at `/pack` (WASI; nothing else),
   and `tab = "…"` in plugin.toml (such a plugin isn't a widget). No `tick` export: a game asks for
   `redraw-in-s` 1/35 and steps in `render`, and a closed tab isn't drawn, so it pauses.
   `plugins/tab-test` uses all of it. The app's side (2026-11-25): an empty tab whose name a plugin
   claims (any case) shows it at the tab's size (`ui/plugin_tab.rs`, `PluginType::show_tab`); clicked,
   it takes the keyboard (Tab, arrows and Escape too), the dashboard's shortcuts are off until
   Shift+Escape (asked for by the user; a hint says so for 3 s) or a click elsewhere; a plugin with
   `capture_mouse = true` also holds the mouse meanwhile (cursor hidden and kept still, raw movement as
   `mouse-motion`, every button as pressed or let go with which button; let go while the window is in
   the background, taken again when it comes back); the plugin can give both back itself
   (`release-focus`: a game's Quit), and it gets keys (modifiers as keys of their own: "Ctrl", "Shift", "Alt",
   "Command"), text, the pointer and focus changes. Its worker runs while the tab exists (a closed
   tab isn't drawn, so a game pauses). `play-audio` streams into the sound thread's mixer
   (`sound::AudioOut`/`AudioStream`: a queue per plugin, silence when empty, half a second at most,
   off with the bell). The sandbox opens a tab plugin as its tab. Animations wake the window
   themselves (`render_if_needed`) and get their due time, so they keep their rate. Engine chosen
   (2026-11-25): room4doom (its core crates build for wasm32-wasip2 unchanged; MIT; it interpolates
   between steps, so it draws smoothly at the screen's rate); the plugin lives in raptaboard-doom
   (`plugin/`, `make plugin`), the standalone game kept working beside it. Running it showed the
   limits too tight for a game: the first `render` now may take `Limits::first_render_ms` (3 s;
   loading the WADs took 67 ms in a release build, about 200 ms in a debug one) and a plugin may ask
   for up to 512 MiB (`[limits] memory_mb` in plugin.toml; Doom runs at about 105 MiB, each frame
   about 1 ms) and longer calls, up to 1 s (`call_ms`: Doom loads a level whenever one ends, over
   100 ms). Shown before installing a pack, from the catalog's `needs`. Doom's sound is
   mixed in the plugin (effects and OPL2 music) and streamed through `play-audio`. Storage (in 0.3,
   asked for by the user 2026-11-25): `storage-get`/`-set`/`-remove`/`-keys`, small values a plugin
   keeps between runs (a setting, a CAD file's path; Doom's settings could use it), one set per
   plugin shared by its widgets, needing `[permissions] storage`; kept as a file per key in
   `~/.raptaboard/plugin-data/<plugin>/` (`plugin_host::store::Store`: keys are plain names, writes
   whole or not at all), 64 KiB in all unless `[limits] storage_kb` asks for more (at most 1 MiB).
   A writable `/data` folder for larger things (saved images, recordings) is the planned extension,
   not built yet. Next: the pack. Before 1.0 a plugin runs only on the same 0.x, so the
   content repo's `example-widgets` pack (0.2) is rebuilt for 0.3 before the next release. The
   example plugins moved to their own repo, raptaboard-examples (2026-11-25, asked for by the user),
   released there; `plugins/` keeps the SDK and test plugins (`topic-test` and `table-test` stand in
   for them in raptaboard's tests).
3. **Custom graphs**: binding several tables (the `table` property) and history from the host.
4. **3D scene and a robot viewer**: the host renders and the plugin describes a scene it keeps.
   Meshes come from the pack's GLB files, as a part tree whose transforms the plugin sets. There are
   simple objects (lines, spheres, boxes, cones and frusta, flat tags) with color, opacity and
   visibility per part, and the host's camera views (orbit, top-down, a camera's own view; more
   than one at once). Ray casts against the scene, picks and bounding boxes are queries the plugin
   can make. The reference is Robot-2026's `host/camera_visualizer` (Three.js: CAD part trees,
   AprilTag coverage with FOV cones and line-of-sight). Here it would run on live data: the robot
   publishes its camera and mechanism mounts as `Transform3d` topics, and the viewer shows the robot
   at its estimated pose, mechanisms moving, and which tags the cameras should see against which they
   do. One generic plugin reads a GLB and a mapping file (node `arm` turns by `/Arm/angle_rad`), so
   teams don't write code for their robot. GLBs are cleaned up when the pack is built (Draco decoded,
   draw calls merged per part, plain materials, units and Onshape's axes fixed), not at runtime.
   Check the AdvantageScope field models' license before shipping any in a pack.
5. **UI elements and a log viewer**: a set of elements the host draws with egui (a table with
   columns, sorting, a filter box, buttons, scrolling), asking the plugin only for the rows on
   screen. Plus a file the user picks, handed over as a handle, never a path. The first user is a
   spreadsheet of robot logs (`.wpilog`) to filter.
6. **Picking and measuring in the robot viewer** (points, transforms to copy), with a clipboard
   permission.

Each step adds to the WIT as a new minor version. The host keeps running plugins built for older
minor versions.

Open questions:
- Is a picked log file enough at first, or should the host fetch logs from the roboRIO (SFTP)?
- Is the generic robot-model plugin plus a mapping file the right shape for the CAD view?

## Content packs

Decided with the user 2026-11-23. Extra content (sound packs now, widget plugins and their data
later, such as the Doom field) comes from **content sources**, downloaded only when the user
chooses, never needed at an event: packs are installed beforehand and used from disk.

- **The official source** is the GitHub repo `tuffinmuffin/raptaboard-content` (not Google Drive:
  fixed URLs, versions and review). Each pack is a folder of files and a `pack.toml` (id, name,
  version, kind `sounds` or `plugin`, `min_app`, summary, one-line license, full credits text,
  `suggested`, and its sounds). A `vN` tag there publishes a GitHub release with every file and
  `catalog.json` (each pack with its files' URLs, sizes and SHA-256), signed with Ed25519
  (`catalog.json.sig`, the `CONTENT_SIGNING_KEY` secret; the public key is `catalog-key.pub`
  there and `content::OFFICIAL_KEYS` here). raptaboard reads
  `releases/latest/download/catalog.json` and refuses it unless the signature checks. If the
  latest release has none (a content release that failed or is still publishing), it lists the
  releases (GitHub's API) and uses the newest with a catalog, saying so in the Content window;
  if none works, it says why. Offline, it stops at the first error. A `vN-pre.M` tag publishes a
  GitHub pre-release, which `latest` passes over: with the setting `content_prereleases` (the
  Content window's "Pre-releases" beside the official source, or `raptaboard packs … --prereleases`
  for one command) raptaboard lists the releases and reads the newest with a catalog, a
  pre-release or not, signed the same way, noting a pre-release's tag; if the list can't be had
  (GitHub's API allows 60 an hour unsigned in), the released catalog, saying so. Packs
  that aren't fully open source (the IT Crowd clips) live there too, saying so in their license.
- **What a pack holds:** sections, mixed as needed, each naming its files: `[[sound]]` (name, file),
  `[[image]]`, `[[field]]`, `[[plugin]]` and `[[layout]]` (file, optional name), plus `files` for
  data the sections read (a plugin's WADs). Sounds and plugins are used so far; images (Boolean Box),
  fields (WPILib field JSONs) and layouts (to start a new layout from, or
  add a tab from; later perhaps tabs a plugin provides) come later. The
  single `kind` older catalogs had is optional and only a label.
- **Plugins in packs** (done 2026-11-24): packs hold files only, so a `[[plugin]]` names its
  `.wasm` (`alerts.wasm`) and its manifest, raptaboard's `plugin.toml`, sits beside it named for it
  (`alerts.toml`). The content repo's `build_catalog.py` (and `pack_folder.rs` for `--package`)
  reads the manifest and puts the plugin's name, the permissions it asks for and its needs (`needs`:
  the tab it fills, `capture_mouse`, and its `[limits]`; `content::PluginNeeds`) in the catalog
  (`content::PackPlugin`; older raptaboards ignore them). The Content window, the first-run list and
  `raptaboard packs install` show them before installing ("Plugin alerts: plays sounds, runs while
  its tab is closed"; "Plugin doom: fills the Doom tab, holds the mouse while its tab has the
  keyboard, plays sounds, saves its settings on this computer, uses up to 192 MB of memory, may work
  up to 500 ms at a time"), and a plugin whose manifest asks for more than its catalog entry said
  isn't loaded. A catalog from before needs (content v4 and older) gave none, so they aren't checked
  for packs it installed. The window loads the installed packs' plugins at its first frame and after each install
  or remove (`plugins::use_packs`, `Content::plugins_changed`): where two packs have a plugin of one
  name, the newest install's; a `--plugin` folder of that name wins over both. A removed pack's
  plugin is dropped, and its widgets show "Missing plugin: <name>" with their settings kept.
  A pack with plugins sets `min_app` to the first raptaboard that loads them.
- **Sound layering** (decided with the user 2026-11-23): a pack's sound may add a new name or use a
  built-in name to replace it. Each install records `installed_at`; where packs share a name, the
  newest install wins (`sound::SoundLayers`). A sound setting is `fire 1` (newest pack's, else the
  built-in one), `pack:<id>/fire 1` (that pack's, else built in) or `builtin:fire 1` (ignore packs);
  anything not found falls back to the built-in sound of that name, or none (`sound::SoundRef`).
  The sound pickers (done 2026-11-23; FMSInfo's state sounds and the Alerts tone,
  `widgets/shared/sound_pick.rs`, `sound::sound_choices`) list each built-in name, then each
  sound only packs have; under a name a pack has, indented, each pack's ("fire 1 (IT Crowd
  clips)", newest install first) and the built-in one ("fire 1 (built in)"). A saved sound whose
  pack is gone shows "fire 1 (fan-clips, not installed)" or "horn (not installed)" and a red
  "not installed", and plays the built-in sound of its name, or nothing (`sound::Clip::playable`;
  a pack's file that won't decode plays the built-in one too). In FMSInfo, a setting that's a
  path or ends in .mp3 or .wav is a file the team picked; anything else is a sound's name.
- **Other sources** are any catalog URL the user adds in the Content window (`content_sources` in
  the settings file). They aren't signed; the window says to add only sources you trust. Every
  file from any source is checked against its catalog's size and checksum.
- **Installed** in `~/.raptaboard/content/<id>/` with `pack.json` (the catalog entry and its
  source); an install goes into `<id>.partial` and moves into place only when every file checks.
  A sounds pack's sound with a built-in name (`fire 1`) plays in place of the built-in tone
  (`sound::SoundFiles`). `content.rs` (catalog, signature, store, the download thread),
  `ui/content_window.rs` (the window and the first-run list). Downloads use `ureq` with rustls
  and the computer's own certificates (school and team networks that add theirs work), with a
  5 s connect timeout so a network without internet fails fast and quietly.
- **First run and upgrades:** `content/state.json` keeps `last_version` (the raptaboard that last
  started), `offer_pending` and `shown` (the packs offered). The first start of a new version (or
  the very first; a pre-0.3.0 file counts as one) runs `upgrade.rs`'s one-time tasks, which set
  `offer_pending`: the official catalog is read and the packs not installed are offered, each with
  its license and credits, suggested ones not offered before ticked; nothing downloads without
  "Download". After Download the window stays, showing each pack downloading, then installed (with
  its sounds) or why not, until Done. Answering (Download or Not now) clears `offer_pending`; offline, it stays, and the
  list is offered at the next start. The same version again reads nothing at start. Not shown
  while the robot is enabled. `~/.raptaboard` outlives uninstalling, so a reinstall of the same
  version doesn't ask again.
- **Content window** (⚙ Settings → Content packs…): each pack with Install, Update or Remove and
  its credits, and a line saying what the last install or remove did (`content::Outcome`); packs needing a newer raptaboard say so; sources listed, added and removed.
- **Background check** (done 2026-11-23): `content_check_minutes` in the settings file (60 by
  default, 0 for never; ⚙ Settings → Content packs), first 30 s after starting, then that often,
  only while the robot isn't enabled and no list is showing (`content::CheckTimer`: a check due
  while enabled waits for the next quiet moment). Failures say nothing (the Content window shows
  why if opened). What it found (`Content::news`: updates to installed packs, and packs never
  offered) shows as "📦 Content: 1 update · Review · ✕" at the end of the bar's second row, hidden
  while the robot is enabled. Review opens the Content window; ✕ records each pack's version in
  `state.json`'s `dismissed`, so only something newer shows it again.
- **Next:** the `example-widgets` pack (Number and Alerts), then the Doom field. For now its
  `.wasm` files are built here and copied into raptaboard-content by hand. Later (decided with the
  user 2026-11-24): plugin sources stay in their own repos (the examples in this one's `plugins/`),
  whose CI builds each plugin as a release artifact, and a pack names where its plugin comes from
  (repo, tag, artifact) so the content repo's release workflow downloads and checks it
  instead of keeping built `.wasm` files. A repository manager (asked for by the user 2026-11-23; a later improvement, around 0.5 after
  plugins, since teams aren't expected to use other sources yet): sources saved in
  `~/.raptaboard/content/sources.json` rather than `settings.toml`, each with a name, on or off,
  an optional signing key (so a team's own source can be signed), and when it was last checked
  and how; a Repositories section in the Content window to add, rename, turn off, check and
  remove them, with each one's packs and errors (the official source first, never removed).
  The command line could then add and remove sources too (`raptaboard packs sources` only lists
  them for now).
- **Command line** (done 2026-11-23; `packs_cli.rs`): `raptaboard packs list | install <id>… |
  update [<id>…] | remove <id>… | sources` works without opening the window, on the same packs
  folder and checks, and exits non-zero if anything wasn't done. `install` installs or reinstalls
  what the source has; `update` only a newer version. Packs come from the settings' sources, or
  only from `--url <catalog>` or `--package <folder>` (a pack folder as the content repo keeps it,
  checked as its `build_catalog.py` does: `pack_folder.rs`). `--package` and any `--url` but the
  official catalog need `--unsigned`; the official catalog's signature is always checked, and so
  are HTTPS certificates. Each pack's license and credits are shown and the user asked first,
  unless `--yes`. It reads sources (`content::Source::read`), picks a pack from the first source
  that has it (`content::offers`) and words what it did (`content::Outcome::describe`) exactly as
  the Content window does, sharing that code. A running raptaboard sees the change when it next starts. A Windows release
  build (a window program) joins the terminal it was started from to print, but can't ask (the
  terminal doesn't wait for it and keeps reading the keyboard), so it needs `--yes` to install,
  and its output lands after the next prompt. So Windows packages also ship **`raptaboard-cli.exe`**
  (2026-11-23; `src/bin/raptaboard-cli.rs`): the same commands (`raptaboard-cli packs list`) as a
  console program, which a terminal waits for, so it prints in order and asks. Both programs run
  `packs_cli::run_in_home`. The Store's MSIX leaves it out (2026-11-23: Partner Center refused a
  second, hidden application as a "headless app"; Store users get it from GitHub, or use
  `raptaboard packs` with `--yes`); the installer adds its folder to this user's PATH (a task, ticked by default; uninstalling
  takes it off); the zip just has it. macOS and Linux packages ship it too (2026-11-23), so the
  command is the same everywhere, though `raptaboard packs` works in a terminal there: macOS inside
  the app (`raptaboard.app/Contents/MacOS/raptaboard-cli`, universal like `raptaboard`, signed with
  it; the README shows a symlink to type it by name), Linux beside `raptaboard` in the `.tar.gz`,
  and `install.sh` puts it in `~/.local/bin` (and `--uninstall` removes it). `scripts/package.py`
  builds both programs everywhere (`PROGRAMS`) and stops if either is missing from a package.

## Documentation

Planned with the user 2026-11-25, for the next release (after the pack pipeline chores: Doom's
releases, the content repo's artifacts from other repos and its two catalogs, the pre-release
source setting, and plugins' needs shown before installing, so the pages describe what's there). Changed 2026-11-26: 0.4.0 is released once
the first three pull requests are in (the book publishes from main, apart from releases), and the
rest follow.

**Releases from 0.4.0** (decided with the user 2026-11-26): 0.4.0 goes on GitHub only; the
Microsoft Store stays at 0.3.0. 0.5.0 is the GUI updates, and goes to the Store too: the state
raptaboard is meant to rest in, apart from feature requests.

- **Where:** an mdBook in `book/`, published to GitHub Pages by CI on main; pull requests check it
  builds and its links and images resolve. The README shrinks to what raptaboard is, install and
  links into the book (its status line, still "0.1.0" and "Plugins come next", is fixed then).
  Tutorial code is included from real, tested files, so it can't drift.
- **Screenshots:** `record_docs_screenshots` (`ui/tests/clips.rs`), an ignored test like
  `record_store_screenshots`, drives the app headless against robot-2027's sim (decided with the user
  2026-11-26, instead of a scripted robot: the sim's data is richer, and it's running while the
  screenshots are made) showing `layouts/robot2027_sim.json`. Each tab with widgets is saved whole,
  the window sized to fit it, to `book/src/images/tabs/<tab>.png`, the first card of each widget
  type cut out to `book/src/images/widgets/<type>.png`, and the window unlocked with the tree to
  `book/src/images/window/editing.png` (27 widget types; Multi Color View isn't in the layout yet).
  Run before each release (it needs a GPU and the sim) and commit them. CI only checks every image
  the book uses exists. The sim layout and robot-2027's `raptaboard-test` test topics change as the
  pages need.
- **User guide:** install and first start (each system's unsigned-build prompt, the first-start
  packs); connecting (team, Driver Station, SystemCore, sim, address); a tour of the window
  (tabs, tree, settings bar, lock, status); adding and arranging widgets (drag from the tree, Add
  as, the edit dialog, List Layouts, undo); layouts (Elastic files and what raptaboard adds); a
  page per widget with a screenshot and its settings (the settings tables generated from the
  widget registry); Field, Alerts and FMSInfo in depth; content packs and plugins (sources,
  released and pre-release, what each permission and limit means); troubleshooting.
- **Developer guide**, for both kinds of developer the user named:
  - *Core developers* (in raptaboard): the repo and its architecture (crates; values from NT to
    the app's state to widgets), and a tutorial **adding a built-in widget** step by step: the
    type and its registration, its settings, drawing, a kittest test and its screenshot. Then
    testing, coverage and releasing.
  - *External developers* (in their own repo): the plugin structure (the WIT world and its
    calls, a `plugin.toml` reference: match, tab, properties, permissions, limits, storage,
    sandbox; API versions), and a tutorial **writing a plugin**: a new crate on the SDK, a small
    table widget, harness tests, the sandbox, then releasing it from its repo's CI and adding it
    to a pack. Its code is a `tutorial` plugin in raptaboard-examples, a step per folder or
    commit, so every step builds and passes its tests. Then tab plugins (Doom as the case study:
    pixel frames, input and focus, audio, storage) and content packs (`pack.toml`, artifacts
    from other repos, the two signed catalogs).
- **Order**, a pull request each (**all done** for 0.5.0, the Layouts chapter last on 2026-11-28;
  **the first three done** 2026-11-26: the screenshot recorder second, then First start, Connecting and The window, which found the Driver Station address mode wasn't built (it is now: #115, as Elastic reads the Driver Station; not yet tried with a real one); `book/` with every planned page in
  `SUMMARY.md`, the ones still to write as drafts; the README's sections moved in; its images in
  `book/src/images/`; `scripts/check_book.py` for links, images and chapters; `book.yml` building
  it on pull requests and publishing it from main): the book's scaffold, Pages workflow and README; the screenshot
  recorder; the user guide's first pages; the widget reference and in-depth pages; the
  architecture and built-in widget tutorial; the plugin reference with the tutorial plugin and
  its walkthrough; tab plugins, content packs, testing and releasing.

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
   - Text style (**done** for 0.5.0, 2026-11-26, `shared/text_style.rs`): the settings dialog's "Text"
     row (size and a "Style…" button showing rows under it: a font dropdown with each name in its
     own font, color, bold, italic, Reset style; rows, not a pop-up, as egui closes a pop-up when its color picker or a list opens) for the value text of Text Display,
     Boolean Box (no color: its true and false text colors are its own) and Struct, and a "Title
     text" row (size, Style…) for every widget's and List Layout's title. Fonts are Sans (egui's
     Ubuntu Light), Mono (Hack), Serif (PT Serif), Narrow (PT Sans Narrow) and Digital (DSEG7
     Classic, seven-segment), the same on every computer; all but egui's two regular ones are
     `media/fonts/` files (added to egui as their own families with egui's fonts after them for
     missing characters, `install_fonts`; drawn in egui's own for the one frame before they're in); italic is egui's slant. "Reset settings" clears both styles.
     Not yet: the choosers' and enum widgets' text (egui's own combo boxes and buttons, which can't
     slant), the number widgets' value text, and a layout-wide default style.
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
     Locked, the grid doesn't scroll and the bars hide, so a stray swipe while driving can't move
     the view (asked for by the user 2026-11-22); it stays where it was, and unlocking scrolls again.
   - Reflow while dragging (**done** for 0.5.0, 2026-11-27, `grid::reflow`): widgets in the way slide
     aside (to the nearest free spot) as a widget or List Layout is dragged or resized over them,
     instead of the drop being refused. While dragging they're drawn where they'd go, worked out
     from where everything was when the drag began (so moving on lets them slide back); bigger ones
     are placed first; the drop moves them all in one undo step and the status says how many moved.
     Refused (red outline) only when one has nowhere to go. Topics dropped from the tree and list
     children dragged out onto the grid make room the same way (`grid::make_room`; for several
     topics, room for the first, the rest around it). Escape calls any drag off, putting everything
     back (asked for by the user 2026-11-27).
   - Fill space, Pack widgets and Pin in place (**done** for 0.5.0, asked for by the user
     2026-11-27, `grid::fill_space`, `grid::pack`, `grid::PINNED`): a widget's or list's right-click
     menu has **Fill space** (it grows to the biggest rectangle of empty, fully shown cells holding
     its own; ties keep its corner) and **Pin in place**; the empty grid's has **Pack widgets** (in
     reading order each item slides up, then left, a cell at a time, until nothing moves: sizes and
     arrangement kept). A pinned item (`pinned: true` in its properties, raptaboard's; also a
     checkbox in its settings dialog, not for list children) is never pushed aside (a drop that
     would push it is refused) or packed, but still moves when dragged itself; unlocked, a pin
     shows in its top-left corner. Each is one undo step, said in the status line.
   - Partial overlap (**done** for 0.5.0, 2026-11-27; decided with the user to use a modifier
     instead of settings, reflow staying the default): holding **Alt** (Option on a Mac) while
     dragging or resizing a widget or list places it over others instead of pushing them aside,
     in front of everything (`grid::fits_over`, `grid::place_over`): it gets raptaboard's
     `may_overlap: true` and a front `z`, which `grid::tidy_overlaps` takes off again once it
     overlaps nothing. Partial only: no widget may be completely covered, alone or with others
     (`raptaboard_layout::overlap`, exact, cutting along every edge), so every widget keeps some
     part visible to see and grab; a 1x1 on a 1x2 is fine, two 1x1s on one cell or a big one over a
     smaller one are not (refused, red outline, "It would hide a widget completely there"). Drawn,
     clicked and hit-tested back to front by `z`, then widgets before lists, in file order
     (`grid::draw_order`); a widget overlapping another has **Bring to front** and **Send to back**
     in its right-click menu (refused if it would hide one). Opening a layout keeps overlaps the
     rules allow and fixes the rest as before. Plain drags still push aside (and say "hold Alt to
     overlap instead"); Pack widgets never piles things up. With Alt held a widget never goes
     into a List Layout (nor will it into a Group): it goes over it (asked for by the user
     2026-11-27). The same for topics dropped from the tree and list or group children dragged
     out (`grid::fits_new_over` checks one not on the grid yet; with Alt they never go into a
     list, group or graph).
   - Boolean Box text (**done**): `text_backing`, a see-through box behind the text so it reads over
     any color, and `true_text_color` / `false_text_color` (automatic black or white if not set; white
     on the backing).
   - Group widget (**done** for 0.5.0, 2026-11-27, `widgets/group.rs`): a card holding its
     own small grid, so widgets inside can sit side by side and be any size (a List Layout only
     stacks them). Saved in a tab's `layouts` with type `"Group"` (raptaboard-only; Elastic would
     show a placeholder), `rows` (2 unless set) and `grid_lines` (on unless set). Decided with the user
     2026-11-27: the inner cells scale with the group, and text scales too; after trying it, the
     user found square cells in a fixed columns-by-rows grid left unusable space, so the cells stay
     square, `rows` sets their size (the body's height over rows) and as many columns as fit go
     across (`group::cells`; never fewer than the widgets inside need), and the inner grid's lines
     show unless `grid_lines` is off. Children are saved at
     `group::INNER_CELL` (128) pixels a cell whatever the tab's grid size, so the grid rules work
     inside unchanged (`group::inner_grid`); they're drawn at full size on an egui `Area` layer of
     their own, a sublayer of the grid's, scaled onto the card by a layer transform
     (`group::Mapping`), which scales input too (locked, the layer takes input: toggles, sliders;
     unlocked, it doesn't, and the grid moves and resizes the children). Right-click empty grid →
     New Group (`grid::add_group`); widgets dragged onto it, list or group children dragged onto
     it, and tree topics dropped on it go in the inner cell under the pointer, making room
     (`group::place`, `grid::widget_into_group`, `child_into_group`, `widgets_into_group`); its
     children move and resize inside (the list-child drag with a `resize` mode,
     `grid::resize_in_group`) and drag out onto the grid or into a list; their menu has Edit,
     Edit group and Remove from group. Groups overlap like anything else with Alt (decided with
     the user 2026-11-27: a list and a group should behave alike; at first nothing could overlap a
     group, as its widgets' layer sat above the whole grid). The grid draws front to back
     (`grid::draw_order`): a group's card where it falls, its widgets on a layer of its own, and
     whatever comes after it on a new layer above that (`DashboardApp::draw_items`); those layers are
     sublayers of the grid's, named by their place in the stack, and each asks egui to be on top
     every frame so none passes the others; outlines go on the topmost. Drag previews are drawn
     on a layer above them all. Part 2 (done): while `shared_on` is set (off unless
     set, "Apply to the widgets inside"), the group sets every one of `group::SHARED_KEYS` for its
     children in place of their own (`group::with_shared`, used wherever a child is drawn,
     subscribed or recorded): the update period, the value text's size and style, and the title's,
     each from `shared` (a separate object, as the group's own title uses the same keys) or at its
     usual where `shared` leaves it unset. Decided with the user 2026-11-27, after two other
     versions: first a child's own setting won (a styled widget then seemed to ignore its group),
     then the group's won only for what it set (a widget could still change the rest, breaking
     the group's say). Off by default so opening a layout never takes widgets' own styles away; the
     child's own values are kept, for when it's off or the child opts out. Set in the group's settings under "Widgets
     inside"; a child's own dialog says what the group sets (`group::shared_note`) and greys those
     rows out (`group::controlled_keys`, passed to the rows by `props::set_controlled` /
     `is_controlled` while the dialog draws). Opening a layout checks a group's children (`ElasticLayout::sanitize`,
     `LayoutContainer::sanitize_group`): sizes and places fixed as on the tab, rounded to whole
     cells (`GROUP_CELL`, at most `GROUP_MAX_CELLS`), and any overlap moved to the nearest free
     cell, `may_overlap` or not. List Layouts share the same way (asked for by the user
     2026-11-27): the same `shared` object and "Widgets inside" rows (`group::shared_rows`), with
     the title row styling the list's labels instead ("Labels": `list_layout::labels` takes the
     size, one for the whole list so rows stay even, the label strip growing to fit; each label's
     font, color, bold and italic are its widget's title style as shown). One widget can opt
     out (asked for by the user 2026-11-27, so it can be set up on its own): `use_shared: false`
     (its dialog's "Use the shared settings"; Reset settings clears it).
   - Autoscale for bars and gauges (**done**, after 0.1.0): `autoscale` (and `autoscale_keep_range`)
     in `shared/scale.rs`. The range comes from every value received: an autoscaled widget asks for
     its series through `traced` (keeping no points), and `Traces` keeps each series' lowest and
     highest since it was first recorded or reset (`Traces::range`, `reset_range`), outliving the
     points so a closed tab keeps it; "Reset scale" in the widget's right-click menu
     (`WidgetType::resettable`). Bars and gauges have an accessible label with the scale drawn
     ("Arm: scale 0 to 8"). Not saved; each run starts fresh.
   - Graph history review (**done**, after 0.1.0): `history_kept` (a minute unless set, up to 10
     minutes) is how far back a graph records (`traced` asks for it), and `traces.rs`'s cap follows
     each series' window (`traces::cap`: 200 values a second, 2,000 to 120,000 points) instead of a
     fixed count. Pausing freezes everything kept (shared, an `Arc`); while paused the wheel or a
     drag along the plot moves through time and Ctrl/⌘ with the wheel zooms about the pointer
     (`graph::Window`, kept to what's recorded), and the wheel is used up so the grid behind
     doesn't scroll. Lines are thinned to each pixel column's lowest and highest before drawing
     (`graph::thin`); the cursors and hover read every value. Still to do: the same for X/Y and
     polar graphs (they keep only their trail).
   - Graph sample statistics (**done**, after 0.1.0): `show_stats` on a Graph shows each line's min,
     max and average bottom left, in its color, above the cursor readout: over the time shown (live,
     or the paused window after looking back), or between the A and B cursors once both are placed
     ("A→B"). `graph::stats_between` works them out from the recorded points (the full ones while
     paused), not the thinned ones drawn.
   - Min, max and average on number widgets (**done**, after 0.1.0; Text Display, Number Bar,
     Voltage View, Radial Gauge): `show_stats` and `stats_window` in `shared/stats.rs`, a line under
     the value. From every value received (`traced`), not the widget's sampled value: `Traces`
     keeps each series' statistics since reset (`since_reset`; cleared when the server connects
     again, `Traces::connected`, and by "Reset min/max", `WidgetType::resettable`), and
     `Traces::window` works them out over the last seconds from the points kept, starting with the
     value held when the window began. Bars and gauges draw through `scale::show` /
     `show_body`. Still to do: bars and gauges marking the min and max on their scale.
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
     - Built-in clips (`media/audio/`): "danger danger" (added 2026-11-22: xpoki's
       "danger_normal_lp.wav" from Freesound, CC BY 4.0, played twice) and "auto ghosts" (added
       2026-11-22: dal_kas's "MustphaMondeRemixes (3).aif" from Freesound, a remix of
       deleted_user_44129's "Ghosts In The Machine.wav", both CC BY 3.0) and "go go go" (added 2026-11-22: theuncertainman's "Go Go
       Go - (Version 2) Northern British Soldier" from Freesound, CC BY 4.0). Every clip's
       source and license is in `media/audio/README.md` and `credits.rs`. More clips later.
       "disabled 1" and "2" (the IT Crowd's "I'm disabled") and "fire 1" and "2" (its "Fire!",
       S1E2) aren't ours, so since 2026-11-23 they aren't built in: they're the `fan-clips` content
       pack (see "Content packs"), suggested on the first run, and generated tones play under those
       names until it's installed. They were built into 0.1.0 to 0.2.1 (the Store build left them out).
   - Later, after 0.1.0 (asked for by the user 2026-11-22), from the same robot state:
     - Window border by state (**done**, `ui/robot_state_view.rs`): a colored border around the
       whole window, so the state shows at a glance from any tab. Colors per state set in the
       Settings window: the FMSInfo widget's colors while enabled or e-stopped, and none while
       disabled or with no robot (changed 2026-11-23 at the user's ask: a red border all through
       setup was distracting), any state can be given a color or none; off by default.
     - Flash on disable (**done**, its own option, asked for 2026-11-23): a red border flashing for
       3 s right after the robot is disabled, with or without the border by state; off by default.
     - E-stop pop-up (**done**): a large modal over the whole window when the robot enters e-stop,
       saying so plainly. It clears on a click anywhere, when the robot reconnects (an e-stopped
       robot stays e-stopped until it reboots) or leaves e-stop. On by default; not shown for a
       robot already e-stopped when the dashboard connects, as the sounds aren't.
     - Both are kept in the layout file, not the settings file (decided with the user 2026-11-23:
       they're look and feel, which should travel with the layout and not change with a computer's
       settings file, unlike the team number and connection, which follow the robot or test setup
       being connected to): a raptaboard-only top-level `"robot_state"` object, `{"border": true,
       "border_colors": {"teleop": "none", "disabled": "#C62828"}, "flash": true, "estop_popup":
       false}`, each part
       left out at its default (`robot_state_view::Look`). They're set in the ⚙ Settings window, which
       makes the layout unsaved, but they're settings, not edits: undo and redo leave them alone
       (`History::set_everywhere` makes the change to every version kept).
     - Both read the state from the first FMSInfo widget's table, else the server's
       `/DriverStation` or `/FMSInfo`, and ask for its topics whatever tab is open, so they work
       with no FMSInfo widget at all.
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
   - X/Y and polar graphs (**done** for 0.5.0, 2026-11-27, `builtin/xy_graph.rs`, `egui_plot`): the
     "X/Y Graph" and "Polar Graph" widgets (raptaboard's own) pair two values recorded at the same
     time (x with y, or angle with distance; `xy_graph::paired` holds each one's last value until
     the other changes) and plot one against the other, for a joystick's position, a robot's path,
     an arm joint's reach or a turret's aim. A trail of the last `trail_time` seconds (3) fades as
     it ages, with a dot at the newest point; 0 shows the dot only (tracking). Several pairs per
     graph (`pairs`), each its own color, named after the struct or the topics' folder. Pairs come
     from "Add as" on a number (the first value; the next number dropped on finishes it, more
     drops start new pairs) or on a struct with a known pair (`xy_graph::known_pair`:
     Translation2d/3d, Pose2d/3d, ChassisSpeeds' vx and vy; X/Y only). A Rotation2d is one number, its angle in
     radians; a Polar Graph chosen for one reads radians (`chosen`). X/Y: fixed x and y ranges
     or fit, `same_scale`. Polar: rings and 30° spokes, the outer ring fixed or fit, degrees or
     radians (spokes labelled in π for radians), 0 right, up, left or down, clockwise or not.
     Shares Graph's recording (`traces.rs`) and pause (freeze), and hover shows the pointer's
     place and each pair's newest values. Not yet: looking back while paused and the measuring
     cursors (Graph's), naming a pair in the settings. The sim layout's "X/Y" tab has one of
     each: OrbitPose, commanded and measured ChassisSpeeds, OrbitHeading against the intake's reach.
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
   the Robot State widget with its sounds (built as the FMSInfo widget). Then the release: version
   0.1.0, a changelog, a tag, and a CI workflow building Windows, macOS and Linux binaries for a
   GitHub Release. Packaging (decided with the user 2026-11-22; `scripts/package.py`, `packaging/`,
   `.github/workflows/release.yml`): every install is per-user (no admin): Windows both a portable
   zip and an Inno Setup installer, macOS a universal `.app` in a zip, Linux a `.tar.gz` with
   `install.sh` into `~/.local`; Microsoft Store (2026-11-23): an MSIX the Store signs, free (an
   individual developer account is free since 2025, and the Store signs MSIX packages at no cost),
   identity `Tuffinmuffin.Raptaboard` (`packaging/windows/msix/store.json`), the same program as
   the other Windows packages (since the fan clips became a content pack, 2026-11-23), uploaded by
   hand for now (later: `msstore publish` from the Release workflow with an Entra app's secrets);
   `PRIVACY.md` is the Store listing's privacy policy (no personal data collected); otherwise not code-signed yet (the README and release notes say how to get
   past the first-start warning); an app icon of Raptacon's raptor mark (its brand kit) in a rust-red gear (`media/icon/`, 2026-11-22); the crates' license texts
   from `licenses/THIRD_PARTY_LICENSES.txt` (the About window's list); the changelog's first entry
   is a short summary of 0.1.0, not the history of the phase. After it: the window border by robot
   state and the e-stop pop-up, the Group widget, X/Y and polar graphs, partial overlap, reflow
   while dragging, the text style
   dialog, regex alert filters, and plugins.
4. Plugins v1: wasmtime host, limits, hot reload, plugin template and SDK crate, and Number and Alerts
   plugins matching the built-ins; then Doom, graphs, the 3D robot viewer and a log viewer ("Plugin
   system", "Order").
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
