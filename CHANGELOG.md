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
- **FlexRadio** under **Radio…**, for 6000, 8000 and Aurora series radios,
  straight over the network with the SmartSDR API: no SmartSDR CAT or DAX,
  so it works on macOS and Linux as well as Windows. The entry line follows
  the transmit slice's band, mode and frequency, logged contacts can be
  marked as spots on the panadapter, and **Find radios** fills in the
  radio's address. On a Mac, allow OpenVKCL under System Settings → Privacy
  & Security → Local Network the first time.
- A warning when a contact's time is more than a minute ahead of the clock,
  on the entry line when **Enter time** is ticked and when editing a contact.

### Changed

- RST sent is shown before RST received. In a contest, Space moves from the
  call to RST received, then the serial, number and locator. In a general
  log it moves through call, RST sent, RST received, name and comment.
- The header clock, contest timer and re-work countdowns keep their own
  timers, so the rest of the window is not redrawn every second.
