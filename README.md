# OpenVKCL

A cross-platform (Windows, Linux, macOS) contest logger for Australian and New
Zealand contests, in the spirit of VKCL by Mike VK3AVV (SK).

Built with [Tauri 2](https://tauri.app) (Rust backend) and Svelte.

## Features

- Contests: WIA Remembrance Day, Trans-Tasman Low-Band, Harry Angel Sprint,
  John Moyle Memorial Field Day and the VHF-UHF Field Days.
- Live dupe checking with each contest's re-work rules (time intervals, time
  blocks, new grid square), points, multipliers and running score.
- Distance scoring from 6-character Maidenhead locators for the VHF-UHF Field Days.
- Cabrillo 3.0 export with the contest names vklogchecker.com expects, plus ADIF.
- Radio control through Hamlib `rigctld` or FLRig: band, mode and frequency
  follow the radio.
- Each log is a single SQLite file (`.ovkcl`); every contact is written to disk
  as soon as it's logged.

## Using it

1. **New log** - pick the contest, start time, your callsign and exchange details.
2. Type a callsign. The status line shows DUPE, points and new multipliers as you type.
3. **Space** moves to the next exchange field, **Enter** logs, **Esc** clears.
   Without a radio connected, **Alt+Up/Down** changes band and **Alt+Left/Right** changes mode.
4. Double-click a contact in the log to edit or delete it.
5. **Cabrillo…** previews the log and saves it for upload to
   [vklogchecker.com](https://www.vklogchecker.com).

### Radio control

OpenVKCL talks to the radio through either:

- **Hamlib rigctld** (default port 4532), e.g.
  `rigctld -m 3073 -r /dev/ttyUSB0 -s 19200` (Windows: `-r COM3`).
  `rigctl -l` lists radio model numbers.
- **FLRig** (default port 12345).

Choose **Radio…** and connect. The connection is remembered and re-established
when the app starts.

## Contest definitions

Contest rules live in TOML files in [`src-tauri/contests`](src-tauri/contests)
and are built into the app. To change a rule without rebuilding, or to add a
contest, put a `.toml` file in the app's `contests` folder; a file with the same
`id` as a built-in contest replaces it:

| Platform | Folder |
| --- | --- |
| Linux | `~/.config/nz.zl.openvkcl/contests/` |
| Windows | `%APPDATA%\nz.zl.openvkcl\contests\` |
| macOS | `~/Library/Application Support/nz.zl.openvkcl/contests/` |

Check the current WIA rules before each contest; they change from year to year.

## Development

Prerequisites: Rust (stable), Node.js 20+, and on Linux the WebKitGTK 4.1
development packages (see the [Tauri prerequisites](https://tauri.app/start/prerequisites/)).

```sh
npm install
npm run tauri dev        # run the app with hot reload
npm run check            # type-check the frontend
cd src-tauri && cargo test
npm run tauri build      # build an installer for this platform
```

Pushing a `v*` tag to GitHub builds installers for all three platforms
(`.github/workflows/release.yml`) and attaches them to a draft release.
macOS builds need an Apple Developer ID to be signed and notarised; unsigned
builds will show a security warning when opened.
