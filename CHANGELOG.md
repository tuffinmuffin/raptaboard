# Changelog

What changes in each raptaboard release. Newest first.

## Unreleased

- **Autoscale** for Number Bar, Voltage View and Radial Gauge: the scale follows the lowest and
  highest values seen (short peaks too), optionally keeping the set range, with Reset scale in
  the right-click menu.
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
