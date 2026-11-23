# Changelog

All notable changes to OpenVKCL are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## Unreleased

### Added

- WIA contests that restart serial numbers at 001 on each band tick
  "Separate serial numbers on each band" when that category is chosen.
  The VHF-UHF Field Day does this for any multi-operator entry, including
  multi-single. John Moyle does it for multi-two and multi-multi, and
  Trans-Tasman for multi-multi. Single-operator and multi-single entries
  keep one series across all bands. Changing back to those categories
  clears the tick. The box can still be changed by hand until the category
  changes again, and opening an existing log leaves its saved choice alone.
- A warning when a contact's time is more than a minute ahead of the clock,
  on the entry line when **Enter time** is ticked and when editing a contact.
- A DX cluster tile under **By band** in general logs. It only shows spots
  of DXCC countries not yet in the log, or optionally countries new on the
  spotted band, from the last 30 minutes. Clicking a spot fills in the call,
  band and mode. It connects to the WA9PIE network (hrd.wa9pie.net:8000)
  with your callsign by default; the cluster, port and login callsign are
  under **Lookups…**.

### Changed

- RST sent is shown before RST received. In a contest, Space moves from the
  call to RST received, then the serial, number and locator. In a general
  log it moves through call, RST sent, RST received, name and comment.
- The header clock, contest timer and re-work countdowns keep their own
  timers, so the rest of the window is not redrawn every second.
