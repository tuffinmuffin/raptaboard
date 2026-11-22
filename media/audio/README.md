# Sound clips

Short clips built into raptaboard (`crates/app/src/sound.rs`, `Tone`), offered as alert and robot
state sounds. Each needs a line here saying where it came from and its license; the About window
(⚙ Settings, then "About and licenses") shows the same credits from `crates/app/src/credits.rs`,
so keep the two in step (a test checks every clip here has a credit there).

| File | Offered as | Source | License and credit |
| --- | --- | --- | --- |
| `danger.mp3` | "danger danger" | [danger_normal_lp.wav](https://freesound.org/s/436708/) by xpoki on Freesound (pack "Female Voice SciFi"), played twice and converted to MP3 (128 kb/s mono) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Credit: "danger_normal_lp.wav" by Sarah-Christin Freytag (xpoki), https://freesound.org/s/436708/, looped twice and converted to MP3 |
| `auto-ghosts.mp3` | "auto ghosts" | [MustphaMondeRemixes (3).aif](https://freesound.org/s/124570/) by dal_kas on Freesound, a remix of [Ghosts In The Machine.wav](https://freesound.org/s/124470/) by deleted_user_44129; converted to MP3 (192 kb/s stereo) | [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/). Credit: "MustphaMondeRemixes (3).aif" by dal_kas, https://freesound.org/s/124570/, from "Ghosts In The Machine.wav" by deleted_user_44129, https://freesound.org/s/124470/, converted to MP3 |
| `go-go-go.mp3` | "go go go" | [Go Go Go - (Version 2) Northern British Soldier](https://freesound.org/s/402643/) by theuncertainman on Freesound, converted to MP3 (128 kb/s mono) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Credit: "Go Go Go - (Version 2) Northern British Soldier" by theuncertainman, https://freesound.org/s/402643/, converted to MP3 |
| `fms-down.mp3`, `lost-comms.mp3` | "fms down", "lost comms" | Made for raptaboard on 2026-11-22 with Free TTS Online (text to speech): "FMS lost" and "comms lost" (MP3 48 kb/s mono, about 2 s each). They replaced the first Flite versions | Free TTS Online's terms for the audio it makes: confirm they allow shipping it in raptaboard before it's handed out widely |
| `disabled1.mp3`, `disabled2.mp3` | "disabled 1", "disabled 2" | "I'm disabled", a line from the TV series *The IT Crowd* | Not ours: a short quote kept for team (non-commercial) use, decided 2026-11-19. Remove before raptaboard is handed out more widely |
| `fire1.mp3`, `fire2.mp3` | "fire 1", "fire 2" | Not recorded yet | Unknown: find the source and license, or replace them |

## Adding a clip

Keep clips short (a few seconds) and MP3. Use only sounds that are ours to ship: our own
recordings, public domain (CC0), or a license that allows redistribution, with any credit it asks
for (CC BY) here and in `credits.rs`. Converting the clip from a WAV:

```sh
ffmpeg -i clip.wav -map_metadata -1 -c:a libmp3lame -b:a 128k -ac 1 clip.mp3
# Played twice in one file (what danger.mp3 is):
ffmpeg -stream_loop 1 -i clip.wav -map_metadata -1 -c:a libmp3lame -b:a 128k -ac 1 clip.mp3
```
