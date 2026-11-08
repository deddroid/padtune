# Gyroscope stick mode on the Rainbow 2 Pro

Tests of 2026-11-08: Rainbow 2 Pro, ZKM `6`, profile `280_v36`, USB cable,
XInput mode, Linux. Tags as in [PROTOCOL.md](../PROTOCOL.md).

## Profile byte `0x4C` (HW for left, SRC for the layout)

[BetterBigBigWonAssistant](https://github.com/XBXSean/-BBBWA-BetterBigBigWonAssistant-)
(Rainbow 3, `508_v39`) stores `sensorMode * 16 + enumSensorMode` eight bytes
before the gyroscope parameters, followed by a 4-byte key bitmap. On
`280_v36` the same pattern sits at `0x4C`, with the bitmap at `0x4D..0x50`
and the first parameter (`h2v_ratio = 0a`) at `0x57`.

| Write (from the factory-like profile) | Tilting the controller, sticks untouched |
|---|---|
| `0x4C: 00` (factory) | both sticks at rest |
| `0x5C`, `0x64`, `0x6C: 00 -> 01` (first guess) | both sticks at rest |
| `0x4C: 00 -> 01` | left stick X and Y over the full range −32768..32767, right stick at rest |

Each write used `D7` with read-back (`padtune restore`), one change at a time.
The low nibble follows the official tool's list (`option_joystick_sim_close`,
`_left`, `_right`, `_responsive_right`): `0` off, `1` left (HW), `2` right,
`3` right stick while the response key is held (RE order, ipotesi). The high
nibble (sensor mode) is `0` here and kept untouched by PadTune.

## Axis reversal `0x53` (HW for left Y, SRC for the rest)

BBBWA keeps an axis-flags byte seven bytes after the mode byte: `0x53` here,
`00` on this controller. Its bits depend on the stick: left X `02`, Y `04`,
right X `08`, Y `10` (BBBWA also names `01` for the right stick, meaning
unknown). PadTune exposes them as Invert X / Invert Y for the stick in use. Setting
left Y (`04`) from PadTune reversed the up/down direction of the left stick
while tilting (HW); X and the right-stick bits are not tested yet.

## Behaviour (HW)

The stick follows the rotation speed, not the tilt angle: holding the
controller still at an angle brings the stick back to the centre. With the
factory settings, tilting forward and back moved the left stick's Y the
opposite way the user expected.

## On-controller combos (HW, read-only test)

The manual's combos for motion control did not turn the gyroscope on in
XInput mode. Profile reads (`D6`) after each one showed instead:

| Combo | Profile change |
|---|---|
| FN + L3 | none |
| FN + L3 again | right stick dead zone, set 1 (`0x1A`): 12 -> 0 |
| FN + R3 | `0x1A` back to 12 |
| FN + R3 again | `0x1A` 12 -> 0 |
| trigger + FN + R3 | `0x1A` back to 12 |
| trigger + FN + R3 again | LT hair trigger on (`0x04` bit 0) |

The reads happened a few seconds after each combo; whether a change belongs to
that combo or the previous one is not settled (ipotesi: the controller stores
with a delay). The motion bytes never changed.
