# Building from source

Install Rust with [rustup](https://rustup.rs) (stable, 1.95 or newer), then:

```sh
cargo run -p raptaboard -- crates/layout/tests/data/elastic_sample.json
cargo test --workspace
```

To connect to robot code running in simulation on this computer, add `--address sim`
(`layouts/robot2027_sim.json` is a layout made for the robot-2027 sim). Use `--team N` for another
team's robot, `--port N` for a server on another port, `--hide-tree` to start with the NT tree
hidden (the "◀ Tree" button in the settings bar toggles it), and `--help` for every option.

On Linux you also need the windowing libraries:

```sh
sudo apt install libxkbcommon-x11-0 libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
```

## This book

The book is in `book/`, built with [mdBook](https://rust-lang.github.io/mdBook/) 0.5.4:

```sh
cargo install mdbook --version 0.5.4 --locked
mdbook serve book --open           # rebuilds as you edit
python3 scripts/check_book.py      # every link and image resolves, as CI checks
```
