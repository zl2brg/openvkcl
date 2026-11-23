# OpenVKCL

<img width="1613" height="1014" alt="image" src="https://github.com/user-attachments/assets/c9d3fa3b-152a-418a-a02a-fdfb76908c23" />


A cross-platform (Windows, Linux, macOS) contest logger for Australian and New
Zealand contests, in the spirit of VKCL by Mike VK3AVV (SK).

Built with [Tauri 2](https://tauri.app) (Rust backend) and Svelte.
Changes are listed in [CHANGELOG](CHANGELOG.md).

## Features

- WIA contests: Remembrance Day, Trans-Tasman Low-Band, Harry Angel Sprint,
  John Moyle Memorial Field Day and the VHF-UHF Field Days. Where the rules
  restart serial numbers at 001 on each band, choosing that category ticks
  "Separate serial numbers on each band": any multi-operator entry in the
  VHF-UHF Field Day, multi-two and multi-multi in John Moyle, and multi-multi
  in Trans-Tasman. Single-operator entries keep one series, as do multi-single
  entries in John Moyle and Trans-Tasman.
- NZART HF contests: Jock White Memorial Field Day, Memorial Contest and
  Sangster Shield, with their one-hour (or half-hour) periods, separate serial
  numbers per band, and branch-number multipliers.
- NZART VHF-UHF-SHF-EHF contests: DX Weekend, Low Band, Microwave and Field Day,
  with hourly periods, the Table 1 distance scores, points per km above 70 cm,
  and the machine-mode, QRP and field-station multipliers. Microwave bands are
  labelled in GHz in the UI (`1.2G`, `2.3G`, …); Cabrillo still uses the same
  band designators.
- Live dupe checking with each contest's re-work rules (time intervals, time
  blocks, new grid square), points, multipliers and running score. When a
  station is a dupe, the wait is shown as “in 60 mins” (and counts down), and
  each contact in the log shows a ⏱ countdown beside the call until that
  station can be worked again on that band and mode.
- Distance scoring from 6, 8 or 10-character Maidenhead locators.
- General logging for everyday operating and DX chasing: no contest rules, ADIF
  modes (SSB, CW, FT8, FT4, …) on every band from 2190m up, name, QTH, comment
  and QSL card / LoTW / eQSL status for each contact. As you type a call it
  shows the station's DXCC country and zones and whether it's a new country, or
  new on this band or mode, for you; the side panel counts countries worked and
  confirmed overall, by band and by phone/CW/data. Filter the log, the counts
  and ADIF exports by date (e.g. from 1 January 2026). Countries come from the
  cty.csv country file: download it from **Lookups…**.
- ADIF import into any log: contacts already in the log are skipped, and a
  contest log only takes contacts from the contest period on its bands and modes.
  An optional date range imports part of a large file. Other ADIF fields (power,
  LoTW dates, …) are kept and written back out on export.
- Cabrillo 3.0 export with the contest names vklogchecker.com expects, plus ADIF
  and a per-band CSV score sheet (distance, claimed points and running total
  for each contact).
- Radio control for any radio Hamlib supports (just pick the model and serial
  port), FLRig, TCI (cudaSDR, ExpertSDR, Thetis) or a FlexRadio over the
  network: band, mode and frequency follow the radio.
- Super Check Partial: matching calls from MASTER.SCP and your log, plus N+1
  (one character different) suggestions, as you type. Download the latest
  MASTER.SCP from **Lookups…** or import your own file.
- QRZ.com lookups: name, location and grid for each callsign (needs a QRZ.com
  XML subscription for more than name and country).
- Dark appearance by default, with a light theme one click away (the sun/moon
  icon at the top right).
- Each log is a single SQLite file (`.ovkcl`); every contact is written to disk
  as soon as it's logged.

## Using it

1. **New log** - pick the contest, start time, your callsign and exchange details,
   or **General logging (DX)** for an everyday logbook.
   WIA contests that use an overlay offer Youth, Rookie and YL as well as none.
   Contests with local-time scoring (Remembrance Day's 01:00-06:00 triple points)
   also ask for your time zone during the contest; it defaults to this computer's
   offset on the contest date, so change it if your PC clock is set to UTC or
   you're operating away from home.
   Choosing one of those categories ticks "Separate serial numbers on each
   band". Changing back to a category that uses one series clears the tick.
   The box can still be changed by hand until the category changes again, and
   opening an existing log leaves its saved choice alone.
2. Type a callsign. The status line shows DUPE, points and new multipliers as you type.
   Super Check Partial matches appear underneath (calls already in the log are
   highlighted; click one to use it), with the QRZ.com details when enabled.
3. **Space** moves to the next exchange field, **Enter** logs, **Esc** clears.
   RST sent is shown before RST received. In a contest, Space goes from the
   call to RST received, then the serial, number and locator. In a general log
   it goes call, RST sent, RST received, name, then comment.
   Without a radio connected, **Alt+Up/Down** changes band and **Alt+Left/Right** changes mode.
   Tick **Enter time** to log a contact at a chosen time. A time more than a
   minute ahead is marked as in the future.
4. Double-click a contact in the log to edit or delete it. The same future-time
   warning appears there. Click the clock to show times in UTC or local time;
   the log and exports always use UTC. The sun/moon icon next to it switches
   between dark and light appearance.
5. **Cabrillo…** previews the log and saves it for upload to
   [vklogchecker.com](https://www.vklogchecker.com).
6. **Score sheet** writes per-band log sheets (CSV) with the period, points,
   running total and new multipliers for each contact, plus a band and mode
   summary - what the Jock White Field Day summary sheet and the NZART VHF
   contests ask for. For the VHF contests, email it with your station details
   and signed declaration to vhfcontest@nzart.org.nz within two weeks. To claim
   the multipliers, tick the bands you ran QRP on and set *Station* to PORTABLE
   (field station) in the setup.
7. **About** shows the version, licence and author details.

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
- **FlexRadio** 6000, 8000 and Aurora series, directly over the network with
  the SmartSDR API (port 4992); no SmartSDR CAT or DAX needed, so it works on
  macOS and Linux too. **Find radios** fills in the radio's address. OpenVKCL
  follows the transmit slice (SmartSDR or another client must have a slice
  open) and marks logged contacts as spots on the panadapter.

Choose **Radio…** and connect. The connection is remembered and re-established
when the app starts.

On Linux, the packaged app appears under the **Hamradio** menu where that
submenu exists (KDE with `extra-xdg-menus`), otherwise under Internet.

## Contest definitions

How the five distance-scored contests turn a locator pair into points is
described in [Radial distance scoring](docs/radial-distance-scoring.md).
The other contests award fixed points.

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

## Licence

Copyright © 2026 Simon, ZL2BRG (simon.eatough@gmail.com).

OpenVKCL is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; see [LICENSE](LICENSE) for details.

The Windows installer includes Hamlib's `rigctld`, which is licensed under the
GNU LGPL and GPL; its licence files are installed alongside it.
