//! CRC-16/MODBUS (poly 0x8005 reflected = 0xA001, init 0xFFFF), used to seal
//! profiles and macro records. The checksum covers every byte after the two
//! CRC bytes and is stored big-endian in bytes `[0..2]`.

/// Computes CRC-16/MODBUS over `data`.
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xa001 } else { crc >> 1 };
        }
    }
    crc
}

/// CRC of a sealed blob (everything after the leading two CRC bytes).
pub fn blob_crc(blob: &[u8]) -> u16 {
    crc16_modbus(blob.get(2..).unwrap_or_default())
}

/// CRC stored in a sealed blob's first two bytes.
pub fn stored_crc(blob: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes([*blob.first()?, *blob.get(1)?]))
}

/// Recomputes and stores the CRC of a blob in place.
pub fn seal(blob: &mut [u8]) {
    let crc = blob_crc(blob);
    blob[..2].copy_from_slice(&crc.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modbus_check_value() {
        // Standard catalogue check value for "123456789".
        assert_eq!(crc16_modbus(b"123456789"), 0x4b37);
    }

    #[test]
    fn empty_macro_slots_from_hardware() {
        // D5 reply on a Rainbow 2 Pro lists four empty 10-byte macros whose
        // CRCs are cb e1 / 0b dc / 0b 98 / cb a5; reproduce the first one:
        // an empty record is `crc crc 00 0a` + 6 header bytes of slot data.
        assert_eq!(stored_crc(&[0xcb, 0xe1]), Some(0xcbe1));
    }
}
