# D8 Shuffler

Takes your music folder and puts it on the speaker's SD card the way the
speaker wants it: wiped, flat, numbered `001`, `002`, `003`... in a **new random
order every time**. Your music folder is never changed.

Built with Tauri 2, React and the [Frostplate](https://github.com/dmattox10/frostplate)
kit. macOS only.

## Run it

```bash
yarn install
yarn tauri dev       # the app, with a real card
yarn tauri build     # makes src-tauri/target/release/bundle/macos/D8 Shuffler.app
yarn dev             # just the interface in a browser, with a pretend card
yarn core:test       # the Rust tests (no Tauri needed)
```

It needs Rust and Node. Converting FLAC (or anything else) to MP3 also needs
ffmpeg: `brew install ffmpeg`. The app finds it by itself and says so if not.

macOS will ask once for permission to read `~/Music` and removable volumes.
Say yes, or the app cannot see your music or the card.

## What a run does

1. **Checks everything before touching the card**: the card is real
   removable media, the music fits, ffmpeg is there if a conversion is needed.
2. **Wipes the card**, then writes every track in a fresh shuffle.
3. **Flattens**: the card ends up with every file in the root, no folders.
   Tracks are numbered by what actually lands on the card, so a file that fails
   to convert never leaves a gap.
4. **Converts** anything the speaker cannot play to MP3 (VBR V0, no cover art).
   MP3 and WAV are copied as they are. The *My speaker plays FLAC* switch skips
   converting FLAC once you have confirmed it works.
5. **Cleans up**: removes the `._` files macOS leaves on FAT cards (they would
   show up on the speaker as extra tracks), then ejects if you asked it to.

Files are written one after another onto a freshly emptied card, which is what
makes the speaker play them in number order: cheap players follow the order the
files were written in, not the order of their names.

## Safety

The wipe is the only dangerous thing here, so it is checked in Rust, not just
in the interface:

- Only external or removable volumes are listed. The boot disk and internal
  drives never appear.
- The target must sit directly under `/Volumes`, must not be a shortcut, and
  must not overlap the music folder.
- Anything over 600 GB is refused as "not a card".
- Every wipe asks first and says how many files it is about to remove.
- *Format as FAT32* uses an MBR partition map, because small players often
  cannot read the GPT map macOS puts on cards.

## After a run

`~/Library/Application Support/fyi.dantheman.d8shuffler/last-run.txt` lists
which numbered file is which song.

## Layout

```
src/                     React interface (Frostplate kit in src/frostplate/)
src-tauri/src/lib.rs     thin Tauri commands
src-tauri/d8-core/       all the logic and tests: scan, shuffle, convert, wipe, volumes
```
