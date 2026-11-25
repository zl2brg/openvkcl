# Changelog

All notable changes to OpenVKCL are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Added

- **WIA VK Shires** contest: RS(T) plus shire code (VK) or CQ zone (DX), 4-hour
  repeat slots on each band and mode, and shires and zones as multipliers on
  each band and mode. Shire codes are checked against the WIA's `Shire.db3`,
  which downloads at first startup and can be updated or imported under
  **Lookups…** like the other lookup files. The entry line and setup dialog
  show which shire a code is, warn about unlisted codes, and list matching
  shires as you type part of a code or a place name.
- Contest definitions can name a `number_list` file of valid codes, which lets
  the number field take letters as well as digits.

### Changed

- The voice keyer is off until you tick **Voice keyer** in the toolbar. The
  F-key controls and **Voice keyer…** appear only while it is ticked.
- In a contest, Space and Tab stop on RST sent before RST received.
- Phone, CW and RTTY reports can be 59 or 599.
- A generic contest: name, length, bands and one exchange (none, serial, locator
  or text). RST is optional. No dupes and no score; the contact count is kept.
- Faster with long general logs. Editing or deleting a contact sends only the
  rows that changed rather than the whole log (about 6 MB for a 20,000-contact
  logbook), and the log table redraws only those rows. Contacts imported
  before the country file was downloaded get their DXCC numbers stored the
  next time the log opens, so checking a call no longer looks up every
  contact's country again. Contest logs are scored once per keystroke instead
  of twice.

## [0.6.0] - 2026-11-23

### Fixed

- Black window on older macOS (Big Sur / Monterey): the frontend was built for
  Safari 16.4+ while the system WKWebView is Safari 14–15, so the script never
  parsed. Release builds now target ES2020 / Safari 14 (Chrome 105 on Windows),
  and the macOS minimum is Big Sur (11.0).

### Added

- **General logging (DX)**, a log type for everyday operating and DX
  chasing, offered first in **New log**. There's no contest period, scoring
  or dupes, and it covers every band from 2190m up with ADIF modes (SSB, CW,
  FT8, FT4, RTTY, …). Each contact can have a name, comment and QSL card,
  LoTW and eQSL status.
- DXCC countries in general logs. As a call is typed, the entry line shows
  the station's country, continent and CQ/ITU zones, and whether it's a new
  country, or new on this band or mode. A band-by-mode grid shows where the
  station and its country have been worked and confirmed. The side panel
  counts countries worked and confirmed, by band and by phone/CW/data.
- From/To dates above a general log's table, which narrow the log and the
  country counts, e.g. to contacts since 1 January 2026.
- **Import ADIF…** for any log. Contacts already in the log are skipped, and
  a contest log only takes contacts from the contest period on its bands and
  modes. Imports and **Export ADIF…** can be limited to a date range, and
  ADIF fields the log has no column for are kept and written back out.
- The cty.csv country file, under **Lookups…**. It and MASTER.SCP download
  in the background at startup when missing or more than a week old.
- WIA contests that restart serial numbers at 001 on each band tick
  "Separate serial numbers on each band" when that category is chosen.
  The VHF-UHF Field Day does this for any multi-operator entry, including
  multi-single. John Moyle does it for multi-two and multi-multi, and
  Trans-Tasman for multi-multi. Single-operator and multi-single entries
  keep one series across all bands. Changing back to those categories
  clears the tick. The box can still be changed by hand until the category
  changes again, and opening an existing log leaves its saved choice alone.
- **FlexRadio** under **Radio…**, for 6000, 8000 and Aurora series radios,
  straight over the network with the SmartSDR API: no SmartSDR CAT or DAX,
  so it works on macOS and Linux as well as Windows. The entry line follows
  the transmit slice's band, mode and frequency, logged contacts can be
  marked as spots on the panadapter, and **Find radios** fills in the
  radio's address. On a Mac, allow OpenVKCL under System Settings → Privacy
  & Security → Local Network the first time.
- A voice keyer on F1–F12 for phone, as in N1MM: Run and S&P sets of .wav
  messages, played to a chosen sound output with PTT through the radio
  connection (FlexRadio, TCI, Hamlib, rigctld or FLRig) or the radio's VOX.
  F1 is CQ; Alt+R repeats it after a pause set with Ctrl+R until a call is
  typed or Esc is pressed. Ctrl+Shift+F-key records a message from the
  microphone, Shift+F-key plays the other set's key, and Alt+U switches sets.
  Devices and messages are under **Voice keyer…**.
- A warning when a contact's time is more than a minute ahead of the clock,
  on the entry line when **Enter time** is ticked and when editing a contact.
- A DX cluster tile under **By band** in general logs. It only shows spots
  of DXCC countries not yet in the log, or optionally countries new on the
  spotted band, from a chosen time frame of 10 minutes to 2 hours. Clicking
  a spot fills in the call, band and mode. It connects to the WA9PIE network
  (hrd.wa9pie.net:8000) with your callsign by default; the cluster, port and
  login callsign are under **Lookups…**.

### Changed

- Opening a contest log adds the general logging columns without changing
  its version, so older versions of OpenVKCL can still open it.
- RST sent is shown before RST received. In a contest, Space moves from the
  call to RST received, then the serial, number and locator. In a general
  log it moves through call, RST sent, RST received, name and comment.
- The header clock, contest timer and re-work countdowns keep their own
  timers, so the rest of the window is not redrawn every second.

### Contributors

Thanks to everyone who contributed to this release:

- [@ezypezy22](https://github.com/ezypezy22) — general (DX) logging and ADIF import,
  FlexRadio, DX cluster, log search/sort/countdowns, right-click edit/delete,
  long-log performance, and log schema compatibility
- [@hilaryb](https://github.com/hilaryb) — light/dark header toggle, and local
  time zone only when a contest scores by local time
- [@zl2brg](https://github.com/zl2brg) — releases, contest definitions, and
  macOS Big Sur / build fixes

