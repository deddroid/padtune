//! Live input state of the controller (sticks, triggers, buttons), read
//! through the OS gamepad API via `gilrs` (evdev on Linux, XInput on Windows).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gilrs};

use crate::device::VENDOR_ID;

/// Snapshot of the inputs, axes in -1..=1 (sticks) or 0..=1 (triggers).
#[derive(Debug, Clone, Default)]
pub struct LiveState {
    pub connected: bool,
    pub name: String,
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub lt: f32,
    pub rt: f32,
    /// Pressed buttons by profile slot index.
    pub pressed: u32,
    /// Input events per second (activity indicator, not the polling rate).
    pub events_per_sec: f32,
}

/// Profile slot index for a gilrs button (see `profile::SLOT_NAMES`).
fn slot(b: Button) -> Option<usize> {
    Some(match b {
        Button::South => 0,
        Button::East => 1,
        Button::West => 3,
        Button::North => 4,
        Button::LeftTrigger => 6,
        Button::RightTrigger => 7,
        Button::LeftTrigger2 => 8,
        Button::RightTrigger2 => 9,
        Button::Select => 10,
        Button::Start => 11,
        Button::Mode => 12,
        Button::LeftThumb => 13,
        Button::RightThumb => 14,
        Button::DPadUp => 16,
        Button::DPadDown => 17,
        Button::DPadLeft => 18,
        Button::DPadRight => 19,
        _ => return None,
    })
}

pub type Shared = Arc<Mutex<LiveState>>;

/// Starts the reader thread; `wake` requests a repaint on changes.
pub fn spawn(wake: impl Fn() + Send + 'static) -> Shared {
    let state: Shared = Arc::default();
    let out = state.clone();
    std::thread::Builder::new()
        .name("bigbigctl-input".into())
        .spawn(move || {
            let Ok(mut gilrs) = Gilrs::new() else { return };
            let mut window_start = Instant::now();
            let mut events = 0u32;
            loop {
                let mut changed = false;
                while let Some(ev) = gilrs.next_event_blocking(Some(Duration::from_millis(250))) {
                    let pad = gilrs.gamepad(ev.id);
                    if pad.vendor_id() != Some(VENDOR_ID) {
                        continue;
                    }
                    events += 1;
                    let mut s = state.lock().expect("live state");
                    s.connected = true;
                    s.name = pad.name().to_string();
                    match ev.event {
                        EventType::AxisChanged(axis, v, _) => match axis {
                            Axis::LeftStickX => s.left.0 = v,
                            Axis::LeftStickY => s.left.1 = v,
                            Axis::RightStickX => s.right.0 = v,
                            Axis::RightStickY => s.right.1 = v,
                            Axis::LeftZ => s.lt = (v + 1.0) / 2.0,
                            Axis::RightZ => s.rt = (v + 1.0) / 2.0,
                            _ => {}
                        },
                        EventType::ButtonChanged(b, v, _) => match b {
                            Button::LeftTrigger2 => s.lt = v,
                            Button::RightTrigger2 => s.rt = v,
                            _ => {}
                        },
                        EventType::Disconnected => s.connected = false,
                        _ => {}
                    }
                    if let EventType::ButtonPressed(b, _) | EventType::ButtonReleased(b, _) =
                        ev.event
                        && let Some(i) = slot(b)
                    {
                        let bit = 1u32 << i;
                        if matches!(ev.event, EventType::ButtonPressed(..)) {
                            s.pressed |= bit;
                        } else {
                            s.pressed &= !bit;
                        }
                    }
                    changed = true;
                    if window_start.elapsed() >= Duration::from_secs(1) {
                        s.events_per_sec = events as f32 / window_start.elapsed().as_secs_f32();
                        events = 0;
                        window_start = Instant::now();
                    }
                }
                if !changed {
                    let any = gilrs
                        .gamepads()
                        .any(|(_, p)| p.vendor_id() == Some(VENDOR_ID) && p.is_connected());
                    let mut s = state.lock().expect("live state");
                    if s.connected != any {
                        s.connected = any;
                        changed = true;
                    }
                }
                if changed {
                    wake();
                }
            }
        })
        .expect("spawn input thread");
    out
}
