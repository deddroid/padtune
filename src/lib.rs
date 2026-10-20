//! `bigbigctl` — configure BIGBIG WON controllers (Rainbow 2 Pro and the
//! `413d:2104` family) on Linux and Windows.
//!
//! All packet building and parsing lives in [`protocol`] and is testable
//! without hardware; [`device`] is the thin HID transport on top.

pub mod actions;
pub mod device;
pub mod protocol;

/// Errors produced by the library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("checksum mismatch: expected 0x{expected:02X}, found 0x{found:02X}")]
    Checksum { expected: u8, found: u8 },
    #[error("CRC mismatch: stored 0x{stored:04X}, computed 0x{computed:04X}")]
    Crc { stored: u16, computed: u16 },
    #[error("device timed out waiting for 0x{0:02X}")]
    Timeout(u8),
    #[error("no BIGBIG WON controller found")]
    NotFound,
    #[error("HID error: {0}")]
    Hid(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
