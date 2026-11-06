//! CRC-16/MODBUS (poly 0x8005 reflected = 0xA001, init 0xFFFF), used to seal
//! profiles and macro records, plus the helpers shared by every sealed blob:
//! the checksum covers every byte after the two CRC bytes and is stored
//! big-endian in bytes `[0..2]`; bytes `[2..4]` hold the blob length (BE).

use crate::{Error, Result};

/// Computes CRC-16/MODBUS over `data`.
///
/// Plain bitwise form on purpose: a 256-entry table was measured in-process
/// against it (280-byte profile) and came out within noise (~3 %), so the
/// simpler code stays.
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Length a sealed blob declares in bytes `[2..4]` (big-endian).
pub fn declared_len(blob: &[u8]) -> Option<usize> {
    Some(usize::from(u16::from_be_bytes([
        *blob.get(2)?,
        *blob.get(3)?,
    ])))
}

/// Whether `bytes` (possibly still being reassembled from fragments) already
/// holds the whole blob it declares.
pub fn is_complete(bytes: &[u8]) -> bool {
    declared_len(bytes).is_some_and(|n| bytes.len() >= n)
}

/// Checks that a blob has a header and is exactly as long as it declares;
/// returns the declared length. `what` names the blob in error messages.
pub fn check_length(blob: &[u8], what: &str) -> Result<usize> {
    let declared =
        declared_len(blob).ok_or_else(|| Error::Protocol(format!("{what} too short")))?;
    if declared != blob.len() {
        return Err(Error::Protocol(format!(
            "{what} declares {declared} bytes but has {}",
            blob.len()
        )));
    }
    Ok(declared)
}

/// Checks a blob's stored CRC against the one computed over its body.
pub fn check_crc(blob: &[u8]) -> Result<()> {
    let stored = stored_crc(blob).unwrap_or_default();
    let computed = blob_crc(blob);
    if stored != computed {
        return Err(Error::Crc { stored, computed });
    }
    Ok(())
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
        assert_eq!(crc16_modbus(b""), 0xffff);
    }

    #[test]
    fn sealed_blob_helpers() {
        let mut blob = vec![0u8, 0, 0, 6, 0xaa, 0xbb];
        seal(&mut blob);
        assert_eq!(declared_len(&blob), Some(6));
        assert!(is_complete(&blob));
        assert!(!is_complete(&blob[..5]));
        assert!(!is_complete(&[0, 0, 0]));
        assert_eq!(check_length(&blob, "blob").unwrap(), 6);
        assert!(check_length(&blob[..5], "blob").is_err());
        assert!(check_crc(&blob).is_ok());
        blob[4] ^= 1;
        assert!(matches!(check_crc(&blob), Err(Error::Crc { .. })));
    }

    #[test]
    fn empty_macro_slots_from_hardware() {
        // D5 reply on a Rainbow 2 Pro lists four empty 10-byte macros whose
        // CRCs are cb e1 / 0b dc / 0b 98 / cb a5; reproduce the first one:
        // an empty record is `crc crc 00 0a` + 6 header bytes of slot data.
        assert_eq!(stored_crc(&[0xcb, 0xe1]), Some(0xcbe1));
    }
}
