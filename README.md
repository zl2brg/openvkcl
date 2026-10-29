# OpenVKCL

<img width="1613" height="1014" alt="image" src="https://github.com/user-attachments/assets/c9d3fa3b-152a-418a-a02a-fdfb76908c23" />


A cross-platform (Windows, Linux, macOS) contest logger for Australian and New
Zealand contests, in the spirit of VKCL by Mike VK3AVV (SK).

Built with [Tauri 2](https://tauri.app) (Rust backend) and Svelte.

## Features

- WIA contests: Remembrance Day, Trans-Tasman Low-Band, Harry Angel Sprint,
  John Moyle Memorial Field Day and the VHF-UHF Field Days.
- NZART HF contests: Jock White Memorial Field Day, Memorial Contest and
  Sangster Shield, with their one-hour (or half-hour) periods, separate serial
  numbers per band, and branch-number multipliers.
- NZART VHF-UHF-SHF-EHF contests: DX Weekend, Low Band, Microwave and Field Day,
  with hourly periods, the Table 1 distance scores, points per km above 70 cm,
  and the machine-mode, QRP and field-station multipliers.
- Live dupe checking with each contest's re-work rules (time intervals, time
  blocks, new grid square), points, multipliers and running score.
- Distance scoring from 6, 8 or 10-character Maidenhead locators.
- Cabrillo 3.0 export with the contest names vklogchecker.com expects, plus ADIF
  and a per-band CSV score sheet (distance, claimed points and running total
  for each contact).
- Radio control for any radio Hamlib supports (just pick the model and serial
  port), FLRig or TCI (cudaSDR, ExpertSDR, Thetis): band, mode and frequency
  follow the radio.
- Each log is a single SQLite file (`.ovkcl`); every contact is written to disk
  as soon as it's logged.

## Using it

1. **New log** - pick the contest, start time, your callsign and exchange details.
2. Type a callsign. The status line shows DUPE, points and new multipliers as you type.
3. **Space** moves to the next exchange field, **Enter** logs, **Esc** clears.
   Without a radio connected, **Alt+Up/Down** changes band and **Alt+Left/Right** changes mode.
4. Double-click a contact in the log to edit or delete it. Click the clock to
   show times in UTC or local time; the log and exports always use UTC.
5. **Cabrillo…** previews the log and saves it for upload to
   [vklogchecker.com](https://www.vklogchecker.com).
6. **Score sheet** writes per-band log sheets (CSV) with the period, points,
   running total and new multipliers for each contact, plus a band and mode
   summary - what the Jock White Field Day summary sheet and the NZART VHF
   contests ask for. For the VHF contests, email it with your station details
   and signed declaration to vhfcontest@nzart.org.nz within two weeks, and set
   *Power* to QRP and *Station* to PORTABLE (field station) in the setup to
   claim those multipliers.

In contests that exchange an NZART branch number, the branch box fills in from
your last contact with that station, so you only type it once.

### Radio control

OpenVKCL talks to the radio through one of:

- **A radio on a USB or serial port** - pick the radio model (any radio
  [Hamlib](https://hamlib.github.io) supports), the serial port and baud rate.
  OpenVKCL runs Hamlib's `rigctld` for you while connected and restarts it if
  the radio is switched off and on. `rigctld` is included in the Windows
  installer; on Linux the `.deb`/`.rpm` pull in Hamlib (or
  `sudo apt install libhamlib-utils`), and on macOS install it with
  `brew install hamlib`. On Linux your user needs to be in the `dialout` group
  to open serial ports.
- **A rigctld you run yourself** (default port 4532), e.g. on another computer:
  `rigctld -m 3073 -r /dev/ttyUSB0 -s 19200`.
- **FLRig** (default port 12345).
- **TCI**, the WebSocket interface built into SDR software: cudaSDR (default
  port 50001), ExpertSDR (40001) or Thetis. Enable the TCI server in the SDR
  program. The SDR pushes frequency and mode changes as they happen, and each
  logged contact is marked on its band display (turn this off in **Radio…**).

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

Check the current WIA and NZART rules before each contest; they change from
year to year.

The NZART contests assume the usual NZ daylight-saving dates: periods start at
04:00 UTC on the Saturday (17:00 NZDT), and the Low Band contest's Sunday period
is widened to cover the end of daylight saving overnight. NZART also accepts
latitude and longitude; OpenVKCL only uses locators, so use 8 characters above
2.4 GHz, where the rules ask for about 1 km precision.

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

Windows builds bundle Hamlib from `src-tauri/hamlib/`: copy `rigctld.exe`, the
`.dll` files and the licence files from the
[Hamlib Windows release](https://github.com/Hamlib/Hamlib/releases) there first
(the release workflow does this automatically).

Pushing a `v*` tag to GitHub builds installers for all three platforms
(`.github/workflows/release.yml`) and attaches them to a draft release.
macOS builds need an Apple Developer ID to be signed and notarised; unsigned
builds will show a security warning when opened.
