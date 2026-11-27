# Content packs and plugins

**Content packs** add things to raptaboard without installing a new version: sound clips, widgets
made as plugins, and whole tabs such as a game. raptaboard never downloads one by itself: you pick
what to install, and it shows each pack's license and what its plugins may do first.

## The first start

On its first start, and the first start after an upgrade, raptaboard offers the packs you don't have
yet in a window, **Extra content for raptaboard**. Each one has its license and credits; the
suggested ones are ticked. **Download** installs the ticked ones; **Not now** installs nothing. With
no internet the list waits for the next start. It's never shown while the robot is enabled.

## The Content packs window

Open it from **⚙ Settings → Content packs…**. It lists every pack the sources have, with:

- **Install**, **Update** (a newer version is out) or **Remove**;
- **Credits and license**, the full text;
- for a pack with plugins, what each plugin may do (below);
- "Needs raptaboard x.y.z or newer" for a pack this version can't use.

**Check for packs** reads the sources' lists again. Installed packs work at once, without
restarting raptaboard.

**Pre-releases** also lists packs that are still being tried, before they're released. Leave it off
unless you're helping test one.

### Staying up to date

raptaboard checks for new and updated packs in the background, every hour unless you change it
(**Check for updates every**, under Content packs in ⚙ Settings; 0 never checks). It only checks while the robot
isn't enabled. When it finds something, the file bar shows **📦 Content: 1 update**: **Review**
opens the window, and **✕** hides it until something newer comes.

## What a plugin may do

Before you install a pack with plugins, the window lists what each plugin asks for:

- **send values to the robot**, like a button or a slider;
- **play sounds** (the 🔔 button still mutes them);
- **keep running while its tab is closed**, to keep its tab's warning sign current;
- **save its settings on this computer**, up to a stated size;
- **fill a tab**, **hold the mouse** while it has the keyboard (a game), or more memory or time than
  usual.

raptaboard holds each plugin to that list: a plugin that didn't ask to send values can't. Plugins
run in a sandbox: they can't read your files, reach the internet, or slow the dashboard down.

## Using plugin widgets

A plugin's widgets appear with the built-in ones: in **Add as** on a topic in the tree, and in the
**Widget** list of a widget's settings, after the built-in choices. Their names start with
`plugin:` in layout files. They're set up in the settings dialog like any widget.

If a plugin stops (a bug, or it used more than it's allowed), its widgets say why and offer
**Restart**; the rest of the dashboard carries on. If it keeps stopping, tell whoever made the
plugin what the message says.

A layout that uses a plugin you don't have shows those widgets as **Missing plugin: name** cards.
They keep their settings, and come back when the pack is installed.

## Tab plugins

A pack can bring a plugin that fills a whole tab, such as the **Doom: the 2026 field** pack. Make an
empty tab with the plugin's name (`Doom`), and the plugin fills it. Click the tab's contents to give
it the keyboard (and the mouse, for a game); **Shift+Escape** gives them back to the dashboard.

## Sounds from packs

A pack's sounds appear in the sound lists of FMSInfo and Alerts. A pack sound with the same name as
a built-in one (`fire 1`) plays in its place while the pack is installed. Remove the pack and the
built-in sound comes back. A layout that picked a sound from a pack you don't have shows it as
"not installed" and plays the built-in sound of that name, or nothing.

## Other sources

The official packs come from raptaboard's own source, checked against a signature built into
raptaboard. Your team or another can publish packs too: add the address of their list under
**Sources** with **Add source**. Packs from other sources aren't signed by raptaboard, so add only
sources you trust. Every file is still checked against its list before it's used.

## From the command line

The same can be done without opening the window, which is handy for setting up several Driver
Station laptops:

```sh
raptaboard packs list               # what's installed, and what the sources have
raptaboard packs install fan-clips  # shows the license and asks first; -y accepts it
raptaboard packs update
raptaboard packs remove fan-clips
```

Installed packs are kept in `.raptaboard/content/`, beside your settings (see
[First start](first-start.md#your-settings)). To make your own pack or plugin, see
[Writing plugins](../plugins/overview.md).
