# Testing and coverage

```sh
cargo test --workspace                                  # everything that runs on its own
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Run all three before pushing; CI runs the same on Linux, Windows and macOS and fails on any
warning.

## Kinds of tests

| Kind | Where | What it's for |
| --- | --- | --- |
| **Unit tests** | A `mod tests` at the bottom of each file | Logic on its own: a widget's arithmetic, parsing a setting, reading a struct. Fast, and most tests should be these. |
| **Doctests** | The doc comment on a public item | Short examples that show how to call something. They run with `cargo test` too, so the docs can't go stale. |
| **Drawing tests** | A widget's own `mod tests`, using `egui::Context::run_ui` | What a widget draws: the text and shapes it paints for given values, without a window. |
| **Window tests** | `crates/app/src/ui/tests/` | The whole app, headless (`egui_kittest`), connected to a real NetworkTables server started by the test on this computer. Click, drag and type as a user would, and find things by their accessible names. |
| **Layout tests** | `crates/layout/tests/` | Elastic layouts read and written back without losing anything, and bad files made safe. |
| **Plugin tests** | `crates/plugin-host/tests/`, `plugins/`, and plugins' own repos with `raptaboard-plugin-test` | Plugins built for WebAssembly and run in the real plugin runtime. |
| **Connection tests** | `crates/app/tests/nt_connection.rs` | Connecting, reconnecting and time sync against a real server. |

### Window tests

`ui/tests/mod.rs` has the helpers. `Robot::start()` starts a NetworkTables server on a free port;
publish topics on it (`robot._server.publish::<f64>("/Robot/battery")`) and set values as robot code
would. `harness_for(&robot)` makes the app connected to it. Then:

- `settle(&mut harness)` runs frames until the app is idle.
- `labelled_soon(&mut harness, "Battery: 12.60 V, 80%, ok")` waits until something has that
  accessible name. Values arrive over the network, so wait for them rather than checking at once.
- `harness.get_by_label("Done").click()` clicks by name; `click_at`, `drag_from` and `key` send
  pointer and key events by position.
- `dashboard_has` and `server_sees` check what the dashboard sent back to the server.

The Battery test in the [widget tutorial](widget-tutorial.md#8-tests) is a short example to copy.
Each widget type is also covered by the tests that open a tab with one of every type, so a new type
gets those for free.

## Rules for tests

- **Prove a test can fail.** After writing one, break the code it tests on purpose (flip a
  condition, drop a line) and check the test goes red, then put the code back. A test that passes
  whatever the code does isn't testing it.
- **No internet.** Tests never reach the network: content downloads use a fake web
  (`content::tests::Web`), and servers run on this computer. The one test that uses the real internet
  is `#[ignore]`d.
- **No real sound or files.** Tests use `Sound::silent()` and temporary folders, never your
  `~/.raptaboard`.
- **Mark tests Claude wrote** with a `// Tests written by Claude.` comment.

## Tests that are ignored

Some tests are `#[ignore]` because they need something CI doesn't have. Run them by name with
`--ignored`:

| Test | Needs | Makes |
| --- | --- | --- |
| `record_docs_screenshots` | A GPU, and the robot-2027 sim running | The book's pictures (`book/src/images/`) |
| `record_store_screenshots` | A GPU, and the robot-2027 sim running | The Microsoft Store pictures (`media/store/`) |
| `record_field_clip` | A GPU | The book's Field animation |
| `cpu_load` (release build) | A visible window, about 80 s | 30 widgets updating 50 times a second: raptaboard's CPU use, which should stay under 5% on a Driver Station laptop |

```sh
cargo test -p raptaboard --lib record_docs_screenshots -- --ignored --nocapture
# Only some of the book's pictures: file names, or folder/name
RAPTABOARD_SHOTS=battery,tabs/styles cargo test -p raptaboard --lib record_docs_screenshots -- --ignored
cargo test -p raptaboard --release --test cpu_load -- --ignored --nocapture
```

## Coverage

CI's `coverage` job measures which lines the tests run (with
[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)) and posts a report on each pull
request, compared with the base branch. The full HTML report is a download on the workflow run. To
see it locally:

```sh
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --ignore-filename-regex '(tests|examples|benches)/' --html --open
```

Coverage shows what isn't tested; it doesn't show that what is tested is checked properly. That's
what breaking the code on purpose is for.
