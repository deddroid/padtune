use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use bigbigctl::actions;
use bigbigctl::device::{self, Device};
use bigbigctl::protocol::profile::{SLOT_NAMES, KEY_SLOTS};

#[derive(Parser)]
#[command(version, about = "Configure BIGBIG WON controllers (Rainbow 2 Pro and family)")]
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
            println!("Device:        {:04x}:{:04x} {} ({})", i.vid, i.pid, i.product, i.path);
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
                println!("{side}: {}..{}%{}", t.start, t.end, if t.hair { " (hair)" } else { "" });
            }
            for i in 0..6 {
                let c = p.curve(i);
                println!(
                    "Curve {i} ({}): deadzone {}  comp {}  A{:?} B{:?}  outer {}  max {}",
                    if i % 2 == 0 { "left" } else { "right" },
                    c.deadzone, c.center_comp, c.point_a, c.point_b, c.outer, c.max_out
                );
            }
            println!("Turbo mask: 0x{:08x}  speed index {}", p.turbo_mask(), p.turbo_speed());
            for slot in 0..KEY_SLOTS {
                if let Some(name) = SLOT_NAMES[slot] {
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
