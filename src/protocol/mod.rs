//! Pure, hardware-free protocol code: framing, checksums, commands and
//! profile/macro/lighting encoders. Everything here is unit tested on bytes.

pub mod commands;
pub mod crc;
pub mod frame;
pub mod keys;
pub mod macros;
pub mod profile;
