//! Short commands and their reply decoders.
//!
//! Opcodes come from the official PC assistant (`DevMgr.dll`) and match the
//! independent implementations credited in `docs/PROTOCOL.md`.

use super::frame::{REPORT_LEN, short};
use crate::{Error, Result};

pub mod op {
    pub const ZKM_VERSION: u8 = 0x0b;
    pub const PROFILE_SIZE: u8 = 0xd3;
    pub const MACRO_LIST: u8 = 0xd5;
    pub const GET_PROFILE: u8 = 0xd6;
    pub const SET_PROFILE: u8 = 0xd7;
    pub const SET_MACRO: u8 = 0xd8;
    pub const GET_MACRO: u8 = 0xd9;
    pub const GET_MODE: u8 = 0xe2;
    pub const UUID: u8 = 0xef;
    pub const POLLING: u8 = 0xf6;
    pub const STEP_ACCURACY: u8 = 0xf7;
    pub const POWER: u8 = 0x04;
    pub const SAVE: u8 = 0x0e;
    pub const LED_BRIGHT: u8 = 0x72;
    pub const LIGHTING_MODE: u8 = 0x73;
    pub const GAMEPAD_MODE: u8 = 0xd4;
    pub const DEVICE_MODE_INFO: u8 = 0xe1;
    pub const LOGO_LED: u8 = 0xf5;
    pub const TRIGGER_LOCK: u8 = 0xf8;
    pub const WIRELESS: u8 = 0xfa;
    pub const KEY_REPORT_SWITCH: u8 = 0xd2;
    pub const KEY_REPORT: u8 = 0x02;
}

pub fn get_zkm_version() -> [u8; REPORT_LEN] {
    short(op::ZKM_VERSION, &[])
}
pub fn get_profile_size() -> [u8; REPORT_LEN] {
    short(op::PROFILE_SIZE, &[])
}
pub fn get_macro_list() -> [u8; REPORT_LEN] {
    short(op::MACRO_LIST, &[])
}
pub fn get_profile() -> [u8; REPORT_LEN] {
    short(op::GET_PROFILE, &[])
}
pub fn get_mode() -> [u8; REPORT_LEN] {
    short(op::GET_MODE, &[])
}
pub fn get_uuid() -> [u8; REPORT_LEN] {
    short(op::UUID, &[0; 8])
}
pub fn get_polling() -> [u8; REPORT_LEN] {
    short(op::POLLING, &[])
}
pub fn get_step_accuracy() -> [u8; REPORT_LEN] {
    short(op::STEP_ACCURACY, &[])
}
pub fn get_macro(slot: u8) -> [u8; REPORT_LEN] {
    short(op::GET_MACRO, &[slot])
}

pub fn get_led_brightness() -> [u8; REPORT_LEN] {
    short(op::LED_BRIGHT, &[])
}
pub fn get_logo_led() -> [u8; REPORT_LEN] {
    short(op::LOGO_LED, &[])
}
pub fn get_power() -> [u8; REPORT_LEN] {
    short(op::POWER, &[])
}
pub fn get_gamepad_mode() -> [u8; REPORT_LEN] {
    short(op::GAMEPAD_MODE, &[])
}
pub fn get_wireless_flag() -> [u8; REPORT_LEN] {
    short(op::WIRELESS, &[])
}
pub fn get_trigger_lock() -> [u8; REPORT_LEN] {
    short(op::TRIGGER_LOCK, &[])
}
pub fn get_device_mode_info() -> [u8; REPORT_LEN] {
    short(op::DEVICE_MODE_INFO, &[])
}

/// Polling-rate codes accepted by `F6` (official assistant table).
pub const POLLING_CODES: [(u8, u32); 4] = [(2, 250), (1, 500), (0, 1000), (3, 2000)];

/// Polling-rate code → Hz.
pub fn polling_hz(code: u8) -> Option<u32> {
    POLLING_CODES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, hz)| *hz)
}

/// Hz → polling-rate code.
pub fn polling_code(hz: u32) -> Option<u8> {
    POLLING_CODES
        .iter()
        .find(|(_, h)| *h == hz)
        .map(|(c, _)| *c)
}

/// `SetRateOfReturn`: `A5 05 F6 code sum`.
pub fn set_polling(code: u8) -> Result<[u8; REPORT_LEN]> {
    polling_hz(code).ok_or_else(|| Error::Invalid(format!("unknown polling code {code}")))?;
    Ok(short(op::POLLING, &[code]))
}

/// `SetLEDBrightRequest`: `A5 05 72 value sum`.
pub fn set_led_brightness(value: u8) -> [u8; REPORT_LEN] {
    short(op::LED_BRIGHT, &[value])
}

/// `SetLogoLED`: three RGB colours (Xbox, Switch, PlayStation modes).
pub fn set_logo_led(colors: [[u8; 3]; 3]) -> [u8; REPORT_LEN] {
    let mut payload = [0u8; 9];
    for (i, c) in colors.iter().enumerate() {
        payload[i * 3..i * 3 + 3].copy_from_slice(c);
    }
    short(op::LOGO_LED, &payload)
}

/// `Save2Device`: commit the working configuration to flash.
pub fn save_to_flash() -> [u8; REPORT_LEN] {
    short(op::SAVE, &[0])
}

/// Battery/power report (`04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Power {
    /// Battery level in percent.
    pub battery: u8,
    /// Raw status byte after the level (meaning not yet confirmed; 0 on USB).
    pub status: u8,
}

pub fn decode_power(payload: &[u8]) -> Result<Power> {
    need(payload, 2, "power")?;
    Ok(Power {
        battery: payload[0].min(100),
        status: payload[1],
    })
}

/// Logo LED colours from `F5`.
pub fn decode_logo_led(payload: &[u8]) -> Result<[[u8; 3]; 3]> {
    need(payload, 9, "logo LED")?;
    Ok(std::array::from_fn(|i| {
        [payload[i * 3], payload[i * 3 + 1], payload[i * 3 + 2]]
    }))
}

/// `SetReceiveGamepadKeyPressedEvent`: when enabled the controller pushes a
/// [`KeyReport`] on the vendor interface for every input change.
pub fn set_key_reports(enabled: bool) -> [u8; REPORT_LEN] {
    short(op::KEY_REPORT_SWITCH, &[u8::from(enabled)])
}

/// Input state pushed by the controller (opcode `02`).
///
/// Two layouts exist: the official assistant's `OnGamepadKey` parses a long
/// form with stick axes, while the Rainbow 2 Pro (`ZJ-C1pro`) sends a compact
/// 5-byte form with buttons only (verified on hardware).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyReport {
    /// Pressed buttons, one bit per key slot (big-endian on the wire); the
    /// bit numbers match the profile key-mapping slots.
    pub keys: u32,
    /// Stick axes `[lx, ly, rx, ry]` when the long form is used.
    pub axes: Option<[i16; 4]>,
}

pub fn decode_key_report(payload: &[u8]) -> Result<KeyReport> {
    need(payload, 4, "key report")?;
    let keys = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let axes = (payload.len() >= 12).then(|| {
        let i = |o: usize| i16::from_be_bytes([payload[o], payload[o + 1]]);
        [i(4), i(6), i(8), i(10)]
    });
    Ok(KeyReport { keys, axes })
}

fn need(payload: &[u8], n: usize, what: &str) -> Result<()> {
    if payload.len() < n {
        return Err(Error::Protocol(format!(
            "{what} reply has {} bytes, expected {n}",
            payload.len()
        )));
    }
    Ok(())
}

/// Identity reported by GetMode (E2): firmware marker such as `ZJ-C1pro`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mode {
    /// Three leading bytes before the marker (meaning partly unknown).
    pub prefix: [u8; 3],
    pub marker: String,
}

pub fn decode_mode(payload: &[u8]) -> Result<Mode> {
    need(payload, 4, "GetMode")?;
    let marker = &payload[3..];
    let end = marker.iter().position(|b| *b == 0).unwrap_or(marker.len());
    Ok(Mode {
        prefix: [payload[0], payload[1], payload[2]],
        marker: String::from_utf8_lossy(&marker[..end]).into_owned(),
    })
}

pub fn decode_zkm_version(payload: &[u8]) -> Result<u8> {
    need(payload, 1, "ZKM version")?;
    Ok(payload[0])
}

pub fn decode_uuid(payload: &[u8]) -> Result<String> {
    need(payload, 8, "UUID")?;
    Ok(payload[..8].iter().map(|b| format!("{b:02X}")).collect())
}

/// Profile size reply (D3). Byte meaning beyond the size is not yet known.
pub fn decode_profile_size(payload: &[u8]) -> Result<usize> {
    need(payload, 4, "profile size")?;
    Ok(usize::from(u16::from_le_bytes([payload[2], payload[3]])))
}

/// One entry of the macro list (D5): CRC and length of each slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacroSlotInfo {
    pub crc: u16,
    pub len: usize,
}

pub fn decode_macro_list(payload: &[u8]) -> Result<Vec<MacroSlotInfo>> {
    if !payload.len().is_multiple_of(4) || payload.is_empty() {
        return Err(Error::Protocol(format!(
            "macro list has {} bytes",
            payload.len()
        )));
    }
    Ok(payload
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| MacroSlotInfo {
            crc: u16::from_be_bytes([c[0], c[1]]),
            len: usize::from(u16::from_be_bytes([c[2], c[3]])),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::frame::{Frame, parse};

    fn payload(report: &[u8]) -> Vec<u8> {
        match parse(report).unwrap() {
            Frame::Short { payload, .. } => payload.to_vec(),
            _ => panic!("long frame"),
        }
    }

    // Real replies from a Rainbow 2 Pro (wired), 2026-10-20.
    #[test]
    fn decodes_rainbow2pro_identity() {
        let e2 = [
            0xa5, 0x10, 0xe2, 0x56, 0x51, 0x01, 0x5a, 0x4a, 0x2d, 0x43, 0x31, 0x70, 0x72, 0x6f,
            0x00, 0xd5,
        ];
        let m = decode_mode(&payload(&e2)).unwrap();
        assert_eq!(m.marker, "ZJ-C1pro");
        assert_eq!(m.prefix, [0x56, 0x51, 0x01]);

        let ef = [
            0xa5, 0x0c, 0xef, 0x22, 0x20, 0xb2, 0x1a, 0xff, 0x02, 0x06, 0x08, 0xbd,
        ];
        assert_eq!(decode_uuid(&payload(&ef)).unwrap(), "2220B21AFF020608");

        let d3 = [0xa5, 0x0a, 0xd3, 0x68, 0x0b, 0x18, 0x01, 0x94, 0x02, 0xa4];
        assert_eq!(decode_profile_size(&payload(&d3)).unwrap(), 280);

        let zkm = [0xa5, 0x05, 0x0b, 0x36, 0xeb];
        assert_eq!(decode_zkm_version(&payload(&zkm)).unwrap(), b'6');
    }

    #[test]
    fn write_commands_match_official_encoders() {
        // DevMgr.dll: SetRateOfReturn checksum = value - 0x60.
        assert_eq!(
            set_polling(1).unwrap()[..5],
            [0xa5, 0x05, 0xf6, 0x01, 0x01u8.wrapping_sub(0x60)]
        );
        assert!(set_polling(9).is_err());
        // SetLEDBrightRequest checksum = value + 0x1c.
        assert_eq!(
            set_led_brightness(0x80)[..5],
            [0xa5, 0x05, 0x72, 0x80, 0x9c]
        );
        // Save2Device is the fixed frame a5 05 0e 00 b8.
        assert_eq!(save_to_flash()[..5], [0xa5, 0x05, 0x0e, 0x00, 0xb8]);
        // SetLogoLED checksum = sum(colours) - 0x59.
        let logo = set_logo_led([[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
        assert_eq!(logo[..3], [0xa5, 0x0d, 0xf5]);
        assert_eq!(logo[12], 45u8.wrapping_sub(0x59));
        assert_eq!(polling_code(2000), Some(3));
    }

    #[test]
    fn decodes_rainbow2pro_status() {
        let pw = [0xa5, 0x06, 0x04, 0x64, 0x00, 0x13];
        assert_eq!(decode_power(&payload(&pw)).unwrap().battery, 100);
        let logo = [0xa5, 0x0d, 0xf5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xa7];
        assert_eq!(decode_logo_led(&payload(&logo)).unwrap(), [[0; 3]; 3]);
    }

    #[test]
    fn decodes_compact_key_reports() {
        // Rainbow 2 Pro: A, LT, M1, M2 and the FN triangle (2026-10-20).
        for (bytes, bit) in [
            ([0, 0, 0, 0x01, 0], 0),
            ([0, 0, 0x01, 0, 0], 8),
            ([0, 0x80, 0, 0, 0], 23),
            ([0x01, 0, 0, 0, 0], 24),
            ([0x40, 0, 0, 0, 0], 30),
        ] {
            let k = decode_key_report(&bytes).unwrap();
            assert_eq!(k.keys, 1 << bit);
            assert_eq!(k.axes, None);
        }
        assert_eq!(set_key_reports(true)[..5], [0xa5, 0x05, 0xd2, 0x01, 0x7d]);
    }

    #[test]
    fn decodes_empty_macro_list() {
        let d5 = [
            0xa5, 0x14, 0xd5, 0xcb, 0xe1, 0x00, 0x0a, 0x0b, 0xdc, 0x00, 0x0a, 0x0b, 0x98, 0x00,
            0x0a, 0xcb, 0xa5, 0x00, 0x0a, 0x5c,
        ];
        let list = decode_macro_list(&payload(&d5)).unwrap();
        assert_eq!(list.len(), 4);
        assert!(list.iter().all(|s| s.len == 10));
        assert_eq!(list[0].crc, 0xcbe1);
    }
}
