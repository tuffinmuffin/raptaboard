# App icon

raptaboard's icon: Raptacon's raptor in Raptacon Green (`#84EC5C`) on a dark disc, inside a
rust-red (`#B7410E`) gear. `raptaboard.svg` is the source; the program draws its window icon from
it, and the release packages use the files made from it:

| File | Used by |
| --- | --- |
| `raptaboard.ico` | Windows: built into `raptaboard.exe` (`crates/app/build.rs`) and the installer |
| `raptaboard.icns` | macOS: the app bundle |
| `raptaboard.png` | Linux: the menu entry (512 px) |

After changing the SVG, remake them from the repo's top folder (on macOS, for the `.icns`):

```sh
cargo run -p raptaboard --example make_icons
```

The raptor is Raptacon's raptor mark, from Raptacon's brand kit
(https://github.com/Raptacon/brand-kit, `logo/raptor-mark.svg`), free to use and modify for team
purposes; the gear is drawn for raptaboard.
