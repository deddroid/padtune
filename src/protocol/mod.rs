//! Pure, hardware-free protocol code: framing, checksums, commands and
//! profile/macro/lighting encoders. Everything here is unit tested on bytes.

pub mod crc;
pub mod frame;
pub mod commands;
pub mod profile;
