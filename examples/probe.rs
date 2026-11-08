//! Read-only identity probe for *any* HID gamepad, used to check whether a
//! controller speaks the same vendor protocol as the BIGBIG WON family.
//!
//! It never writes configuration: it only enumerates descriptors and sends the
//! `GET`/identity short commands (GetMode, GetZkmVersion, GetUUID,
//! GetProfileSize, polling, wireless flag, gamepad mode, device-mode info,
//! power, macro list). No SET, SAVE, RESET or CALIBRATION frame is ever sent.
//!
//! Usage:
//!   cargo run --no-default-features --example probe              # list + probe likely config interfaces
//!   cargo run --no-default-features --example probe -- --all     # probe every openable interface
//!   cargo run --no-default-features --example probe -- /dev/hidrawN   # force one interface

use hidapi::{DeviceInfo, HidApi, HidDevice};

use padtune::device::hex;
use padtune::protocol::commands;
use padtune::protocol::frame::{self, Frame, REPORT_LEN};

/// Config interface signature shared by the BIGBIG WON family.
const CONFIG_USAGE_PAGE: u16 = 0xff7a;
const CONFIG_INTERFACE: i32 = 2;

/// Read-only commands to try, as `(label, request frame)`.
fn read_commands() -> Vec<(&'static str, [u8; REPORT_LEN])> {
    vec![
        ("GetMode        (E2)", commands::get_mode()),
        ("GetZkmVersion  (0B)", commands::get_zkm_version()),
        ("GetUUID        (EF)", commands::get_uuid()),
        ("GetProfileSize (D3)", commands::get_profile_size()),
        ("GetPolling     (F6)", commands::get_polling()),
        ("GetWireless    (FA)", commands::get_wireless_flag()),
        ("GetGamepadMode (D4)", commands::get_gamepad_mode()),
        ("GetDeviceMode  (E1)", commands::get_device_mode_info()),
        ("GetPower       (04)", commands::get_power()),
        ("GetMacroList   (D5)", commands::get_macro_list()),
    ]
}

/// Does this interface look like a vendor configuration channel?
fn looks_like_config(info: &DeviceInfo) -> bool {
    info.usage_page() == CONFIG_USAGE_PAGE
        || (info.usage_page() == 0 && info.interface_number() == CONFIG_INTERFACE)
}

/// Sends one short request and returns the matching reply payload. `None`
/// covers both "no reply" and an I/O error: for a probe the distinction does
/// not matter, the interface simply does not speak this protocol.
fn request(hid: &HidDevice, report: &[u8; REPORT_LEN]) -> Option<Vec<u8>> {
    let op = report[2];
    // Drain stale input. A read error here is treated as "nothing pending":
    // the write below surfaces a broken handle anyway.
    let mut scratch = [0u8; REPORT_LEN + 1];
    while hid.read_timeout(&mut scratch, 5).unwrap_or(0) > 0 {}

    let mut buf = [0u8; REPORT_LEN + 1];
    buf[1..].copy_from_slice(report);
    hid.write(&buf).ok()?;

    // Collect for up to ~600 ms, skipping unrelated pushed frames.
    for _ in 0..6 {
        let mut rx = [0u8; REPORT_LEN + 1];
        let n = hid.read_timeout(&mut rx, 100).ok()?;
        if n == 0 {
            continue;
        }
        if let Ok(Frame::Short { cmd, payload }) = frame::parse(&rx[..n])
            && cmd == op
        {
            return Some(payload.to_vec());
        }
    }
    None
}

/// Human-readable decode of the identity replies we understand.
fn decode(label: &str, payload: &[u8]) -> Option<String> {
    if label.contains("E2") {
        return commands::decode_mode(payload)
            .ok()
            .map(|m| format!("marker={:?} prefix={:02x?}", m.marker, m.prefix));
    }
    if label.contains("0B") {
        return commands::decode_zkm_version(payload)
            .ok()
            .map(|v| format!("zkm='{}' (0x{v:02x})", v as char));
    }
    if label.contains("EF") {
        return commands::decode_uuid(payload)
            .ok()
            .map(|u| format!("uuid={u}"));
    }
    if label.contains("D3") {
        return commands::decode_profile_size(payload)
            .ok()
            .map(|s| format!("profile={s} bytes"));
    }
    None
}

fn probe(hid: &HidDevice) {
    for (label, report) in read_commands() {
        match request(hid, &report) {
            Some(payload) => {
                let decoded = decode(label, &payload)
                    .map(|d| format!("  -> {d}"))
                    .unwrap_or_default();
                println!("    {label}: [{}]{decoded}", hex(&payload));
            }
            None => println!("    {label}: (no reply)"),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let probe_all = args.iter().any(|a| a == "--all");
    let forced: Vec<&String> = args.iter().filter(|a| a.starts_with('/')).collect();

    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("cannot open hidapi: {e}");
            std::process::exit(1);
        }
    };

    println!("== HID devices ==");
    for i in api.device_list() {
        let flag = if looks_like_config(i) {
            " <- config?"
        } else {
            ""
        };
        println!(
            "{:04x}:{:04x}  iface {:>2}  usage {:04x}/{:02x}  {:<20} {:<28} {}{}",
            i.vendor_id(),
            i.product_id(),
            i.interface_number(),
            i.usage_page(),
            i.usage(),
            i.manufacturer_string().unwrap_or_default(),
            i.product_string().unwrap_or_default(),
            i.path().to_string_lossy(),
            flag,
        );
    }

    println!("\n== Read-only identity probe ==");
    let targets: Vec<&DeviceInfo> = api
        .device_list()
        .filter(|i| {
            if !forced.is_empty() {
                return forced.iter().any(|p| i.path().to_string_lossy() == **p);
            }
            probe_all || looks_like_config(i)
        })
        .collect();

    if targets.is_empty() {
        println!(
            "No candidate interface. Re-run with --all to try every one, or pass a /dev/hidrawN path."
        );
        return;
    }

    for info in targets {
        println!(
            "\n[{:04x}:{:04x} iface {} {} — {}]",
            info.vendor_id(),
            info.product_id(),
            info.interface_number(),
            info.product_string().unwrap_or_default(),
            info.path().to_string_lossy(),
        );
        match info.open_device(&api) {
            Ok(hid) => {
                // Only affects plain read(); read_timeout() works either way.
                hid.set_blocking_mode(false).ok();
                probe(&hid);
            }
            Err(e) => println!("    cannot open: {e} (try running with sudo)"),
        }
    }
}
