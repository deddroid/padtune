//! Short commands and their reply decoders.
//!
//! Opcodes come from the official PC assistant (`DevMgr.dll`) and match the
//! independent implementations credited in `docs/PROTOCOL.md`.

use super::frame::{short, REPORT_LEN};
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
    if payload.len() % 4 != 0 || payload.is_empty() {
        return Err(Error::Protocol(format!("macro list has {} bytes", payload.len())));
    }
    Ok(payload
        .chunks_exact(4)
        .map(|c| MacroSlotInfo {
            crc: u16::from_be_bytes([c[0], c[1]]),
            len: usize::from(u16::from_be_bytes([c[2], c[3]])),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::frame::{parse, Frame};

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

        let ef = [0xa5, 0x0c, 0xef, 0x22, 0x20, 0xb2, 0x1a, 0xff, 0x02, 0x06, 0x08, 0xbd];
        assert_eq!(decode_uuid(&payload(&ef)).unwrap(), "2220B21AFF020608");

        let d3 = [0xa5, 0x0a, 0xd3, 0x68, 0x0b, 0x18, 0x01, 0x94, 0x02, 0xa4];
        assert_eq!(decode_profile_size(&payload(&d3)).unwrap(), 280);

        let zkm = [0xa5, 0x05, 0x0b, 0x36, 0xeb];
        assert_eq!(decode_zkm_version(&payload(&zkm)).unwrap(), b'6');
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
