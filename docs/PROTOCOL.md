# BIGBIG WON configuration protocol (413d:2104 family)

This document consolidates what `padtune` knows about the vendor protocol of
BIGBIG WON controllers, with the Rainbow 2 Pro (`ZJ-C1pro`) as the reference
device. Every entry says how it is known:

* **HW** — verified on a real Rainbow 2 Pro (firmware ZKM `6`, profile `280_v36`),
  over the USB cable unless stated otherwise;
* **RE** — read from the official Windows assistant (`BigBigWonAssistant`
  PC Tool 1.0.7.7, `DevMgr.dll` / `WndMgr.dll`) with Ghidra/REA, not yet tested;
* **SRC** — taken from another open-source project (see [Sources](#sources)).

## Transport

| | |
|---|---|
| USB ID | `413d:2104` for the wired controller *and* for the 2.4G dongle while a controller is linked (HW) |
| Idle dongle | `413d:2106`, "Zikway" / "HID zk", single HID interface, when its controller is off or on the cable (HW) |
| Interfaces | 0 = XInput (vendor class FF/5D/01), 1 = HID keyboard + consumer (key-mapping output), 2 = **configuration** (HW) |
| Config interface | HID usage page `0xFF7A`, usage `0x01`, 64-byte input and output reports, **no report ID** (HW). hidapi writes need a leading `0x00` |
| Linux access | `/dev/hidrawN`; udev rule with `TAG+="uaccess"` for both PIDs |

Cable and dongle are indistinguishable by VID/PID, strings or descriptors.
Use `FA` (below): `01` = cable, `00` = dongle (HW).

## Framing

Short frame (requests, replies, ACKs):

```
A5 len cmd payload… sum        len = total frame length, sum = 8-bit sum of all previous bytes
```

Long frame (fragments of a blob, numbered from 1):

```
A4 len op seq data… sum        up to 59 data bytes per 64-byte report
```

Blobs (profiles, macros) are sealed with **CRC-16/MODBUS** (poly `0xA001`
reflected, init `0xFFFF`) over every byte after the CRC, stored big-endian in
bytes `[0..2]`; bytes `[2..4]` hold the blob length, big-endian (HW).

## Commands

| Op | Name (official) | Request | Reply | Status |
|---|---|---|---|---|
| `0B` | GetZkmVersion | `a5 04 0b b4` | `a5 05 0b VV sum` (`36` = '6') | HW |
| `EF` | GetUUID | `a5 0c ef` + 8×`00` + `a0` | 8-byte UUID | HW |
| `E2` | GetMode | `a5 04 e2 8b` | `56 51 01` + ASCII model marker (`ZJ-C1pro`) | HW |
| `D3` | GetProfileSize | `a5 04 d3 7c` | `68 0b` + size u16 LE + `94 02` | HW |
| `D6` | GetBaseProfile | `a5 04 d6 7f` | profile as `A4/D6` fragments | HW |
| `D7` | SetBaseProfile | profile as `A4/D7` fragments | ACK `a5 05 d7 00 81`, then `a5 05 ff d7 80` | HW, persists across power-off without `0E` |
| `D5` | GetMacroList | `a5 04 d5 7e` | 4 × (CRC u16 BE, length u16 BE) | HW |
| `D9` | GetMacroInfo | `a5 05 d9 SS sum` | record in one `A5` frame (≤ 60 bytes) or `A4/D9` fragments | HW (short form) |
| `D8` | SetMacro | `A4/D8` fragments of `[slot] + record` | ACK `a5 05 d8 00 82` | HW |
| `F6` | Get/SetRateOfReturn (polling) | get `a5 04 f6 9f`; set `a5 05 f6 CC sum` | `a5 06 f6 00 CC sum` | HW (codes: 2 = 250, 1 = 500, 0 = 1000, 3 = 2000 Hz) |
| `F7` | Get/SetStepAccuracy | `a5 04 f7 a0` | `00 20 00 00` (adaptive, 32) | HW read; write RE |
| `72` | Get/SetLEDBright | `a5 04 72 1b`; `a5 05 72 VV sum` | value echoed | HW: brightness of the RGB light strip, `00` = off |
| `F5` | Get/SetLogoLED | `a5 04 f5 9e`; `a5 0d f5` + 3×RGB + sum | colours echoed | HW: HOME logo colour; **slot 3 = XInput mode**, `000000` = follow the mode colour |
| `04` | GetDevicePowerInfo | `a5 04 04 ad` | battery %, status | HW |
| `FA` | GetWirelessFlag | `a5 04 fa a3` | `01` cable / `00` dongle | HW |
| `D4` | Get/SetGamepadMode | `a5 04 d4 7d` | `06 01 01` cable / `06 01 00` dongle | HW read |
| `E1` | GetDeviceModeInfo | `a5 04 e1 8a` | `6d 06 00 20 00 00` | HW read |
| `F8` | Get/Set SmartStartStop / SmartTriggerLock | `a5 04 f8 a1` | `01 01 01` | HW read |
| `D2` | SetReceiveGamepadKeyPressedEvent | `a5 05 d2 01 7d` / `… 00 7c` | echo | HW: **pauses gamepad output** while on |
| `02` | key report (pushed) | — | 5-byte payload, first 4 = key bits (u32 BE) | HW over cable only; nothing over the dongle |
| `0D` | (pushed, unknown) | — | 5 zero bytes, ~2/s while `D2` is on | HW |
| `0E` | Save2Device | `a5 05 0e 00 b8` | — | RE (not needed for D7) |
| `1A` | Reset (factory) | `a5 05 1a 00 c4` | — | RE — **never sent** |
| `1B` | Calibration | `a5 06 1b PP QQ sum` | — | RE — **never sent** |
| `70` | SetLED (light strip) | `a5 len 70 00 00 00 ZZ MM 00 SS BB ff` + RGB×1..7 + sum | ACK `a5 05 70 00 1a` | HW, see [Light strip](#light-strip) |
| `70` | SetLEDShow | `a5 04 70 19` | — | RE, never sent |
| `73` | Get/SetLightingMode | `a5 04 73 1c` | no reply on Rainbow 2 Pro | HW |
| `DD`, `DC`, `DA`, `DB` | charge LED, competitive mode, motion rate | | no reply on Rainbow 2 Pro | HW |

No command reports which of the four on-board configurations (white LEDs) is
active; the Configuration key changes it on the controller only (HW).

## Key slots

The same 32 bit numbers index the profile key map, the turbo mask, the live
key report and macro step inputs (HW, every button pressed and decoded):

| Bit | Key | Bit | Key | Bit | Key |
|---|---|---|---|---|---|
| 0 | A | 11 | Menu | 22 | MACRO (front) |
| 1 | B | 12 | Home (+ bit 31) | 23 | M1 |
| 3 | X | 13 | L3 | 24 | M2 |
| 4 | Y | 14 | R3 | 25, 26 | M3, M4 (other models) |
| 6 | LB | 15 | Share / Screenshot | 27 | TURBO (front) |
| 7 | RB | 16–19 | D-pad ↑ ↓ ← → | 28 | Configuration (front) |
| 8 | LT | | | 29 | Mapping (front) |
| 9 | RT | | | 30 | FN (red triangle) |
| 10 | View | | | | |

## Profile `280_v36` (Rainbow 2 Pro)

Same field order as BlitzForge's `484_v37` with the six stick-curve blocks
shrunk from 44 to 10 bytes (everything after them moves by `−0xCC`). The
official assistant also knows `240_v34`, `240_v35`, `335_v60`, `454_v61`,
`484_v37` and `508_v39` (RE); the device announces its own length in `D3` and
in bytes `[2..4]`.

| Offset | Size | Field | Status |
|---|---|---|---|
| `00` | 2 | CRC-16/MODBUS (BE) | HW |
| `02` | 2 | length `0x0118` (BE) | HW |
| `04` | 1 | hair trigger: bit0 = LT, bit1 = RT | SRC |
| `05`/`06` | 1+1 | LT dead zone start, `100 − full press` | HW (5, 90 factory) |
| `07`/`08` | 1+1 | RT, same | HW |
| `0C` | 1 | feature flags (`0x66` factory) | unknown, preserved |
| `0E` | 6 × 10 | stick curves, L/R interleaved × 3 sets: `01 20`, dead zone, anti dead zone, A.x, A.y, B.x, B.y, outer, max | HW layout, SRC semantics |
| `4D` | 4 | bitmap (BE, byte-swapped by the official writer) | unknown, preserved |
| `51`/`52` | 2 | `sensor_r_key[0..1]` (`20 20`) | RE name |
| `57` | 3 × 8 | motion: `h2v_ratio, gyro_sens, gyro_sens, resp_curve, …` (`0a 3c 3c 2a`) | matches official defaults |
| `74` | 4 | turbo mask, key-slot bits (BE) | SRC |
| `78` | 1 | turbo speed: 0 = 5, 1 = 10, 2 = 20 Hz | SRC + manual |
| `7C` | 8 | vibration (`50 50 50 50 ff ff ff ff`) | unknown, preserved |
| `84` | 1 | `0x33` | unknown, preserved |
| `98` | 32 × 4 | key map: `00…` default, `01 TT` controller button `TT`, `01 ff` disabled, `02 MM K1 K2` keyboard (modifiers + HID usages on interface 1) | SRC |

Verified write: changing the LT start from 5 to 6 changed only byte `05` and the
CRC, survived a power cycle, and was restored from the backup.

## Macros

Four slots. Record = 10-byte header + 10 bytes per step (≤ 64 steps).

```
header: CRC(2) len(2) 00 slot runkey p7 p8 p9
        runkey = key slot of the back button (0x17 = M1, 0x18 = M2), 0x20 = unbound
        p7..p9 = 01 00 0d when recorded on the controller (meaning unknown, preserved)
step:   b0 = duration low nibble <<4 | marker bit0, b1 = duration high byte
        duration = 12 bits × 8 ms; inputs u32 BE (key-slot bits); 4 signed analog bytes
```

HW: an empty slot is `crc 00 0a 00 SS 20 00 00 00`; a macro recorded on the
controller (A, B, ↑) decoded to the expected bits and timings; a macro written
by `padtune` (X, pause, Y on M1; A, pause, B on M2) played back correctly.

## Light strip

The "custom light" effect of the RGB strip (shown after a double-click on FN)
is set with one short `70` frame per side; it persists without `0E` and cannot
be read back (HW). Decoded from the official Android app 4.0.9
(`gamepadset.dart::writeLightConfig`, which routes the Rainbow 2 Pro to the
two-zone layout) and the PC Tool's `CDeviceBBwon::SetLED`.

```
A5 len 70 | 00 00 00 zone mode 00 speed bright FF | R G B × 1..7 | sum
zone   1 = left, 2 = right
mode   1 static, 2 breathing, 3 gradient, 5 flicker
speed  (255 - appSpeed) / 10 + 10   (smaller = faster; 0x0A fastest)
bright 50..255
```

HW results: static, breathing, gradient and flicker on both sides; 7 colours in
one 34-byte frame; breathing/flicker with one colour run once and then hold the
colour. Full notes: [hw-notes/light-strip-re.md](hw-notes/light-strip-re.md).

## Not yet decoded
* Meaning of `D4`, `E1`, `F8` fields, profile bytes `0C`, `4D`, `7C..84`.
* Vibration and motion settings semantics on `280_v36`.

## Sources

* [tikokuma/BlitzForge](https://github.com/tikokuma/BlitzForge) (MIT) — Blitz2
  configurator in Rust/Tauri: framing, CRC, `484_v37` profile, F6/F7, macros.
* [XBXSean/BetterBigBigWonAssistant](https://github.com/XBXSean/-BBBWA-BetterBigBigWonAssistant-)
  (MIT) — Rainbow 3 WebHID configurator: `508_v39` layout, frame checksum.
* [feshinkof-boop/armorx-toolkit](https://github.com/feshinkof-boop/armorx-toolkit)
  — ArmorX research, Android opcode index (`writeLightConfig`, `0E`, `D2`).
* Official BIGBIG WON PC Tool 1.0.7.7 (static analysis only, nothing
  redistributed) and the official Rainbow 2 manual (button names, combos).
* [morluto/rea](https://github.com/morluto/rea) and Ghidra for the analysis.
