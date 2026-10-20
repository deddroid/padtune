# Rainbow 2 Pro: custom light-strip colour/effect (RE of the official Android app, 2026-10-20)

Status: **recovered statically, NOT yet tested on hardware.** Nothing was sent to the controller.

Summary: the app sets the "custom light" strip with opcode **0x70** (the PC Tool calls the same opcode
`CDeviceBBwon::SetLED`), one command per side (left/right):

```
A5 len 70 | 00 00 00 | zone mode 00 spd bright FF | R G B [R G B …] | sum8
           \________ 9-byte light header ________/  1..7 colours
```

`len` = total frame length including the checksum (= 13 + 3·n). The checksum is the usual sum8 of every
byte before it. The app sends left then right, then `A5 05 0E 00 B8` (Save2Device).

## Sources and integrity

| Item | Value |
|---|---|
| App | "BIGBIG WON", package `com.moojiang.bigbigwon.mygt`, **v4.0.9 (versionCode 411)**, minSdk 24, targetSdk 36 |
| Where from | bigbigwon.com → Support → RAINBOW 2 Pro‑APP links to Google Play `com.moojiang.bigbigwon.mygt`; the XAPK was fetched from APKPure with `apkeep` 0.17.0 (EFF) |
| XAPK sha256 | `eb2590a83e40dbf39115a7a5ff489947156dc2559e56671c80fbbfc47ac5e14e` |
| base.apk sha256 | `0b109e1c78408338e26a93b8ba9f91960bac739e974cc3ee8ef03900d10203c9` |
| config.arm64_v8a.apk sha256 | `30a96c9fb728218b89f115e18003e370f63cfa35bf42d2db63cccf4400ca41fc` |
| libapp.so sha256 | `dcb5067fce7c15b65333ce9f2c2952c48a8c31795ebb7cba09af26b8776cb479` |
| Signer (v2+v3, same for base and all 4 splits) | cert SHA‑256 `F7:45:43:34:0E:EE:32:74:63:10:5E:2A:FB:A8:FB:AC:0C:9C:F5:FD:FE:AD:32:FC:9D:F7:12:AB:09:18:7B:62` (Google Play App Signing key, "CN=Android, O=Google Inc.", valid from 2026‑01‑15). Certificates were extracted and compared; the v2/v3 signature itself was not verified cryptographically (no apksigner available). |
| Flutter / Dart | Dart 3.12.2, snapshot `ace654289f5abc240509fc941453ebc5`, arm64, compressed pointers |

The app includes the asset `assets/device/rainbow_c1_pro.png`, and our device reports the model `ZJ-C1pro`.
Note: this is **not** the package that armorx-toolkit analysed (`com.moojiang.bigbigwon` 2.23/2.24). In
4.0.9 the function `writeLightConfig` uses opcode 0x70. armorx's index lists "A4/FF" for it. The FF there
is most likely the constant `0xFF` that the function stores (mask/trailer byte), not an opcode.

Tools (all in `ZTOOL/`): `apkeep/`, `apk/` (XAPK, extracted splits, `libs/`, blutter output
`apk/blutter-out/`), `blutter/` (worawit/blutter, built against Dart 3.12.2 with the portable `cmake/` and `ninja/`), Ghidra 12.1.4
headless with `ghidra/scripts/DartDecomp.java`, which applies blutter's names and decompiles chosen addresses (output
`apk/ghidra-light.c`). REA 6.7.0 (`rea decompile`, Ghidra provider) loaded libapp.so but found no procedure,
because Ghidra does not discover Dart AOT functions by itself. So the Ghidra pass ran headless with the blutter
symbol script. Addresses below are blutter/ELF vaddrs; Ghidra shows them +0x100000 (image base).

## Device routing in the app

- `Device` enum: `devRainbow2Pro` = 3, `devRainbow3` = 4 (objs.txt).
- `rainbow_root.dart::_initTabViews` @ 0x8e9f34: Rainbow2Lite/C2SL/Gale2 get other tabs. Every other Rainbow,
  including devRainbow2Pro, gets `RainbowDeviceConfig1s` + **`RainbowLightConfigR3`** (the light tab).
- `rainbow3_tab_light.dart::navigatorToLightSetting` @ ~0x98d6d8: if `curDevice == devRainbow3`, it opens
  `Rainbow3NewConfigLightWidget` (per-LED, 14 zones). Otherwise it opens **`RainbowNewConfigLightWidget`**
  (`rainbow_config_light.dart`), the path for the Rainbow 2 Pro.
- Every write goes through `gamepadset.dart::writeLightConfig` @ **0x834f84** (size 0x17b0). Inside it,
  `curDevice == devRainbow3` selects an R3 header (LED mask from `lightIds`). Every other device gets the
  layout below.

## Data model (`LightData`, toJson @ 0x82f9c0 / fromJson @ 0x836b9x)

| JSON key | field | meaning |
|---|---|---|
| `mode` | +0x07 | 1 = Normal (static), 2 = Breathing, 3 = Gradient, 5 = Flicker (UI index 0..3 maps to 1,2,3,5 in the `_RainbowLightModeState` closure @ 0xaaf540; reverse map in initState @ 0x905ec8). 4 is never produced. |
| `speed` | +0x0f | slider 0..255 (default 255, SharedPreferences key `LightSpeed`) |
| `bright` | +0x17 | slider **50..255** (default 255, key `LightBright`) |
| `same` | +0x1f | "Lighting of the same color" (左右同色): the left list is used for both sides |
| `leftList` / `rightList` | +0x23 / +0x27 | colour lists stored as strings `"[r, g, b]-[r, g, b]-…"`; **max 7 colours** per side (`onChangedColor` @ 0xab0958 `cmp #7`) |

Built-in default for a new config (pp+0x5bc18):
`{"mode":3,"speed":255,"bright":255,"same":false,"leftList":"[0, 0, 255]","rightList":"[255, 0, 255]"}`.

A colour list is the set of colours the effect cycles through. It is not one colour per LED: Rainbow 2 Pro only has
left and right zones. A Normal/static effect uses the first colour (assumption, see confidence).

## Callers

`_RainbowNewConfigLightWidgetState::setEffect` @ 0x90520c and `RainbowLightConfigR3::applyEffect` @ ~0x834d00:

```
if same:  writeLightConfig(ctx, lightLeft,  mode, speed, bright, leftList)
          writeLightConfig(ctx, lightRight, mode, speed, bright, leftList)
else:     writeLightConfig(ctx, lightLeft,  mode, speed, bright, leftList,  lightIds:[7..13])
          writeLightConfig(ctx, lightRight, mode, speed, bright, rightList, lightIds:[1..6,14])
applyEffect then: writeDevice()  -> A5 05 0E 00 B8   (gamepadset.dart::writeDevice @ 0x8297a4)
```

`lightIds` are used only on the Rainbow 3 branch. `LightSelect` enum: lightLeft = 0, lightRight = 1.

## Frame layout (non-Rainbow3 branch of writeLightConfig)

Payload (9-byte header + colours):

| off | value | source |
|---|---|---|
| 0..2 | `00 00 00` | constant (initial 3-element list) |
| 3 | zone: **1 = left, 2 = right** (3 for any other LightSelect index; none exists) | `LightSelect.index + 1` @ 0x8353b4.. |
| 4 | mode (1/2/3/5) | arg |
| 5 | `00` | constant @ 0x8355fc |
| 6 | speed byte = `(255 - speed) / 10 + 10` (integer division): 255→0x0A (fastest?), 128→0x16, 0→0x23 | @ 0x835604..0x83560c; Ghidra: `(0xff - lVar6) / 10 + 10` |
| 7 | bright (50..255) | arg |
| 8 | `FF` | constant @ 0x835830 |
| 9.. | R,G,B per colour | `changeLightColorList` @ 0x836970 (r=+7, g=+0xf, b=+0x17) |

Framing in the app (BLE, `mtu()` = 23 and `subpackageLength()` = 20 for devRainbow2Pro, define.dart @ 0x836864 / 0x8294e8):

- colours·3 < mtu−14 = 9 bytes (1–2 colours): **short frame** `A5 len 70 <payload> sum` @ 0x835dac..
- otherwise: **long frames** `A4 len 70 seq <≤15 data bytes> sum`, seq from 1, `len` = data+5, followed by
  an empty **terminator** frame `A4 05 70 <last seq+1> sum` (@ 0x835c24..0x835da8).
- checksum: `define.dart::getCheckSum` @ 0x80a5e8 = sum of all bytes & 0xFF (same as the rest of the protocol).

The device answers with an `A5 … 70 …` frame. The app only logs it ("灯效命令发送成功", light command sent
successfully, `rainbow3_provider.dart` listener @ 0x90b8a4) and moves its command queue on. By analogy with D7
(`a5 05 d7 00 81`) the ACK is probably `A5 05 70 00 1A`. This is unverified.

### What to use over USB HID

The PC Tool's `CDeviceBBwon::SetLED` (DevMgr.c @ 0x10021b00) builds exactly this frame in **one short
frame**: a 12-byte header (`A5 len 70 00 | 00 00 zone mode | 00 spd bright FF`) followed by RGB triples and a
checksum, with `len = 12 + 3n + 1`. A second overload @ 0x10021a30 sends a fixed 16-byte frame (1 colour). On a
64-byte report even 7 colours fit (34 bytes), so the safest USB form is the **single A5 frame**. The A4 form is
the app's BLE fragmentation. It should also work if needed (one 59-byte fragment + terminator), but our D7
writer sends no terminator. Mimicking the app here means sending the terminator.

## Example frames (checksums computed)

USB short form (recommended):

| effect | frame |
|---|---|
| left, static blue, bright 255, speed 255 | `A5 10 70 00 00 00 01 01 00 0A FF FF 00 00 FF 2E` |
| right, static blue | `A5 10 70 00 00 00 02 01 00 0A FF FF 00 00 FF 2F` |
| left, static red | `A5 10 70 00 00 00 01 01 00 0A FF FF FF 00 00 2E` |
| left, breathing green, speed 128, bright 200 | `A5 10 70 00 00 00 01 02 00 16 C8 FF 00 FF 00 04` |
| left, gradient R→Y→G, 255/255 | `A5 16 70 00 00 00 01 03 00 0A FF FF FF 00 00 FF FF 00 00 FF 00 33` |
| right, gradient R→Y→G | `A5 16 70 00 00 00 02 03 00 0A FF FF FF 00 00 FF FF 00 00 FF 00 34` |
| left, flicker blue, speed 0 | `A5 10 70 00 00 00 01 05 00 23 FF FF 00 00 FF 4B` |
| left, gradient 7 colours (max) | `A5 22 70 00 00 00 01 03 00 0A FF FF FF 00 00 FF FF 00 00 FF 00 00 FF FF 00 00 FF FF 00 FF FF FF FF 37` |

The same "left gradient R,Y,G" frame exactly as the app sends it over BLE (15-byte chunks + terminator):

```
A4 14 70 01 00 00 00 01 03 00 0A FF FF FF 00 00 FF FF 00 32
A4 08 70 02 00 FF 00 1D
A4 05 70 03 1C
```

Long form on USB (one 59-byte-capable fragment + terminator):
`A4 17 70 01 00 00 00 01 03 00 0A FF FF FF 00 00 FF FF 00 00 FF 00 34` then `A4 05 70 02 1B`.

## Companion commands

| frame | meaning | evidence |
|---|---|---|
| `A5 05 0E 00 B8` | Save2Device (flash commit), sent by the app after the two light writes | gamepadset `writeDevice` @ 0x8297a4; Ghidra `0x14a,10,0x1c` |
| `A5 04 73 1C` | GetLightingMode / light-effect enable state (获取灯效模式). Reply byte[3]: `2` → state flag false, else true | `getLightEnableState` @ 0x82a3d4, reply parse @ 0x82ac90; PC Tool `GetLightingMode` 0x1c7304a5 |
| `A5 05 73 02 1F` / `A5 05 73 00 1D` | set light effect on / off (the app refuses "电量低" (battery low) under 10 %) | `setLightEnable` @ 0x8347d0 (`enabled ? 2 : 0`) |
| `A5 04 70 19` | PC Tool `CDeviceBBwon::SetLEDShow` (0x70 with no payload). Meaning unknown (maybe preview/apply or a read). The app never sends it. | DevMgr.c FUN_10021dd0 |
| `A5 05 F5 …` | HOME logo colour (already implemented). The light tab reads it back via F5 | listener @ 0x82a770 `cmp #0x1ea` |

Read-back: **none known.** The app keeps light configs locally (SharedPreferences / its server configs) and
never reads the strip colours from the device. Our earlier test of 0x73 got no reply on the cable link. The
Rainbow 3 also uses 0x73 for the enable flag, so on the Rainbow 2 Pro, 0x73 may not apply at all.

## Cross-check with the Windows PC Tool

- `CDeviceBBwon::SetLED` (two overloads @ 0x10021a30 / 0x10021b00) matches the header size (12), the length
  formula and the RGB-triple tail above. `SetLEDShow` = `a5 04 70`. `Get/SetLightingMode` = opcode 0x73.
- The Rainbow 3 JSON model (`effectMode, colorMode, lightZoneR3, speed, bright, symmetrical, lightColorMap{1..14}`,
  DefaultLight.ini) is the **per-LED R3 model** (14 zones = app `lightIds` 1..14, mask header). It does not
  apply to the Rainbow 2 Pro, which uses the simpler 2-zone header above.

## Confidence

- Opcode 0x70, frame/header layout, zone and mode values, speed formula, checksum, max 7 colours: **high**. The app
  code (blutter asm + Ghidra decompile) and the PC Tool's SetLED builder agree independently.
- That this exact path runs for the Rainbow 2 Pro: **high** (explicit device routing above).
- Firmware accepting the single A5 frame over USB with 3–7 colours: **medium-high** (the PC Tool does it; the app
  fragments only because of the BLE MTU).
- Exact visual meaning: **medium**. Speed direction (larger byte = slower?), whether static uses only the
  first colour, and whether the custom mode must be selected (FN double-click) for the change to show are all open.
  Persistence without `0E` is unknown (D7 persisted without it).
- ACK format: **low** (inferred).

## Proposed first hardware test (needs the user's OK; the main session sends)

Preconditions: USB cable link (FA = 01), strip set to **custom light** mode with FN double-click (it now shows the
red/yellow/green gradient), brightness 72 = ff. Do not send 0E, 1A or 1B.

1. Send `A5 10 70 00 00 00 01 01 00 0A FF FF 00 00 FF 2E` (left zone, static blue). Read the reply (expect
   `A5 05 70 00 1A` or similar) and ask the user what the left half of the strip shows.
2. If step 1 works, send `A5 10 70 00 00 00 02 01 00 0A FF FF 00 00 FF 2F` (right zone, static blue).
3. Rollback to something close to the factory look: `A5 16 70 00 00 00 01 03 00 0A FF FF FF 00 00 FF FF 00 00 FF 00 33`
   and `A5 16 70 00 00 00 02 03 00 0A FF FF FF 00 00 FF FF 00 00 FF 00 34` (gradient red→yellow→green).
   The factory values cannot be read back, so this is an approximation. Factory reset 1A is the last resort.
4. Persistence: power-cycle and check whether the colour survives without `0E`. Only if it does not, ask before
   sending `A5 05 0E 00 B8` (the app always sends it).

If step 1 gets no reply or no visible change, try the app's exact BLE framing over HID
(`A4 11 70 01 00 00 00 01 01 00 0A FF FF 00 00 FF 2F`, then `A4 05 70 02 1B`) before concluding anything.
Then try a breathing/speed sweep (`spd` 0x0A vs 0x23) to fix the speed direction.

## Key evidence index (libapp.so 4.0.9, blutter addresses)

- `gamepadset.dart::writeLightConfig` 0x834f84, `changeLightColorList` 0x836970, `lightIdsToMask` 0x836734,
  `writeDevice` 0x8297a4, `LightData.toJson` 0x82f9c0, `LightData.fromJson` 0x836b9x.
- `define.dart::getCheckSum` 0x80a5e8, `subpackageLength` 0x8294e8, `mtu` 0x836864.
- `rainbow_config_light.dart::_RainbowNewConfigLightWidgetState::setEffect` 0x90520c, mode map 0xaaf540 / 0x905ec8,
  colour limit 0xab0958, sliders in `_RainbowLightLightState::build` (bright 50..255, speed 0..255).
- `rainbow3_tab_light.dart::applyEffect` ~0x834d00, `getLightEnableState` 0x82a3d4, `setLightEnable` 0x8347d0,
  `navigatorToLightSetting` 0x98d6d8; `rainbow_root.dart::_initTabViews` 0x8e9f34.
- Ghidra decompile of these: `ZTOOL/apk/ghidra-light.c`. Full blutter asm: `ZTOOL/apk/blutter-out/asm/moojiang/`.
