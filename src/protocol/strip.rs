//! Custom colour/effect of the RGB light strip (opcode `70`).
//!
//! Decoded from the official Android app (`writeLightConfig`) and the PC
//! Tool (`SetLED`), verified on a Rainbow 2 Pro over USB: one short frame per
//! side, ACK `a5 05 70 00 1a`, persists without Save2Device. The strip shows
//! this configuration while it is in "custom light" mode (double-click FN on
//! the controller). There is no command to read it back.
//!
//! ```text
//! A5 len 70 | 00 00 00 zone mode 00 speed bright FF | R G B × 1..=7 | sum
//! zone: 1 = left, 2 = right    mode: 1 static, 2 breathing, 3 gradient, 5 flicker
//! speed byte = (255 - speed) / 10 + 10   (smaller = faster)
//! ```

use serde::{Deserialize, Serialize};

use super::frame::{REPORT_LEN, short};
use crate::{Error, Result};

pub const OP: u8 = 0x70;
pub const MAX_COLORS: usize = 7;
/// Lowest brightness the official app allows.
pub const MIN_BRIGHT: u8 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Static,
    Breathing,
    #[default]
    Gradient,
    Flicker,
}

impl Effect {
    pub const ALL: [Effect; 4] = [
        Effect::Static,
        Effect::Breathing,
        Effect::Gradient,
        Effect::Flicker,
    ];

    pub fn code(self) -> u8 {
        match self {
            Effect::Static => 1,
            Effect::Breathing => 2,
            Effect::Gradient => 3,
            Effect::Flicker => 5,
        }
    }
}

/// Strip configuration as the official app models it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Strip {
    pub effect: Effect,
    /// 0 (slowest) ..= 255 (fastest), the app's slider scale.
    pub speed: u8,
    /// 50 ..= 255.
    pub bright: u8,
    /// Use the left colours on both sides.
    pub same: bool,
    pub left: Vec<[u8; 3]>,
    pub right: Vec<[u8; 3]>,
}

impl Default for Strip {
    /// Close to the factory look: red → yellow → green gradient, full speed.
    fn default() -> Self {
        let colors = vec![[255, 0, 0], [255, 255, 0], [0, 255, 0]];
        Strip {
            effect: Effect::Gradient,
            speed: 255,
            bright: 255,
            same: true,
            left: colors.clone(),
            right: colors,
        }
    }
}

impl Strip {
    /// One solid colour on the whole strip.
    pub fn solid(color: [u8; 3], bright: u8) -> Self {
        Strip {
            effect: Effect::Static,
            speed: 255,
            bright,
            same: true,
            left: vec![color],
            right: vec![color],
        }
    }

    fn side(&self, right: bool) -> &[[u8; 3]] {
        if right && !self.same {
            &self.right
        } else {
            &self.left
        }
    }

    /// The two frames (left, right) that set this configuration.
    pub fn frames(&self) -> Result<[[u8; REPORT_LEN]; 2]> {
        Ok([frame(self, false)?, frame(self, true)?])
    }
}

/// Speed byte sent on the wire for the app's 0..=255 slider value.
pub fn speed_byte(speed: u8) -> u8 {
    (255 - speed) / 10 + 10
}

fn frame(s: &Strip, right: bool) -> Result<[u8; REPORT_LEN]> {
    let colors = s.side(right);
    if colors.is_empty() || colors.len() > MAX_COLORS {
        return Err(Error::Invalid(format!(
            "a strip side needs 1..={MAX_COLORS} colours"
        )));
    }
    let mut payload = vec![
        0,
        0,
        0,
        if right { 2 } else { 1 },
        s.effect.code(),
        0,
        speed_byte(s.speed),
        s.bright.max(MIN_BRIGHT),
        0xff,
    ];
    payload.extend(colors.iter().flatten());
    Ok(short(OP, &payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(f: &[u8; REPORT_LEN]) -> &[u8] {
        &f[..usize::from(f[1])]
    }

    #[test]
    fn frames_match_hardware_verified_examples() {
        // Static blue, both sides (verified: whole strip blue).
        let [l, r] = Strip::solid([0, 0, 255], 255).frames().unwrap();
        assert_eq!(
            wire(&l),
            [
                0xa5, 0x10, 0x70, 0, 0, 0, 1, 1, 0, 0x0a, 0xff, 0xff, 0, 0, 0xff, 0x2e
            ]
        );
        assert_eq!(
            wire(&r),
            [
                0xa5, 0x10, 0x70, 0, 0, 0, 2, 1, 0, 0x0a, 0xff, 0xff, 0, 0, 0xff, 0x2f
            ]
        );
        // Gradient red → yellow → green (verified: factory-like look).
        let [l, r] = Strip::default().frames().unwrap();
        assert_eq!(
            wire(&l),
            [
                0xa5, 0x16, 0x70, 0, 0, 0, 1, 3, 0, 0x0a, 0xff, 0xff, 0xff, 0, 0, 0xff, 0xff, 0, 0,
                0xff, 0, 0x33
            ]
        );
        assert_eq!(*wire(&r).last().unwrap(), 0x34);
    }

    #[test]
    fn speed_and_limits() {
        assert_eq!(speed_byte(255), 0x0a);
        assert_eq!(speed_byte(128), 0x16);
        assert_eq!(speed_byte(0), 0x23);
        let mut s = Strip::solid([1, 2, 3], 10);
        assert_eq!(s.frames().unwrap()[0][10], MIN_BRIGHT);
        s.left = vec![[0; 3]; 8];
        assert!(s.frames().is_err());
        s.left = vec![[0; 3]; 7];
        assert_eq!(s.frames().unwrap()[0][1], 34);
    }

    #[test]
    fn separate_sides() {
        let s = Strip {
            same: false,
            right: vec![[9, 9, 9]],
            ..Strip::default()
        };
        let [_, r] = s.frames().unwrap();
        assert_eq!(r[12..15], [9, 9, 9]);
    }
}
