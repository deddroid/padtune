# padtune — multi-pad architecture

How `padtune` grows from a single-controller BIGBIG WON configurator into a core
that drives several controller families, **without disturbing the working
BIGBIG WON support that ships today**.

Status: design note (2026-10-30). Nothing here is implemented yet; the current
crate is unchanged. This is the plan the refactor should follow once enough
devices are understood.

## The problem the hardware revealed

The original hope was "same ODM, similar protocol — add a few layouts". Probing
real hardware disproved the easy version of that:

| Controller | Transport | Config protocol | How config is reached |
|---|---|---|---|
| Rainbow 2 Pro *(reference)* | USB-HID (vendor usage page `0xFF7A`) | **BIGBIG WON** `A5/A4` + CRC-16/MODBUS | wired / dongle, [`docs/PROTOCOL.md`](PROTOCOL.md) |
| EasySMX X10 (`3537:1023`) | **Bluetooth LE** (`FF00`/`FF01`/`FF02`) | **Kr** (`com.krly.platform`): `[len][opcode][content][SN]`, no checksum | Android app only, [`docs/hw-notes/x10.md`](hw-notes/x10.md) |
| Machenike G5 Pro V2 (`2345:2104`) | **unknown** — shares the `2104` XInput PID; no `0xFF7A` HID channel in wired/XInput | **unknown** (A5? Kr? other?) | to be determined |

Two independent things vary across devices, not one:

1. **Transport** — how bytes reach the controller: USB-HID (`hidraw`) vs
   Bluetooth LE (GATT) vs possibly USB bulk/interrupt.
2. **Protocol** — the command language on top: BIGBIG WON `A5/A4` vs Kr
   `[len][opcode][SN]`.

A single "profile layout" table (today's [`protocol/profile.rs`](../src/protocol/profile.rs)
`Layout`) only ever addressed a sliver of axis 2 within one protocol. The real
abstraction has to span both axes.

## Design principles

* **Keep `main` shippable at every step.** BIGBIG WON support is released
  (v0.1.2, AUR) and HW-verified by tests. It is refactored *behind* new traits
  without behavior change, guarded by the existing tests — never rewritten.
* **Do not design the abstraction before the devices are known.** Each new
  family is first reverse-engineered and prototyped *standalone* (read-only).
  The common traits are derived from the **intersection of real capabilities**
  of 2–3 understood devices, not invented up front. Premature abstraction is the
  main risk here.
* **One repository, interchangeable backends — not separate apps.** Separate
  projects duplicate UI/config/i18n/packaging and never merge cleanly later. A
  Cargo workspace lets each family be its own crate while the generic parts stay
  shared from day one.
* **Optional, feature-gated backends.** A BLE backend pulls async + platform BT
  permissions; it must not weigh down the lean USB-HID build. Each transport is
  a Cargo feature.

## Target shape (Cargo workspace)

```
padtune/
├─ padtune-core/        # the contracts + the device-neutral model
│   Transport           #   trait: open/send/recv framed bytes (USB-HID | BLE | USB-bulk)
│   Controller          #   trait: capabilities as methods (triggers, curves,
│                       #          lights, macros, polling, battery, identity…)
│   Capabilities        #   what THIS unit supports (so the UI shows only real controls)
│   PadLayout           #   physical button/stick map for drawing the pad
├─ padtune-bigbigwon/   # today's A5/A4 + CRC-MODBUS + profile layouts, behind Controller
├─ padtune-kr/          # Kr command set (EasySMX/KRLY) + BLE transport  [feature: ble]
├─ padtune-gui/         # the shared UI, driven by Capabilities + PadLayout
└─ padtune-cli/         # the shared CLI
```

The GUI and CLI talk **only** to `Controller` + `Capabilities` + `PadLayout`.
They never know whether the bytes went over `hidraw` or GATT, or whether the
frame was `A5` or Kr.

### The two traits (sketch, not final)

* `Transport` — thin byte pipe: `send(&[u8])`, `recv(timeout) -> Vec<u8>`,
  framing left to the protocol layer. Implementations: `HidTransport`
  (today's [`device.rs`](../src/device.rs)), `BleTransport` (btleplug,
  `FF01` write / `FF02` notify), and — if the X10 USB experiment pans out —
  a USB interrupt transport reusing `hidraw`.
* `Controller` — capability methods returning a neutral model
  (`Trigger`, `Curve`, `Lights`, `Macro`, …, which largely already exist in
  [`protocol/profile.rs`](../src/protocol/profile.rs)). Each family implements it
  over its own protocol. `Capabilities` lets a device say "I have no light
  strip" / "I have 4 macro slots" so the UI adapts instead of assuming a
  Rainbow 2 Pro.

### Why the UI must become capability-driven

[`gui/pad.rs`](../src/gui/pad.rs) is traced from a Rainbow 2 Pro product photo
and the control set is the BIGBIG WON one. To be shared it must be driven by
`PadLayout` (per-device geometry) + `Capabilities` (which panels to show). This
is the single biggest GUI change and the main reason to unify rather than fork
the UI into separate apps.

## Phased plan

**Phase 1 — Explore (now).** One session per controller family (forked from the
main session), each read-only, each producing:
* a protocol spec under `docs/hw-notes/<device>.md` (done for the X10);
* a minimal standalone probe/prototype under `examples/` (e.g.
  [`examples/probe.rs`](../examples/probe.rs), `examples/kr_probe.rs`).
No shared abstraction is written yet. `main` is untouched.

**Phase 2 — Find the common shape.** With 2–3 devices understood, read the
intersection of their real capabilities and *then* write `Transport` +
`Controller` + `Capabilities` from that evidence. This is where the pieces are
"brought together".

**Phase 3 — Refactor behind the traits.** Move the current BIGBIG WON code into
`padtune-bigbigwon` implementing `Controller`, with **no behavior change** (the
HW-verified tests are the safety net). Make the GUI/CLI consume the traits. Only
then land the second backend (`padtune-kr`) as a feature.

## Per-device state & open questions

* **Rainbow 2 Pro / BIGBIG WON** — fully working today; becomes the reference
  `Controller` implementation. No action.
* **EasySMX X10** — protocol fully reversed (Kr over BLE; see
  [`docs/hw-notes/x10.md`](hw-notes/x10.md)). Needs a BLE transport + Kr command
  layer. Cheap experiment pending: does the firmware also answer Kr frames over
  USB iface0 interrupt (`examples/kr_probe.rs`)? If yes, no BLE dependency is
  needed for the X10. Separate, non-blocking issue: XInput-wired won't engage on
  Linux (firmware/host handshake).
* **Machenike G5 Pro V2** — **family unknown.** Shares the `2104` XInput PID with
  BIGBIG WON but exposes no `0xFF7A` HID config channel in wired/XInput mode
  (only a keyboard HID iface + the XInput vendor iface). Must determine whether
  it is A5/USB (→ reuse `padtune-bigbigwon`), Kr/BLE (→ reuse `padtune-kr`), or a
  third family. Next step: BLE scan for a `QMacro`-like device and check for a
  config app; try the other input modes (DInput) over USB.

## References

* [`docs/PROTOCOL.md`](PROTOCOL.md) — BIGBIG WON `A5/A4` protocol.
* [`docs/hw-notes/x10.md`](hw-notes/x10.md) — EasySMX X10 / Kr protocol.
* [`examples/probe.rs`](../examples/probe.rs) — read-only USB-HID identity probe
  (any vendor).
