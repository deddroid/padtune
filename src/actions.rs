//! High-level operations shared by the CLI and the GUI.

use crate::device::Device;
use crate::protocol::commands::{self, MacroSlotInfo, Mode};
use crate::protocol::profile::Profile;
use crate::Result;

/// Everything the controller reports about itself.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: String,
    pub vid: u16,
    pub pid: u16,
    pub product: String,
    pub mode: Option<Mode>,
    pub zkm_version: Option<u8>,
    pub uuid: Option<String>,
    pub profile_size: Option<usize>,
    pub polling_code: Option<u8>,
}

/// Polling-rate code → Hz, as used by the official assistant.
pub fn polling_hz(code: u8) -> Option<u32> {
    match code {
        0 => Some(1000),
        1 => Some(500),
        2 => Some(250),
        3 => Some(2000),
        _ => None,
    }
}

/// Reads identity and status (read-only commands only).
pub fn read_info(dev: &Device) -> Result<DeviceInfo> {
    let mode = dev.request(&commands::get_mode()).and_then(|p| commands::decode_mode(&p)).ok();
    let zkm_version = dev
        .request(&commands::get_zkm_version())
        .and_then(|p| commands::decode_zkm_version(&p))
        .ok();
    let uuid = dev.request(&commands::get_uuid()).and_then(|p| commands::decode_uuid(&p)).ok();
    let profile_size = dev
        .request(&commands::get_profile_size())
        .and_then(|p| commands::decode_profile_size(&p))
        .ok();
    let polling_code = dev.request(&commands::get_polling()).ok().and_then(|p| p.get(1).copied());
    Ok(DeviceInfo {
        path: dev.info.path.clone(),
        vid: dev.info.vendor_id,
        pid: dev.info.product_id,
        product: dev.info.product.clone(),
        mode,
        zkm_version,
        uuid,
        profile_size,
        polling_code,
    })
}

/// Reads the base profile.
pub fn read_profile(dev: &Device) -> Result<Profile> {
    let blob = dev.request_blob(&commands::get_profile(), |b| {
        b.len() >= 4 && b.len() >= usize::from(u16::from_be_bytes([b[2], b[3]]))
    })?;
    let declared = usize::from(u16::from_be_bytes([blob[2], blob[3]]));
    Profile::from_bytes(&blob[..declared])
}

/// Reads the macro slot table.
pub fn read_macro_list(dev: &Device) -> Result<Vec<MacroSlotInfo>> {
    commands::decode_macro_list(&dev.request(&commands::get_macro_list())?)
}
