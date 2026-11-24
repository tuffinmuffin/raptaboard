# Arm plugin (example)

Draws an arm table tagged `.type = "Arm"`: the pivot, the arm at `angle_rad`
(green when `at_goal`, orange otherwise) and a thin line at `setpoint_rad`.

Only `plugin.toml` is here; the plugin-host tests load it. The first plugins
with code are the Number and Alerts examples (`docs/spec.md`, "Plugin system").
