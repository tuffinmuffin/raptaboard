# Changelog

What changes in each raptaboard release. Newest first.

## Unreleased

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
