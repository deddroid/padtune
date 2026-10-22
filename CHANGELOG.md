# Changelog

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
