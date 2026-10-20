use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use padtune::actions;
use padtune::device::{self, Device};
use padtune::protocol::profile::SLOT_NAMES;

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
    /// Print every button press reported by the controller (key test).
    Monitor {
        /// Seconds to listen.
        #[arg(default_value_t = 30)]
        seconds: u64,
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
    /// HOME logo colour in XInput/PC mode (RRGGBB; 000000 = factory/follow mode).
    Logo {
        color: String,
        /// Also set the two other console-mode slots (RRGGBB each).
        #[arg(long, num_args = 2, value_names = ["SLOT1", "SLOT2"])]
        other: Option<Vec<String>>,
    },
    /// Trigger range (profile write).
    Trigger {
        /// left or right
        side: Side,
        /// Travel (%) below which the trigger reads as released.
        #[arg(long)]
        start: Option<u8>,
        /// Travel (%) at which it reads as fully pressed.
        #[arg(long)]
        end: Option<u8>,
        /// Hair trigger on/off.
        #[arg(long)]
        hair: Option<bool>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Side {
    Left,
    Right,
}

fn parse_rgb(s: &str) -> padtune::Result<[u8; 3]> {
    let b = padtune::config::from_hex(s.trim_start_matches('#'))?;
    b.try_into()
        .map_err(|_| padtune::Error::Invalid(format!("colour {s} must be RRGGBB")))
}

fn run(cli: Cli) -> padtune::Result<()> {
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
            let mut delta = padtune::config::DeviceConfig::default();
            match what {
                SetCmd::Polling { hz } => {
                    delta.polling_code = Some(
                        padtune::protocol::commands::polling_code(hz).ok_or_else(|| {
                            padtune::Error::Invalid(format!("{hz} Hz is not supported"))
                        })?,
                    )
                }
                SetCmd::Brightness { value } => delta.led_brightness = Some(value),
                SetCmd::Trigger {
                    side,
                    start,
                    end,
                    hair,
                } => {
                    let mut p = actions::read_profile(&dev)?;
                    let right = matches!(side, Side::Right);
                    let mut t = p.trigger(right);
                    t.start = start.unwrap_or(t.start);
                    t.end = end.unwrap_or(t.end);
                    t.hair = hair.unwrap_or(t.hair);
                    p.set_trigger(right, t)?;
                    delta.profile = Some(padtune::config::to_hex(&p.to_bytes()));
                }
                SetCmd::Logo { color, other } => {
                    use padtune::protocol::commands::{
                        LOGO_SLOT_XINPUT, decode_logo_led, get_logo_led,
                    };
                    let mut colors = decode_logo_led(&dev.request(&get_logo_led())?)?;
                    colors[LOGO_SLOT_XINPUT] = parse_rgb(&color)?;
                    if let Some(o) = other {
                        colors[0] = parse_rgb(&o[0])?;
                        colors[1] = parse_rgb(&o[1])?;
                    }
                    delta.logo_colors = Some(colors)
                }
            }
            actions::apply(&dev, &delta)?;
            println!("OK (verified by read-back)");
        }
        Cmd::Monitor { seconds } => {
            use padtune::protocol::commands::{self, decode_key_report, op};
            use padtune::protocol::frame::{self, Frame};
            use padtune::protocol::profile::SLOT_NAMES;
            let dev = Device::open(cli.device.as_deref())?;
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = stop.clone();
            // Ctrl+C must still switch the key reports off again.
            let _ =
                ctrlc::set_handler(move || flag.store(true, std::sync::atomic::Ordering::SeqCst));
            dev.request(&commands::set_key_reports(true))?;
            println!("Listening for {seconds}s — press buttons one at a time (Ctrl+C to stop).");
            let end = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
            let mut last = None;
            while std::time::Instant::now() < end && !stop.load(std::sync::atomic::Ordering::SeqCst)
            {
                let r = match dev.recv(std::time::Duration::from_millis(200)) {
                    Ok(Some(r)) => r,
                    Ok(None) => continue,
                    // A signal interrupts the blocking read; the loop condition then stops.
                    Err(_) if stop.load(std::sync::atomic::Ordering::SeqCst) => break,
                    Err(e) => {
                        let _ = dev.request(&commands::set_key_reports(false));
                        return Err(e);
                    }
                };
                let Ok(Frame::Short { cmd, payload }) = frame::parse(&r) else {
                    continue;
                };
                if cmd != op::KEY_REPORT {
                    // Other pushed frames (e.g. status notifications) are shown raw.
                    println!("frame 0x{cmd:02x} {payload:02x?}");
                    continue;
                }
                let Ok(k) = decode_key_report(payload) else {
                    println!("raw {payload:02x?}");
                    continue;
                };
                if last == Some(k.keys) {
                    continue;
                }
                last = Some(k.keys);
                let names: Vec<String> = (0..32)
                    .filter(|b| k.keys & (1 << b) != 0)
                    .map(|b| SLOT_NAMES[b].map_or(format!("bit{b}"), |n| format!("{n}(bit{b})")))
                    .collect();
                println!(
                    "keys 0x{:08x} [{}]  raw {:02x?}",
                    k.keys,
                    names.join(" "),
                    payload
                );
            }
            let _ = dev.request(&commands::set_key_reports(false));
            println!("Key reports switched off.");
        }
        Cmd::Restore { file } => {
            let dev = Device::open(cli.device.as_deref())?;
            let p = padtune::protocol::profile::Profile::from_bytes(&std::fs::read(&file)?)?;
            actions::write_profile(&dev, &p)?;
            println!(
                "Restored {} ({} bytes), verified by read-back",
                file.display(),
                p.layout().len
            );
        }
        Cmd::Macros => {
            let dev = Device::open(cli.device.as_deref())?;
            for slot in 0..padtune::protocol::macros::SLOTS as u8 {
                match actions::read_macro(&dev, slot) {
                    Ok(m) => {
                        let key = match m.run_key {
                            padtune::protocol::macros::UNBOUND => "unbound".to_string(),
                            k => SLOT_NAMES
                                .get(usize::from(k))
                                .copied()
                                .flatten()
                                .unwrap_or("?")
                                .to_string(),
                        };
                        println!(
                            "Slot {}: {} steps, run key {key}, repeat {}{}{}",
                            slot + 1,
                            m.steps.len(),
                            m.repeat,
                            if m.looped { ", loop" } else { "" },
                            if m.run_after_release {
                                ", after release"
                            } else {
                                ""
                            },
                        );
                        for (i, st) in m.steps.iter().enumerate() {
                            println!(
                                "  {i:2}: {:5} ms  inputs 0x{:08x}  analog {:?}{}",
                                st.duration_ms,
                                st.inputs,
                                st.analog,
                                if st.marker { "  [marker]" } else { "" }
                            );
                        }
                    }
                    Err(e) => println!("Slot {}: error: {e}", slot + 1),
                }
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
