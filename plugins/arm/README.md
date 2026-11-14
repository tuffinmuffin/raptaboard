# Arm plugin (example)

Draws an arm table tagged `.type = "Arm"`: the pivot, the arm at `angle_rad`
(green when `at_goal`, orange otherwise) and a thin line at `setpoint_rad`.

Only `plugin.toml` is here for now; the plugin-host tests load it. The Rust
source and `widget.wasm` arrive with milestone 4, when the host can run them.
The planned code is in `docs/spec.md` under "Example plugin: arm widget".
