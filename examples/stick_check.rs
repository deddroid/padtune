//! Checks stick centre and range and trigger travel through the OS gamepad
//! API (gilrs), e.g. after a calibration. Two timed phases with prompts:
//! hands off (centre), then full circles and full trigger pulls (range).
//!
//!   cargo run --release --example stick_check

use std::time::{Duration, Instant};

use gilrs::{Axis, Button, Gamepad, Gilrs};

use padtune::device::VENDOR_ID;

/// Readings within this distance of 0 count as centred.
const CENTRE_OK: f32 = 0.05;
/// Readings at least this far out count as reaching the edge / full press.
const EDGE_OK: f32 = 0.95;

#[derive(Default)]
struct Span {
    min: f32,
    max: f32,
}

impl Span {
    fn add(&mut self, v: f32) {
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }
    fn deviation(&self) -> f32 {
        self.min.abs().max(self.max.abs())
    }
}

/// Six readings: LX, LY, RX, RY, LT, RT (triggers 0..=1).
fn read(pad: &Gamepad) -> [f32; 6] {
    // xpad reports the triggers as analog buttons on some kernels and as
    // Z axes on others: take whichever is further pressed.
    let trigger = |b: Button, a: Axis| {
        let button = pad.button_data(b).map_or(0.0, |d| d.value());
        let axis = (pad.value(a) + 1.0) / 2.0;
        if pad.axis_data(a).is_some() {
            button.max(axis)
        } else {
            button
        }
    };
    [
        pad.value(Axis::LeftStickX),
        pad.value(Axis::LeftStickY),
        pad.value(Axis::RightStickX),
        pad.value(Axis::RightStickY),
        trigger(Button::LeftTrigger2, Axis::LeftZ),
        trigger(Button::RightTrigger2, Axis::RightZ),
    ]
}

/// Samples the controller for `secs`, returning the span of every reading.
fn phase(gilrs: &mut Gilrs, secs: u64) -> Option<[Span; 6]> {
    let mut spans: [Span; 6] = Default::default();
    let end = Instant::now() + Duration::from_secs(secs);
    let mut seen = false;
    while Instant::now() < end {
        while gilrs.next_event().is_some() {}
        if let Some((_, pad)) = gilrs
            .gamepads()
            .find(|(_, p)| p.vendor_id() == Some(VENDOR_ID))
        {
            seen = true;
            for (s, v) in spans.iter_mut().zip(read(&pad)) {
                s.add(v);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    seen.then_some(spans)
}

fn mark(ok: bool) -> &'static str {
    if ok { "OK" } else { "CHECK" }
}

fn main() {
    let Ok(mut gilrs) = Gilrs::new() else {
        eprintln!("cannot open the OS gamepad API");
        std::process::exit(1);
    };
    match gilrs
        .gamepads()
        .find(|(_, p)| p.vendor_id() == Some(VENDOR_ID))
    {
        Some((_, pad)) => println!("Gamepad: {:?} ({:04x})", pad.name(), VENDOR_ID),
        None => {
            eprintln!("no {VENDOR_ID:04x} gamepad found by the OS gamepad API");
            std::process::exit(1);
        }
    }

    println!("\nPHASE 1/2 - hands OFF the sticks and triggers (3 s)...");
    let Some(rest) = phase(&mut gilrs, 3) else {
        eprintln!("controller disappeared");
        std::process::exit(1);
    };
    println!(
        "PHASE 2/2 - NOW rotate BOTH sticks along the edge and pull BOTH triggers fully (8 s)..."
    );
    let Some(moved) = phase(&mut gilrs, 8) else {
        eprintln!("controller disappeared");
        std::process::exit(1);
    };

    println!("\nCentre at rest (max distance from 0, ok below {CENTRE_OK}):");
    for (name, i) in [
        ("left  X", 0),
        ("left  Y", 1),
        ("right X", 2),
        ("right Y", 3),
    ] {
        let d = rest[i].deviation();
        println!("  {name}  {d:.3}  {}", mark(d < CENTRE_OK));
    }
    for (name, i) in [("LT", 4), ("RT", 5)] {
        let d = rest[i].max;
        println!("  {name}       {d:.3}  {}", mark(d < CENTRE_OK));
    }
    println!("\nRange while moving (ok when both ends reach {EDGE_OK}):");
    for (name, i) in [
        ("left  X", 0),
        ("left  Y", 1),
        ("right X", 2),
        ("right Y", 3),
    ] {
        let s = &moved[i];
        let ok = s.min <= -EDGE_OK && s.max >= EDGE_OK;
        println!("  {name}  {:+.3} .. {:+.3}  {}", s.min, s.max, mark(ok));
    }
    for (name, i) in [("LT", 4), ("RT", 5)] {
        let m = moved[i].max;
        println!("  {name}       max {m:.3}  {}", mark(m >= EDGE_OK));
    }
}
