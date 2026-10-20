//! Base profile ("pad set") stored on the controller.
//!
//! The profile is a CRC-sealed blob whose bytes `[2..4]` declare its own
//! length (big-endian). The official assistant knows several layouts, named
//! after length and firmware generation (`240_v34`, `280_v36`, `484_v37`,
//! `508_v39`, …). They share the same field order; the differences that
//! matter here are the size of the six stick-curve blocks and the offsets
//! that follow them.
//!
//! The struct is kept as raw bytes so fields we do not understand survive a
//! read-modify-write untouched.

use serde::{Deserialize, Serialize};

use super::commands::op;
use super::crc;
use super::frame::{self, REPORT_LEN};
use crate::{Error, Result};

/// Byte layout of one profile generation.
#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    pub name: &'static str,
    pub len: usize,
    /// Six stick-curve blocks start here.
    pub curves: usize,
    /// Distance between two curve blocks.
    pub curve_stride: usize,
    /// Whether the curve block ends with a stabilisation byte.
    pub curve_has_stabilization: bool,
    /// Motion (gyro) parameter blocks: offset, count, stride.
    pub motion: (usize, usize, usize),
    pub turbo_mask: usize,
    pub turbo_speed: usize,
    pub vibration: usize,
    pub keymap: usize,
}

/// `280_v36` — Rainbow 2 Pro (`ZJ-C1pro`), verified on hardware.
pub const V36_280: Layout = Layout {
    name: "280_v36",
    len: 280,
    curves: 0x0e,
    curve_stride: 10,
    curve_has_stabilization: false,
    motion: (0x57, 3, 8),
    turbo_mask: 0x74,
    turbo_speed: 0x78,
    vibration: 0x7c,
    keymap: 0x98,
};

/// `484_v37` — Blitz2 (from BlitzForge, not verified here).
pub const V37_484: Layout = Layout {
    name: "484_v37",
    len: 484,
    curves: 0x0e,
    curve_stride: 44,
    curve_has_stabilization: true,
    motion: (0x123, 3, 8),
    turbo_mask: 0x140,
    turbo_speed: 0x144,
    vibration: 0x148,
    keymap: 0x164,
};

pub const LAYOUTS: &[&Layout] = &[&V36_280, &V37_484];

/// Number of key-mapping slots.
pub const KEY_SLOTS: usize = 32;
/// Number of stick-curve blocks (left/right × three curve sets).
pub const CURVE_BLOCKS: usize = 6;

/// Trigger dead-zone settings (percent, 0..=100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trigger {
    /// Travel below which the trigger reads as released.
    pub start: u8,
    /// Travel above which the trigger reads as fully pressed.
    pub end: u8,
    /// Hair-trigger mode (digital at the first touch).
    pub hair: bool,
}

/// One stick response curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Curve {
    /// Centre dead zone (percent).
    pub deadzone: u8,
    /// Anti-dead-zone / centre compensation (percent).
    pub center_comp: u8,
    pub point_a: (u8, u8),
    pub point_b: (u8, u8),
    /// Outer dead zone end (percent of travel that reaches full output).
    pub outer: u8,
    /// Maximum output (percent).
    pub max_out: u8,
    /// Stabilisation filter (only on layouts that have it).
    pub stabilization: Option<i8>,
}

/// One key-mapping slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyMap {
    /// Factory behaviour (the button does what it says).
    Default,
    /// Disabled.
    Disabled,
    /// Acts as another controller button (target slot index).
    Button { target: u8 },
    /// Sends a keyboard combination (HID usages) on the keyboard interface.
    Keyboard { modifiers: u8, key1: u8, key2: u8 },
    /// Unknown encoding, preserved verbatim.
    Raw { bytes: [u8; 4] },
}

impl KeyMap {
    pub fn decode(b: [u8; 4]) -> Self {
        match b {
            [0, 0, 0, 0] => KeyMap::Default,
            [1, 0xff, ..] => KeyMap::Disabled,
            [1, t, 0, 0] if usize::from(t) < KEY_SLOTS => KeyMap::Button { target: t },
            [2, m, k1, k2] if k1 != 0 => KeyMap::Keyboard {
                modifiers: m,
                key1: k1,
                key2: k2,
            },
            bytes => KeyMap::Raw { bytes },
        }
    }

    pub fn encode(self) -> [u8; 4] {
        match self {
            KeyMap::Default => [0; 4],
            KeyMap::Disabled => [1, 0xff, 0, 0],
            KeyMap::Button { target } => [1, target, 0, 0],
            KeyMap::Keyboard {
                modifiers,
                key1,
                key2,
            } => [2, modifiers, key1, key2],
            KeyMap::Raw { bytes } => bytes,
        }
    }
}

/// Number of vibration motors in the profile.
pub const VIBRATION_MOTORS: usize = 4;

/// Output range of one vibration motor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vibration {
    pub min: u8,
    pub max: u8,
}

impl Vibration {
    /// Factory value on the Rainbow 2 Pro.
    pub const FACTORY: Vibration = Vibration {
        min: 0x50,
        max: 0xff,
    };
    /// Motor disabled (as the official app writes "off").
    pub const OFF: Vibration = Vibration { min: 0, max: 1 };
}

/// Rapid-fire speed setting.
pub const TURBO_SPEEDS_HZ: [u8; 3] = [5, 10, 20];

/// A profile read from or destined to the controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    raw: Vec<u8>,
    layout: &'static Layout,
}

impl Profile {
    /// Parses a profile blob and checks its declared length and CRC.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(Error::Protocol("profile too short".into()));
        }
        let declared = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
        if declared != bytes.len() {
            return Err(Error::Protocol(format!(
                "profile declares {declared} bytes but has {}",
                bytes.len()
            )));
        }
        let layout = LAYOUTS
            .iter()
            .copied()
            .find(|l| l.len == declared)
            .ok_or_else(|| {
                Error::Protocol(format!("unsupported profile layout ({declared} bytes)"))
            })?;
        let stored = crc::stored_crc(bytes).unwrap_or_default();
        let computed = crc::blob_crc(bytes);
        if stored != computed {
            return Err(Error::Crc { stored, computed });
        }
        Ok(Self {
            raw: bytes.to_vec(),
            layout,
        })
    }

    pub fn layout(&self) -> &'static Layout {
        self.layout
    }

    /// Sealed bytes (CRC recomputed).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.raw.clone();
        crc::seal(&mut out);
        out
    }

    /// Long frames (opcode D7) that write this profile.
    pub fn write_frames(&self) -> Vec<[u8; REPORT_LEN]> {
        frame::long_frames(op::SET_PROFILE, &self.to_bytes())
    }

    /// Raw byte access for diagnostics.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    // ---- triggers --------------------------------------------------------

    pub fn trigger(&self, right: bool) -> Trigger {
        let base = if right { 7 } else { 5 };
        Trigger {
            start: self.raw[base],
            end: 100u8.saturating_sub(self.raw[base + 1]),
            hair: self.raw[4] & (1 << u8::from(right)) != 0,
        }
    }

    pub fn set_trigger(&mut self, right: bool, t: Trigger) -> Result<()> {
        if t.end > 100 || t.start >= t.end {
            return Err(Error::Invalid(format!(
                "trigger range {}..{} is invalid (0 <= start < end <= 100)",
                t.start, t.end
            )));
        }
        let base = if right { 7 } else { 5 };
        self.raw[base] = t.start;
        self.raw[base + 1] = 100 - t.end;
        let bit = 1 << u8::from(right);
        if t.hair {
            self.raw[4] |= bit;
        } else {
            self.raw[4] &= !bit;
        }
        Ok(())
    }

    // ---- stick curves ----------------------------------------------------

    fn curve_offset(&self, index: usize) -> usize {
        self.layout.curves + index * self.layout.curve_stride
    }

    /// Curve block `index` (0..6): even = left stick, odd = right stick.
    pub fn curve(&self, index: usize) -> Curve {
        let o = self.curve_offset(index);
        let r = &self.raw;
        Curve {
            deadzone: r[o + 2],
            center_comp: r[o + 3],
            point_a: (r[o + 4], r[o + 5]),
            point_b: (r[o + 6], r[o + 7]),
            outer: r[o + 8],
            max_out: r[o + 9],
            stabilization: self.layout.curve_has_stabilization.then(|| r[o + 10] as i8),
        }
    }

    pub fn set_curve(&mut self, index: usize, c: Curve) -> Result<()> {
        let pct = [
            c.deadzone,
            c.center_comp,
            c.point_a.0,
            c.point_a.1,
            c.point_b.0,
            c.point_b.1,
            c.outer,
            c.max_out,
        ];
        if pct.iter().any(|v| *v > 100) {
            return Err(Error::Invalid("curve values must be 0..=100".into()));
        }
        if c.point_a.0 > c.point_b.0 || c.point_a.1 > c.point_b.1 {
            return Err(Error::Invalid(
                "curve point A must not exceed point B".into(),
            ));
        }
        if c.deadzone >= c.outer {
            return Err(Error::Invalid(
                "centre dead zone must be below the outer limit".into(),
            ));
        }
        let o = self.curve_offset(index);
        self.raw[o] = 0x01;
        self.raw[o + 1] = 0x20;
        self.raw[o + 2..o + 10].copy_from_slice(&pct);
        if self.layout.curve_has_stabilization
            && let Some(s) = c.stabilization
        {
            self.raw[o + 10] = s as u8;
        }
        Ok(())
    }

    // ---- motion ----------------------------------------------------------

    /// Raw motion parameter blocks (`h2v_ratio, gyro_sens, gyro_sens, resp_curve, …`).
    pub fn motion_block(&self, index: usize) -> [u8; 8] {
        let (o, _, stride) = self.layout.motion;
        let s = o + index * stride;
        self.raw[s..s + 8].try_into().expect("8 bytes")
    }

    // ---- rapid fire ------------------------------------------------------

    pub fn turbo_mask(&self) -> u32 {
        let o = self.layout.turbo_mask;
        u32::from_be_bytes(self.raw[o..o + 4].try_into().expect("4 bytes"))
    }

    pub fn set_turbo_mask(&mut self, mask: u32) {
        let o = self.layout.turbo_mask;
        self.raw[o..o + 4].copy_from_slice(&mask.to_be_bytes());
    }

    /// Rapid-fire speed index into [`TURBO_SPEEDS_HZ`].
    pub fn turbo_speed(&self) -> u8 {
        self.raw[self.layout.turbo_speed]
    }

    pub fn set_turbo_speed(&mut self, index: u8) -> Result<()> {
        if usize::from(index) >= TURBO_SPEEDS_HZ.len() {
            return Err(Error::Invalid(format!(
                "turbo speed index {index} out of range"
            )));
        }
        self.raw[self.layout.turbo_speed] = index;
        Ok(())
    }

    // ---- vibration -------------------------------------------------------

    /// Raw vibration bytes (four minimums, then four maximums).
    pub fn vibration_raw(&self) -> [u8; 8] {
        let o = self.layout.vibration;
        self.raw[o..o + 8].try_into().expect("8 bytes")
    }

    /// Output range of a vibration motor: the game's rumble strength is
    /// scaled between `min` and `max` (0..=255). Motors: 0 = left grip,
    /// 1 = right grip, 2/3 = triggers (order of 2/3 to be confirmed).
    /// BlitzForge (v37) documents motors 0 and 1 with the same layout.
    pub fn vibration(&self, motor: usize) -> Vibration {
        let o = self.layout.vibration;
        Vibration {
            min: self.raw[o + motor],
            max: self.raw[o + 4 + motor],
        }
    }

    pub fn set_vibration(&mut self, motor: usize, v: Vibration) -> Result<()> {
        if motor >= VIBRATION_MOTORS {
            return Err(Error::Invalid(format!(
                "vibration motor {motor} out of range"
            )));
        }
        if v.max < v.min {
            return Err(Error::Invalid("vibration max must not be below min".into()));
        }
        let o = self.layout.vibration;
        self.raw[o + motor] = v.min;
        self.raw[o + 4 + motor] = v.max;
        Ok(())
    }

    // ---- key mapping -----------------------------------------------------

    pub fn key(&self, slot: usize) -> KeyMap {
        let o = self.layout.keymap + slot * 4;
        KeyMap::decode(self.raw[o..o + 4].try_into().expect("4 bytes"))
    }

    pub fn set_key(&mut self, slot: usize, map: KeyMap) -> Result<()> {
        if slot >= KEY_SLOTS {
            return Err(Error::Invalid(format!("key slot {slot} out of range")));
        }
        let o = self.layout.keymap + slot * 4;
        self.raw[o..o + 4].copy_from_slice(&map.encode());
        Ok(())
    }

    /// Byte ranges that differ from `other` (same layout only).
    pub fn diff(&self, other: &Profile) -> Vec<(usize, u8, u8)> {
        self.raw
            .iter()
            .zip(&other.raw)
            .enumerate()
            .skip(2) // CRC
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect()
    }
}

/// Names of the 32 key slots. The same bit numbers are used by the profile
/// key map, the turbo mask and the live key report (`02`), verified on a
/// Rainbow 2 Pro by pressing every button. `None` = not present.
pub const SLOT_NAMES: [Option<&str>; KEY_SLOTS] = [
    Some("A"),
    Some("B"),
    None,
    Some("X"),
    Some("Y"),
    None,
    Some("LB"),
    Some("RB"),
    Some("LT"),
    Some("RT"),
    Some("View"),
    Some("Menu"),
    Some("Home"),
    Some("L3"),
    Some("R3"),
    Some("Share"),
    Some("Up"),
    Some("Down"),
    Some("Left"),
    Some("Right"),
    None,
    None,
    Some("Rec"),
    Some("M1"),
    Some("M2"),
    Some("M3"),
    Some("M4"),
    Some("Turbo"),
    Some("Profile"),
    Some("Map"),
    Some("FN"),
    None,
];

/// Slots of the front function keys (they trigger on-board features and are
/// not remapped by the profile).
pub const FUNCTION_SLOTS: [usize; 5] = [27, 29, 22, 28, 30];

/// Factory profile read from a Rainbow 2 Pro (ZJ-C1pro) on 2026-10-20.
pub const RAINBOW2PRO_FACTORY: [u8; 280] = {
    let mut p = [0u8; 280];
    let head: &[u8] = &[
        0x87, 0x8d, 0x01, 0x18, 0x00, 0x05, 0x0a, 0x05, 0x0a, 0x00, 0x00, 0x00, 0x66, 0x00, 0x01,
        0x20, 0x0c, 0x00, 0x1e, 0x1e, 0x46, 0x46, 0x5f, 0x64, 0x01, 0x20, 0x0c, 0x00, 0x1e, 0x1e,
        0x46, 0x46, 0x5f, 0x64, 0x01, 0x20, 0x0c, 0x00, 0x1e, 0x1e, 0x46, 0x46, 0x5f, 0x64, 0x01,
        0x20, 0x0c, 0x00, 0x1e, 0x1e, 0x46, 0x46, 0x5f, 0x64, 0x01, 0x20, 0x0c, 0x00, 0x1e, 0x1e,
        0x46, 0x46, 0x5f, 0x64, 0x01, 0x20, 0x0c, 0x00, 0x1e, 0x1e, 0x46, 0x46, 0x5f, 0x64, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x3c, 0x3c,
        0x2a, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x3c, 0x3c, 0x2a, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x3c,
        0x3c, 0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x50, 0x50, 0x50, 0x50, 0xff, 0xff, 0xff, 0xff, 0x33,
    ];
    let mut i = 0;
    while i < head.len() {
        p[i] = head[i];
        i += 1;
    }
    p
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rainbow2pro_factory_profile() {
        let p = Profile::from_bytes(&RAINBOW2PRO_FACTORY).unwrap();
        assert_eq!(p.layout().name, "280_v36");
        assert_eq!(
            p.trigger(false),
            Trigger {
                start: 5,
                end: 90,
                hair: false
            }
        );
        assert_eq!(
            p.trigger(true),
            Trigger {
                start: 5,
                end: 90,
                hair: false
            }
        );
        let c = p.curve(0);
        assert_eq!(c.deadzone, 12);
        assert_eq!(c.point_a, (30, 30));
        assert_eq!(c.point_b, (70, 70));
        assert_eq!((c.outer, c.max_out), (95, 100));
        assert_eq!(c.stabilization, None);
        assert_eq!(p.motion_block(0)[..4], [10, 60, 60, 42]);
        assert_eq!(p.turbo_mask(), 0);
        assert!((0..KEY_SLOTS).all(|s| p.key(s) == KeyMap::Default));
        // Re-sealing an unmodified profile reproduces it byte for byte.
        assert_eq!(p.to_bytes(), RAINBOW2PRO_FACTORY);
    }

    #[test]
    fn rejects_corrupted_profile() {
        let mut bad = RAINBOW2PRO_FACTORY;
        bad[100] ^= 1;
        assert!(matches!(Profile::from_bytes(&bad), Err(Error::Crc { .. })));
    }

    #[test]
    fn edits_are_sealed_and_framed() {
        let mut p = Profile::from_bytes(&RAINBOW2PRO_FACTORY).unwrap();
        p.set_trigger(
            false,
            Trigger {
                start: 2,
                end: 98,
                hair: true,
            },
        )
        .unwrap();
        p.set_key(0, KeyMap::Button { target: 1 }).unwrap();
        p.set_turbo_mask(1 << 3);
        let bytes = p.to_bytes();
        assert_eq!(bytes[4..7], [0x01, 2, 2]);
        assert_eq!(bytes[0x98..0x9c], [1, 1, 0, 0]);
        assert_eq!(bytes[0x74..0x78], [0, 0, 0, 8]);
        let reparsed = Profile::from_bytes(&bytes).unwrap();
        assert_eq!(
            reparsed
                .diff(&Profile::from_bytes(&RAINBOW2PRO_FACTORY).unwrap())
                .len(),
            6
        );
        let frames = p.write_frames();
        assert_eq!(frames.len(), 5);
        assert_eq!(frames[0][..4], [0xa4, 0x40, 0xd7, 0x01]);
    }

    #[test]
    fn vibration_fields() {
        let mut p = Profile::from_bytes(&RAINBOW2PRO_FACTORY).unwrap();
        for m in 0..VIBRATION_MOTORS {
            assert_eq!(p.vibration(m), Vibration::FACTORY);
        }
        p.set_vibration(1, Vibration { min: 10, max: 120 }).unwrap();
        assert_eq!(p.to_bytes()[0x7d], 10);
        assert_eq!(p.to_bytes()[0x81], 120);
        assert!(p.set_vibration(4, Vibration::OFF).is_err());
        assert!(p.set_vibration(0, Vibration { min: 9, max: 8 }).is_err());
    }

    #[test]
    fn keymap_roundtrip() {
        for b in [
            [0, 0, 0, 0],
            [1, 0xff, 0, 0],
            [1, 3, 0, 0],
            [2, 0x02, 0x04, 0],
            [9, 9, 9, 9],
        ] {
            assert_eq!(KeyMap::decode(b).encode(), b);
        }
    }

    #[test]
    fn invalid_values_are_refused() {
        let mut p = Profile::from_bytes(&RAINBOW2PRO_FACTORY).unwrap();
        assert!(
            p.set_trigger(
                true,
                Trigger {
                    start: 50,
                    end: 40,
                    hair: false
                }
            )
            .is_err()
        );
        let mut c = p.curve(1);
        c.point_a = (80, 80);
        assert!(p.set_curve(1, c).is_err());
        assert!(p.set_key(32, KeyMap::Default).is_err());
        assert!(p.set_turbo_speed(3).is_err());
    }
}
