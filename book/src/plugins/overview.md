# Plugins at a glance

A plugin is a folder with `plugin.toml` and `widget.wasm`. The manifest says what it can show, one
topic or a table:

```toml
name = "number"
version = "0.1.0"
api = "0.3"

[match]
topic_types = ["double", "float", "int"]   # or, for a table: type = "Arm" (its .type topic)

[properties]
decimal_places = { type = "number", default = -1 }
```

The WebAssembly component implements the `widget` world from `wit/dashboard.wit`: the dashboard
pushes it the values it asked for, and it returns shapes to draw. In Rust, the SDK in
`plugins/sdk` does the plumbing: implement its `Widget` trait and pass the type to `widget!`. The
example plugins are in [raptaboard-examples](https://github.com/tuffinmuffin/raptaboard-examples):
Number, a small one, and Alerts, the built-in Alerts widget remade, a bigger one. The plugins in
`plugins/` are the ones raptaboard's own tests use.

## Trying one

Build it and start the dashboard with it:

```sh
rustup target add wasm32-wasip2
(cd plugins && cargo build --release --target wasm32-wasip2)
cargo run -p raptaboard -- --plugin plugins/topic-test --plugin plugins/table-test --address sim
```

Their widget types (`plugin:topic-test`, `plugin:table-test`) are then in "Add as" and the
settings dialog's widget choice for the topics and tables they show, after the built-in ones.
Rebuild one while the dashboard runs and its widgets restart with the new build.

## The sandbox

To try one by hand without a robot, open it in the sandbox: the dashboard with only that plugin's
widget and a NetworkTables server of its own, serving the sample values from `[sandbox]` in its
`plugin.toml`. A side panel changes, adds and removes values and sets the widget's size, the
theme, the connection and the lock. It also shows the plugin's clickable regions outlined, how
long each kind of call takes and how much memory it uses against the limits, its log, and why it
stopped, with Restart; the widget restarts whenever the `.wasm` is rebuilt. Nothing is saved.
(`--address sim --topic <path>` shows the sim's values instead.)

```sh
cargo run -p raptaboard -- plugin dev plugins/table-test
```

## Testing

To test one without the dashboard, `crates/plugin-test` (`raptaboard-plugin-test`) runs its built
`.wasm` in the dashboard's own plugin runtime from `cargo test`: set values, the size and the
connection, click its regions, and check the text it drew, its tab mark, the sounds it asked for
and the values it saved. raptaboard-examples' `number/tests/harness.rs` and
`alerts/tests/harness.rs` are examples:

```rust
let mut widget = raptaboard_plugin_test::Harness::build(env!("CARGO_MANIFEST_DIR"));
widget.unit("", "m").set("", 1.5);
assert!(widget.render().has_text("1.5 m"));
```

## Tab plugins

A plugin can fill a tab instead of being a widget: with `tab = "Doom"` in its `plugin.toml`, an
empty tab named Doom shows it at the tab's size, and clicking it gives it the keyboard (the
dashboard's shortcuts are off until Shift+Esc or a click elsewhere). It can draw its own pixels (a
game's frame), stream sound, read its own folder at `/pack`, keep small values between runs
(`storage = true`), and hold the mouse while it has the keyboard (`capture_mouse = true`: a game
turning with it). `plugins/tab-test` is a small example:
`cargo run -p raptaboard -- plugin dev plugins/tab-test`.
[raptaboard-doom](https://github.com/tuffinmuffin/raptaboard-doom) is a big one.

## Sharing one

To share a plugin, put it in a content pack: its `.wasm` and its `plugin.toml`, renamed for it
(`alerts.wasm` and `alerts.toml`), with a `[[plugin]]` naming the `.wasm` in the pack's
`pack.toml`. A pack can take its files from the plugin's own repository's releases. Try a pack
folder with `raptaboard packs install --package <folder> --unsigned`. Installed packs' plugins load
at once, without a restart. Before installing, raptaboard shows what each plugin may do (send
values, play sounds, run while its tab is closed, save its settings) and what else it needs (a
tab, the mouse, more memory or time), and it holds the plugin to that.
