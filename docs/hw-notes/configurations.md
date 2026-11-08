# On-board configurations (Rainbow 2 Pro)

Read-only test of 2026-11-08: Rainbow 2 Pro, ZKM `6`, profile `280_v36`, USB
cable. After each press of the Configuration key, the profile (`D6`) and the
replies to the GET commands were read. Tags as in [PROTOCOL.md](../PROTOCOL.md).

## Results (HW)

| White LED | `D4` GetGamepadMode | Profile (`D6`) |
|---|---|---|
| 2 (start) | `06 01 01` | the one PadTune had been editing |
| 3 | `06 02 01` | another profile |
| 4 | `06 03 01` | another profile (same bytes as LED 3) |
| 1 | `06 00 01` | another profile (turbo mask and LT hair trigger set) |
| 2 again | `06 01 01` | identical to the start |

* **`D4` byte 1 is the active configuration**, `0`..`3` from the top LED.
* **`D6` and `D7` work on the active configuration.** Each configuration has
  its own full profile; to read or write another one, switch to it first.
* `E1` (`6d 06 00 20 00 00`), `F6` and the macro list (`D5`) did not change:
  macros are shared by all configurations.

## Selecting a configuration from the PC (not tested)

The official PC Tool 1.0.7.7 never switches configuration: it edits the
"device current configuration" and keeps other configurations as a list on
the PC. `CDeviceBBwon::SetGamepadModeRequest(a, b, c)` sends
`a5 07 d4 a b c sum`, the write counterpart of the `D4` reply, but its caller
in `WndMgr.dll` was not found. Candidate frame to select LED 1, keeping the
other two bytes as read: `a5 07 d4 06 00 01 87` (ipotesi).
