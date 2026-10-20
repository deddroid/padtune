# Rainbow 2 Pro: USB cable vs 2.4G dongle (read-only comparison, 2026-10-20)

Device: BIGBIG WON Rainbow 2 Pro, model marker `ZJ-C1pro`, profile layout 280_v36.
Both links were read with read-only GETs only (no `set`/`restore`, no 1A/1B/0E, no D7/D8).
Short frames: `A5 len cmd payload sum8`. Checksums of every reply below are OK.

## USB enumeration

| | Cable | Dongle |
|---|---|---|
| VID:PID | 413d:2104 | 413d:2104 |
| iManufacturer / iProduct | "2.4G" / "XBOX 360 For Windows" | "2.4G" / "XBOX 360 For Windows" |
| iSerial | none (0) | none (0) |
| bcdDevice / bcdUSB / MaxPower | 1.00 / 2.00 / 100 mA | 1.00 / 2.00 / 100 mA |
| Interfaces | identical to dongle | 3: if0 vendor FF/5D/01 (XInput, EP 0x81 IN / 0x02 OUT, 32 B); if1 HID boot keyboard (EP 0x82 IN, 16 B, report desc 86 B); if2 HID vendor (EP 0x83 IN / 0x03 OUT, 64 B, report desc 29 B) |
| xpad input name | "Black Shark Green Ghost Gamepad" | "Black Shark Green Ghost Gamepad" |
| Report descriptors (if1, if2) | identical (below) | identical (below) |

Config interface (if2) report descriptor (29 bytes, same on both links), usage page 0xFF7A, 64-byte in/out, no report ID:

```
06 7a ff 09 01 a1 01 09 02 a1 00 09 03 15 00 25 ff 75 08 95 40 81 02 09 05 91 02 c0 c0
```

if1 (keyboard + consumer, report IDs 1 and 3):

```
05010906a1018501050719e029e71500250175019508810295037501050819012903910295057501910195077508150026ff00
050719002aff008100c0050c0901a1018503751095011500268c0219002a8c028100c0
```

The USB identity is the same on both links, so VID/PID/strings cannot tell
them apart. See "Telling the links apart" below.

## GET replies, byte for byte

| Op | Name | Cable reply | Dongle reply | Same? |
|---|---|---|---|---|
| E2 | GetMode | `a5 10 e2 56 51 01 5a 4a 2d 43 31 70 72 6f 00 d5` ("VQ\x01" + "ZJ-C1pro") | `a5 10 e2 56 51 01 5a 4a 2d 43 31 70 72 6f 00 d5` | yes |
| 0B | ZKM version | `36` | `a5 05 0b 36 eb` → `36` | yes |
| EF | UUID | `22 20 b2 1a ff 02 06 08` | `a5 0c ef 22 20 b2 1a ff 02 06 08 bd` | yes |
| D3 | Profile size | `68 0b 18 01 94 02` (280 B) | `a5 0a d3 68 0b 18 01 94 02 a4` | yes |
| F6 | Polling | `00 00` (1000 Hz) | `a5 06 f6 00 00 a1` | yes |
| F7 | Step accuracy | `00 20 00 00` | `a5 08 f7 00 20 00 00 c4` | yes |
| 72 | LED brightness | `ff` | `a5 05 72 ff 1b` | yes |
| F5 | Logo LED | 9 × `00` | `a5 0d f5 00 00 00 00 00 00 00 00 00 a7` | yes |
| D4 | Gamepad mode | `06 01 01` | `a5 07 d4 06 01 00 87` → `06 01 00` | **NO** (3rd byte 01 → 00) |
| 04 | Power info | `64 00` (100 %, status 00) | `a5 06 04 62 00 11` → `62 00` (98 %, status 00) | battery differs, status same |
| FA | Wireless flag | `01` | `a5 05 fa 00 a4` → `00` | **NO** (01 → 00) |
| F8 | Smart trigger lock | `01 01 01` | `a5 07 f8 01 01 01 a7` | yes |
| E1 | Device mode info | `6d 06 00 20 00 00` | `a5 0a e1 6d 06 00 20 00 00 23` | yes |
| 73 | Lighting mode | no reply | no reply | yes |
| DD | Charge LED effect | no reply | no reply | yes |
| DC | Competitive mode | no reply | no reply | yes |
| DA | Motion rate | no reply | no reply | yes |
| DB | Motion rate options | no reply | no reply | yes |

Profile (D6, 280 bytes) read over the dongle is **byte-identical** to
`hw-backups/profile-2026-10-20-original.bin` (read over the cable). Macros:
4 empty slots on both links (CRCs cbe1/0bdc/0b98/cba5 over the dongle).

Notes:
- The FA value is the opposite of what its name suggests: `01` on the cable, `00` on the dongle.
  It may mean "wired" (or "charging/USB power present") rather than "wireless".
- The power status byte is `00` on both links, so it is not a link indicator either.
- D4's third byte follows FA (cable 01, dongle 00) and could serve as a second check.

## Key reports (D2) over the dongle

`padtune monitor 60` over the dongle, run twice (once by Claude, once by the user):
- `D2 01` is acknowledged (`frame 0xd2 [01]`).
- **No opcode 02 key reports at all**, even with the user pressing A, M1, M2, TURBO, Mapping,
  MACRO, Configuration and FN.
- Instead a frame `a5 .. 0d 00 00 00 00 00` arrives about every 0.5 s (~120 in 60 s), with the
  payload always zero. (Over the cable a single unknown `a5 09 0d 00..` frame had been seen once.)
- `D2 00` at the end is accepted ("Key reports switched off").

So live key reports (and the GUI key-test mode) do **not** work over the 2.4G dongle. They are verified over the cable only.
Opcode 0D is probably a link/heartbeat frame from the dongle (or an empty
key/axis frame); its meaning is unknown.

### Gamepad output while D2 is active (dongle): it stops completely

Combined test: evdev log of the xpad gamepad (`/dev/input/event8`) for 45 s,
with `padtune monitor 20` running only from t=15 s to t=35 s. The user moved
the sticks and pressed A/M1/M2/TURBO the whole time.

- t=0–15.03 s: continuous ABS events (sticks, ~2700 lines).
- **t=15.03 → 35.25 s: zero input events of any kind** (no ABS, no KEY).
- t=35.25 s (right after `D2 00`): events resume; the first one is a re-centre (`ABS_X 0`, `ABS_Y -1`).
- Monitor in that window again showed only `D2 01` echo + ~40 zero `0x0d` frames, no opcode 02.

So over the dongle, D2 01 **pauses the XInput gamepad output** and gives nothing
back on the config interface: the controller is unusable while key-report mode
is on, and the key test shows nothing. Always send `D2 00` when leaving the key-test view
(the GUI already does this) and do not offer the key test over the dongle.
### Same test over the USB cable

Same script on the cable (`/dev/input/event12`, `/dev/hidraw12`):
- Gamepad events also stop: the last one is at t=15.0x s, then **zero events from ~15.1 s to 35.03 s**, and they resume at 35.03 s after `D2 00`.
- Key reports DO arrive on the cable (opcode 02): A → `00 00 00 01`, M1 → `00 80 00 00`,
  M2 → `01 00 00 00`, TURBO → `08 00 00 00`, LT bit8 seen while M1 held; each release sends `00 00 00 00`.
  The 5th payload byte varies (69, 89, 88, 51, a8, a5, 6a, 2a) and its meaning is unknown.
- ~40 zero `0x0d` frames in 20 s on the cable too, so 0D is not dongle-specific (heartbeat?).

**Conclusion:** D2 01 pauses gamepad output on both links. It is a dedicated key-test
mode, not a passive sniffer. Only the cable gets opcode 02 reports back.

## Both cable and dongle connected

There are never two config interfaces. When the cable is plugged in while the
controller is paired to the dongle, the controller drops the wireless link
and stays on the cable (the user saw this directly). The dongle then
re-enumerates as a different, idle device:

| | Dongle, controller linked | Dongle, controller on cable (idle) |
|---|---|---|
| VID:PID | 413d:2104 | **413d:2106** |
| Strings | "2.4G" / "XBOX 360 For Windows" | **"Zikway" / "HID zk"** |
| Interfaces | 3 (XInput + keyboard + config) | **1**: HID if0, EP 0x83 IN / 0x03 OUT, 64 B, same 29-byte 0xFF7A descriptor |

- `padtune list` shows only the cable (413d:2104, `/dev/hidraw12`); its FA = `01`, D4 = `06 01 01`, 04 = `64 00`.
- The idle dongle's hidraw node (`/dev/hidraw10`) is `crw------- root`, because the udev rule only matches
  PID 2104, so it could not be probed. It is probably the dongle's own config/pairing channel.
  To try it, add `ATTRS{idProduct}=="2106"` to the udev rule. That would also let `list` show "dongle present, controller not linked".
- Cable re-enumerated on another bus (Bus 003) and the dongle stayed on Bus 001; hidraw numbers changed.

Consequences for padtune:
- Only one 2104 config interface exists at a time, so `--device` disambiguation is not needed for cable+dongle.
- PID 2106 is a useful state: "dongle plugged, controller asleep or on cable". `list` currently filters it out.

## Proposed `link_from_flag` (src/actions.rs)

```rust
/// Maps the `FA` flag to a link type. Verified on hardware (2026-10-20):
/// `01` over the USB cable, `00` over the 2.4G dongle (same VID/PID/strings
/// on both, so this is the only reliable way to tell them apart).
pub fn link_from_flag(flag: u8) -> Option<Link> {
    match flag {
        0x01 => Some(Link::Usb),
        0x00 => Some(Link::Dongle),
        _ => None,
    }
}
```

Optional cross-check: `D4` reply byte 2 (`06 01 xx`) is `01` on the cable and
`00` on the dongle.

Also worth doing in the GUI: when `connection == Some(Link::Dongle)`, disable or
label the key-test / live key-report view ("requires USB cable"), because no
opcode 02 reports arrive over the dongle.
