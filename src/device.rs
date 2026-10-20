//! HID transport: discovery and request/response on the vendor interface.
//!
//! Set `BIGBIGCTL_DEBUG=1` to print every report sent and received.

use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::protocol::frame::{self, Frame, REPORT_LEN};
use crate::{Error, Result};

pub const VENDOR_ID: u16 = 0x413d;
/// Wired controller and its 2.4 GHz dongle both enumerate with this PID.
pub const PRODUCT_IDS: &[u16] = &[0x2104];
pub const CONFIG_USAGE_PAGE: u16 = 0xff7a;
/// The 2.4G dongle re-enumerates as `413d:2106` ("Zikway" / "HID zk") while
/// its controller is not linked (asleep, or switched to the USB cable).
pub const IDLE_DONGLE_PID: u16 = 0x2106;
const CONFIG_INTERFACE: i32 = 2;

const READ_TIMEOUT: Duration = Duration::from_millis(1000);
const RETRIES: usize = 3;

fn debug_enabled() -> bool {
    std::env::var_os("BIGBIGCTL_DEBUG").is_some_and(|v| v != "0")
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Trims a report to its declared frame length for logging.
fn frame_slice(report: &[u8]) -> &[u8] {
    let len = report.get(1).map_or(report.len(), |l| usize::from(*l));
    &report[..len.clamp(1, report.len())]
}

/// A controller found during enumeration.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub product: String,
    pub manufacturer: String,
    pub interface: i32,
}

fn is_config(info: &DeviceInfo) -> bool {
    info.vendor_id() == VENDOR_ID
        && PRODUCT_IDS.contains(&info.product_id())
        && (info.usage_page() == CONFIG_USAGE_PAGE
            // Some backends do not report usages; fall back to the interface number.
            || (info.usage_page() == 0 && info.interface_number() == CONFIG_INTERFACE))
}

/// Lists every connected configuration interface.
pub fn list() -> Result<Vec<Candidate>> {
    let api = HidApi::new().map_err(|e| Error::Hid(e.to_string()))?;
    let mut out: Vec<Candidate> = api
        .device_list()
        .filter(|i| is_config(i))
        .map(|i| Candidate {
            path: i.path().to_string_lossy().into_owned(),
            vendor_id: i.vendor_id(),
            product_id: i.product_id(),
            product: i.product_string().unwrap_or_default().to_string(),
            manufacturer: i.manufacturer_string().unwrap_or_default().to_string(),
            interface: i.interface_number(),
        })
        .collect();
    out.dedup_by(|a, b| a.path == b.path);
    Ok(out)
}

/// Whether a 2.4G dongle without a linked controller is plugged in.
pub fn idle_dongle_present() -> bool {
    HidApi::new().is_ok_and(|api| {
        api.device_list()
            .any(|i| i.vendor_id() == VENDOR_ID && i.product_id() == IDLE_DONGLE_PID)
    })
}

/// An open configuration channel.
pub struct Device {
    hid: HidDevice,
    pub info: Candidate,
    debug: bool,
}

impl Device {
    /// Opens the first controller found, or the one at `path`.
    pub fn open(path: Option<&str>) -> Result<Self> {
        let api = HidApi::new().map_err(|e| Error::Hid(e.to_string()))?;
        let info = api
            .device_list()
            .filter(|i| is_config(i))
            .find(|i| path.is_none_or(|p| i.path().to_string_lossy() == p))
            .ok_or(Error::NotFound)?;
        let hid = info
            .open_device(&api)
            .map_err(|e| Error::Hid(e.to_string()))?;
        let info = Candidate {
            path: info.path().to_string_lossy().into_owned(),
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            product: info.product_string().unwrap_or_default().to_string(),
            manufacturer: info.manufacturer_string().unwrap_or_default().to_string(),
            interface: info.interface_number(),
        };
        Ok(Self {
            hid,
            info,
            debug: debug_enabled(),
        })
    }

    /// Writes one 64-byte report (prefixed with report ID 0).
    pub fn send(&self, report: &[u8; REPORT_LEN]) -> Result<()> {
        if self.debug {
            eprintln!(">> {}", hex(frame_slice(report)));
        }
        let mut buf = [0u8; REPORT_LEN + 1];
        buf[1..].copy_from_slice(report);
        let n = self
            .hid
            .write(&buf)
            .map_err(|e| Error::Hid(e.to_string()))?;
        if n < REPORT_LEN {
            return Err(Error::Hid(format!("short write ({n} bytes)")));
        }
        Ok(())
    }

    /// Reads one report, or `None` on timeout.
    pub fn recv(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; REPORT_LEN + 1];
        let n = self
            .hid
            .read_timeout(&mut buf, timeout.as_millis() as i32)
            .map_err(|e| Error::Hid(e.to_string()))?;
        if n == 0 {
            return Ok(None);
        }
        let report = buf[..n].to_vec();
        if self.debug {
            eprintln!("<< {}", hex(frame_slice(&report)));
        }
        Ok(Some(report))
    }

    /// Discards pending input (stale replies from a previous exchange).
    pub fn drain(&self) -> Result<()> {
        while self.recv(Duration::from_millis(5))?.is_some() {}
        Ok(())
    }

    /// Collects valid frames whose opcode is `op` until `done` says the
    /// sequence is complete or the timeout expires.
    fn collect(
        &self,
        op: u8,
        timeout: Duration,
        mut done: impl FnMut(&[Vec<u8>]) -> bool,
    ) -> Result<Vec<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        let mut got = Vec::new();
        while Instant::now() < deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(report) = self.recv(left)? else {
                break;
            };
            match frame::parse(&report) {
                Ok(f) if f.op() == op => {
                    got.push(report);
                    if done(&got) {
                        return Ok(got);
                    }
                }
                // Unrelated or corrupt frames are skipped; the caller retries.
                Ok(_) => {}
                Err(e) => {
                    if self.debug {
                        eprintln!("!! {e}");
                    }
                }
            }
        }
        if got.is_empty() {
            Err(Error::Timeout(op))
        } else {
            Ok(got)
        }
    }

    /// Sends a short request and returns the payload of the matching reply.
    pub fn request(&self, report: &[u8; REPORT_LEN]) -> Result<Vec<u8>> {
        let op = report[2];
        let mut last = Error::Timeout(op);
        for _ in 0..RETRIES {
            self.drain()?;
            self.send(report)?;
            match self.collect(op, READ_TIMEOUT, |_| true) {
                Ok(frames) => match frame::parse(&frames[0])? {
                    Frame::Short { payload, .. } => return Ok(payload.to_vec()),
                    Frame::Long { .. } => last = Error::Protocol("unexpected long reply".into()),
                },
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Sends a short request answered by long fragments; `complete` decides
    /// from the reassembled bytes so far whether the blob is finished.
    pub fn request_blob(
        &self,
        report: &[u8; REPORT_LEN],
        complete: impl Fn(&[u8]) -> bool,
    ) -> Result<Vec<u8>> {
        let op = report[2];
        let mut last = Error::Timeout(op);
        for _ in 0..RETRIES {
            self.drain()?;
            self.send(report)?;
            let assembled = |reports: &[Vec<u8>]| -> Result<Vec<u8>> {
                let frames: Vec<Frame> = reports
                    .iter()
                    .map(|r| frame::parse(r))
                    .collect::<Result<_>>()?;
                frame::reassemble(op, &frames)
            };
            let res = self.collect(op, Duration::from_secs(3), |got| {
                assembled(got).is_ok_and(|b| complete(&b))
            });
            match res.and_then(|r| assembled(&r)) {
                Ok(blob) if complete(&blob) => return Ok(blob),
                Ok(blob) => {
                    last = Error::Protocol(format!("incomplete reply ({} bytes)", blob.len()))
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Sends long frames and waits for the short ACK `A5 05 op status sum`.
    /// Returns the status byte (0 = success on every observed firmware).
    pub fn send_blob(&self, frames: &[[u8; REPORT_LEN]]) -> Result<u8> {
        let op = frames
            .first()
            .map(|f| f[2])
            .ok_or(Error::Invalid("empty blob".into()))?;
        self.drain()?;
        for f in frames {
            self.send(f)?;
        }
        let ack = self.collect(op, Duration::from_secs(2), |_| true)?;
        match frame::parse(&ack[0])? {
            Frame::Short { payload, .. } => Ok(payload.first().copied().unwrap_or(0)),
            Frame::Long { .. } => Err(Error::Protocol("unexpected long ACK".into())),
        }
    }
}
