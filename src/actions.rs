//! High-level operations shared by the CLI and the GUI.

use crate::config::{DeviceConfig, to_hex};
use crate::device::Device;
use crate::protocol::commands::{self, MacroSlotInfo, Mode};
use crate::protocol::profile::Profile;
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
    let mode = dev
        .request(&commands::get_mode())
        .and_then(|p| commands::decode_mode(&p))
        .ok();
    let zkm_version = dev
        .request(&commands::get_zkm_version())
        .and_then(|p| commands::decode_zkm_version(&p))
        .ok();
    let uuid = dev
        .request(&commands::get_uuid())
        .and_then(|p| commands::decode_uuid(&p))
        .ok();
    let profile_size = dev
        .request(&commands::get_profile_size())
        .and_then(|p| commands::decode_profile_size(&p))
        .ok();
    let polling_code = dev
        .request(&commands::get_polling())
        .ok()
        .and_then(|p| p.get(1).copied());
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
    pub macros: Vec<crate::protocol::macros::Macro>,
    /// Last strip effect applied by PadTune (the controller cannot report it).
    pub strip: Option<crate::protocol::strip::Strip>,
    /// Raw `GetWirelessFlag` (FA) reply byte.
    pub wireless_flag: Option<u8>,
    pub connection: Option<Link>,
}

/// How the controller reaches the PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Usb,
    Dongle,
}

/// Maps the `FA` flag to a link type. Verified on hardware (2026-10-20):
/// `01` over the USB cable, `00` over the 2.4G dongle. VID/PID and strings are
/// identical on both links, so this is the only reliable way to tell them apart.
pub fn link_from_flag(flag: u8) -> Option<Link> {
    match flag {
        0x01 => Some(Link::Usb),
        0x00 => Some(Link::Dongle),
        _ => None,
    }
}

impl Snapshot {
    /// The parts of this snapshot that can be written back.
    pub fn config(&self) -> DeviceConfig {
        DeviceConfig {
            profile: self.profile.as_ref().map(|p| to_hex(&p.to_bytes())),
            polling_code: self.info.polling_code,
            led_brightness: self.led_brightness,
            logo_colors: self.logo_colors,
            macros: self
                .macros
                .iter()
                .filter_map(|m| m.encode().ok().map(|r| (m.slot, to_hex(&r))))
                .collect(),
            strip: self.strip.clone(),
        }
    }
}

/// Reads everything readable.
pub fn read_snapshot(dev: &Device) -> Result<Snapshot> {
    let info = read_info(dev)?;
    let profile = read_profile(dev).ok();
    let led_brightness = dev
        .request(&commands::get_led_brightness())
        .ok()
        .and_then(|p| p.first().copied());
    let logo_colors = dev
        .request(&commands::get_logo_led())
        .and_then(|p| commands::decode_logo_led(&p))
        .ok();
    let power = read_power(dev).ok();
    let macros = (0..crate::protocol::macros::SLOTS as u8)
        .filter_map(|s| read_macro(dev, s).ok())
        .collect();
    let wireless_flag = dev
        .request(&commands::get_wireless_flag())
        .ok()
        .and_then(|p| p.first().copied());
    let connection = wireless_flag.and_then(link_from_flag);
    let strip = info.uuid.as_ref().and_then(|u| {
        crate::config::State::load()
            .devices
            .get(u)
            .and_then(|c| c.strip.clone())
    });
    Ok(Snapshot {
        strip,
        info,
        profile,
        led_brightness,
        logo_colors,
        power,
        macros,
        wireless_flag,
        connection,
    })
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
        Err(Error::Protocol(format!(
            "{what}: read-back does not match (got {readback:02x?})"
        )))
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
    if let Some(colors) = delta.logo_colors {
        set_and_verify(
            dev,
            &commands::set_logo_led(colors),
            &commands::get_logo_led(),
            |p| commands::decode_logo_led(p).is_ok_and(|c| c == colors),
            "logo colours",
        )?;
    }
    for m in delta.macro_records()? {
        write_macro(dev, &m)?;
    }
    // The strip effect (70) also overwrites the strip brightness (minimum
    // 50, verified): remember the current one to put it back afterwards.
    let mut brightness = delta.led_brightness;
    if let Some(strip) = &delta.strip {
        if brightness.is_none() {
            brightness = dev
                .request(&commands::get_led_brightness())?
                .first()
                .copied();
        }
        write_strip(dev, strip)?;
    }
    if let Some(profile) = delta.profile() {
        write_profile(dev, &profile?)?;
    }
    // Last: the strip effect (70) also sets the strip brightness (min 50),
    // so the general brightness (72) must come after it to stick.
    if let Some(v) = brightness {
        set_and_verify(
            dev,
            &commands::set_led_brightness(v),
            &commands::get_led_brightness(),
            |p| p.first() == Some(&v),
            "LED brightness",
        )?;
    }
    Ok(())
}

/// Writes a profile (D7), checks the ACK and verifies by reading it back.
/// A lost ACK is tolerated when the read-back already matches; otherwise the
/// write is retried once.
pub fn write_profile(dev: &Device, profile: &Profile) -> Result<()> {
    let want = profile.to_bytes();
    let mut last = Error::Protocol("profile write failed".into());
    for _ in 0..2 {
        match dev.send_blob(&profile.write_frames()) {
            Ok(0) | Err(Error::Timeout(_)) => {}
            Ok(status) => {
                return Err(Error::Protocol(format!(
                    "profile write refused (status 0x{status:02x})"
                )));
            }
            Err(e) => return Err(e),
        }
        match read_profile(dev) {
            Ok(back) if back.to_bytes() == want => return Ok(()),
            Ok(_) => {
                last = Error::Protocol("profile read-back differs from what was written".into())
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Reads one macro slot (D9). Short records come back in one `A5` frame,
/// longer ones as `A4` fragments.
pub fn read_macro(dev: &Device, slot: u8) -> Result<crate::protocol::macros::Macro> {
    use crate::protocol::macros::Macro;
    let req = commands::get_macro(slot);
    let record = match dev.request(&req) {
        Ok(p) => p,
        Err(Error::Protocol(m)) if m.contains("long") => dev.request_blob(&req, |b| {
            b.len() >= 4 && b.len() >= usize::from(u16::from_be_bytes([b[2], b[3]]))
        })?,
        Err(e) => return Err(e),
    };
    Macro::decode(&record)
}

/// Stores a macro (D8), checks the ACK and verifies it by reading it back.
pub fn write_macro(dev: &Device, m: &crate::protocol::macros::Macro) -> Result<()> {
    let status = dev.send_blob(&m.write_frames()?)?;
    if status != 0 {
        return Err(Error::Protocol(format!(
            "macro write refused (status 0x{status:02x})"
        )));
    }
    let back = read_macro(dev, m.slot)?;
    if back.encode()? != m.encode()? {
        return Err(Error::Protocol(
            "macro read-back differs from what was written".into(),
        ));
    }
    Ok(())
}

/// Sends the custom light-strip effect (left and right) and checks both ACKs.
pub fn write_strip(dev: &Device, strip: &crate::protocol::strip::Strip) -> Result<()> {
    for frame in strip.frames()? {
        let ack = dev.request(&frame)?;
        if ack.first() != Some(&0) {
            return Err(Error::Protocol(format!(
                "light strip write refused ({ack:02x?})"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wireless_flag_mapping_from_hardware() {
        assert_eq!(link_from_flag(1), Some(Link::Usb));
        assert_eq!(link_from_flag(0), Some(Link::Dongle));
        assert_eq!(link_from_flag(7), None);
    }
}
