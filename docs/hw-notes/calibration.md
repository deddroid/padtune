# Calibration (`1B`) on the Rainbow 2 Pro

Hardware test of 2026-11-08: Rainbow 2 Pro, ZKM `6`, profile `280_v36`, USB
cable, Linux. Tool: [`examples/calibrate.rs`](../../examples/calibrate.rs);
check: [`examples/stick_check.rs`](../../examples/stick_check.rs) plus raw
evdev reads of the trigger axes.

Provenance tags as in [PROTOCOL.md](../PROTOCOL.md): **HW** verified here,
**RE** from PC Tool 1.0.7.7, **ipotesi** inferred, still to prove.

## Command (RE, then HW)

`CDeviceBBwon::Calibration` in `DevMgr.dll` builds `a5 06 1b TT SS sum`. The
calibration window in `WndMgr.dll` calls it with:

| Target `TT` | State `SS` | Frame | Meaning |
|---|---|---|---|
| `00` sticks and triggers | `01` | `a5 06 1b 00 01 c7` | start |
| `00` | `00` | `a5 06 1b 00 00 c6` | end, after the user's moves |
| `01` gyroscope | `01` | `a5 06 1b 01 01 c8` | start |
| `01` | `00` | `a5 06 1b 01 00 c7` | cancel (never sent here) |

All three frames that were sent are accepted (HW). The official texts ask to
open the trigger stops first, to rotate both sticks along the edge 3 times and
pull both triggers fully 3 times; for the gyroscope, to lay the controller on a
level desk (RE, `skin/lang/lan_en.xml`).

## Replies (HW)

The controller answers with `1B` frames whose payload is `[TT, step, 00]`:

| Run | Time | Payload |
|---|---|---|
| Sticks: after START | +44 ms | `00 01 00` |
| | +52 ms | `00 02 00` |
| | +60 ms | `00 03 00` |
| Sticks: after END (sent at +29.1 s) | +24 ms | `00 04 00` |
| | +32 ms | `00 06 00` |
| Gyro: after START | +12 ms | `01 01 00` |
| | +2.0 s | `01 02 00` |
| | +5.0 s | `01 04 00` |

The gyroscope run finishes by itself in about 5 s with no END frame, which
matches the official tool (it only ever sends cancel). Ipotesi on the steps:
`01` accepted, `02` sampling, `03` waiting for the user's moves (sticks only),
`04` finished, `06` stored. The gyroscope never sends `06`.

## Progress reports during the sticks run (HW)

Between START and END the controller pushes `02` reports (the key-report
opcode) without `D2` being on. They are progress flags, not key bits:

* byte 4, high nibble: where the **right** stick is at the edge: up `10`,
  right `80`, down `20`, left `40`; two neighbours together are the diagonal
  (up-right `90`). A clockwise rotation from the top reads
  `10 90 80 a0 20 60 40 50`.
* byte 4, low nibble: the same bits for the **left** stick: up `01`, right
  `08`, down `02`, left `04`.
* The directions were logged one at a time on a second run (2026-11-08); the
  first run's rotation, `20 a0 80 90 10 50 40 60`, was counter-clockwise from
  the bottom.
* bytes 0..3: the usual key bits (u32 BE, key slots as in the key report).
  The `20`/`40` seen in byte 2 on the first run are L3 and R3 (slots 13, 14),
  not the triggers: PadTune counted them as LT/RT until the user noticed that
  clicking the sticks filled the trigger dots. LT and RT are slots 8 and 9
  (byte 2 `01`, `02`).
* Sticks (HW, 2026-11-08): rotating the left stick alone in PadTune lit the
  ring of the other stick until the nibbles were swapped, so low = left.
* Whether LT/RT key bits are set during the run at full travel or at a
  lower threshold is not measured yet (ipotesi: same threshold as the key
  report).

## Result (HW)

After the sticks run, measured through evdev (`stick_check` and raw reads):

| Axis | At rest | Range |
|---|---|---|
| Left X / Y | 0.000 / 0.000 | −0.997..+1.000 / −0.994..+0.999 |
| Right X / Y | 0.000 / 0.000 | −1.000..+0.996 / −1.000..+0.995 |
| LT (ABS_Z, 0..255) | 0 | max 255 |
| RT (ABS_RZ, 0..255) | 0 | max 255 |

The gyroscope's effect cannot be measured here: the XInput and vendor reports
carry no raw motion data, only its effect on the sticks once it is mapped.

## Side effects (HW)

* The profile read with `D6` is byte-identical before and after the sticks run
  (`hw-backups/2026-11-08-before-calibration.bin`): calibration data lives
  outside the profile.
* `F8` reads `01 01 01` before and after both runs.

## Open points

* Meaning of step `05`, if it exists, and of a failed run's reply.
* Whether gamepad output pauses during the sticks run, as it does with `D2`.
* What the controller replies after END when the sticks were not rotated
  (PadTune reported a missing confirmation once; it now logs every step to
  stderr).
