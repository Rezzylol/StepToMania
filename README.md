# StepToMania — StepMania to osu!mania converter

Convert StepMania charts (`.sm`, and `.ssc` as a bonus) into `.osz` beatmaps so you can play them in **osu!mania**.

Drag a song folder (or a whole pack) onto the window, wait a moment, then double-click the `.osz` files it makes to import them into osu!. No command line needed.

## Using it

1. Open `sm2osz` (on Windows: `sm2osz.exe`).
2. **Drag and drop** a song folder, or a whole pack of songs, onto the window.
   - You can also drag a folder straight onto the `.exe` icon, or use the **Choose folder…** / **Choose .sm file(s)…** buttons.
3. When it says **Done**, click **Open output folder**.
4. Double-click an `.osz` file to import it into osu!.

By default the `.osz` files are saved in a folder called `osz output` next to the folder you dropped. Use **Change…** to pick a different folder.

### Options

| Option | Default | What it does |
| --- | --- | --- |
| Overall Difficulty | 8 | How strict the timing windows are in osu!mania |
| HP Drain | 8 | How fast the health bar drains |

## What gets converted

- **One `.osz` per song, one difficulty per chart.** A song with Beginner through Challenge charts becomes one beatmap set with several difficulties.
- **Key count comes from the chart.** The number of columns in the chart becomes the number of keys, for example `dance-single` becomes 4K, `dance-solo` 6K, `pump-single` 5K and `dance-double` 8K. osu!mania supports up to 18 keys; wider charts are skipped.
- **Difficulty names** look like `4K Hard Lv.9`. Edit charts include the description, for example `4K Edit - name Lv.12`.
- **Notes:** taps become normal notes; holds and rolls become long notes. Mines, fakes and keysounds are ignored.
- **Timing:** BPM changes, stops, delays and the song offset are converted to exact millisecond timings. For `.ssc` files, warps, fake segments and per-chart timing are handled too. Notes that sit inside a warp or fake region can't be hit in StepMania, so they are left out, and the log tells you when that happens.
- **Included in the `.osz`:** the song's audio file, plus a background image if one is found (`#BACKGROUND`, then `#BANNER`, then any image with `bg` or `background` in its name).
- **Metadata:** title, subtitle, artist (including transliterations), step author (as Creator), genre (as tags) and the preview start point.
- **Folders with both `.sm` and `.ssc`:** the `.sm` file is used.

## Limitations

- **Audio:** osu! reliably plays `.mp3`, `.ogg` and `.wav`. If a song uses another format (for example `.flac`), the tool warns you and you will need to convert the audio yourself.
- **Backgrounds:** only `.jpg`, `.jpeg` and `.png` images are carried over. Video backgrounds and background-change scripts are ignored.
- **Difficulty rating:** osu! calculates its own star rating, so StepMania meters (the `Lv.` number) won't match.
- **Scroll speed changes:** `#SPEEDS` and `#SCROLLS` are not converted. Notes are placed at the correct times, but StepMania's visual scroll effects are lost.
- **Hitsounds:** none are created; osu! uses your skin's defaults.
- **Two-player charts:** `dance-couple` style charts are treated as one wide chart (8 columns).

## Building from source

You need [Rust](https://rustup.rs) (stable).

```bash
cargo build --release
```

The program is created at:

- Windows: `target\release\sm2osz.exe`
- macOS / Linux: `target/release/sm2osz`

The first build takes several minutes because the GUI library is large. After that, rebuilds are fast.

**Linux:** install the GTK 3 development files first (used by the file-picker dialogs), for example `sudo apt install libgtk-3-dev`.

**Windows:** the included `.cargo/config.toml` links the C runtime statically, so the `.exe` runs on any PC without installing the Visual C++ Redistributable. The release `.exe` is a single file you can give to anyone.


## Troubleshooting

| Problem | Fix |
| --- | --- |
| "No .sm files found" | Drop the folder that contains the `.sm` files (or a pack of such folders). |
| `audio file "…" not found` | The `#MUSIC` file named in the chart isn't next to the `.sm`. Check the file exists and the name matches. |
| The song imports but has no sound | The audio is probably not `.mp3`, `.ogg` or `.wav`. Convert it and update the `.osz`. |
| Notes feel off-sync | Check the chart in StepMania first. If it also feels off there, the chart's `#OFFSET` is the cause; otherwise please report it with the chart attached. |
| The window won't open on an old PC | The app needs OpenGL 2.0 or newer. Update your graphics drivers. |
| Linux: build fails mentioning `gtk` | Install `libgtk-3-dev` (see above). |

## Project layout

```
Cargo.toml
.cargo/config.toml   Windows static-runtime build setting
src/
  main.rs            GUI (egui/eframe): drag and drop, buttons, log
  convert.rs         Finds songs, builds .osu files, writes the .osz (zip)
  sm.rs              .sm / .ssc parser and beat-to-time conversion
```

Dependencies: [`eframe`/`egui`](https://github.com/emilk/egui) for the window, [`rfd`](https://github.com/PolyMeilex/rfd) for file dialogs, [`zip`](https://github.com/zip-rs/zip2) for writing `.osz` files.

## Notes

StepMania and osu! are trademarks of their respective owners; this project is not affiliated with either. Only convert charts and audio you have the right to use.

