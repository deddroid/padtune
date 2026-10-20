//! High-level operations shared by the CLI and the GUI.

use crate::device::Device;
use crate::protocol::commands::{self, MacroSlotInfo, Mode};
use crate::protocol::profile::Profile;
use crate::config::{to_hex, DeviceConfig};
use crate::{Error, Result};

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

pub use crate::protocol::commands::polling_hz;

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

/// Full picture of a controller, as shown by the GUI.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub info: DeviceInfo,
    pub profile: Option<Profile>,
    pub led_brightness: Option<u8>,
    pub logo_colors: Option<[[u8; 3]; 3]>,
    pub power: Option<commands::Power>,
    pub macros: Vec<MacroSlotInfo>,
}

impl Snapshot {
    /// The parts of this snapshot that can be written back.
    pub fn config(&self) -> DeviceConfig {
        DeviceConfig {
            profile: self.profile.as_ref().map(|p| to_hex(&p.to_bytes())),
            polling_code: self.info.polling_code,
            led_brightness: self.led_brightness,
            logo_colors: self.logo_colors,
        }
    }
}

/// Reads everything readable.
pub fn read_snapshot(dev: &Device) -> Result<Snapshot> {
    let info = read_info(dev)?;
    let profile = read_profile(dev).ok();
    let led_brightness = dev.request(&commands::get_led_brightness()).ok().and_then(|p| p.first().copied());
    let logo_colors = dev
        .request(&commands::get_logo_led())
        .and_then(|p| commands::decode_logo_led(&p))
        .ok();
    let power = read_power(dev).ok();
    let macros = read_macro_list(dev).unwrap_or_default();
    Ok(Snapshot { info, profile, led_brightness, logo_colors, power, macros })
}

pub fn read_power(dev: &Device) -> Result<commands::Power> {
    commands::decode_power(&dev.request(&commands::get_power())?)
}

/// Sends a short "set" command, then reads the value back with `get` and
/// checks it with `matches`.
fn set_and_verify(
    dev: &Device,
    set: &[u8; crate::protocol::frame::REPORT_LEN],
    get: &[u8; crate::protocol::frame::REPORT_LEN],
    matches: impl Fn(&[u8]) -> bool,
    what: &str,
) -> Result<()> {
    // The reply to a set is an echo/ACK with the same opcode; tolerate silence.
    let _ = dev.request(set);
    let readback = dev.request(get)?;
    if matches(&readback) {
        Ok(())
    } else {
        Err(Error::Protocol(format!("{what}: read-back does not match (got {readback:02x?})")))
    }
}

/// Applies only the given parts (use [`DeviceConfig::changes_from`] first).
/// Every write is verified by reading it back.
pub fn apply(dev: &Device, delta: &DeviceConfig) -> Result<()> {
    if let Some(code) = delta.polling_code {
        set_and_verify(
            dev,
            &commands::set_polling(code)?,
            &commands::get_polling(),
            |p| p.get(1) == Some(&code),
            "polling rate",
        )?;
    }
    if let Some(v) = delta.led_brightness {
        set_and_verify(
            dev,
            &commands::set_led_brightness(v),
            &commands::get_led_brightness(),
            |p| p.first() == Some(&v),
            "LED brightness",
        )?;
    }
    if let Some(colors) = delta.logo_colors {
        set_and_verify(
            dev,
            &commands::set_logo_led(colors),
            &commands::get_logo_led(),
            |p| commands::decode_logo_led(p).is_ok_and(|c| c == colors),
            "logo colours",
        )?;
    }
    if let Some(profile) = delta.profile() {
        write_profile(dev, &profile?)?;
    }
    Ok(())
}

/// Writes a profile (D7), checks the ACK and verifies by reading it back.
pub fn write_profile(dev: &Device, profile: &Profile) -> Result<()> {
    let status = dev.send_blob(&profile.write_frames())?;
    if status != 0 {
        return Err(Error::Protocol(format!("profile write refused (status 0x{status:02x})")));
    }
    let back = read_profile(dev)?;
    if back.to_bytes() != profile.to_bytes() {
        return Err(Error::Protocol("profile read-back differs from what was written".into()));
    }
    Ok(())
}
