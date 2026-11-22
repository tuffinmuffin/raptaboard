# App icon

raptaboard's icon: a gauge and two widgets on a dashboard tile. `raptaboard.svg` is the source;
the program draws its window icon from it, and the release packages use the files made from it:

| File | Used by |
| --- | --- |
| `raptaboard.ico` | Windows: built into `raptaboard.exe` (`crates/app/build.rs`) and the installer |
| `raptaboard.icns` | macOS: the app bundle |
| `raptaboard.png` | Linux: the menu entry (512 px) |

After changing the SVG, remake them from the repo's top folder (on macOS, for the `.icns`):

```sh
cargo run -p raptaboard --example make_icons
```

Drawn for raptaboard (Team 3200, Raptacon) and dedicated to the public domain under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
