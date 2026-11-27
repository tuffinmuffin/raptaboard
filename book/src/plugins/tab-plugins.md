# Tab plugins

A tab plugin fills a whole tab instead of being one widget on the grid: a game, a log viewer, a
camera stream, anything that needs the space. It's the same kind of plugin, with the same SDK and
interface as a widget; only its `plugin.toml` and what it does with the space differ.

The examples:

- `plugins/tab-test` in the raptaboard repository: small, and the code below. It draws a square
  bouncing across a pixel frame, counts keys and mouse movement, and beeps on Space.
- [raptaboard-doom](https://github.com/tuffinmuffin/raptaboard-doom): Doom on the 2026 field, the
  big one.

Try tab-test in the sandbox:

```sh
rustup target add wasm32-wasip2
(cd plugins && cargo build --release --target wasm32-wasip2)
cargo run -p raptaboard -- plugin dev plugins/tab-test
```

## Its manifest

`tab` names the tab it fills, in place of `[match]`. An empty tab with that name shows the plugin,
at the tab's size; a tab with widgets on it stays a grid. Doom's:

```toml
name = "doom"
version = "0.1.0"
api = "0.3"
tab = "Doom"
# Turn with the mouse, fire with its left button: held while the tab has the keyboard.
capture_mouse = true

[permissions]
sound = true
storage = true      # the Options menu's settings, kept between runs

[limits]
memory_mb = 192     # the game's data and textures (about 105 MiB in use)
call_ms = 500       # loading a level takes longer than the usual 100 ms
```

The [plugin.toml reference](manifest.md) has every key.

## Drawing a frame of pixels

Shapes and text work as in a widget, but a game draws its own pixels: a `Pixels` shape is an image
the plugin made, RGBA, stretched over a rectangle. `smooth: false` keeps each pixel a sharp square
when it's enlarged, as pixel art should be. Setting `redraw_in_s` asks for the next frame that many
seconds later, which makes it an animation: 35 frames a second here, as Doom runs.

```rust,noplayground
{{#include ../../../plugins/tab-test/src/lib.rs:render}}
```

`pack_file` reads a file from the plugin's own folder, the content pack it was installed from, which
the plugin sees as `/pack`. That's where a game's data goes. A plugin can read no other files.

## The keyboard and the mouse

Clicking the tab gives the plugin the keyboard: it gets `Input::Focus(true)`, then every key down and
up (`KeyDown`, `KeyUp`) and typed text, and the dashboard's own shortcuts are off. It gives the
keyboard back on **Shift+Escape**, a click elsewhere, or when the window goes to the background, and
the plugin gets `Focus(false)`: treat every key as let go then, since keys let go elsewhere don't
arrive. A plugin can give it back itself with `host::release_focus()`, as a game's Quit does.

With `capture_mouse = true`, the plugin also holds the mouse while it has the keyboard: the cursor
is hidden and stays put, and its movement arrives as `MouseMotion`, which keeps coming at any edge of
the screen. Clicks arrive as `PointerDown` and `PointerUp`.

```rust,noplayground
{{#include ../../../plugins/tab-test/src/lib.rs:input}}
```

If the plugin stops, with a panic or a limit reached, the dashboard takes the keyboard and mouse back
and shows why, with **Restart**.

## Sound

With `sound = true`, a plugin can play the dashboard's sounds (`play_sound`), and stream its own:
`play_audio` queues 16-bit samples, mono or stereo, at any rate. Send about a frame's worth each
frame to stay in step; anything queued past half a second is dropped. The dashboard's 🔔 button
mutes it.

## Keeping settings

With `storage = true`, `storage_set` saves small values on this computer between runs, such as a
game's options or a high score, and `storage_get` reads them back. Each plugin has its own set, 64 KiB
unless `[limits] storage_kb` asks for more (up to 1024).

## Testing

`raptaboard-plugin-test`'s harness runs a tab plugin as it runs a widget, and can also send keys and
pointer input (`input`), check the pixel frames it drew (`pixels`), whether it gave the keyboard
back (`take_releases`), what it saved (`stored`), and that it stays inside its limits (`usage`). See
the [tutorial](tutorial.md#its-tests) for the harness itself.

## Sharing it

A tab plugin goes in a [content pack](content-packs.md) with its data files. Installing the pack is
all it takes: the user adds an empty tab with the plugin's tab name, and the plugin fills it.
