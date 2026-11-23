# Microsoft Store listing art

Images for raptaboard's Microsoft Store listing (uploaded by hand in Partner Center).

| File | What | Made by |
| --- | --- | --- |
| `poster-720x1080.png`, `poster-1440x2160.png` | the listing's 9:16 poster art: the app icon on a dark background | `cargo run -p raptaboard --example make_icons`, from `media/icon/raptaboard.svg` |
| `square-1080x1080.png`, `square-2160x2160.png` | the listing's 1:1 art: the same, square | the same |
| `tile-300x300.png`, `tile-150x150.png`, `tile-50x50.png` | the listing's app tile icons: the icon itself, with see-through corners | the same |
| `screenshot-field.png` | the Field tab: fields, gyros and swerve views | the screenshot recorder below |
| `screenshot-graphs.png` | the "More widgets" tab after 10 s: graphs, bars, gauges, min/max | the screenshot recorder below |
| `screenshot-widgets.png` | the "Widgets" tab (many of its widgets show missing topics on purpose; not for the listing) | the screenshot recorder below |

The screenshots are the app itself at 1920 x 1080, drawn headless against robot-2027's sim with
`layouts/robot2027_sim.json`, after 10 seconds on each tab. Start the sim on this computer (on
robot-2027's `raptaboard-test` branch, for the `/DashTest/` topics), then:

```sh
cargo test -p raptaboard --lib record_store_screenshots -- --ignored --nocapture
```

The icon is Raptacon's raptor mark from its brand kit (see `media/icon/README.md`); the screenshots
show the 2026 field from WPILib (BSD 3-Clause, `fields/LICENSE-WPILib.md`).
