//! On-controller calibration (`1B`), decoded from the official PC Tool
//! 1.0.7.7 and verified on a Rainbow 2 Pro (ZKM 6, USB cable). Details and
//! raw logs: `docs/hw-notes/calibration.md`.
//!
//! Sticks and triggers: send [`start`], let the user rotate both sticks along
//! the edge and pull both triggers fully, then send [`finish`]. Meanwhile the
//! controller pushes `02` reports: bytes 0..4 are the usual key bits, byte 4
//! holds the sticks' edge flags (see [`Progress`]). The gyroscope run needs only [`start`]: it ends by itself after
//! about 5 s, and [`finish`] cancels it.

use super::frame::{REPORT_LEN, short};
use crate::{Error, Result};

pub const OP: u8 = 0x1b;

/// Laps per stick and full presses per trigger asked by the official tool.
pub const GOAL: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Both sticks and both triggers together.
    Sticks,
    Gyro,
}

impl Target {
    fn code(self) -> u8 {
        match self {
            Target::Sticks => 0,
            Target::Gyro => 1,
        }
    }

    /// Step of the last reply of a run: `06` after the sticks' end,
    /// `04` when the gyroscope finishes by itself.
    fn last_step(self) -> u8 {
        match self {
            Target::Sticks => 6,
            Target::Gyro => 4,
        }
    }
}

pub fn start(target: Target) -> [u8; REPORT_LEN] {
    short(OP, &[target.code(), 1])
}

/// Ends the sticks run (the controller then stores it) or cancels the
/// gyroscope run.
pub fn finish(target: Target) -> [u8; REPORT_LEN] {
    short(OP, &[target.code(), 0])
}

/// A `1B` reply: `[target, step, 00]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub target: Target,
    pub step: u8,
}

impl Step {
    /// The run is over: nothing more will come for it.
    pub fn is_last(self) -> bool {
        self.step >= self.target.last_step()
    }
}

pub fn decode_step(payload: &[u8]) -> Result<Step> {
    let target = match payload.first() {
        Some(0) => Target::Sticks,
        Some(1) => Target::Gyro,
        other => {
            return Err(Error::Protocol(format!(
                "calibration reply for unknown target {other:02x?}"
            )));
        }
    };
    let step = *payload
        .get(1)
        .ok_or_else(|| Error::Protocol("calibration reply too short".into()))?;
    Ok(Step { target, step })
}

/// Edge flags of one stick, clockwise from the top: up, right, down, left.
/// Two neighbours together are the diagonal between them. The low nibble is
/// the left stick, the high nibble the right one; both use the same bits
/// (verified one direction at a time on a Rainbow 2 Pro). Sector `0` is up
/// and the sectors go clockwise.
const EDGE_BITS: [u8; 4] = [0x1, 0x8, 0x2, 0x4];
/// Key bits of LT and RT in the first four bytes (key slots 8 and 9, as in
/// the key report). Slots 13 and 14 next to them are L3 and R3.
const TRIGGER_KEYS: [u32; 2] = [1 << 8, 1 << 9];

/// Edge sector (0..8, circular) of a stick from its 4 flag bits; `None`
/// while the stick is away from the edge.
fn sector(nibble: u8, order: [u8; 4]) -> Option<u8> {
    (0..4u8).find_map(|i| {
        let here = order[usize::from(i)];
        let next = order[usize::from((i + 1) % 4)];
        if nibble == here {
            Some(2 * i)
        } else if nibble == here | next {
            Some(2 * i + 1)
        } else {
            None
        }
    })
}

/// Where the sticks and triggers are, from a `02` report pushed during the
/// sticks run. Index 0 = left, 1 = right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    pub sectors: [Option<u8>; 2],
    pub triggers: [bool; 2],
}

pub fn decode_progress(payload: &[u8]) -> Result<Progress> {
    let [k0, k1, k2, k3, edges, ..] = *payload else {
        return Err(Error::Protocol("calibration progress too short".into()));
    };
    let keys = u32::from_be_bytes([k0, k1, k2, k3]);
    Ok(Progress {
        sectors: [
            sector(edges & 0x0f, EDGE_BITS),
            sector(edges >> 4, EDGE_BITS),
        ],
        triggers: TRIGGER_KEYS.map(|b| keys & b != 0),
    })
}

/// Counts laps along the edge and full trigger presses over a sticks run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tracker {
    /// Sectors reached in the current lap, one bit each.
    pub seen: [u8; 2],
    pub laps: [u8; 2],
    pub presses: [u8; 2],
    held: [bool; 2],
}

impl Tracker {
    pub fn update(&mut self, p: Progress) {
        for i in 0..2 {
            if let Some(s) = p.sectors[i] {
                self.seen[i] |= 1 << s;
                if self.seen[i] == 0xff {
                    self.laps[i] = self.laps[i].saturating_add(1);
                    self.seen[i] = 0;
                }
            }
            if p.triggers[i] && !self.held[i] {
                self.presses[i] = self.presses[i].saturating_add(1);
            }
            self.held[i] = p.triggers[i];
        }
    }

    /// Every lap and press the official tool asks for has been seen.
    pub fn complete(&self) -> bool {
        self.laps.iter().chain(&self.presses).all(|&n| n >= GOAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(f: &[u8; REPORT_LEN]) -> &[u8] {
        &f[..usize::from(f[1])]
    }

    #[test]
    fn frames_match_the_ones_verified_on_hardware() {
        assert_eq!(
            wire(&start(Target::Sticks)),
            [0xa5, 0x06, 0x1b, 0x00, 0x01, 0xc7]
        );
        assert_eq!(
            wire(&finish(Target::Sticks)),
            [0xa5, 0x06, 0x1b, 0x00, 0x00, 0xc6]
        );
        assert_eq!(
            wire(&start(Target::Gyro)),
            [0xa5, 0x06, 0x1b, 0x01, 0x01, 0xc8]
        );
        assert_eq!(
            wire(&finish(Target::Gyro)),
            [0xa5, 0x06, 0x1b, 0x01, 0x00, 0xc7]
        );
    }

    // Replies logged on 2026-11-08.
    #[test]
    fn runs_end_on_the_logged_last_step() {
        let last = |p: [u8; 3]| decode_step(&p).unwrap().is_last();
        assert!(!last([0, 3, 0]));
        assert!(!last([0, 4, 0]));
        assert!(last([0, 6, 0]));
        assert!(!last([1, 2, 0]));
        assert!(last([1, 4, 0]));
        assert!(decode_step(&[7, 1, 0]).is_err());
    }

    #[test]
    fn logged_clockwise_rotations_go_through_the_sectors_in_order() {
        // Clockwise from the top, logged on 2026-11-08 (right, then left).
        let high = [0x10, 0x90, 0x80, 0xa0, 0x20, 0x60, 0x40, 0x50];
        let low = [0x01, 0x09, 0x08, 0x0a, 0x02, 0x06, 0x04, 0x05];
        for (i, (h, l)) in high.into_iter().zip(low).enumerate() {
            let p = decode_progress(&[0, 0, 0, 0, h | l]).unwrap();
            assert_eq!(p.sectors, [Some(i as u8), Some(i as u8)]);
        }
        assert_eq!(
            decode_progress(&[0, 0, 0, 0, 0]).unwrap().sectors,
            [None, None]
        );
    }

    #[test]
    fn tracker_counts_laps_and_presses() {
        let mut t = Tracker::default();
        let rotation = [0x20, 0xa0, 0x80, 0x90, 0x10, 0x50, 0x40, 0x60];
        for _ in 0..GOAL {
            for e in rotation {
                t.update(decode_progress(&[0, 0, 0, 0, e]).unwrap());
            }
        }
        assert_eq!(t.laps, [0, GOAL]);
        assert_eq!(t.seen[1], 0);
        for _ in 0..GOAL {
            t.update(decode_progress(&[0, 0, 0x03, 0, 0]).unwrap());
            t.update(decode_progress(&[0, 0, 0x00, 0, 0]).unwrap());
        }
        assert_eq!(t.presses, [GOAL, GOAL]);
        assert!(!t.complete(), "the left stick has no laps");
    }

    // The left stick alone moves the low nibble, the right stick the high one.
    #[test]
    fn stick_clicks_are_not_triggers() {
        // L3 + R3 (slots 13, 14): byte 2 = 0x60.
        let p = decode_progress(&[0, 0, 0x60, 0, 0]).unwrap();
        assert_eq!(p.triggers, [false, false]);
        let lt = decode_progress(&[0, 0, 0x01, 0, 0]).unwrap();
        assert_eq!(lt.triggers, [true, false]);
    }

    #[test]
    fn single_directions_from_the_hardware_log() {
        // Up, right, down, left, one at a time: right stick, then left.
        for (sector, (right, left)) in [
            (0, (0x10, 0x01)),
            (2, (0x80, 0x08)),
            (4, (0x20, 0x02)),
            (6, (0x40, 0x04)),
        ] {
            let r = decode_progress(&[0, 0, 0, 0, right]).unwrap();
            assert_eq!(r.sectors, [None, Some(sector)]);
            let l = decode_progress(&[0, 0, 0, 0, left]).unwrap();
            assert_eq!(l.sectors, [Some(sector), None]);
        }
    }
}
