# Changelog

## [Unreleased]

### Added
- Sticks and Triggers tabs: **Calibrate** recentres the sticks and relearns
  the full travel of sticks and triggers, with live progress (edge sectors
  reached per stick, full presses per trigger).
- Sticks tab → Gyroscope: choose which stick tilting the controller moves
  (off, left, right), invert its X or Y axis, and calibrate the gyroscope.

### Fixed
- Sticks and Triggers tabs: the right-hand cards no longer run past the window
  edge.

### Verified on hardware
- Rainbow 2 Pro (ZKM 6, USB): stick and trigger calibration and gyroscope
  calibration started from the PC with command `1B`. After the stick run every
  axis rests at 0 and reaches full travel, both triggers go from 0 to 255.
  Details in `docs/hw-notes/calibration.md`.
- Rainbow 2 Pro: profile byte `0x4C` = 1 makes the gyroscope move the left
  stick (full range while tilting); bit `04` of byte `0x53` reverses its Y
  axis. Details in `docs/hw-notes/gyro.md`.

## [0.2.1] - 2026-11-08

### Added
- Settings → Theme: choose System, Light or Dark; the choice is saved and
  applied immediately. `PADTUNE_THEME` still overrides it.

## [0.2.0] - 2026-11-08

### Added
- Light theme: the GUI follows the system theme, with "Carta calda" (light)
  and "Viola notte" (dark, the app's violet identity). `PADTUNE_THEME=light|dark`
  forces one.
- Embedded fonts (SIL Open Font License): Inter for text and controls,
  Rajdhani for titles, so the interface looks the same on every system.

### Changed
- Interface polish: consistent rounded corners (rows and cards 12 px, menus 16,
  window 20, pill-shaped chips), taller rows (32 px), slider rails, plots and
  the trigger bar on the track colour, status colours readable in both themes.
- Lower idle CPU use: the GUI's per-frame "modified?" check is about 15×
  cheaper and the device thread polls 13× less often while the key test is off.

### Verified on hardware
- Rainbow 2 Pro (ZKM 6): request round trips measured before and after the
  internal refactor with `examples/bench_io.rs` — unchanged, every request is
  answered on the controller's fixed 40 ms tick.

## [0.1.2] - 2026-10-22

### Fixed
- Rainbow 2 Pro: the M3 and M4 keys (top edge, between LT and RT) are shown,
  remappable, usable with turbo and as macro keys. Verified on hardware: M3 is
  key bit 25 (left), M4 is bit 26 (right).

## [0.1.1] - 2026-10-22

### Added
- Macro editor shows and edits the left/right stick direction of every step
  (8 directions), as recorded by the controller (both sticks verified);
  0 ms steps allowed.
- Verified on hardware: long macros (56 steps, fragmented D9/D8) read and write.

All notable changes are listed here. The project follows
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-20

First release, developed and tested on a BIGBIG WON Rainbow 2 Pro (firmware
ZKM 6, profile `280_v36`).

### Added
- Library with the complete framing, CRC and command set, unit tested on bytes
  captured from the controller.
- GUI (egui) with Device, Buttons, Sticks, Triggers, Lights, Macros, Profiles
  and Settings pages, English and Italian.
- CLI: `list`, `info`, `profile`, `backup`, `restore`, `macros`, `monitor`,
  `set polling|brightness|logo|trigger`.
- Profile, macro, polling, light-strip brightness and HOME logo colour writes,
  each verified by reading it back.
- Light-strip effect (opcode `70`, decoded from the official Android app):
  static, breathing, gradient, flicker, speed, brightness, up to 7 colours per
  side, optional sync with the HOME logo colour; `padtune set strip`.
- Live input: sticks and triggers through the OS gamepad API, every button
  through the controller's key test.
- USB cable vs 2.4G dongle detection; idle dongle detection.
- Tray icon, autostart, single instance; named profiles that send only what
  changed; factory settings.
- Vibration editor (four motors, min/max strength, presets, preview by
  clicking a slider); the controller requires max > min.
- Button remapping to combinations of up to three controller buttons; stick
  curve presets of your own and "copy to the other stick".
- udev rule, desktop entry, icons, `install.sh`, AUR packages.
