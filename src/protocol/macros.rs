//! Macro records stored in the controller (four slots, bound to back keys).
//!
//! Record layout (CRC-16/MODBUS sealed like profiles):
//!
//! ```text
//! [0..2]  CRC (BE)          [2..4] record length (BE) = 10 + 10 × steps
//! [4]     ?                 [5]    slot index 0..=3
//! [6]     run key slot (23 = M1, 24 = M2, …; 0x20 = not bound)
//! [7]     flags: bit0 run after release, bit1 loop
//! [8..10] repeat count (BE)
//! then per step (10 bytes):
//! [0] bits7..4 = duration low nibble, bit0 = marker
//! [1]     duration high byte  (duration = 12 bits × 8 ms)
//! [2..6]  input mask (BE)     [6..10] analog values (signed)
//! ```
//!
//! Header bytes verified on a Rainbow 2 Pro (empty slots); the step layout
//! comes from BlitzForge (Blitz2) and is checked on hardware separately.

use super::commands::op;
use super::crc;
use super::frame::{self, REPORT_LEN};
use crate::{Error, Result};

pub const SLOTS: usize = 4;
pub const HEADER_LEN: usize = 10;
pub const STEP_LEN: usize = 10;
pub const MAX_STEPS: usize = 64;
/// Run-key value meaning "not bound to any key".
pub const UNBOUND: u8 = 0x20;
/// Duration resolution of a step.
pub const MS_PER_UNIT: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Step {
    pub duration_ms: u32,
    pub marker: bool,
    pub inputs: u32,
    pub analog: [i8; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    pub slot: u8,
    pub run_key: u8,
    pub run_after_release: bool,
    pub looped: bool,
    pub repeat: u16,
    pub steps: Vec<Step>,
    /// Header byte 4 (meaning unknown), preserved.
    pub reserved: u8,
}

/// Index in [`Step::analog`] of the left stick's X axis (Y follows).
/// Verified on hardware: a stick rotation recorded on the controller sits in
/// bytes 8..10 of each step, quantised to eight directions.
pub const LEFT_STICK: usize = 2;
/// Index of the right stick's X axis (bytes 6..8), also verified on hardware
/// with a recording that moved the left stick, then the right, then both.
pub const RIGHT_STICK: usize = 0;

/// Stick position in a macro step: none or one of eight directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickDir {
    Center,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
    /// Any other position (kept as recorded).
    Other(i8, i8),
}

impl StickDir {
    pub const ALL: [StickDir; 9] = [
        StickDir::Center,
        StickDir::Up,
        StickDir::UpRight,
        StickDir::Right,
        StickDir::DownRight,
        StickDir::Down,
        StickDir::DownLeft,
        StickDir::Left,
        StickDir::UpLeft,
    ];

    /// Axis values the controller itself records (full tilt; diagonals 90).
    pub fn axes(self) -> (i8, i8) {
        match self {
            StickDir::Center => (0, 0),
            StickDir::Up => (0, 127),
            StickDir::UpRight => (90, 90),
            StickDir::Right => (127, 0),
            StickDir::DownRight => (90, -91),
            StickDir::Down => (0, -128),
            StickDir::DownLeft => (-91, -91),
            StickDir::Left => (-128, 0),
            StickDir::UpLeft => (-91, 90),
            StickDir::Other(x, y) => (x, y),
        }
    }

    pub fn from_axes(x: i8, y: i8) -> Self {
        StickDir::ALL
            .into_iter()
            .find(|d| d.axes() == (x, y))
            .unwrap_or(StickDir::Other(x, y))
    }

    /// Arrow symbol for the UI.
    pub fn arrow(self) -> &'static str {
        match self {
            StickDir::Center => "·",
            StickDir::Up => "↑",
            StickDir::UpRight => "↗",
            StickDir::Right => "→",
            StickDir::DownRight => "↘",
            StickDir::Down => "↓",
            StickDir::DownLeft => "↙",
            StickDir::Left => "←",
            StickDir::UpLeft => "↖",
            StickDir::Other(..) => "∗",
        }
    }
}

impl Step {
    /// Position of a stick (`LEFT_STICK` or `RIGHT_STICK`).
    pub fn stick(&self, axis: usize) -> StickDir {
        StickDir::from_axes(self.analog[axis], self.analog[axis + 1])
    }

    pub fn set_stick(&mut self, axis: usize, dir: StickDir) {
        let (x, y) = dir.axes();
        self.analog[axis] = x;
        self.analog[axis + 1] = y;
    }
}

impl Macro {
    /// An empty, unbound slot (as found on a factory controller).
    pub fn empty(slot: u8) -> Self {
        Self {
            slot,
            run_key: UNBOUND,
            run_after_release: false,
            looped: false,
            repeat: 0,
            steps: Vec::new(),
            reserved: 0,
        }
    }

    pub fn decode(record: &[u8]) -> Result<Self> {
        if record.len() < HEADER_LEN || !(record.len() - HEADER_LEN).is_multiple_of(STEP_LEN) {
            return Err(Error::Protocol(format!(
                "bad macro record length {}",
                record.len()
            )));
        }
        let declared = usize::from(u16::from_be_bytes([record[2], record[3]]));
        if declared != record.len() {
            return Err(Error::Protocol(format!(
                "macro declares {declared} bytes but has {}",
                record.len()
            )));
        }
        let stored = crc::stored_crc(record).unwrap_or_default();
        let computed = crc::blob_crc(record);
        if stored != computed {
            return Err(Error::Crc { stored, computed });
        }
        let steps = record[HEADER_LEN..]
            .as_chunks::<STEP_LEN>()
            .0
            .iter()
            .map(|s| Step {
                duration_ms: ((u32::from(s[0]) >> 4) | (u32::from(s[1]) << 4)) * MS_PER_UNIT,
                marker: s[0] & 1 != 0,
                inputs: u32::from_be_bytes([s[2], s[3], s[4], s[5]]),
                analog: [s[6] as i8, s[7] as i8, s[8] as i8, s[9] as i8],
            })
            .collect();
        Ok(Self {
            reserved: record[4],
            slot: record[5],
            run_key: record[6],
            run_after_release: record[7] & 1 != 0,
            looped: record[7] & 2 != 0,
            repeat: u16::from_be_bytes([record[8], record[9]]),
            steps,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.steps.len() > MAX_STEPS {
            return Err(Error::Invalid(format!(
                "a macro holds at most {MAX_STEPS} steps"
            )));
        }
        if usize::from(self.slot) >= SLOTS {
            return Err(Error::Invalid(format!(
                "macro slot {} out of range",
                self.slot
            )));
        }
        let len = HEADER_LEN + STEP_LEN * self.steps.len();
        let mut out = vec![0u8; len];
        out[2..4].copy_from_slice(&(len as u16).to_be_bytes());
        out[4] = self.reserved;
        out[5] = self.slot;
        out[6] = self.run_key;
        out[7] = u8::from(self.run_after_release) | (u8::from(self.looped) << 1);
        out[8..10].copy_from_slice(&self.repeat.to_be_bytes());
        for (i, s) in self.steps.iter().enumerate() {
            let units = (s.duration_ms / MS_PER_UNIT).min(0xfff);
            let o = HEADER_LEN + i * STEP_LEN;
            out[o] = ((units & 0x0f) << 4) as u8 | u8::from(s.marker);
            out[o + 1] = (units >> 4) as u8;
            out[o + 2..o + 6].copy_from_slice(&s.inputs.to_be_bytes());
            for (k, a) in s.analog.iter().enumerate() {
                out[o + 6 + k] = *a as u8;
            }
        }
        crc::seal(&mut out);
        Ok(out)
    }

    /// Long frames (opcode D8) that store this macro: payload = slot + record.
    pub fn write_frames(&self) -> Result<Vec<[u8; REPORT_LEN]>> {
        let mut data = vec![self.slot];
        data.extend_from_slice(&self.encode()?);
        Ok(frame::long_frames(op::SET_MACRO, &data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::frame::{Frame, parse};

    /// D9 replies of a factory Rainbow 2 Pro (slots 0..3), 2026-10-20.
    const EMPTY: [[u8; 14]; 4] = [
        [
            0xa5, 0x0e, 0xd9, 0xcb, 0xe1, 0x00, 0x0a, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x62,
        ],
        [
            0xa5, 0x0e, 0xd9, 0x0b, 0xdc, 0x00, 0x0a, 0x00, 0x01, 0x20, 0x00, 0x00, 0x00, 0x9e,
        ],
        [
            0xa5, 0x0e, 0xd9, 0x0b, 0x98, 0x00, 0x0a, 0x00, 0x02, 0x20, 0x00, 0x00, 0x00, 0x5b,
        ],
        [
            0xa5, 0x0e, 0xd9, 0xcb, 0xa5, 0x00, 0x0a, 0x00, 0x03, 0x20, 0x00, 0x00, 0x00, 0x29,
        ],
    ];

    fn payload(r: &[u8]) -> Vec<u8> {
        match parse(r).unwrap() {
            Frame::Short { payload, .. } => payload.to_vec(),
            _ => panic!(),
        }
    }

    #[test]
    fn decodes_and_reencodes_factory_slots() {
        for (slot, reply) in EMPTY.iter().enumerate() {
            let rec = payload(reply);
            let m = Macro::decode(&rec).unwrap();
            assert_eq!(m, Macro::empty(slot as u8));
            assert_eq!(m.encode().unwrap(), rec, "slot {slot}");
        }
    }

    #[test]
    fn steps_roundtrip() {
        let mut m = Macro::empty(1);
        m.run_key = 24; // M2
        m.repeat = 3;
        m.steps = vec![
            Step {
                duration_ms: 96,
                marker: false,
                inputs: 0x0000_0002,
                analog: [0, 0, 0, 0],
            },
            Step {
                duration_ms: 32_760,
                marker: true,
                inputs: 0,
                analog: [-128, 127, 0, -1],
            },
        ];
        let rec = m.encode().unwrap();
        assert_eq!(rec.len(), 30);
        assert_eq!(Macro::decode(&rec).unwrap(), m);
        let frames = m.write_frames().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0][..5], [0xa4, 36, 0xd8, 0x01, 0x01]);
    }

    #[test]
    fn stick_directions_match_controller_recording() {
        // Values from a left-stick rotation recorded on a Rainbow 2 Pro.
        for (x, y, d) in [
            (0, 127, StickDir::Up),
            (-91, 90, StickDir::UpLeft),
            (-128, 0, StickDir::Left),
            (-91, -91, StickDir::DownLeft),
            (0, -128, StickDir::Down),
            (90, -91, StickDir::DownRight),
            (127, 0, StickDir::Right),
            (90, 90, StickDir::UpRight),
        ] {
            assert_eq!(StickDir::from_axes(x, y), d);
            assert_eq!(d.axes(), (x, y));
        }
        let mut s = Step::default();
        s.set_stick(LEFT_STICK, StickDir::UpLeft);
        assert_eq!(s.analog, [0, 0, -91, 90]);
        assert_eq!(s.stick(RIGHT_STICK), StickDir::Center);
        assert_eq!(StickDir::from_axes(5, 5), StickDir::Other(5, 5));
    }

    #[test]
    fn rejects_bad_records() {
        let mut rec = payload(&EMPTY[0]);
        rec[6] = 0x17;
        assert!(matches!(Macro::decode(&rec), Err(Error::Crc { .. })));
        assert!(Macro::decode(&rec[..9]).is_err());
    }
}
