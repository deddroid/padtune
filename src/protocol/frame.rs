//! Wire framing shared by every BIGBIG WON controller on the `413d:2104` family.
//!
//! Two packet families travel over the 64-byte vendor HID interface:
//!
//! * **short** frames: `A5 len cmd payload… sum` — requests, replies and ACKs;
//! * **long** frames: `A4 len op seq payload… sum` — fragments of a larger blob
//!   (profiles, macros, lighting), `seq` counting from 1.
//!
//! In both, `len` is the frame length including the header and the trailing
//! checksum byte, and the checksum is the 8-bit sum of every previous byte.

use crate::Error;

/// Size of one HID report on the configuration interface (no report ID).
pub const REPORT_LEN: usize = 64;
/// Magic byte of a short frame.
pub const SHORT: u8 = 0xa5;
/// Magic byte of a long (fragmented) frame.
pub const LONG: u8 = 0xa4;
/// Maximum payload bytes in one long frame (`64 - 4 header - 1 checksum`).
pub const LONG_PAYLOAD_MAX: usize = REPORT_LEN - 5;

/// 8-bit additive checksum used by every frame.
pub fn sum8(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

/// Builds a short frame `A5 len cmd payload… sum`, zero-padded to [`REPORT_LEN`].
pub fn short(cmd: u8, payload: &[u8]) -> [u8; REPORT_LEN] {
    let len = payload.len() + 4;
    assert!(len <= REPORT_LEN, "short frame payload too long");
    let mut out = [0u8; REPORT_LEN];
    out[0] = SHORT;
    out[1] = len as u8;
    out[2] = cmd;
    out[3..3 + payload.len()].copy_from_slice(payload);
    out[len - 1] = sum8(&out[..len - 1]);
    out
}

/// Builds the long frames carrying `data` for opcode `op`.
pub fn long_frames(op: u8, data: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    data.chunks(LONG_PAYLOAD_MAX)
        .enumerate()
        .map(|(i, chunk)| {
            let len = chunk.len() + 5;
            let mut out = [0u8; REPORT_LEN];
            out[0] = LONG;
            out[1] = len as u8;
            out[2] = op;
            out[3] = (i + 1) as u8;
            out[4..4 + chunk.len()].copy_from_slice(chunk);
            out[len - 1] = sum8(&out[..len - 1]);
            out
        })
        .collect()
}

/// A validated incoming frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame<'a> {
    /// `A5` frame: command and payload (without checksum).
    Short { cmd: u8, payload: &'a [u8] },
    /// `A4` frame: opcode, sequence number and payload (without checksum).
    Long { op: u8, seq: u8, payload: &'a [u8] },
}

impl Frame<'_> {
    /// Command/opcode byte of the frame.
    pub fn op(&self) -> u8 {
        match self {
            Frame::Short { cmd, .. } => *cmd,
            Frame::Long { op, .. } => *op,
        }
    }
}

/// Parses and validates one report read from the device.
///
/// A leading `0x00` report-ID byte (as some HID backends return) is skipped.
pub fn parse(report: &[u8]) -> Result<Frame<'_>, Error> {
    let wire = match report.first() {
        Some(0) => &report[1..],
        _ => report,
    };
    if wire.len() < 4 {
        return Err(Error::Protocol(format!("frame too short ({} bytes)", wire.len())));
    }
    let len = wire[1] as usize;
    let min = if wire[0] == LONG { 5 } else { 4 };
    if len < min || len > wire.len() {
        return Err(Error::Protocol(format!("invalid frame length {len}")));
    }
    let expected = sum8(&wire[..len - 1]);
    if expected != wire[len - 1] {
        return Err(Error::Checksum {
            expected,
            found: wire[len - 1],
        });
    }
    match wire[0] {
        SHORT => Ok(Frame::Short {
            cmd: wire[2],
            payload: &wire[3..len - 1],
        }),
        LONG => Ok(Frame::Long {
            op: wire[2],
            seq: wire[3],
            payload: &wire[4..len - 1],
        }),
        other => Err(Error::Protocol(format!("unknown frame magic 0x{other:02X}"))),
    }
}

/// Reassembles long-frame fragments (in any arrival order) into one blob.
///
/// Fragments must be numbered 1..=n without gaps or duplicates.
pub fn reassemble(op: u8, frames: &[Frame<'_>]) -> Result<Vec<u8>, Error> {
    let mut parts: Vec<(u8, &[u8])> = Vec::with_capacity(frames.len());
    for f in frames {
        match f {
            Frame::Long { op: o, seq, payload } if *o == op => parts.push((*seq, payload)),
            other => {
                return Err(Error::Protocol(format!(
                    "unexpected frame 0x{:02X} while reassembling 0x{op:02X}",
                    other.op()
                )));
            }
        }
    }
    parts.sort_by_key(|(seq, _)| *seq);
    let mut out = Vec::new();
    for (i, (seq, payload)) in parts.iter().enumerate() {
        if usize::from(*seq) != i + 1 {
            return Err(Error::Protocol(format!(
                "missing or duplicated fragment: got {seq}, expected {}",
                i + 1
            )));
        }
        out.extend_from_slice(payload);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_requests_match_official_bytes() {
        assert_eq!(short(0x0b, &[])[..4], [0xa5, 0x04, 0x0b, 0xb4]);
        assert_eq!(short(0xd6, &[])[..4], [0xa5, 0x04, 0xd6, 0x7f]);
        assert_eq!(short(0xd3, &[])[..4], [0xa5, 0x04, 0xd3, 0x7c]);
        assert_eq!(short(0xf6, &[])[..4], [0xa5, 0x04, 0xf6, 0x9f]);
        let uuid = short(0xef, &[0; 8]);
        assert_eq!(uuid[..12], [0xa5, 0x0c, 0xef, 0, 0, 0, 0, 0, 0, 0, 0, 0xa0]);
        assert!(uuid[12..].iter().all(|b| *b == 0));
    }

    #[test]
    fn parses_real_rainbow2pro_replies() {
        // Captured from a Rainbow 2 Pro (ZJ-C1pro) over USB, 2026-10-20.
        let zkm = [0xa5, 0x05, 0x0b, 0x36, 0xeb];
        assert_eq!(
            parse(&zkm).unwrap(),
            Frame::Short { cmd: 0x0b, payload: &[0x36] }
        );
        let size = [0xa5, 0x0a, 0xd3, 0x68, 0x0b, 0x18, 0x01, 0x94, 0x02, 0xa4];
        assert_eq!(parse(&size).unwrap().op(), 0xd3);
        // Leading report ID is tolerated.
        let mut with_id = vec![0u8];
        with_id.extend_from_slice(&zkm);
        assert!(parse(&with_id).is_ok());
    }

    #[test]
    fn rejects_bad_checksum() {
        let bad = [0xa5, 0x05, 0x0b, 0x36, 0xec];
        assert!(matches!(parse(&bad), Err(Error::Checksum { expected: 0xeb, found: 0xec })));
    }

    #[test]
    fn long_frames_roundtrip_and_layout() {
        let data: Vec<u8> = (0..280u16).map(|v| v as u8).collect();
        let frames = long_frames(0xd7, &data);
        // 280 = 4×59 + 44 → five frames, last one 49 bytes long (as seen on hardware).
        assert_eq!(frames.len(), 5);
        assert_eq!(frames[0][..4], [0xa4, 0x40, 0xd7, 0x01]);
        assert_eq!(frames[4][1], 49);
        let parsed: Vec<_> = frames.iter().rev().map(|f| parse(f).unwrap()).collect();
        assert_eq!(reassemble(0xd7, &parsed).unwrap(), data);
    }

    #[test]
    fn reassemble_detects_gaps() {
        let frames = long_frames(0xd6, &[1u8; 130]);
        let parsed = [parse(&frames[0]).unwrap(), parse(&frames[2]).unwrap()];
        assert!(reassemble(0xd6, &parsed).is_err());
    }
}
