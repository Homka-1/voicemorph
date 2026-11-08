# How the original changes your voice (and what this app uses)

Studied: `ClownfishVoiceChanger.exe` (UI, settings, hotkeys) and `ClownfshAPO64.dll`
(the audio-processing module that does the actual effects). Addresses are in those files.

## Effect ids and behaviour — process function `0x18002d000` (APO)

| Effect | id | What it does | Numbers |
|---|---|---|---|
| Robot | 4 | multiplies the voice by a sine wave | table length = sample rate / 25 → **25 Hz** (`0x180003fa0`, 2π at `0x180051cd8`) |
| Male | 5 | pitch shift | **−3** semitones |
| Female | 6 | pitch shift | **+4** |
| Helium | 7 | pitch shift | **+8** |
| Baby | 8 | pitch shift | **+12** |
| Alien | 9 | plays short chunks backwards with fade in/out | chunk = **20 audio periods** (`0x180003fa0`, ×20); fade = 1 period |
| Mutation | 10 | pitch sweeps up and down | **−4 … +13** st (`0x180051d5c`, `0x180051d10`), step **0.1** st per period |
| Slow mutation | 11 | same | step **0.01** |
| Fast mutation | 12 | same | step **0.3** |
| Atari (8-bit) | 13 | pitch flips between two values | counter −3…4 (`0x180051d58`, `0x180051d00`) step 1; pitch **+6** going up, **−3** going down |
| Custom pitch | 14 | pitch from slider | slider step **0.05** (exe `0x4cc498`), default **4.50** (dialog text) |
| Clone | 15 | adds a delayed copy | delay **6000** samples, both parts × **0.5** (`0x180051c2c`) |
| Old radio | 16 | keeps only the positive half of the wave | max(0, x) |
| Silence | 17 | mutes | 0 |
| Chainer | 20 | runs up to **4** effects one after another | settings bytes 9–12 |

Pitch shifting in the original uses a time-stretch library with sequence **40 ms**, seek window **15 ms**,
overlap **8 ms**, quick seek off, anti-alias filter on (`0x180003fa0`). This app uses its own
low-latency two-tap pitch shifter with the same **40 ms** window.

Update period for the moving effects (Mutation, Atari, Alien chunk, silence gate): the original steps once
per audio period. **10 ms — estimated** (Windows' usual audio period; not stored in the file).

## Room effects (second stage, settings byte 6)

| Room | id | Numbers |
|---|---|---|
| Cave | 1 | one echo, delay = ¼ of the ¼-second buffer = **62.5 ms**, gain **0.5** |
| City hall | 2 | one echo, delay **0.25 s**, gain **0.25** (`0x180051c20`) |
| Chorus | 3 | (voice + voice 400 samples ago) × **0.5** |
| Ghost | 4 | the last **0.5 s** played backwards, gain **0.5** |

Buffer length = sample rate × channels / 4 (set in `0x180003fa0`).

## Settings packet — handler `0x180002d70`

| Byte | Meaning | Formula |
|---|---|---|
| 0 | voice effect id | |
| 1–4 | custom pitch (float, semitones) | |
| 5 | silence threshold | value / 100 (`0x180051d18` = 100); muted after **10** quiet periods in a row |
| 6 | room effect | |
| 7 | sound/music mix volume | value / 1000 (`0x180051d20`) |
| 13 | AI noise removal on/off | |
| 16 / 17 | bass / treble | value × **0.5 dB**, range **±30 dB** (`0x180003720`) |
| 19 | output boost | gain = 1 + value × **0.05**, up to 100 → ×6 (`0x180051c10`) |

Bass/treble filters: shelves at **250 Hz** and **4000 Hz**, slope **0.4** (`0x180003fa0`).
Output is clamped to −1…+1 (`0x180051d38`, `0x180051c60`).

Noise removal in the original is a neural denoiser working on **480-sample** frames with 22 bands
(`0x1800079d0`). This app uses its own spectral noise remover instead (**different method — estimated tuning**).

## Hotkeys — exe `0x44e610`, help text `0x4c6f20`

Hold the chosen modifier key(s) (two settings, "None" for both turns hotkeys off), then:
F4 on/off · F7 voice assistant · F8 previous voice · F9 next voice · F10 stop sound/music ·
F11 play music / next track · F12 play assigned sound. Keys are polled with GetAsyncKeyState.
Default modifiers: **Ctrl + Shift — estimated** (defaults not found in the file).
Voice order for F8/F9: table `0x4c8c50` (None, Alien, Atari, Clone, Mutation, Fast, Slow, Male, Female,
Helium, Baby, Radio, Robot, Custom, Silence, Chainer).

## Other features of the original

Sound player (sounds folder + your own), music player with loop/shuffle and its own volume, "hear microphone"
/ "hear music" monitoring with a local volume, vocoder with carrier sounds from its `vocoders` folder,
VST plugin folder, cloud text-to-speech and an online AI voice assistant.

## How the original replaces the microphone — exe `0x40dc50`

The original doesn't make a new microphone: it plugs its audio-processing module into the real one.
For each capture endpoint under `HKLM\...\MMDevices\Audio\Capture\{id}` it:

- reads the endpoint's `FxProperties` and checks the "enhancements disabled" switch
  `{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5`,
- backs up the effect values (`{d04e05a6-…},0…15`, `{d3993a3f-…},5/6/7`) to
  `HKLM\SOFTWARE\ClownfishAPO\BackupAPOs\{id}`, writing `!KEY` for values that weren't there,
- adds its class id `{C18E2F7E-933D-4965-B7D1-1EEF228D2AF3}` to the **composite endpoint-effect list**
  `{d3993a3f-99c2-4402-b5ec-a92a0367664b},7` (REG_MULTI_SZ) — seen on this PC's active microphone.

Its module then runs inside Windows' audio service, so every app recording from that microphone gets the
changed voice; the app sends it the settings packet above.

**Removed from this app at the person's request** (it didn't work well on their PC). An earlier version
put its own add-on (class id `{7B3F5C2E-9A41-4D8E-B6F0-3C5A2E9D71A4}`) into the microphone the same way.
Settings now shows a "Remove it" banner while that add-on is still registered on any microphone; it
re-runs the app as admin with `--apo-uninstall`, which restores the backed-up values
(`HKLM\SOFTWARE\VoiceMorph\Backup\{id}`, `!KEY` = value wasn't there) and restarts Windows audio.

The app now uses only the virtual microphone: the voice goes into a virtual cable and the cable becomes
Windows' default microphone while the app runs (the old default is restored on quit).

## Tray — the original imports `Shell_NotifyIconW` (SHELL32)

The original has a notification-area icon. This app: closing the window hides it to the tray (setting,
on by default); click the icon to show it, right-click for Show / Voice changing (tick) / Stop sounds and
music / Quit. A one-time balloon explains it. Hotkeys, music and sounds keep running while hidden
(a background thread handles them every **15 ms**, own choice).

## Music player — exe `sub_45f700`, settings `sub_44f850`

The original has: playlist (`MUSIC_LIST`), Loop (`MUSIC_LOOP`), Shuffle (`MUSIC_SHUFFLE`), Music Volume
shown as `%d%%` (`MUSIC_VOLUME`), "Add music from folder" (`sub_4473e0`) and "apply music effects"
(`APPLY_MUSIC_EFFECTS`, 0x450c0f). Kept, plus new advanced options (own design, numbers are own choices):

| Option | Range / value |
|---|---|
| Seek bar | click/drag; −10 s / +30 s buttons |
| Previous | restarts the song when more than **3 s** in |
| Repeat | off / playlist / one song |
| Speed | **0.5–2.0×** (tape style: pitch follows), presets 0.75/1/1.25/1.5 |
| Cross-fade | **0–8 s**; Stop also fades out |
| Music bass / treble | **±12 dB**, same shelf filters as the voice tone (250 Hz / 4000 Hz) |
| Lower music while I talk | mic above **0.03** (≈ −30 dB) → music to the chosen level (default **30 %**), glide **80 ms**, hold **400 ms** — estimated |
| Who hears it | others (on/off), me (listening device), voice effect on music |
| Playlist | search, move up/down, remove, sort A–Z, clear, drag-and-drop files |

## Sound board — exe `sub_45c7e0`

The original: sound list from its `sounds\` folder and `ClownfishCustomSounds\`, "Sound Volume ( %d%% )",
hotkey + F12 plays the assigned sound (`HOTKEY_SOUND`). Kept, plus (own design):

- per-sound **name, volume 0–200 %, speed 0.5–2.0×, loop, favorite**, hotkey slot;
- hotkey + **1…9, 0** plays the sound in that slot (one sound per slot);
- press mode: play on top (like the original) / stop the others / press again to stop;
- search, filters (all, favorites, mine, built in), playing-now chips with stop, progress on each tile,
  add folder, drag-and-drop, big buttons;
- stopping a sound ramps out over **8 ms** so it never clicks.

## Not in this version / changed

- VST plugins and the online AI assistant are not included; text to speech uses the offline Windows voices.
- Vocoder: own 18-band design with built-in synth carriers; it can also use the original's carrier sounds.
- "Replace my real microphone" was removed (see above).

## New in this version

Deep (−6 st), Demon (−7 st + grit), Telephone (300–3400 Hz), Megaphone, Underwater; "Big room" reverb;
room amount slider; mic volume; built-in sounds; synth vocoder carriers; starred sound for F12; tray icon;
advanced music and sound board options above. Icons come from Windows' own icon font (Segoe Fluent Icons /
Segoe MDL2 Assets), text from Segoe UI, with symbol and emoji fonts as fallbacks, so none show as boxes.

## Making the microphone key writable (exe strings 0x4ab0d8-0x4ab718)

Windows lets even administrators only read `MMDevices\Audio\Capture\{guid}` keys (error 5 on write).
The original enables SeTakeOwnershipPrivilege, makes itself the key's owner (SetSecurityDescriptorOwner)
and adds a write entry to its permissions (SetEntriesInAcl / SetSecurityDescriptorDacl, "ensuring writability").
The rebuild: on error 5 it first reopens the key with the restore privilege (REG_OPTION_BACKUP_RESTORE);
if that fails it makes Administrators the owner, grants them full access, and writes again.

## App sound into the microphone (new; not in the original)

The original exe has no per-app recording (no "Loopback" text in ClownfishVoiceChanger.exe, evidence
ev_e8ee4676…). Our own design: Windows' per-app loopback device `VAD\Process_Loopback` activated with the
app's top process id and "include child processes" (so every browser tab is caught), 32-bit float stereo at
the engine rate (Windows converts), mixed to mono. It goes into the call before the voice effect (option
"Change it with my voice effect") or after it, at its own volume (0–200 %). It is never added to what you hear
yourself, because the app already plays on your speakers. Queue kept at most 120 ms (estimated), playback
starts after 20 ms is waiting (estimated). The chosen app is remembered by program name; every 3 s the app
is looked for again, so it starts when you open the app and stops when you close it.

## Hearing changed sounds and music

When "Change the music / sounds with my voice effect" is on, what you hear yourself is the changed version
whether or not "Hear my changed voice" is on: without your own voice played back, those sounds run through a
second copy of the effect chain (voice, room, vocoder, tone, boost; no mic volume, noise removal or gate).
