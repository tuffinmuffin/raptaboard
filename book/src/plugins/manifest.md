# plugin.toml reference

Every plugin has a `plugin.toml` beside its `.wasm`. raptaboard reads it before running the plugin:
it says what the plugin shows, what its settings are, and what it's allowed to do. An unknown key is
an error, so a typo doesn't go unnoticed.

```toml
name = "alerts"
version = "0.1.0"
api = "0.3"

[match]
type = "Alerts"

[size]
default = [2, 2]
min = [1, 1]

[properties]
show_text = { type = "bool", default = true }
tone = { type = "sound", default = "beep" }

[permissions]
sound = true
background = true

[sandbox.values]
errors = ["Encoder lost"]
```

## The top

| Key | What it is |
| --- | --- |
| `name` | The plugin's name. Its widget type is `plugin:<name>` in layouts. Use lowercase and `-`. |
| `version` | The plugin's version, like `0.1.0`. Raise it whenever it changes, so content packs offer the update. |
| `api` | The plugin interface it was built for, like `"0.3"`. Before 1.0 it must match raptaboard's exactly (each 0.x can change the interface); from 1.0, the same major version and a minor no newer. raptaboard 0.4 and 0.5 run API 0.3. |
| `tab` | For a [tab plugin](tab-plugins.md): the name of the tab it fills, like `"Doom"`. A tab plugin has no `[match]`. |
| `capture_mouse` | Tab plugins only: hold the mouse while the tab has the keyboard (a game turning with it). `false` unless set. |

## [match]: what it shows

A widget plugin shows either **one topic** or **a table**, not both.

**One topic:** list the NetworkTables types it can show.

```toml
[match]
topic_types = ["double", "float", "int"]
```

**A table:** give one or more rules; a table must pass all of them.

| Key | The table must... |
| --- | --- |
| `type` | have this `.type`, like `"Alerts"` or `"Arm"`. |
| `path` | have a path matching this pattern, like `/Drive/Module/*`. |
| `has` | have all of these keys under it, like `["angle_rad", "length_m"]`. |

```toml
[match]
type = "Arm"
```

When more than one widget could show a table, the most specific match wins: a `type` counts most,
then a `path`, then each key in `has`. Built-in widgets come before plugins.

## [size]

The widget's size in grid cells, `[width, height]`: `default` when it's made, and the smallest it can
be resized to (`min`). Both are `[1, 1]` unless set.

## [properties]: its settings

Each property is a setting in the widget's settings dialog, saved in the layout. Read them in the
plugin with `host::property` (the SDK's `property_number`, `property_bool`, `property_string`).

| `type` | The setting | Also needs |
| --- | --- | --- |
| `number` | A number | `default = 14` |
| `bool` | A checkbox | `default = true` |
| `string` | A line of text | `default = "m/s"` |
| `color` | A color picker | `default = "#FF8800"` (or `#RRGGBBAA`) |
| `enum` | A list to pick from | `default = "auto"`, `options = ["auto", "right", "below"]` |
| `sound` | The dashboard's sound picker: built-in sounds and installed packs' | `default = "beep"` (`""` for none) |

Using a built-in widget's setting names (`font_size`, `decimal_places`, `min_value`) lets a widget
switch between the built-in and your plugin and keep its settings.

## [permissions]: what else it may do

Everything is off unless asked for. raptaboard lists what a plugin asks for before it's installed
from a content pack, and refuses what it didn't ask for.

| Key | Lets the plugin |
| --- | --- |
| `publish` | Write values under its bound table (`host::publish`), such as a button sending a command. |
| `sound` | Play sounds (`play-sound`) and stream its own audio (`play-audio`). The 🔔 button still mutes them. |
| `background` | Keep getting values while its tab is closed, so its [tab mark](#its-tab-mark) stays current. |
| `storage` | Save small values on this computer between runs (`storage-get`, `storage-set`). |

## [limits]: more than the usual resources

A plugin normally gets 64 MiB of memory, 100 ms for each call into it (3 s for its first picture, to
load), and 64 KiB of saved values. Ask for more if it needs it; raptaboard shows this before
installing too.

| Key | What | Most |
| --- | --- | --- |
| `memory_mb` | Memory, in MiB | 512 |
| `call_ms` | The longest one call may take, in milliseconds | 1000 |
| `storage_kb` | Saved values in all, in KiB | 1024 |

A plugin that goes past a limit is stopped. Its widgets say why and offer **Restart**, and the rest
of the dashboard carries on.

## [sandbox]: sample values

What the sandbox window (`raptaboard plugin dev <folder>`) starts with, so the widget shows
something at once without a robot. For a topic plugin, `value`; for a table plugin, its keys under
`[sandbox.values]`:

```toml
[sandbox]
value = 1.23456

# or, for a table:
[sandbox.values]
errors = ["Encoder lost"]
warnings = []
```

A TOML decimal is a `double`, a whole number an `int`, text a `string`, and a list a list of those (an
empty list is a `string[]`).

## Its tab mark

Not a key, but often asked: a widget plugin can put an error, warning or info sign on its tab by
returning one from `tab_mark` (see the [tutorial](tutorial.md)). With `background = true` it keeps
doing so while the tab is closed.
