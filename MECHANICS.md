# Lab Video Player – mechanics sheet

Values read from the original program's code (VLC media player 3.0.24, Windows x64) with the
Deep engine. Addresses are in the original files; nothing here is copied code, only the numbers
and rules the rebuild follows. "estimated" marks values that were looked for and not found.

## Program start (vlc.exe)
- `WinMain` (vlc.exe 0x140001510): starts the core, adds the `hotkeys` and `globalhotkeys`
  interfaces, then the default interface, then starts playing whatever was given on the command
  line (`libvlc_playlist_play(-1)` at 0x140001939). The rebuild also plays files given on its
  command line straight away.
- Command line (WinMain 0x1400017e1–0x14000182e): every argument goes to the core except
  `--no-crashdump` (turns off the crash reporter) and `--language` (with `=value`, or the next
  argument when that isn't another `--` option), which sets `LANG` (`auto` = system language;
  else the `Software\VideoLAN\VLC\Lang` registry value). The rebuild skips both the same way and
  treats the remaining non-option arguments as media.
- If the core can't start, a message box says the options were invalid (0x140001bf8).
- Crash reporting (crash dump folder `%APPDATA%\vlc\crashdump`, vlc.exe 0x140001d70 /
  0x140003010) is not part of the rebuild. WinMain's remaining helper (0x140001ef0) only frees
  the crash handler at exit; the main program holds no other player rules, which live in the
  core and plugin libraries cited below.
- Window title of the original: "VLC media player"; the rebuild uses its own name
  "Lab Video Player" and shows "<media title> - Lab Video Player" while playing
  (qt-name-in-title behaviour, libqt_plugin vlc_entry 0x1400021f5).

## Seeking (libvlccore config table, function 0x140002120)
| setting | value | where |
|---|---|---|
| very short jump | 3 s | 0x14000e42a (`extrashort-jump-size`) |
| short jump | 10 s | 0x14000e492 (`short-jump-size`) |
| medium jump | 60 s | 0x14000e4fa (`medium-jump-size`) |
| long jump | 300 s | 0x14000e562 (`long-jump-size`) |

Menu "Jump Forward / Jump Backward" use the short jump (libqt_plugin 0x1400262c0 / 0x140026340:
`time-offset = ±short-jump-size × 1 000 000 µs`).

- Jump keys (libhotkeys `sub_140001d00`, cases 0x12–0x19): only when the input can seek
  (`can-seek`); `time-offset = size × direction × 1 000 000 µs`; a negative size does nothing;
  then the position message is shown.
- Applying a jump (`time-offset` handler, libvlccore 0x14005f9d0): `time = max(0, time + offset)`,
  **no upper cap** — a jump past the end finishes the item (the next one starts as at a natural end).
- Position message (libhotkeys 0x140001b10): `elapsed / total` with `%s / %s`, or only the
  elapsed time when the length is unknown (0).
- Time text (`secstotimestr`, libvlccore 0x140095a40): `%02u:%02u` under an hour,
  `%u:%02u:%02u` from an hour, `-` in front of negative values.
- Time labels (libqt_plugin `TimeLabel` 0x1400c5e80): `--:--` when idle or the length is unknown;
  the right label shows the total, or `-` + (whole length − whole elapsed seconds) after a click.
  The choice is saved (`MainWindow/ShowRemainingTime`, libqt_plugin 0x1400ca6a0).

## Volume
- Volume step: 12.8 on a 256 scale = **5 %** per step (libvlccore 0x140002410, constant 0x14019cd78; range 1..256).
- Rule (`aout_VolumeUpdate`, libvlccore 0x14006bc80):
  `vol = clamp(vol + steps × step, 0, 2.0)` then `vol = round(vol / step) × step`.
  So keyboard volume goes up to **200 %**.
- On-screen message: `Volume %ld%%` (libhotkeys 0x14000e243).
- Volume slider in the window goes to **125 %** (qt-max-volume default 125, range 60..300; libqt_plugin 0x1400029d1).
- Slider gradient colours (qt-slider-colours default, libqt_plugin 0x140b97d18):
  (153,210,153) → (20,210,20) → (255,199,15) → (245,39,29).
- Volume keys and the wheel only change the level; they don't unmute (aout_VolumeUpdate stores
  the new level only).
- Mute key (libhotkeys `sub_140001d00` case 0x1c): toggles mute without forgetting the level;
  if now muted or the volume is 0 it shows the **mute icon**, otherwise `Volume N%`.
- Mouse wheel over the video changes the volume (hotkeys-y-wheel-mode, libvlccore 0x140093700; default mode = volume, actions 0x71/0x72).

## Playback speed
- Speed table (libvlccore 0x14019fac0, used by `RateOffsetCallback` 0x1400236a0):
  1/64, 1/32, 1/16, 1/8, 1/4, 1/3, 1/2, 2/3, **1**, 3/2, 2, 3, 4, 8, 16, 32, 64.
- Faster: first table value greater than `rate × 1.1`. Slower: the value before the first
  value that is ≥ `rate × 0.9` (constants 0x14019fb48 = 0.9, 0x14019fb4c = 1.1).
- Fine steps (libhotkeys 0x140001d89): `rate = floor(|rate| × 10 + dir + 0.05) / 10`
  with dir = +1 (faster) or −1 (slower), clamped to **[1/32, 31.25]** (constants 0x14000e940–0x14000e9b0).
- Normal speed resets to 1.0. On-screen message `Speed: %.2fx`.
- Pitch is kept at other speeds: `audio-time-stretch` defaults to on (libvlccore 0x14000299c).
  Tempo scaler settings (libscaletempo_plugin `vlc_entry` 0x1400032a0): stride **30 ms**,
  overlap **0.20** of the stride (constant 0x1400063a0), search **14 ms**. The rebuild uses its
  own overlap-and-crossfade scaler with these three numbers.
- The rebuild plays audio at speeds 0.25×–4×; outside that range the sound is muted (estimated limit of the rebuild, see below).

## Aspect ratios (libvlccore table 0x1401a8b88)
Default, 16:9, 4:3, 1:1, 16:10, 2.21:1, 2.35:1, 2.39:1, 5:4 — cycled with the A key,
message `Aspect ratio: %s` (libhotkeys 0x14000e4fb).

## Keyboard (libvlccore key table 0x140199398 onwards)
| action | key |
|---|---|
| Play/Pause | Space |
| Fullscreen | F (also double-click on the video) |
| Leave fullscreen | Esc |
| Faster / Slower | `+` / `-` |
| Normal speed | `=` |
| Faster (fine) / Slower (fine) | `]` / `[` |
| Next / Previous | N / P |
| Stop | S |
| Show position | T |
| Jump 3 s | Shift+Left / Shift+Right |
| Jump 10 s | Alt+Left / Alt+Right |
| Jump 60 s | Ctrl+Left / Ctrl+Right |
| Jump 300 s | Ctrl+Alt+Left / Ctrl+Alt+Right |
| Next frame | E |
| Volume up / down | Ctrl+Up / Ctrl+Down |
| Mute | M |
| Random | R |
| Loop (Normal → Loop all → Repeat one) | L |
| Aspect ratio | A |
| Snapshot | Shift+S |
| Quit | Ctrl+Q |
| Open file | Ctrl+O (menu shortcut) |
| Playlist | Ctrl+L (menu shortcut) |
| Media information | Ctrl+I (menu shortcut) |

## Window layout (libqt_plugin 0x140067660 defaults)
- Under the video: time bar `43;33-4;44` = elapsed time, seek slider, total/remaining time.
- Controls `0-2;64;3;1;4;64;7;9;64;10;20;19;64-4;37;65;35-4` =
  big Play, gap, Previous, Stop, Next, gap, Fullscreen, Extended settings, gap,
  Playlist, Loop/Repeat, Random, gap, (DVD menu buttons), stretch, Volume.
- Menus: Media, Playback, Audio, Video, Subtitle, Tools, View, Help.
- Speed submenu: Faster, Faster (fine), Normal Speed, Slower (fine), Slower.
- When nothing is playing the video area is black with a centred logo (qt-bgcone);
  the rebuild draws its own original logo (a play-button badge), not the original's cone.

## Playlist (libvlccore playlist/thread.c: `NextItem` 0x140021670, `ResetCurrentlyPlaying` 0x140021270)
- Loop key (libhotkeys case 0x5c): if Repeat One is on → off (`Loop: Off`); else if Loop All is on →
  Repeat One (`Loop: One`); else → Loop All (`Loop: All`). So Off → All → One → Off.
- Random key (case 0x5b): toggles, `Random: On` / `Random: Off` (strings at libhotkeys 0x14000e0c2–0x14000e0ce).
- Play order: the playlist order; with Random on it is shuffled once (Fisher-Yates from the last
  item down: swap item i with item `rand % (i+1)`), and the play position is the current item's
  place before shuffling. Changing the playlist or Random rebuilds the order.
- When an item ends by itself: Repeat One plays it again ("repeating item"); otherwise the next
  item in the order; after the last item, Loop All starts over (reshuffled when Random is on),
  otherwise playback stops ("nothing to play"); with "Quit at the end of playlist" the program quits.
- Next / Previous asked for by the person always wrap around the ends ("looping - restarting at
  beginning/end of node"), whatever the Loop setting, reshuffling first when Random is on.
- Next / Previous keys show `Next` / `Previous` (libhotkeys 0x14000e18b).
- Quit at the end of playlist option in the Media menu.

## Recent media (libqt_plugin `RecentsMRL`)
- Adding (`addRecent` 0x14002e120): nothing when "save recently played items" is off; an item
  already in the list moves to the top, a new one goes on top; the list is cut back once it holds
  more than **30** (`0x1e < count`). Local files also go to Windows' recent documents.
- Menu (0x1400069b0): the first **10** items as `N: name` (names cut in the middle), the first
  nine with **Ctrl+1 … Ctrl+9**; then `Clear`; the menu is greyed out when the list is empty.

## Snapshots (libvlccore `vout_snapshot_SaveImage` 0x1400722a0, `VoutSaveSnapshot` 0x14007fac0)
- Folder: the user's Pictures folder (`config_GetUserDir(0x86)`, 0x140072290) unless set.
- Name: `<prefix><YYYY-MM-DD-HHhMMmSSs><milliseconds as 3 digits>.<format>`
  (`%s\%s%s%03lu.%s`); format default `png` (0x14019362d). The original's prefix is its own
  name; the rebuild uses `labsnap-`.
- After saving: the full path as an on-screen message, and (snapshot-preview default on,
  0x140003823) the picture shown top-left at a quarter of its size for **4 s**.

## Subtitles (libhotkeys `sub_140001d00`)
- Delay keys H (+) / G (−) (key table 0x14019a396): **±50 ms** per press (cases 0x32/0x33),
  message `Subtitle delay %i ms`; with no active subtitle the message is `No active subtitle`.
- Text size (cases 0x3e–0x40; keys Ctrl+0, Ctrl+Mouse Wheel Up/Down at 0x14019c010): normal is
  100 %; the step is 10 while the size is within 50–150 %, otherwise 25; the new size is rounded
  down to the step and kept within **25–500 %**; message `Subtitle text scale %d%%`.

## On-screen messages and icons
- Messages (`vout_OSDMessage`, libvlccore 0x140076e60): top right, shown **1 s**.
- Icons (`vout_OSDIcon`, libvlccore 0x14007a830): shown **1.2 s**, fading out.
- Play/Pause key (libhotkeys case 2): the play icon when it was paused, the pause icon otherwise.
- Only keys and the mouse wheel (the hotkeys plugin) show these; the window's menus and buttons
  act on the player directly without messages.

## Dolby Digital (AC-3) sound
- Windows 11 here has no AC-3 decoder (only the "Dolby AC-3 and E-AC-3 IEC-61937 converter",
  which passes the bitstream to a receiver). The original decodes AC-3 itself with its codec
  library (`libavcodec_plugin.dll`) after `libpacketizer_a52_plugin.dll` splits the frames, so
  the rebuild has its own decoder (`ac3.rs`), written from the public ATSC A/52 standard.
- Its tables are the standard's; each was checked against the copy in the original's library:
  hearing threshold 50×3 at 0x14112e700, log-addition 260 at 0x14112e840, bin→band at
  0x14112e960, band starts (…49, 55, 61 … 229, 253) at 0x14112ea60, bit-allocation pointers (64)
  at 0x140e7e5c0, fast gain 0x80–0x400 at 0x140e7e570, floor (0x2f0 … 0xf800) at 0x140e7e580,
  dB per bit (0, 0x700, 0x900, 0xb00) at 0x140e7e590, slow gain (0x540, 0x4d8, 0x478, 0x410) at
  0x140e7e598, fast decay (0x3f, 0x53, 0x67, 0x7b) and slow decay (0x0f, 0x11, 0x13, 0x15) at
  0x140e7e5a0, rematrix bands (13, 25, 37, 61, 253) at 0x140e7e612, bit rates 32–640 kb/s at
  0x140e7e620, sample rates 48000/44100/32000 at 0x140e7e650, frame sizes (38×3 words) at
  0x140e7e6e0, quantizer levels/bits (0,3,5,7,11,15, 5…12,14,16) at 0x14112ebcc, mantissa levels
  at 0x14112ebe0. Unit tests compare the frame sizes and bands with those tables.
- Frames: 1536 samples (6 blocks × 256); frame bytes = 2 × words (48 kHz: kb/s × 2; 44.1 kHz:
  kb/s × 320/147, +1 on odd codes; 32 kHz: kb/s × 3). Window: Kaiser-Bessel-derived, alpha 5.
- Downmix to two speakers follows the original's channel mixer (libsimple_channel_mixer
  `FUN_140001770`, constant 0x1400061b8 = 0.7071): left = L + 0.7071·(C + SL),
  right = R + 0.7071·(C + SR), LFE left out. Applies to every 5.1 source, not only AC-3.
- Frame finding follows the original's packetizer (`libpacketizer_a52_plugin.dll`): the header is
  read by the parse function at 0x1400015e0 (sync 0x0B77, bsid ≤ 10 for AC-3, fscod 3 refused,
  frame size from the size code); then, in the packetizer loop (0x140002a00, check at
  0x140002e3f–0x140002e6d), a frame is only trusted when the two bytes right after it are another
  sync word or two zero bytes. Otherwise the sync word was false ("emulated sync word (no sync
  on following frame)", message at 0x1400070f8) and the search goes on one byte later. At the end
  of the stream the last frame is decoded without that check.
- Dynamic range compression: the decoder option `drc_scale` (libavcodec_plugin.dll 0x1410a4020,
  option table 0x1410a4c40) is a float, default 1.0, range 0–6; gain = stream gain ^ scale, so by
  default the stream's own compression words are applied as written. Dither for unallocated
  coefficients is uniform ±0.5 (estimated: the code wasn't found; the standard's range).
- Plain .ac3 files are read directly (constant frame size → exact length and seeks); AC-3 inside
  MKV, MP4 or TS comes through Windows' file readers with their timestamps.
- Not covered: Dolby Digital Plus (E-AC-3, bsid 16) frames are skipped.

## Estimated / not found
- Default first-run window size: no fixed size in the code. With no saved geometry the window keeps
  its layout's natural size and is centred on screen (QStyle::alignedRect, AlignCenter 0x84,
  libqt_plugin 0x140019d0d). The rebuild uses an estimated 860×560, centred.
- Codec support: the rebuild uses Windows' own decoders (Media Foundation), so it plays what
  Windows can play (MP4/H.264/HEVC, MOV, MKV, AVI, WMV, MP3, AAC/M4A, WAV, FLAC…).
