# Changelog

What changes in each raptaboard release. Newest first.

## 0.5.0 (not released yet)

- **Text style:** a widget's title, and the value text of Text Display, Boolean Box and Struct, can
  be Sans, Mono, Serif, Narrow or Digital (a seven-segment display), bold, italic, and any color, from the settings dialog's "Style…" buttons; the
  title's size too.
- **X/Y and polar graphs:** the X/Y Graph plots one value against another (a joystick, a robot's
  path, commanded against measured speeds) and the Polar Graph a distance at an angle (a turret's
  aim), each with a fading trail. Make one with "Add as" on a number, then drop the second value
  on it from the tree, or on a Pose, Translation or ChassisSpeeds for its x and y at once.

- **Widgets make room:** a widget moved or resized onto others pushes them aside to the nearest
  free spots, shown as you drag, instead of the move being refused. One Undo puts them all back.
- **The book's Widgets chapter:** every widget, how to add and change widgets, the settings dialog
  and text styles, with pictures from the sim robot.

## 0.4.0

Widget plugins: widgets and whole tabs made as small WebAssembly programs, installed from content
packs without updating raptaboard.

- **Plugin widgets** from content packs: ⚙ Settings → Content packs lists what each plugin may do
  (send values to the robot, play sounds, keep running while its tab is closed, save values on
  this computer) and what else it needs (a tab it fills, holding the mouse, more memory or time)
  before installing, and raptaboard holds it to that. A plugin's widgets are in "Add
  as" and the settings dialog for the topics and tables it shows, after the built-in ones. A
  plugin that stops shows why, with Restart; one can't slow the window down or use more than its
  share of memory.
- **The "Example widget plugins" pack:** Number and Alerts, raptaboard's Text Display and Alerts
  widgets remade as plugins, to show what plugins can do and as examples for writing your own.
- **Tab plugins:** a plugin can fill a whole tab, which takes the keyboard (and the mouse, if it
  asks) when clicked; Shift+Escape gives them back, as does the plugin stopping. It can play sound
  through raptaboard's (off with the bell) and keep its settings between runs.
- **The "Doom: the 2026 field" pack:** Doom on the 2026 FRC field, with KitBots as the monsters
  (Freedoom's game data, the room4doom engine): an empty tab named "Doom" becomes the game.
- **For plugin authors:** the plugin interface (API 0.3: shapes and pixel images, regions, keys
  and the mouse, sound, the plugin's own files, small saved values), a Rust SDK, a test harness for
  `cargo test` (`raptaboard-plugin-test`), `raptaboard plugin dev <folder>` (the plugin alone in a
  window, with sample values, its time and memory against its limits, and its log), and the
  examples in [raptaboard-examples](https://github.com/tuffinmuffin/raptaboard-examples). Packs can
  take their files from a plugin's own repository's releases.
- **Pre-release packs:** "Pre-releases" beside the official source in the Content window (or
  `raptaboard packs list --prereleases`) offers packs being tried before they're released, still
  signed.
- **Driver Station** as the server choice now works: the robot's address comes from the
  Driver Station on the same computer, the 2027 one or the NI one, as in Elastic.
- **Documentation:** a book at <https://tuffinmuffin.github.io/raptaboard/>: installing,
  connecting, the window, building raptaboard and writing plugins, with more pages coming.
- **Not in the Microsoft Store:** the Store stays at 0.3.0 until 0.5.0. Get 0.4.0 from GitHub.

## 0.3.0

Content packs: extra sounds (and later plugins, fields and layouts) to download when you choose,
and a command line to manage them.

- **Content packs** from raptaboard-content on GitHub: offered with each pack's license on the
  first start and after an update, never downloaded without asking. ⚙ Settings → Content packs
  installs, updates and removes them, and can add other sources. Every download is checked
  (the official list is signed); without internet nothing changes.
- **The IT Crowd clips** are now a pack ("IT Crowd clips"), not built in; every package, the
  Microsoft Store's too, can install it. "disabled 1/2" and "fire 1/2" play tones until it is.
- **Update check:** about once an hour while the robot isn't enabled (change or turn off in
  ⚙ Settings), a dismissible "📦 Content" notice in the bar says when there's something new.
- **Sound pickers** (FMSInfo's sounds, the Alerts tone) list the packs' sounds too: "fire 1"
  plays the newest pack's, or pick one pack's or the built-in one. An Alerts widget can now
  play a pack's sound.
- **raptaboard-cli** (also `raptaboard packs …`): `raptaboard-cli packs list`, `install`,
  `update`, `remove`, from a pack folder (`--package`) or another catalog (`--url`) too. On
  Windows the installer can add it to the PATH (on by default); the Microsoft Store version
  doesn't include it. On macOS it's inside the app, on Linux `install.sh` installs it.
- **Privacy policy** in ⚙ Settings → About and licenses → Privacy, and online at
  <https://github.com/tuffinmuffin/raptaboard/blob/main/PRIVACY.md>.

## 0.2.1

- **Microsoft Store package:** an MSIX the Store signs, so Windows trusts it. The Store build
  leaves out the IT Crowd clips and plays tones in their place.

## 0.2.0

Measuring and the robot's state: scales that fit the values, min/max/average, graph history,
and the robot's state for the whole window.

- **Autoscale** for Number Bar, Voltage View and Radial Gauge: the scale follows the lowest and
  highest values seen (short peaks too), optionally keeping the set range, with Reset scale in
  the right-click menu.
- **Min, max and average** under the value on Text Display, Number Bar, Voltage View and
  Radial Gauge: over the last few seconds or since a reset (the robot reconnecting, or Reset
  min/max in the right-click menu), from every value received so short peaks count.
- **Graph history:** graphs keep the last minute (the new "History kept" setting, up to 10
  minutes); paused, drag or use the mouse wheel to look back through it, Ctrl or ⌘ with the
  wheel to zoom, and measure anywhere in it with the cursors.
- **Graph statistics:** each line's min, max and average over the time shown, or between the
  measuring cursors.
- **Robot state for the whole window** (⚙ Settings, kept in the layout): a pop-up over everything
  when the robot is e-stopped (on by default), a window border colored by the robot's state, and
  a red border flashing after a disable (both off by default; colors per state).
- The Settings window opens below the bars, so the ⚙ and Undo stay clickable while it's open.
- Touchscreens tested on Windows (a laptop), without issues.

## 0.1.0

The first release: the basics of a driver dashboard, opening Elastic layouts.

- **Layouts:** opens and saves Elastic layout files, and reopens the last one at start. Tabs,
  a snap grid, List Layouts, undo and redo, and a lock so nothing moves while driving.
- **NetworkTables:** connects by team number, the Driver Station, SystemCore or the sim. A live
  tree of every topic, WPILib structs included; drag topics onto the grid to make widgets.
- **Widgets:** Text Display, Boolean Box, Number Bar, Voltage View, Radial Gauge, Battery, Graph,
  Match Time, Color Views, Toggles, Number Slider, Choosers and Enum widgets, Field, Gyro,
  Swerve, Chassis Speeds, 3D Pose, Mechanism2d, Struct and Alerts (with an alert history).
- **Settings:** colors that follow the value, fitted text, icons and images on Boolean Boxes,
  and copying one list child's settings to the rest.
- **Packages:** Windows installer and portable zip, macOS app, Linux tarball. Not signed yet;
  see README.txt for the first start.
