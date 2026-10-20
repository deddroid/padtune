use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use bigbigctl::actions;
use bigbigctl::device::{self, Device};
use bigbigctl::protocol::profile::SLOT_NAMES;

#[derive(Parser)]
#[command(
    version,
    about = "Configure BIGBIG WON controllers (Rainbow 2 Pro and family)"
)]
struct Cli {
    /// HID path of the controller (default: first one found).
    #[arg(long, global = true)]
    device: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List connected controllers.
    List,
    /// Show identity, firmware and status.
    Info,
    /// Show the current profile.
    Profile,
    /// Save the current profile to a binary file (backup).
    Backup { file: PathBuf },
    /// Show the macro slots.
    Macros,
    /// Change a setting (each write is verified by reading it back).
    Set {
        #[command(subcommand)]
        what: SetCmd,
    },
    /// Restore a profile from a backup file.
    Restore { file: PathBuf },
}

#[derive(Subcommand)]
enum SetCmd {
    /// Polling rate in Hz (250, 500, 1000, 2000).
    Polling { hz: u32 },
    /// LED brightness, 0-255.
    Brightness { value: u8 },
    /// Logo colours for Xbox, Switch and PlayStation modes (RRGGBB each).
    Logo {
        xbox: String,
        switch: String,
        ps: String,
    },
}

fn parse_rgb(s: &str) -> bigbigctl::Result<[u8; 3]> {
    let b = bigbigctl::config::from_hex(s.trim_start_matches('#'))?;
    b.try_into()
        .map_err(|_| bigbigctl::Error::Invalid(format!("colour {s} must be RRGGBB")))
}

fn run(cli: Cli) -> bigbigctl::Result<()> {
    match cli.cmd {
        Cmd::List => {
            let found = device::list()?;
            if found.is_empty() {
                println!("No controller found.");
            }
            for c in found {
                println!(
                    "{:04x}:{:04x}  {} {}  interface {}  {}",
                    c.vendor_id, c.product_id, c.manufacturer, c.product, c.interface, c.path
                );
            }
        }
        Cmd::Info => {
            let dev = Device::open(cli.device.as_deref())?;
            let i = actions::read_info(&dev)?;
            println!(
                "Device:        {:04x}:{:04x} {} ({})",
                i.vid, i.pid, i.product, i.path
            );
            if let Some(m) = &i.mode {
                println!("Model marker:  {}", m.marker);
            }
            if let Some(v) = i.zkm_version {
                println!("ZKM version:   {} (0x{v:02x})", v as char);
            }
            if let Some(u) = &i.uuid {
                println!("UUID:          {u}");
            }
            if let Some(s) = i.profile_size {
                println!("Profile size:  {s} bytes");
            }
            if let Some(c) = i.polling_code {
                match actions::polling_hz(c) {
                    Some(hz) => println!("Polling rate:  {hz} Hz"),
                    None => println!("Polling rate:  unknown code {c}"),
                }
            }
        }
        Cmd::Profile => {
            let dev = Device::open(cli.device.as_deref())?;
            let p = actions::read_profile(&dev)?;
            println!("Layout: {} ({} bytes)", p.layout().name, p.layout().len);
            for (side, right) in [("Left trigger", false), ("Right trigger", true)] {
                let t = p.trigger(right);
                println!(
                    "{side}: {}..{}%{}",
                    t.start,
                    t.end,
                    if t.hair { " (hair)" } else { "" }
                );
            }
            for i in 0..6 {
                let c = p.curve(i);
                println!(
                    "Curve {i} ({}): deadzone {}  comp {}  A{:?} B{:?}  outer {}  max {}",
                    if i % 2 == 0 { "left" } else { "right" },
                    c.deadzone,
                    c.center_comp,
                    c.point_a,
                    c.point_b,
                    c.outer,
                    c.max_out
                );
            }
            println!(
                "Turbo mask: 0x{:08x}  speed index {}",
                p.turbo_mask(),
                p.turbo_speed()
            );
            for (slot, name) in SLOT_NAMES.iter().enumerate() {
                if let Some(name) = name {
                    println!("  {name:6} -> {:?}", p.key(slot));
                }
            }
        }
        Cmd::Backup { file } => {
            let dev = Device::open(cli.device.as_deref())?;
            let p = actions::read_profile(&dev)?;
            std::fs::write(&file, p.to_bytes())?;
            println!("Saved {} bytes to {}", p.layout().len, file.display());
        }
        Cmd::Set { what } => {
            let dev = Device::open(cli.device.as_deref())?;
            let mut delta = bigbigctl::config::DeviceConfig::default();
            match what {
                SetCmd::Polling { hz } => {
                    delta.polling_code = Some(
                        bigbigctl::protocol::commands::polling_code(hz).ok_or_else(|| {
                            bigbigctl::Error::Invalid(format!("{hz} Hz is not supported"))
                        })?,
                    )
                }
                SetCmd::Brightness { value } => delta.led_brightness = Some(value),
                SetCmd::Logo { xbox, switch, ps } => {
                    delta.logo_colors =
                        Some([parse_rgb(&xbox)?, parse_rgb(&switch)?, parse_rgb(&ps)?])
                }
            }
            actions::apply(&dev, &delta)?;
            println!("OK (verified by read-back)");
        }
        Cmd::Restore { file } => {
            let dev = Device::open(cli.device.as_deref())?;
            let p = bigbigctl::protocol::profile::Profile::from_bytes(&std::fs::read(&file)?)?;
            actions::write_profile(&dev, &p)?;
            println!(
                "Restored {} ({} bytes), verified by read-back",
                file.display(),
                p.layout().len
            );
        }
        Cmd::Macros => {
            let dev = Device::open(cli.device.as_deref())?;
            for (i, s) in actions::read_macro_list(&dev)?.iter().enumerate() {
                let steps = s.len.saturating_sub(10) / 10;
                println!("Slot {}: {} steps (crc 0x{:04x})", i + 1, steps, s.crc);
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
