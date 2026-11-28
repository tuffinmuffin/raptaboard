# Tutorial: adding a built-in widget

This chapter walks through a real widget, **Battery**, from the code in the repo. Battery shows a
voltage as a battery that fills and changes color. It's small, but it uses each part a widget
needs: reading a value, drawing, settings, tests and docs. The code shown here is included from
the source files, so it's always the current version.

A built-in widget is compiled into raptaboard. To add a widget *without* changing raptaboard, write
a [plugin](../plugins/overview.md) instead.

## 1. Plan it

Before writing code, decide:

- **What it shows.** Which NetworkTables types it can show (Battery: `double`, `float`, `int`),
  and whether it should be the *default* widget for a type, made when that topic is dragged from
  the tree. Battery isn't: a number dragged from the tree makes a Text Display, and Battery comes
  from **Add as**. Most new widgets should be like that, so dragging stays predictable.
- **Its settings and their names.** If Elastic has the same widget, use Elastic's `type` name and
  setting keys exactly, so Elastic layouts open unchanged. Battery is raptaboard's own, but it
  reuses Number Bar's `min_value` and `max_value` for its range, so a widget changed from one to
  the other keeps them.
- **What it does with no value**, or a value of the wrong type. Every widget must draw something
  sensible before the robot connects.

## 2. The file and the type

Each built-in widget is one file in `crates/app/src/widgets/builtin/`, starting with a `//!` comment
that says what it is in plain words. The widget is a unit struct (`pub struct Battery;`) that
implements the `WidgetType` trait (`crates/app/src/widgets/mod.rs`). The first methods say what it
is:

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:identity}}
```

- `names` is the `type` saved in layout files. The first is the one raptaboard writes; add others if
  Elastic has older names for the same widget.
- `accepts` says whether a topic of that type, dragged from the tree, becomes this widget.
- `shows` says whether this widget can show that type. It decides what **Add as** and the settings
  dialog's **Widget** list offer.

## 3. Logic you can test on its own

Keep the arithmetic out of the drawing code, in small functions with a doc comment and a doctest.
The doctest runs with `cargo test`, and it shows a student how to call the function:

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:charge}}
```

Notice the backwards range: if someone sets "full" below "empty", the function still returns a
sensible answer instead of dividing by zero.

## 4. Reading the value and the settings

A widget's settings are a JSON map, `widget.properties`. The `props` helpers
(`widgets/shared/props.rs`) read them with a default, so a setting that isn't saved uses the default.
The newest value of the widget's topic is `live.value`:

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:reading}}
```

With no value, or one that isn't a number, Battery draws an empty battery and says why. Each
setting's key is a named constant with a doc comment:

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:keys}}
```

## 5. Drawing

A widget can be drawn in three places, so `WidgetType` has three drawing methods:

| Method | Where | What Battery does |
| --- | --- | --- |
| `show` | On the grid, with a full `egui::Ui`. | Draws the card (`card::draw_card`: background and styled title), then the battery in the card's body. |
| `show_body` | As a row of a List Layout, without a card. | The battery alone. |
| `draw_body` | With only a painter: while the widget is being dragged, or as a preview. | A plain battery, 70% full. |

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:drawing}}
```

A widget that sends values to the robot, such as a toggle or a slider, returns the new value from
`show` or `show_body` (`Some(Value)`), and the app sends it. Battery only shows, so it returns `None`.

**Give it an accessible name.** Screen readers read it, and so do the UI tests: they find widgets by
these names, as a person would.

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:accessible}}
```

## 6. Settings

`settings` adds the widget's own rows to the settings dialog, after the rows every widget has
(title, topic, update period). The `props` row helpers save a value only when it isn't the
default, which keeps layout files short, and name their control by the row's label, so screen
readers can say what it is. A row written by hand does the same: `let name = ui.label("…").id;`,
then `.labelled_by(name)` on its control. `setting_keys` lists every key, so **Reset settings**
knows what to remove.

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:settings}}
```

## 7. Register it

Two lines make raptaboard use it:

1. `pub mod battery;` in `crates/app/src/widgets/builtin/mod.rs`.
2. `&builtin::battery::Battery,` in the `TYPES` list in `crates/app/src/widgets/mod.rs`. Order
   matters only for `accepts`: when two types accept a dragged topic, the earlier one wins.

Nothing in `ui/` names a widget type: the window only talks to the trait. The registration also
puts the widget into the tests that cover every widget type: a tab with one of each drawn against
live values, every widget's settings dialog opened and cancelled, and every control in its
settings having a name.

## 8. Tests

Write two kinds. **Unit tests** go in the widget's file, for its logic:

```rust,noplayground
{{#include ../../../crates/app/src/widgets/builtin/battery.rs:unit_test}}
```

**A window test** goes in `crates/app/src/ui/tests/`. It runs the whole app headless against a real
NetworkTables server on this computer, publishes values, and checks what the widget shows. This one
finds the widget by its accessible name as the voltage changes:

```rust,noplayground
{{#include ../../../crates/app/src/ui/tests/battery.rs:ui_test}}
```

Then **check that the tests can fail**: break the code on purpose (swap two colors, say) and make
sure a test goes red, then put it back. A test that passes whatever the code does isn't testing it.
[Testing and coverage](testing.md) has more.

## 9. Docs and pictures

- Add the widget to the book's [Widgets](../guide/widgets.md) chapter, with a table of its
  settings named as the dialog shows them.
- Put one on the sim layout (`layouts/robot2027_sim.json`), then take its picture with the
  robot-2027 sim running (the command is below). The recorder saves a picture of the first card of
  each widget type, named after the type (`book/src/images/widgets/battery.png`).
  `RAPTABOARD_SHOTS` limits it to the pictures named: leave the others as they are, since the
  sim's values move and every retaken picture changes a little.
- Add a line to `CHANGELOG.md` under the version being built, and update `docs/spec.md` and
  `CLAUDE.md` where they list widgets.

```sh
RAPTABOARD_SHOTS=battery cargo test -p raptaboard --lib record_docs_screenshots -- --ignored
```

## Other things a widget can do

`WidgetType` has more methods, each with a default that does nothing. Override the ones you need:

| Method | For | Example |
| --- | --- | --- |
| `table_types` | A widget bound to a whole table, by its `.type`. | Choosers (`"String Chooser"`), Mechanism2d |
| `sends_to` | Sending to a topic other than the widget's own. | A chooser sends to its table's `selected` |
| `extra_topics` | Values of other topics, in `live.frame.others`. | A swerve view's heading topic |
| `traced` | Every value over time, not just the newest, recorded for you. | Graph, X/Y Graph |
| `takes_topics`, `add_topics` | Taking topics dropped onto the widget from the tree. | A Graph adds lines |
| `chosen` | Setting itself up when it's picked for a topic. | Enum widgets guess where their names come from |
| `resettable` | A **Reset** item in its right-click menu. | Min/max, autoscale |
| `tab_mark` | A sign on its tab, seen from other tabs. | Alerts |
| `state_sounds` | Playing sounds when the robot's state changes. | FMSInfo |
| `default_period` | Wanting values more often than 16 a second. | Graphs |

The doc comments on each method in `crates/app/src/widgets/mod.rs` have the details, and the
widgets listed show them in use.
