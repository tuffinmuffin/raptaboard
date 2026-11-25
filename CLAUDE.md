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
# The README's clips (ui/tests/clips.rs: the app headless against a scripted robot, saved as GIFs
# in docs/images/; needs a GPU, ~30 s):
cargo test -p raptaboard --lib record_ -- --ignored --nocapture
# The Microsoft Store listing's screenshots (media/store/; needs a GPU and robot-2027's sim running here):
cargo test -p raptaboard --lib record_store_screenshots -- --ignored --nocapture
# Log pointer and touch input to the terminal (input_log.rs: egui's events each frame and what the
# Field's target made of them), for odd clicks and taps:
RAPTABOARD_LOG_INPUT=1 cargo run -p raptaboard -- --address sim layouts/robot2027_sim.json
# Plugins (their own workspace, for WebAssembly; plugin-host's tests build them too):
(cd plugins && cargo build --release --target wasm32-wasip2 && cargo clippy --target wasm32-wasip2 --all-targets -- -D warnings)
# Milestone 1 CPU test (opens a window, ~40 s; keep it visible):
cargo test -p raptaboard --release --test cpu_load -- --ignored --nocapture
```

Release builds (`--release`) use full LTO and one codegen unit and strip the program (about
11.5 MB instead of 20 MB; about a minute to build). Debug info goes to a file beside it
(`raptaboard.dSYM` on macOS, `.pdb` on Windows, `.dwp` on Linux) for decoding crash dumps; keep it with
each build handed out. Day-to-day work uses debug builds, which the release profile doesn't touch.

Release packages (`scripts/package.py`, files in `packaging/`): Windows gets a portable zip (with its
own `.raptaboard/` settings folder) and a per-user Inno Setup installer (`packaging/windows/raptaboard.iss`;
no admin, `%LOCALAPPDATA%\Programs\raptaboard`); macOS a zipped universal `raptaboard.app` (ad-hoc
signed only); Linux a `.tar.gz` with `install.sh` (per-user, `~/.local`). Each has README.txt (the user's
first steps), CHANGELOG.md, the licenses and `THIRD-PARTY-LICENSES.txt` (copied from `licenses/THIRD_PARTY_LICENSES.txt`, the
About window's list), plus a `-debug` archive. Nothing is code-signed yet (decided 2026-11-22), except through the Microsoft Store: with `--msix`
(the Release workflow's Windows job) `package.py` also makes an unsigned MSIX for the Store, which signs it
(`packaging/windows/msix/`: the manifest, and `store.json` with the identity from Partner Center), in
`dist-store/` and its own `store-msix` artifact, never on the GitHub release; it holds the same `raptaboard.exe`
as the other Windows packages, and not `raptaboard-cli.exe` (decided 2026-11-23: Partner Center refused 0.3.0's
second, hidden application as a "headless app" needing a waiver; an execution alias on the main application
worked in testing, but the Store package stays as simple as 0.2.1's; Store users get the CLI from GitHub). Each release's `.msix` is uploaded in Partner Center by hand for now.
`PRIVACY.md` is the privacy policy the Store listing links to (its GitHub URL on `main`, from the release after 0.2.1;
0.2.1's listing had the text pasted in), built into the About window's Privacy page
(`credits::PRIVACY`, drawn from its Markdown by `credits::policy_blocks`) (raptaboard collects nothing, connects only to the
NetworkTables server chosen and, when asked, the content sources, and stores only its settings, saved layouts,
imported images and content packs): update it, and its date, in the same PR as anything that makes raptaboard
connect somewhere new or store something new.

Content packs (`docs/spec.md` "Content packs"): sounds and widget plugins, downloaded only when the user chooses,
from `tuffinmuffin/raptaboard-content` (its signed `catalog.json`, its own repo and release process: tag `vN` there)
or sources the user adds. `content.rs` is the catalog, signature (`OFFICIAL_KEYS`, raptaboard-content's
`catalog-key.pub`), checksums, the store in `~/.raptaboard/content/` and the download thread (`Content`; tests use
`Content::with_fetch` and `content::tests::Web`, never the network; the one real-network test is ignored);
`packs_cli.rs` is `raptaboard packs list|install|update|remove|sources` (docs/spec.md "Command line");
`ui/content_window.rs` the Content window (⚙ Settings → Content packs…) and the list offered on the first start and
after an upgrade (`content::State`: `last_version`, `offer_pending`, `shown`). `upgrade.rs` holds the one-time tasks
for a new version's first start: add any migration there. The background check (`content_check_minutes`, hourly by
default, only while the robot isn't enabled: `content::CheckTimer`) feeds `Content::news`, the bar's dismissible
"📦 Content: …" notice (`content_notice`). A pack holds sections
(`[[sound]]`, `[[image]]`, `[[field]]`, `[[plugin]]`, `[[layout]]`; sounds and plugins are used so far: a `[[plugin]]` is its `.wasm` with its manifest beside it,
`alerts.toml` for `alerts.wasm`, the catalog giving its name and permissions, `content::PackPlugin`). A pack's sound may be new or use a
built-in name to replace it, the newest install winning (`sound::SoundLayers`, `installed_at`); a setting picks
`fire 1`, `pack:<id>/fire 1` or `builtin:fire 1` (`sound::SoundRef`), from the shared sound picker
(`widgets/shared/sound_pick.rs`, FMSInfo and Alerts; `sound::sound_choices`). That's how the IT Crowd clips (the
`fan-clips` pack, not built in since 2026-11-23) come back. The signing key's private half is the content repo's
`CONTENT_SIGNING_KEY` secret, with a copy kept outside the repos by the user.

The icon is
`media/icon/raptaboard.svg` (Raptacon's raptor mark from its brand kit in a rust-red gear); `cargo run -p raptaboard --example make_icons` remakes the `.ico`,
`.icns` and `.png`, and `build.rs` puts the `.ico` into the Windows program. To release: bump
`version` in the workspace `Cargo.toml`, add its `## x.y.z` section to `CHANGELOG.md` (short: what's
in it, not every PR), merge, then push a tag `vx.y.z` on `main`; `.github/workflows/release.yml` builds
the packages and publishes the GitHub Release (a tag with a suffix, `vx.y.z-rc.1`, publishes a prerelease
instead, for trying the process; delete it afterwards with `gh release delete vx.y.z-rc.1 --cleanup-tag`) (it also builds them, without publishing, on every push to
`main`, kept as workflow artifacts for 14 days so the newest `main` can be tried, and on PRs that change packaging files;
`CHANGELOG.md` isn't one, so ordinary PRs stay quick).

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
- `crates/plugin-host`: manifest parsing and matching (`plugin.toml`: a table plugin's `type`/`path`/`has`, or a
  topic plugin's `topic_types`); `path` has topic path helpers (`.type` tables); `runtime.rs` runs plugins with
  wasmtime (`Runtime` compiles and keeps the clock behind the time limits, `Plugin` one compiled folder,
  `Instance` one widget: `subscriptions`, `update`, `render`, `tab_mark`, `on_input`, answering the plugin through
  a `Host`; a trap, a panic, running past `Limits::call_ms` or `memory_bytes` stops it for good, as `Stopped`;
  WASI with no network or environment and no files but the plugin's own folder, read-only at `/pack`, stdout to the
  log; `Instance::usage` its call times and memory); `dev.rs` builds a plugins workspace
  (`repo_plugins()`: `plugins/`, once per test run). `tests/runtime.rs` runs them. `docs/spec.md` "Plugin system"
  has the plan (next the example-widgets pack, then Doom, graphs, a 3D robot viewer, a log viewer).
- `crates/plugin-test`: `raptaboard-plugin-test`, the harness for testing a plugin with `cargo test`: `Harness::build`
  builds the plugin crate for wasm32-wasip2 (into `target/raptaboard-plugin-test/`, as the running `cargo test` may hold
  its own target folder) and runs it in `plugin-host`'s runtime with the real limits; a test sets values, properties,
  size, connection, theme and time, clicks regions by label, and reads what was drawn (`Drawn`: texts, regions,
  summary, redraw), the tab mark, sounds, published values and log lines. Its `dark_theme`/`light_theme` are checked
  against the app's (`plugins/convert.rs`); `measure_text` is an estimate. The `plugins/` workspace dev-depends on it
  (native only); its `Cargo.lock` pins the same versions as this one's.
- Plugin sandbox (`raptaboard plugin dev <folder>`, `cli::PluginCommand`): `sandbox.rs` (no egui) runs an `nt4::Server`
  of its own on 127.0.0.1 (a free port) serving the topic or table the plugin's widget is bound to (`/Sandbox/value`,
  or `/Sandbox/<type>` with its `.type`), with the plugin's sample values (`[sandbox]` in `plugin.toml`,
  `plugin_host::SandboxValues`) and any added; `ui/sandbox_panel.rs` is its side panel (`DashboardApp::open_sandbox`:
  size presets, theme, the Connected switch (the address pointed at a closed port), locked, and the values to edit,
  add and remove; and the plugin's overlay: its regions outlined (`plugins::set_outline_regions`), each kind of call's
  time and its memory against the limits (`runtime::Instance::usage`, kept by the worker), its log, why it stopped,
  Restart; `plugins::dev_view`). `main.rs` starts it with nowhere to save settings and no content packs. With `--address`, no
  server: the widget is bound to `--topic`. `ui/tests/sandbox.rs` runs it with Number and Alerts.
- Tab plugins (plugin.toml `tab = "…"`): `plugins::tab_plugin(tab)` finds the plugin claiming an empty tab's name,
  `tab_widget` its stand-in widget (its worker kept while the tab exists); `ui/plugin_tab.rs` draws it in place of the
  grid and turns the dashboard's shortcuts off while it has focus (`plugin_has_keys`); `PluginType::show_tab` and
  `widget::tab_inputs` turn egui's events into its input (with `capture_mouse` in plugin.toml, the cursor grabbed and hidden
  while it has focus, egui's raw `MouseMoved` as `mouse-motion`). Key names are egui's: "Up", "1", "Enter". Plugins' `play-audio` streams through `sound::AudioOut`
  (per window, set by `frame_started`; on and off with the bell).
- `crates/app/src/plugins/`: plugins in the app. `mod.rs` the registry (plugins from installed packs, `use_packs`, run by
  `ui/content_window.rs` at the first frame and after each install or remove; and from `--plugin`, which wins; saved in
  layouts as `plugin:<name>`, found by `widgets::find`; listed by `widgets::types_showing` and the settings dialog's
  widget choice after the built-ins; a changed `.wasm` is loaded again within a second and its widgets restart) and
  the workers, one per widget, by window (egui context) and the widget's type and settings, so a moved widget keeps its
  worker and a changed setting starts a new one; `frame_started` (from `ui/mod.rs` each frame) drops a window's workers
  whose widgets are gone. `worker.rs` runs a plugin on its own thread (values, frames and input in; the newest picture,
  tab mark, what stopped it and its requests (sounds, publishing, log lines) out; a new picture is asked for only when
  values, input, size, theme, connection or lock changed or the plugin's `redraw-in-s` came). `widget.rs` is
  `PluginType`, the `WidgetType` adapter (card, picture, regions as accessible controls, the summary as the widget's
  label, "Plugin stopped" with Restart, settings rows from `plugin.toml`); `draw.rs` the shapes in egui (images not
  yet); `convert.rs` values, settings and the theme in the plugin interface's types. `ui/tests/plugins.rs` runs the
  Number plugin beside a Text Display and the Alerts plugin beside the built-in Alerts.
- `plugins/`: a workspace of its own, built for WebAssembly (`cd plugins && cargo build --release --target
  wasm32-wasip2`; needs `rustup target add wasm32-wasip2`): `sdk` is `raptaboard-plugin` (the `Widget` trait and
  `widget!` macro over the wit-bindgen bindings, and helpers), `number` the first example plugin (a number topic
  as text, with the Text Display's setting names), `alerts` the second (the built-in Alerts widget remade: a
  table plugin with a tab mark, a tone and a pulse, with the built-in's setting names), `limits-test` a plugin that misbehaves on purpose for the
  runtime and harness tests (its `region` mode draws a "Bell" button, `files` tries files in and out of `/pack`), and
  `tab-test` a plugin that fills a tab ("Tab test": a bouncing pixel frame at 35 Hz, keys, a beep through
  `play-audio`, a file from `/pack`), the small stand-in for Doom. Each example plugin has harness tests in its `tests/`. CI lints and tests it in the lint job.
- `crates/app`: the eframe app. `ui/` holds the window (`DashboardApp` in `ui/mod.rs` with file open/save and the edit and tab
  actions; its state is grouped in `ui/state.rs`: `file` (`LayoutFile`), `address`, `tree` (`TreeView`), `grid`
  (`GridState`: drags, where the grid was), `alerts` (the history and its panel) and `sends` (failed sends),
  beside `layout`, `current_tab`, `locked`, `selected` and `status`; `panels.rs` the settings and tab bars; `tree_panel.rs` the tree panel and adding topics;
  `grid_view.rs` the grid (a frame's input, drawing, outlines, then `apply`; inside a `ScrollArea` sized to the
  widgets, `GridView::area` the whole scrolled grid and `visible` the part on screen, with drags near an edge
  scrolling it, `edge_push`); `drag.rs` drag and drop (the drag
  types, drop targets, previews and drops); `dialogs.rs` the settings dialogs; `live.rs`
  the topics the tab needs and graph recording; `style.rs` the drawing constants; each file adds
  its own `impl DashboardApp` block; `ui/tests/` runs the whole window headless against an
  in-process `nt4::Server` with one widget of every type (and struct and struct-array topics),
  the place for new UI tests (`mod.rs` the harness, test server, layouts and shared helpers; one file per
  area: window, tree, writing, sliders, choosers, enums, alerts, files, field, gyro, pose3d, swerve, chassis, struct_view, mechanism, clips); its `egui_kittest` harness finds
  tree rows and menu items by label and clicks or drags them;
  `DashboardApp::show` is one frame without an `eframe::Frame`), `connection.rs` the robot address modes,
  `cli.rs` the startup flags (`clap` derive, `cli::Args`: `--team`, `--address`, `--port`,
  `--hide-tree`, `--plugin <folder>`; this run only; `cli::parse_all`) and `cli::Command` (`raptaboard packs …`, in `packs_cli.rs`, with `pack_folder.rs`
  for `--package`; `main.rs` runs it instead of the window, and on Windows release builds joins the terminal;
  `src/bin/raptaboard-cli.rs` is the same commands as a console program, shipped in every package (Windows: on
  the PATH from the installer, not in the Store package; macOS: inside the app; Linux: `install.sh` puts it in
  `~/.local/bin`): `cargo run --bin raptaboard-cli -- packs list`),
  `settings.rs` settings kept between runs (command line, then `./.raptaboard/settings.toml`, then
  `~/.raptaboard/settings.toml`, made on the first run, then defaults; changes saved to the local file
  if there is one, else the home file, by `settings::Tracker`; `ui/` only says what the current
  values are),
  `credits.rs` licenses and credits (no egui: raptaboard's license, `MEDIA` for every built-in clip, image and
  field, and the crate list and license texts from `licenses/`, made by `scripts/crate_licenses.py`; rerun it after
  dependency changes, a test checks it against `Cargo.lock`; a new built-in clip or image needs a `MEDIA` entry,
  a test checks that too), `ui/settings_window.rs` the ⚙ Settings window (connection, sound, robot state) and the About and
  licenses window, `ui/robot_state_view.rs` the robot's state for the whole window (`RobotView`: a border colored by
  state (none while disabled or with no robot unless set), a red flash after a disable (its own option), and the
  e-stop pop-up; the state from the first FMSInfo widget's table or the
  server's `/DriverStation` / `/FMSInfo`, its topics asked for whatever tab is open; their settings, `Look`, are kept
  in the layout file's `"robot_state"` object, look and feel being the layout's and connections the settings file's,
  and changed with `History::set_everywhere` so undo leaves them alone), `nt.rs` the live connection (`NtConnection`: a 1-thread tokio runtime that owns the `nt4::Client`
  and restarts it when the address changes; the UI only makes non-blocking calls; `write` sends a
  widget's value to an announced topic in that topic's type, or returns a `WriteError` saying why
  not), `sound.rs` the sounds (`Sound`: a failed write's tone, and the Alerts widget's `Tone`s; its own
  thread, `rodio`; `Sound::silent` in tests), `types/` the robot data types raptaboard reads, apart
  from the widgets that draw them: no egui and no widget setting keys here, just topics in and typed
  data and rules out (`types/alerts.rs` WPILib's alert groups and `AlertWatch`, which spots new errors
  and keeps the last alerts seen; `types/chooser.rs` a SendableChooser's table; `types/angle.rs` headings (Rotation2d, Rotation3d yaw, a pose's, a gyro's degrees) and which way they count; `types/swerve.rs` swerve modules' speeds and angles; `types/motion.rs` a robot's velocity and spin (`ChassisSpeeds`); `types/pose3d.rs` 3D poses and quaternions (roll, pitch, yaw); `types/mechanism.rs` WPILib's Mechanism2d (roots, ligaments, where each lands); `types/enums.rs` named
  states and where their names come from, with `guess`; `types/field.rs` a field's size, image bounds and AprilTags, and poses in WPILib 2027's frame;
  `types/structs.rs` struct values decoded with (and `nested_struct`, a struct inside one, as its own type and bytes)
  the server's `/.schema/` topics (`nt4::proto::structs::StructDatabase` + `DynamicStruct`);
  `types::format_number`; more custom data structures go here), `images.rs` images widgets show (a Boolean Box's: PNG, JPEG, SVG via `resvg`), found in `images/` folders as
  fields are, imported into `~/.raptaboard/images/`, or built in (`BUILT_IN`: the repo's `images/*.svg`, our own
  CC0 icons; credits for any outside image go in `images/README.md`), with a texture cache; `alert_history.rs` the alert history (every
  group from connect, count-based; its panel and top-bar button are `ui/alert_history_panel.rs`), `tree/` the read-only NT tree (`mod.rs` building it, `selection.rs` row picking in
  `TreeSelection` and the drag payload `DraggedTopics`, `view.rs` drawing and the right-click menu (Add to grid, Add as ▸
  any type that shows the row via `widgets::types_showing`, Add to list), `format.rs` short value text;
  values only for topics in open folders, via `NtConnection::watch_values`),
  `widgets/` the grid widgets,
  one file per type implementing `widgets::WidgetType` and listed in `widgets::TYPES`; `ui/` only talks to
  the trait and never names a widget type (hooks: `extra_topics` with the server's tables, `background_topics`,
  `tab_mark`, `traced` for recorded series, `takes_topics`/`add_topics` for topics dropped on a widget) (Text Display,
  Boolean Box (Elastic's Large Text Display opens as a Text Display with `fit_text` on: `widgets::upgrade`), Number Bar, Voltage View, Radial Gauge, Match Time, Battery (`builtin/battery.rs`), Single and
  Multi Color View, Graph (`egui_plot`; widgets needing a full `Ui` override `WidgetType::show`; records
  `history_kept` seconds, and paused, the wheel or a drag moves back through them, `graph::Window`, with lines
  thinned per pixel column, `graph::thin`; `show_stats` gives each line's min, max and average over the time shown or
  between the cursors, `graph::stats_between`),
  Toggle Switch and Toggle Button (`builtin/toggle.rs`), Number Slider (`builtin/number_slider.rs`), Dropdown and Split Button Chooser (`builtin/chooser.rs`: bound to a SendableChooser table, `table_types`, `sends_to` and `Frame::others`), Enum Dropdown and Enum Toggle (`builtin/enums.rs`: string or int topics, names from a list, a string[] topic or a chooser, guessed by `WidgetType::chosen`), Field (`builtin/field/`: a Pose2d/Pose3d (or array) struct topic or a `Field2d` table on a field from `fields.rs`,
  WPILib `fields` JSONs from the repo's `fields/` folder (2026 built in) or one picked with a file dialog;
  poses read by `types/field.rs` in the 2027 centered frame, `origin` for the old one; `layers.rs` the
  overlays: AprilTags, and layers of other robots, paths, the tags seen and objects (markers, on the field or
  robot-relative), each from a topic; `attachments.rs` cameras' views and parts fixed to the robot; `target.rs` click to publish a target,
  through `Effect::PublishStruct` and `NtConnection::publish_struct`), Gyro (`builtin/gyro.rs`: a Rotation2d or Rotation3d struct topic, a number of
  degrees or a WPILib `Gyro` table, read by `types/angle.rs`; a Rotation3d's yaw too), SwerveDrive (`builtin/swerve.rs`: a `SwerveModuleState[]` with
  requested states, the whole robot's `ChassisSpeeds` (measured and requested) and a heading from other topics, or Elastic's `SwerveDrive` table, read by `types/swerve.rs`), Chassis Speeds (`builtin/chassis_speeds.rs`: a
  `ChassisSpeeds` or `Translation2d` as velocity and spin arrows, with requested speeds and a heading, read by `types/motion.rs`), 3D Pose (`builtin/pose3d.rs`: a Pose3d, Transform3d,
  Rotation3d or Quaternion as turning X, Y, Z axes; drag to orbit the view; read by `types/pose3d.rs`), Struct
  (`builtin/struct_view.rs`: any other struct or struct array as its fields, the way the tree shows them; last in
  `TYPES`; every whole struct dragged from the tree makes one, the Field, Gyro, 3D Pose, swerve view and Chassis
  Speeds come from "Add as"), Mechanism2d (`builtin/mechanism.rs`: WPILib's `Mechanism2d` table, its ligaments drawn on its
  canvas, zoomed to what the mechanism has used (`fit`); read by `types/mechanism.rs`), Alerts (`builtin/alerts.rs`, reading them with `types/alerts.rs`; tones through `Frame::effects`, tab marks through `WidgetType::tab_mark`, and the lists it needs while its tab is closed through `background_topics`), FMSInfo (`builtin/fms_info.rs`: the Driver
  Station's table, 2027's `/DriverStation` (`ControlWord`) or 2026's `/FMSInfo` (`FMSControlData`), read by
  `types/robot_state.rs` with the match details: the state large with a built-in icon, the alliance, match, station,
  replay, event, game data and DS/FMS, each part on or off (`show_*`); a sound per state change, built in or a file,
  from `WidgetType::state_sounds`: the first FMSInfo's plan goes to `NtConnection::set_state_sounds`, and
  `state_sounds.rs` plays it on the NT thread as values arrive (`Watch` finds the changes), so it works with the
  window in the background); writing widgets return the value to send
  from `show`, and `Place::locked` says whether their input covers the whole card; `show_body` is
  the same for a List Layout child, which `list_layout::show` gives a `Ui` of its own); other types
  draw as placeholders), widget files in `widgets/builtin/`, `widgets/list_layout.rs` (List Layout drawing
  and its settings dialog), helpers in `widgets/shared/`:
  `card.rs`, `entry.rs` (the box a value is typed into: Number Slider, editable Text Display), `pointer.rs` (where a
  tap really was: a touchscreen on macOS sends a whole tap in one frame and puts the cursor back, so
  positions come from the release event; `ui/taps.rs` `TapHold`, run from `raw_input_hook`, holds the
  move back to the mouse for the next frame so egui gives the tap to the widget tapped; tests replay it
  with `tap_burst`, mouse on another widget; a Windows touchscreen laptop worked without issues, tested by the
  user 2026-11-22), `format.rs` (number and
  enum text, `EnumDisplay`), `alert_sign.rs` (alert level colors and signs, for the widget, tab marks
  and history), `props.rs` (period, font size, colors, a topic picker), `sound_pick.rs` (the sound dropdown:
  built-in and installed packs' sounds), `value_colors.rs` (color by value: thresholds or a gradient), `scale.rs` (bars' and gauges' shared settings,
  autoscale, and their `show`/`show_body`), `stats.rs` (min, max and average under a number), `robot_view.rs` (a robot from above with
  speed and spin arrows, for the swerve view and Chassis Speeds) and `settings.rs` (the settings dialog's shared
  rows; the dialog window is in `ui/dialogs.rs`); setting keys are constants in the file that owns them,
  listed in `docs/spec.md` under "raptaboard widget settings" (keep that table in step); adding a
  widget is a new file in `builtin/` (listed in `builtin/mod.rs`) plus one line in `TYPES`, `layout_file.rs` opening and saving layout files (atomic
  writes, Ctrl+O/S/Shift+S; a file that's missing or isn't a layout, at startup or from Open, gets a dialog,
  `LayoutFile::load_problem`, offering Open, a new layout saved at that path (missing only), or Close), `grid.rs` the snap-grid rules
  (snapping, overlap) and `tabs.rs` tab actions (moving one by dragging it along the tab bar: `TabAction::Move`, `drop_place`) and their shortcuts, `edit.rs` undo/redo (`History`, which
  notices any layout change once a frame) and the widget editing shortcuts; the drag handling is in
  `ui/drag.rs` and `ui/grid_view.rs`. A widget draws from `widgets::Live`: its value (`Live::newest` for its topic's newest, the part for a
  widget bound to part of a struct), the frame's shared
  `Frame` (traces, the time, other topics' values, connection state, effects) and its `Place` (locked,
  input and free areas). `binding.rs` (no egui) is what a widget is bound to: `WidgetValues::of` (its value
  now: sampled per period, struct fields decoded) and `write_target` (where a sent value goes, or why it
  can't). Widget values come from a
  second set of subscriptions (`NtConnection::watch_widget_values`), one per `period` on the open
  tab, and `traces.rs` (`Traces`) records the values over time that graphs show (every value, from the NT client's event stream, fed by
  `ui/live.rs` `record_traces`; a growable ring buffer per series; and each series' lowest and highest since a reset,
  `Traces::range`, for autoscaled bars and gauges: `shared/scale.rs` `with_autoscale`; and statistics, min, max and
  average, since reset or reconnect, `since_reset`, or over the last seconds, `window`, for number widgets'
  `shared/stats.rs`; both reset from the right-click menu through `WidgetType::resettable`), `sampling.rs` (`Sampler`) has each widget take a new value only once per its period, so
  fast redraws (mouse moves) or the tree watching the same topic don't make it jump ahead.
  `tests/cpu_load.rs` is the ignored 30-widgets-at-50-Hz CPU test (runs with the tree shown and
  hidden; prints % of one core and of the machine); its `other_dashboard` test runs the same load against Elastic for comparison
  (set `RAPTABOARD_CPU_OTHER=elastic_dashboard`, Elastic on localhost, port 5810 free).
  Results are in `docs/spec.md`; VMs without hardware DX12 render in software, so their numbers
  don't count. Milestone 2 has tabs, the snap grid, the lock and layout open/save
  (`layout_file.rs`, native dialogs via `rfd`); topics and struct fields drag from the tree onto the grid (`tree::DraggedTopic`,
  `widgets::for_topic`; struct fields use Elastic's `struct_meta`, read with
  `types::structs::field_value`); widgets can be selected, duplicated, deleted and edited (settings dialog),
  with undo; next is List Layout editing.
- `layouts/robot2027_sim.json`: a layout for trying widgets against the robot-2027 sim
  (`cargo run -p raptaboard -- --address sim layouts/robot2027_sim.json`). Its `/DashTest/`
  topics come from robot-2027's `utils/dashboard_test_topics.py` (sim only), on robot-2027's
  `raptaboard-test` branch: commit test-topic changes there (never robot-2027's `main`).
  It also keeps widgets for things that don't exist, on purpose, to test that handling: "Never
  published" and "Missing text" (topics never sent), "Missing field" (a field `OrbitPose` doesn't
  have), and the "Missing child" row in Module CAN. Keep them when editing the layout.
  Its Writing tab has a Toggle Switch and a Toggle Button on `/DashTest/Toggle` (published once
  by the test topics, never overwritten) and a Boolean Box on `/DashTest/ToggleEcho` (the value
  robot code read back), plus "Not published (fails)", a toggle on a topic that's never sent,
  for the failed-write flash, tone and message; keep it too. Two Number Sliders on
  `/DashTest/Setpoint` (0 to 100; one sends on release, one every value in steps of 5) with a Text
  Display on `/DashTest/SetpointEcho`, and "Setpoint (typed)", an editable Text Display. "Auto mode" (Dropdown) and "Test" (Split Button) are choosers on
  robot-2027's `/SmartDashboard/DashTest/Auto mode` and `DashTest/Test mode` SendableChoosers. "Manual chooser" is on
  `/DashTest/ManualChooser`, one made by hand from plain topics (`.type` and all) to show a chooser is
  found by its `.type` at any path.
  Its Enums tab has enum widgets on robot-2027's `/DashTest/IntakeMode` (two-way: the robot steps
  it every 5 s, names in `IntakeModeOptions`) and `/DashTest/ArmLevel` (int, names listed), an
  Enum Toggle on the Test chooser, and watch-only (not editable) widgets. Its Widgets tab has a Boolean Box on `IntakeDeployed` showing the built-in `motor-on.svg` / `motor-off.svg`, a Struct on `/DashTest/CyclingMode` (an enum that
  steps through values, one with no name) and a Mechanism2d on robot-2027's
  `/SmartDashboard/DashTest/Lift` (an elevator rising and falling with an arm swinging on top). Its Field tab has Fields on `OrbitPose` (blue-corner origin, robot-2027 being on RobotPy 2026),
  one with layers for `Fleet`, `OrbitPath`, `SeenTags` (the tags Robot-2026's cameras would see), `Fuel` (balls,
  field positions) and `SeenFuel` (the balls ahead, robot-relative: each ring should land on a ball), with Robot-2026's two
  cameras and an intake (on `IntakeDeployed`, reaching `IntakeExtensionIn` inches) attached to the robot. Below them, two Gyros: one on `OrbitHeading` (a Rotation2d) and one on the `Gyro` table (degrees,
  clockwise), which should read the same, and two swerve views: `ModuleStates` (spinning) and robot-2027's
  drivetrain, `/Drive/MeasuredStates` against `/Drive/SetpointStates` with `/Drive/MeasuredSpeeds` against
  `/Drive/CommandedSpeeds`, turned by `/Drive/Pose`. Beside the red-side Field, a Chassis Speeds on `/Drive/MeasuredSpeeds` against
  `/Drive/CommandedSpeeds` (the keyboard joystick can't spin the sim drivetrain: rotation is Xbox axis 4
  and it has three axes), and one on robot-2027's scripted `/DashTest/CommandedSpeeds` against
  `MeasuredSpeeds`, which lag 0.3 s and reach 90% of the speed and 80% of the spin. Below the Gyros, a 3D Pose on robot-2027's `TumblePose` (a
  Pose3d that pitches and rolls). Its Alerts tab shows robot-2027's alerts (its
  test topics add a warning, an info, and an error on for 3 s every 10 s), and an FMSInfo on `/FMSInfo` (the sim
  runs RobotPy 2026; enabling, disabling or switching modes in the sim's Driver Station plays its sounds). The
  layout turns on the window border by state (cyan with no robot, none while disabled) and the red flash after a
  disable (its `"robot_state"` object). "Writing in a list" holds the toggles and a slider as List
  Layout children.
- `wit/dashboard.wit`: plugin ABI (0.3: widgets and tab plugins, shapes including `pixels`, regions, input,
  `play-audio`, the plugin's folder read-only at `/pack`). Bump the package version and `HOST_API` in plugin-host
  together; before 1.0 a plugin runs only on the same 0.x (`api_compatible`), so rebuild the plugins and the
  content repo's `example-widgets` pack with it.
- `plugins/arm`: a manifest only (no code); its `plugin.toml` is a test fixture.

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
