# raptaboard privacy policy

Last updated: November 23, 2026

raptaboard is a driver dashboard for FIRST Robotics Competition robots, made by Team 3200
(Raptacon). This policy covers the raptaboard app, however you install it: from the Microsoft
Store or from GitHub. It's online at
<https://github.com/tuffinmuffin/raptaboard/blob/main/PRIVACY.md>, and raptaboard shows it too,
in ⚙ Settings → About and licenses → Privacy.

## We collect no personal data

raptaboard doesn't collect, store or share any personal information. It has no accounts, no
analytics, no advertising and no crash reporting, and it sends nothing to us or to anyone else.

## What raptaboard connects to

raptaboard connects only to the robot, or another NetworkTables server you choose (such as a
robot simulator), on your own network. It does this to show and send your robot's values. It
finds the robot from the team number and address you set; this may include asking the FRC Driver
Station on the same computer for the robot's address, or looking for the robot on your local
network.

### Content packs

raptaboard can download **content packs**: extra sounds, and later widget plugins, fields and
layouts. They come from raptaboard-content on GitHub, or from another source you add yourself.

- **Packs download only when you choose them**: from the list raptaboard offers, or with Install
  or Update in ⚙ Settings → Content packs.
- **The list of packs** (a small file, the same for everyone) is read so raptaboard can tell you
  about new packs and updates:
  - on its first start, and on the first start after raptaboard is updated;
  - when you open the Content packs window or press "Check for packs";
  - in the background about once an hour, only while the robot isn't enabled. Change how often,
    or turn this off, in ⚙ Settings ("Check for updates every … min"; 0 for never).
- **What these requests send:** nothing about you. Like any download, the server sees your IP
  address and that the request comes from raptaboard and which version. We don't receive these
  requests. GitHub handles its own under the
  [GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).
- **Without the internet** (on the robot's network or at an event), raptaboard works the same,
  using the packs already installed; a check that can't connect just stops.

Otherwise raptaboard doesn't connect to the internet. Links in its About window open in your web
browser only when you click them.

## What raptaboard stores on your computer

raptaboard keeps these files on your computer, and never sends them anywhere:

- its settings, such as your team number, robot address, window size and the last layout you
  opened, in a `.raptaboard` folder in your home folder;
- the dashboard layouts you save, wherever you choose to save them;
- images you add to widgets, copied into the `.raptaboard` folder;
- the content packs you download, in `.raptaboard/content`, with a note of which packs you've
  been offered or dismissed and the raptaboard version that last started (`state.json`), and
  the content sources you add, in its settings.

You can delete any of these at any time. Uninstalling raptaboard doesn't remove them; delete the
`.raptaboard` folder in your home folder to remove its settings.

## Microsoft Store

If you install raptaboard from the Microsoft Store, Microsoft handles the download, installation
and updates under the
[Microsoft Privacy Statement](https://privacy.microsoft.com/privacystatement). We don't receive
any personal information about you from Microsoft.

## Changes

If a future version of raptaboard changes what it connects to or stores, we'll update this policy
before that version is released, and change the date at the top.

## Contact

Questions about this policy: open an issue at
<https://github.com/tuffinmuffin/raptaboard/issues>.
