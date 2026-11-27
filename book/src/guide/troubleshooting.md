# Troubleshooting

## Installing and starting

**Windows says "Windows protected your PC".** raptaboard's downloads from GitHub aren't code-signed
yet. Click **More info**, then **Run anyway**. The Microsoft Store version doesn't ask. See
[Install](../install.md).

**macOS says it can't check raptaboard for malicious software.** Right-click raptaboard.app,
choose **Open**, then **Open** again. macOS remembers the choice.

**Linux: it won't start, or complains about a library.** It needs the usual desktop libraries
(`libxkbcommon`, `libxcb`); see [Building from source](../dev/building.md) for the list.

## Connecting

**The status stays on "Connecting…".** Check, in order:

1. The team number in ⚙ Settings, and the **Server** choice: Team Number for a roboRIO, SystemCore
   mDNS or WiFi for a SystemCore, localhost for the simulator. See
   [Connecting to the robot](connecting.md).
2. That this computer is on the robot's network: its WiFi, or a cable to the radio.
3. That robot code is running, and is a WPILib version with NetworkTables 4 (2023 or later).
4. A firewall on this computer blocking port 5810.

**"Waiting for Driver Station".** The Server is set to Driver Station, and the Driver Station on
this computer hasn't given the robot's address yet. Start the Driver Station, or set the Server to
Team Number.

**It connects to the simulator but not the robot**, or the other way round: the Server choice
picks one. `--address sim` on the command line uses the simulator for one run without changing
the saved setting.

## Widgets

**A widget says "no value".** Nothing has been published on its topic since connecting. Check the
topic's name in the tree: the widget shows its topic under **Shows** in its settings. Robot code may
publish it only in some modes, or under another name.

**Values stopped changing.** Check the connection status in the settings bar: while it's
disconnected, widgets keep the last values they had (the Alerts widget greys its list). raptaboard
reconnects by itself when the robot is back.

**A widget says "not a number" or "not text".** The topic's type doesn't suit the widget, for
example a string on a Number Bar. Change the widget's **Widget** in its settings to one that shows
that type, or check robot code publishes the type you expect.

**Nothing moves when I drag a widget, and there's no + for tabs.** The layout is locked. Click
**🔒 Locked** in the settings bar to unlock it.

**A toggle or slider doesn't change the robot.** Values can only be sent while connected; a short
tone says one couldn't be sent. Check that robot code reads the same topic, and doesn't overwrite it
straight back.

**A graph is empty after switching tabs.** Widgets only get values while their tab is open, so a
graph's history starts when its tab is shown. Keep the graph's tab open to record.

**A "Missing plugin" card.** The layout uses a plugin widget that isn't installed here. Install its
pack in ⚙ Settings → Content packs; the widget kept its settings. See
[Content packs and plugins](content-packs.md).

**A plugin widget says it stopped.** It hit a bug or a limit. Click **Restart**. If it keeps
stopping, tell the plugin's author what it says.

## Layouts

**"⚠ fixed N problems" after opening a layout.** The file had places or sizes raptaboard couldn't
use (a widget of zero size, off the grid, or on top of another; a grid size out of range; no tabs),
and it changed them so the layout opens. A layout Elastic saved normally needs no fixes. Hover the note to
read what changed; right-click it to copy the list. Saving keeps the fixed values.

**An Elastic layout looks different.** raptaboard opens Elastic's layouts and saves them so Elastic
can open them again, but draws some widgets its own way. Widgets Elastic has that raptaboard doesn't
show as cards with their type and topic, and keep their settings.

## Sound

**No sounds.** Check the 🔔 button in the settings bar (🔕 is muted), the computer's volume, and
that the widget's sound is set: FMSInfo and Alerts each have their own sound settings. A sound from
a pack that isn't installed shows as "not installed" and plays the built-in sound of its name, or
nothing.

## Content packs

**No packs listed.** Packs need the internet to list and download. **Check for packs** in ⚙
Settings → Content packs tries again and says why it failed. School and team networks that check
secure connections with their own certificates work, since raptaboard uses the computer's own.

## Starting fresh

raptaboard keeps everything in a `.raptaboard` folder: usually in your home folder, or beside the
program for the portable Windows zip (see [First start](first-start.md#your-settings)).

- To reset the **settings** (team, address, window), close raptaboard and delete
  `.raptaboard/settings.toml`.
- Deleting the whole `.raptaboard` folder also removes installed content packs, imported images and
  what plugins saved.

Your layouts are separate files, wherever you saved them, and aren't touched by either.

## Reporting a problem

Open an issue on [GitHub](https://github.com/tuffinmuffin/raptaboard/issues) with what you did,
what happened, raptaboard's version (⚙ Settings → About and licenses), and your system. Starting
raptaboard from a terminal shows messages that may help. If it's about a layout, attach the layout
file.
