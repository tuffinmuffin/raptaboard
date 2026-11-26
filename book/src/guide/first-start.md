# First start

Start raptaboard from the Start menu (Windows), Applications (macOS) or your app menu (Linux), or
run `raptaboard` from a terminal. The first time, the system may ask whether to run it: see
[Install](../install.md).

## What you see

A new, empty layout called **Untitled**, with two empty tabs, **Teleoperated** and
**Autonomous**, the way Elastic starts. The NetworkTables tree is on the left (empty until you
connect), and the layout is unlocked, so widgets can be added and moved.

At the top right, the status says **Set a team number**: raptaboard isn't tied to a team, so it
doesn't know where your robot is yet. [Connecting to the robot](connecting.md) is next.

## Extra content

raptaboard offers its content packs once, on the first start and again after an upgrade: a
window, "Extra content for raptaboard", lists them with their licenses, the suggested ones ticked.
**Download** fetches the ticked ones; **Not now** fetches nothing. Either way, packs can be added
or removed any time in ⚙ Settings → Content packs. Nothing is downloaded unless you choose it, and
with no internet the list waits for the next start.

## Your settings

raptaboard remembers its settings between runs: the team, the robot address and port, whether
the tree is shown, the lock, the layout you last opened or saved and its tab, the window's size and
place, sounds, and how often to check for content updates.

They're kept in `settings.toml`, in a `.raptaboard` folder:

- **Usually your home folder:** `~/.raptaboard/settings.toml` (on Windows,
  `C:\Users\<you>\.raptaboard\settings.toml`), made on the first start.
- **The portable Windows zip** has its own `.raptaboard` folder beside the program, so it carries
  its settings with it.

Options on the command line win over the file for that run (`raptaboard --help` lists them). The
same folder holds installed content packs, imported images, and what plugins save.
