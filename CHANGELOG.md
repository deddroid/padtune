# Changelog

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
- Live input: sticks and triggers through the OS gamepad API, every button
  through the controller's key test.
- USB cable vs 2.4G dongle detection; idle dongle detection.
- Tray icon, autostart, single instance; named profiles that send only what
  changed; factory settings.
- udev rule, desktop entry, icons, `install.sh`, AUR packages.
