//! Hardware test of the calibration command `1B`, decoded from the official
//! PC Tool 1.0.7.7: `CDeviceBBwon::Calibration` sends `A5 06 1B target state
//! sum`, and the calibration window calls it with target 0 = sticks and
//! triggers, 1 = gyroscope; state 1 = start, 0 = end (sticks) or cancel
//! (gyroscope). Verified on a Rainbow 2 Pro (docs/hw-notes/calibration.md).
//! Nothing is sent until you press Enter; every frame the controller sends
//! back is printed with its time.
//!
//!   cargo run --release --no-default-features --example calibrate -- sticks
//!   cargo run --release --no-default-features --example calibrate -- gyro

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use padtune::device::{Device, hex};
use padtune::protocol::frame::{self, Frame, REPORT_LEN, short};

const CALIBRATION: u8 = 0x1b;
/// SmartStartStop / SmartTriggerLock (read only here, for the record).
const TRIGGER_LOCK: u8 = 0xf8;
/// Upper bound on one calibration step, in case Enter never comes.
const STEP_LIMIT: Duration = Duration::from_secs(300);

fn wire(f: &[u8; REPORT_LEN]) -> String {
    hex(&f[..usize::from(f[1])])
}

/// Prints `prompt`, waits for Enter and returns what was typed (lowercase).
fn ask(prompt: &str) -> String {
    println!("{prompt}");
    let mut line = String::new();
    // EOF or a read error means "go on"; Ctrl+C still aborts.
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().to_lowercase()
}

/// Prints every frame received until `stop` is set or `limit` elapses.
fn listen(dev: &Device, t0: Instant, stop: &AtomicBool, limit: Duration) -> padtune::Result<()> {
    let end = Instant::now() + limit;
    while !stop.load(Ordering::SeqCst) && Instant::now() < end {
        let Some(r) = dev.recv(Duration::from_millis(100))? else {
            continue;
        };
        let ms = t0.elapsed().as_millis();
        match frame::parse(&r) {
            Ok(Frame::Short { cmd, payload }) => {
                println!("  +{ms:>6} ms  <- op {cmd:02x}  [{}]", hex(payload))
            }
            Ok(Frame::Long { op, seq, payload }) => {
                println!(
                    "  +{ms:>6} ms  <- long op {op:02x} #{seq}  [{}]",
                    hex(payload)
                )
            }
            Err(e) => println!("  +{ms:>6} ms  <- raw [{}] ({e})", hex(&r)),
        }
    }
    Ok(())
}

fn main() -> padtune::Result<()> {
    let target = match std::env::args().nth(1).as_deref() {
        Some("sticks") => 0u8,
        Some("gyro") => 1,
        _ => {
            eprintln!("usage: calibrate sticks|gyro");
            std::process::exit(2);
        }
    };
    let dev = Device::open(None)?;
    println!(
        "Controller {:04x}:{:04x} {} ({})",
        dev.info.vendor_id, dev.info.product_id, dev.info.product, dev.info.path
    );
    match dev.request(&short(TRIGGER_LOCK, &[])) {
        Ok(p) => println!("Trigger lock (F8) reads [{}]", hex(&p)),
        Err(e) => println!("Trigger lock (F8): {e}"),
    }

    let start = short(CALIBRATION, &[target, 1]);
    let finish = short(CALIBRATION, &[target, 0]);
    if target == 0 {
        println!(
            "\nSTICKS AND TRIGGERS. Before starting: trigger stops open, hands off the sticks."
        );
    } else {
        println!("\nGYROSCOPE. Put the controller on a flat, level surface and do not touch it.");
    }
    if ask(&format!(
        "Press Enter to send START [{}], or Ctrl+C to abort.",
        wire(&start)
    )) == "q"
    {
        return Ok(());
    }

    dev.drain()?;
    let t0 = Instant::now();
    dev.send(&start)?;
    println!("  +     0 ms  -> START [{}]", wire(&start));
    if target == 0 {
        println!(
            "Now rotate BOTH sticks along the outer edge 3 times, together;\n\
             then pull BOTH triggers all the way 3 times, together.\n\
             When done, press Enter to send END [{}].",
            wire(&finish)
        );
    } else {
        println!(
            "Wait until the controller finishes (light stops pulsing or a reply appears),\n\
             then press Enter. Type c + Enter instead to send CANCEL [{}].",
            wire(&finish)
        );
    }

    // stdin on its own thread, so frames keep printing while we wait.
    let stop = Arc::new(AtomicBool::new(false));
    let typed = Arc::new(Mutex::new(String::new()));
    {
        let (stop, typed) = (stop.clone(), typed.clone());
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = std::io::stdin().lock().read_line(&mut line);
            if let Ok(mut t) = typed.lock() {
                *t = line.trim().to_lowercase();
            }
            stop.store(true, Ordering::SeqCst);
        });
    }
    listen(&dev, t0, &stop, STEP_LIMIT)?;

    let cancel = typed.lock().map(|t| t.starts_with('c')).unwrap_or(false);
    if target == 0 || cancel {
        dev.send(&finish)?;
        println!(
            "  +{:>6} ms  -> {} [{}]",
            t0.elapsed().as_millis(),
            if target == 0 { "END" } else { "CANCEL" },
            wire(&finish)
        );
    }
    let quiet = AtomicBool::new(false);
    listen(&dev, t0, &quiet, Duration::from_secs(3))?;
    match dev.request(&short(TRIGGER_LOCK, &[])) {
        Ok(p) => println!("Trigger lock (F8) now reads [{}]", hex(&p)),
        Err(e) => println!("Trigger lock (F8): {e}"),
    }
    println!("Done.");
    Ok(())
}
