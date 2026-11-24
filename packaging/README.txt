raptaboard {VERSION}
====================

An FRC driver dashboard from FRC Team 3200 (Raptacon). It opens Elastic
layouts, shows NetworkTables values as widgets, and saves layouts back.

  Project, help and newer versions: https://github.com/tuffinmuffin/raptaboard
  What's new:                        CHANGELOG.md
  License:                           LICENSE.txt (MIT); third-party licenses
                                     in THIRD-PARTY-LICENSES.txt, and in the
                                     app: Settings (gear), About and licenses

Starting it
-----------

Windows (installer):  Start menu > raptaboard.
Windows (zip):        unzip anywhere (a USB stick is fine) and run raptaboard.exe.
macOS:                move raptaboard.app to Applications and open it.
Linux:                run ./install.sh to add it to your applications menu,
                      or run ./raptaboard straight from this folder.

These builds aren't signed yet, so the first start asks first:

  Windows: "Windows protected your PC". Click "More info", then "Run anyway".
  macOS:   "raptaboard can't be opened". Right-click (or Control-click)
           raptaboard.app, choose Open, then Open again. If macOS still
           refuses: System Settings > Privacy & Security > "Open Anyway".

First steps
-----------

1. Type your team number into "Team" in the settings bar at the top, and pick
   how to reach the robot beside it: "Driver Station" on the driver station
   laptop, "Team Number (10.TE.AM.2)", or "localhost (sim)" for robot code
   simulated on this computer.
2. Open a layout (Ctrl+O; Cmd+O on macOS), or drag topics from the
   NetworkTables tree on the left onto the grid to make widgets.
3. Save it (Ctrl+S). raptaboard opens the last layout again next time.

The "Locked" / "Unlocked" button in the settings bar switches between them:
unlocked, widgets can be moved, resized and edited; lock it for driving so
nothing moves by accident.

Content packs from a terminal
-----------------------------

Extra sounds (content packs) can be listed, installed, updated and removed
without opening the dashboard (Settings > Content packs does the same):

  raptaboard-cli packs list

Then "packs install <id>", "packs update", "packs remove <id>"; add --help to
any of them for more. raptaboard-cli comes with raptaboard:

  Windows:  on the PATH after the Microsoft Store or the installer adds it
            (an option while installing, on by default); from the zip, run
            it from its folder.
  macOS:    inside the app. To type it by name, link it into a folder on
            your PATH once, for example:
              ln -s /Applications/raptaboard.app/Contents/MacOS/raptaboard-cli ~/.local/bin/
  Linux:    install.sh puts it beside raptaboard in ~/.local/bin; from the
            .tar.gz, run ./raptaboard-cli.

Where things are kept
---------------------

Settings:  .raptaboard/settings.toml in your home folder. The Windows zip
           has its own .raptaboard folder beside raptaboard.exe, so its
           settings stay with that folder.
Images:    images imported for widgets are copied to .raptaboard/images in
           your home folder. Images in an "images" folder beside the program
           are offered too.
Fields:    the 2026 field is built in. More field JSONs (WPILib's format) go
           in .raptaboard/fields in your home folder, or a "fields" folder
           beside the program.

Linux needs the usual desktop libraries (X11 or Wayland, and ALSA for
sound); most desktops have them already.
