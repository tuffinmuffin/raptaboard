# Tutorial: the example plugins

This chapter takes apart the two plugins in
[raptaboard-examples](https://github.com/tuffinmuffin/raptaboard-examples), the repository to copy
when starting your own:

- **Number** shows one number topic as text. It's the smallest useful plugin: the whole of it is
  below.
- **Alerts** shows a WPILib alert group, a *table*. It adds what a bigger widget needs: values
  that change between frames, a tab mark, sound, and running while its tab is closed.

The code here is shortened from the repository; follow the links for every line.

## Setting up

You need Rust and its WebAssembly target:

```sh
rustup target add wasm32-wasip2
git clone https://github.com/tuffinmuffin/raptaboard-examples
cd raptaboard-examples
cargo build --release --target wasm32-wasip2   # target/wasm32-wasip2/release/number.wasm
cargo test                                     # each plugin's tests
```

The repository is a Cargo workspace with one folder per plugin. Its
[`Cargo.toml`](https://github.com/tuffinmuffin/raptaboard-examples/blob/main/Cargo.toml) pins the
plugin SDK (`raptaboard-plugin`) and the test harness (`raptaboard-plugin-test`) to one raptaboard
commit. Move both together when you move to a newer plugin API:

```toml
[workspace.dependencies]
raptaboard-plugin = { git = "https://github.com/tuffinmuffin/raptaboard", rev = "6a68932…" }
raptaboard-plugin-test = { git = "https://github.com/tuffinmuffin/raptaboard", rev = "6a68932…" }
```

Each plugin is a `cdylib` crate (that's what makes a `.wasm`), with the harness as a dev-dependency
for everything but WebAssembly:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
raptaboard-plugin.workspace = true

[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
raptaboard-plugin-test.workspace = true
```

## Number

### Its manifest

[`number/plugin.toml`](https://github.com/tuffinmuffin/raptaboard-examples/blob/main/number/plugin.toml)
says it shows a number topic, has four settings, and starts at 1.23456 in the sandbox. Its settings
use the Text Display's names, so a widget switched between the two keeps them.

```toml
name = "number"
version = "0.1.0"
api = "0.3"

[match]
topic_types = ["double", "float", "int"]

[properties]
decimal_places = { type = "number", default = -1 }   # -1: up to 4, trailing zeros dropped
font_size = { type = "number", default = 14 }
fit_text = { type = "bool", default = false }
show_unit = { type = "bool", default = true }

[sandbox]
value = 1.23456
```

The [plugin.toml reference](manifest.md) has every key.

### Its code

[`number/src/lib.rs`](https://github.com/tuffinmuffin/raptaboard-examples/blob/main/number/src/lib.rs)
has three parts. The SDK's `Widget` trait is the dashboard's side; everything else is plain Rust
that tests can call directly.

**1. The settings, read once a frame** into a struct, so the rest of the code doesn't ask the
dashboard for them:

```rust,noplayground
struct Settings {
    decimals: Option<usize>,   // None: up to 4 places
    font_size: f32,
    fit_text: bool,
    show_unit: bool,
}

impl Settings {
    fn read() -> Settings {
        let places = property_number("decimal_places", -1.0);
        Settings {
            decimals: (places >= 0.0).then(|| places.min(10.0) as usize),
            font_size: property_number("font_size", 14.0) as f32,
            fit_text: property_bool("fit_text", false),
            show_unit: property_bool("show_unit", true),
        }
    }
}
```

**2. The picture**, a plain function: given the frame's size and theme, the settings, the value
and a way to measure text, it returns the shapes to draw. A `Picture` is a list of shapes in pixels
from the widget's top-left corner (text, lines, rectangles, circles, arcs, images), plus a
`summary` that screen readers say and tests read.

```rust,noplayground
fn picture(
    frame: &FrameInfo,
    settings: &Settings,
    value: Option<f64>,
    unit: Option<String>,
    measure: &dyn Fn(&str, f32) -> Size,
) -> Picture {
    let unit = unit.filter(|_| settings.show_unit);
    let text = shown(value, settings.decimals, unit);   // "1.23 m", or "—"
    let mut size = settings.font_size;
    if settings.fit_text {
        // Measure at 100 px, then scale to fill 90% of the width, at most 80% of the height.
        let at_100 = measure(&text, 100.0);
        let by_width = frame.size.width * 0.9 / at_100.width.max(1.0) * 100.0;
        let by_height = frame.size.height * 0.8 / at_100.height.max(1.0) * 100.0;
        size = by_width.min(by_height).max(6.0);
    }
    let height = measure(&text, size).height;
    let mut picture = Picture::empty();
    picture.text(Text {
        pos: Point { x: frame.size.width / 2.0, y: (frame.size.height - height) / 2.0 },
        text: text.clone(),
        size,
        // Greyed while the dashboard isn't connected.
        color: if frame.connected { frame.theme.text } else { frame.theme.weak_text },
        align: Align::Center,
        max_width: None,
        monospace: false,
    });
    picture.summary = Some(text);
    picture
}
```

Colors come from `frame.theme`, so the widget matches the dashboard's light or dark theme.

**3. The dashboard's side.** `subscriptions` says which values it wants: `Bound`, its own topic.
`render` is called each time something changes (a value, the size, the theme). It fetches what
`picture` needs from the host, the dashboard, and hands it over:

```rust,noplayground
#[derive(Default)]
struct Number;

impl Widget for Number {
    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::Bound]
    }

    fn render(&mut self, frame: FrameInfo) -> Picture {
        let measure = |t: &str, size| host::measure_text(t, size, None, false);
        picture(&frame, &Settings::read(), number(&host::get("")), host::unit(""), &measure)
    }
}

raptaboard_plugin::widget!(Number);
```

`host::get("")` is the bound topic's newest value ("" means the topic itself), and `number` turns
any number type into an `f64`. `host::unit("")` is the unit robot code gave the topic, if any.
`widget!` connects the type to the WebAssembly interface; use it once.

### Its tests

There are two kinds, as the [README](https://github.com/tuffinmuffin/raptaboard-examples#how-the-plugins-are-built-for-testing)
explains.

**Unit tests** in `src/lib.rs` call `picture` directly, with a fake frame and a fake text measurer
(every character 0.6 of the size wide), and check exactly where the text goes:

```rust,noplayground
let p = picture(&frame(200.0, 100.0, true), &two_places, Some(1.23456), Some("m".into()), &measure);
assert_eq!(p.summary.as_deref(), Some("1.23 m"));
let t = line(&p);
assert_eq!((t.pos.x, t.pos.y), (100.0, 43.0));   // centered
```

**Harness tests** in
[`number/tests/harness.rs`](https://github.com/tuffinmuffin/raptaboard-examples/blob/main/number/tests/harness.rs)
build the real `.wasm` and run it in raptaboard's own plugin runtime. Set values, settings and the
size, then check what it drew:

```rust,noplayground
use raptaboard_plugin_test::Harness;

#[test]
fn its_settings_change_the_text() {
    let mut w = Harness::build(env!("CARGO_MANIFEST_DIR"));
    w.property("decimal_places", 2.0)
        .property("show_unit", false)
        .unit("", "m")
        .set("", -7.5);
    assert!(w.render().has_text("-7.50"));
    w.set("", 3i64);   // an int topic reads as a number too
    assert!(w.render().has_text("3.00"));
}
```

The repository's CI fails a pull request under 80% line coverage. Only the unit tests count:
coverage can't see inside WebAssembly, which is why the logic lives in plain functions.

### Trying it

From a raptaboard checkout beside raptaboard-examples, open it in the **sandbox**: the dashboard
with only this widget, its sample value, and a panel to change values, the size, the theme and the
connection, and to see its log, timings and memory.

```sh
cargo run -p raptaboard -- plugin dev ../raptaboard-examples/number
```

Or load it into the full dashboard against the robot sim:

```sh
cargo run -p raptaboard -- --plugin ../raptaboard-examples/number --address sim
```

Either way, raptaboard finds the build in the workspace's `target/` folder, and reloads the widget
whenever you rebuild. Its widget type, `plugin:number`, is in **Add as** for number topics.

## Alerts: a table widget

[`alerts/`](https://github.com/tuffinmuffin/raptaboard-examples/tree/main/alerts) does what the
built-in Alerts widget does. Its code is longer, mostly drawing, but the new ideas are few.

### Matching a table

WPILib's alert groups are tables with `.type` `"Alerts"`, holding three string arrays: `errors`,
`warnings` and `infos`. The manifest matches that `.type`, and asks for sound, and to keep running
while its tab is closed:

```toml
[match]
type = "Alerts"

[size]
default = [2, 2]

[properties]
all_groups = { type = "bool", default = false }
min_level = { type = "enum", default = "info", options = ["info", "warning", "error"] }
sound = { type = "bool", default = false }
tone = { type = "sound", default = "beep" }

[sandbox.values]
errors = ["Encoder lost"]
warnings = ["Battery low", "Hot motor"]

[permissions]
sound = true
background = true
```

### Subscribing to what it needs

A table widget names the keys it wants, relative to its table. With `all_groups` on, it asks for
every table of that type on the server instead, including ones that appear later; their values
arrive by full path (`/SmartDashboard/Intake/errors`):

```rust,noplayground
fn subscriptions(&self) -> Vec<Subscription> {
    if Settings::read().all_groups {
        vec![Subscription::TablesOfType("Alerts".into())]
    } else {
        vec![
            Subscription::Key("errors".into()),
            Subscription::Key("warnings".into()),
            Subscription::Key("infos".into()),
        ]
    }
}
```

The dashboard asks again when a setting changes, so switching `all_groups` takes effect at once.

### Every change, not just the newest

`render` sees only each value's newest state. To notice something that happened between frames, such
as a new error that was raised and cleared quickly, a widget implements `update`, which gets every
change since the last call, oldest first. Alerts uses it to spot errors it hasn't seen before:

```rust,noplayground
fn update(&mut self, samples: Vec<Sample>) {
    self.see(samples);   // remember new errors, for the pulse and the tone
}
```

### Sound, a pulse, and the tab mark

When `render` finds a new error, it returns its picture and plays the `tone` setting. A failure
(sound off, say) goes to the dashboard's log rather than stopping the widget:

```rust,noplayground
if let Some(tone) = tone
    && let Err(e) = host::play_sound(&tone)
{
    host::log(&format!("no sound: {e}"));
}
```

While it pulses, its picture sets `redraw_in_s`, so the dashboard draws it again 30 times a
second without a new value. Otherwise a picture is only drawn again when something changes.

`tab_mark` puts the most urgent active level on the widget's tab. With `background = true` the
dashboard keeps sending values while the tab is closed, so the mark is right from any tab:

```rust,noplayground
fn tab_mark(&self) -> Option<Level> {
    let settings = Settings::read();
    let current = self.current(settings.all_groups, &host::get);
    mark(&current.unwrap_or_default(), settings.min_level)
}
```

### Its tests

The harness sets several table keys at once (`set_all`), moves time on (`advance`), and keeps the
sounds the widget asked for (`take_sounds`) and whether it asked to be drawn again
(`redraw_in_s`). The file's `alerts()` makes the widget and `group()` sets its three lists. This test checks that an error already up when the widget first looks is quiet,
and a new one pulses and plays the tone:

```rust,noplayground
#[test]
fn a_new_error_pulses_and_sounds_once_but_not_on_the_first_look() {
    let mut w = alerts();
    w.property("sound", true).property("tone", "chirp");
    group(&mut w, vec!["Already up"], vec![], vec![]);
    let first = w.render();
    assert!(w.take_sounds().is_empty(), "already up on the first look");
    assert_eq!(first.redraw_in_s(), None);
    group(&mut w, vec!["Brownout", "Already up"], vec![], vec![]);
    assert!(w.advance(1.0).render().redraw_in_s().is_some(), "pulsing");
    assert_eq!(w.take_sounds(), ["chirp"]);
}
```

`w.tab_mark()` reads the widget's tab mark the same way.

See
[`alerts/tests/harness.rs`](https://github.com/tuffinmuffin/raptaboard-examples/blob/main/alerts/tests/harness.rs)
for the rest.

## Taking input

Neither example takes clicks, but a widget can. Add **regions** to the picture: parts that can be
clicked, tapped or reached with the keyboard, each with a label for screen readers and tests. The
dashboard sends `Input::Activate(name)` to `on_input` when one is used. Pointer, wheel, key and text
input come to `on_input` too. To send a value to the robot, the plugin needs `publish = true` and
calls `host::publish` with a key under its table.

## Sharing it

Release the `.wasm` and its `plugin.toml` (renamed for the plugin: `number.wasm` and `number.toml`)
from your repository, then put them in a [content pack](content-packs.md) so teams can install it
from raptaboard. raptaboard-examples' Release workflow does this on each `vX.Y.Z` tag.
