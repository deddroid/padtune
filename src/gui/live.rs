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
        // Linux xpad reports X/Y swapped relative to gilrs' North/West naming
        // (observed on hardware); XInput on Windows follows the labels.
        Button::North if cfg!(target_os = "linux") => 3,
        Button::West if cfg!(target_os = "linux") => 4,
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

/// Handle to the input thread.
pub struct Live {
    pub state: Shared,
    rumble: std::sync::mpsc::Sender<(u16, u16)>,
}

impl Live {
    /// Plays a short rumble so the vibration settings can be felt, through
    /// the OS force-feedback API: `strong` drives the left grip motor, `weak`
    /// the right one (0..=65535 each; the controller then scales it with its
    /// own min/max). Trigger motors are not reachable this way.
    pub fn rumble(&self, strong: u16, weak: u16) {
        let _ = self.rumble.send((strong, weak));
    }
}

/// Starts the reader thread; `wake` requests a repaint on changes.
pub fn spawn(wake: impl Fn() + Send + 'static) -> Live {
    let state: Shared = Arc::default();
    let out = state.clone();
    let (rumble_tx, rumble_rx) = std::sync::mpsc::channel::<(u16, u16)>();
    std::thread::Builder::new()
        .name("padtune-input".into())
        .spawn(move || {
            let Ok(mut gilrs) = Gilrs::new() else { return };
            let mut window_start = Instant::now();
            let mut events = 0u32;
            let mut playing: Option<(gilrs::ff::Effect, Instant)> = None;
            loop {
                if let Ok((strong, weak)) = rumble_rx.try_recv() {
                    playing = play_rumble(&mut gilrs, strong, weak).map(|e| (e, Instant::now()));
                }
                if playing
                    .as_ref()
                    .is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(2))
                {
                    playing = None;
                }
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
                    let mut s = state.lock().expect("live state");
                    // Triggers are analog axes: derive their pressed bits.
                    for (v, bit) in [(s.lt, 8), (s.rt, 9)] {
                        if v > 0.5 {
                            s.pressed |= 1 << bit;
                        } else {
                            s.pressed &= !(1 << bit);
                        }
                    }
                    drop(s);
                    wake();
                }
            }
        })
        .expect("spawn input thread");
    Live {
        state: out,
        rumble: rumble_tx,
    }
}

fn play_rumble(gilrs: &mut Gilrs, strong: u16, weak: u16) -> Option<gilrs::ff::Effect> {
    use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
    let ids: Vec<_> = gilrs
        .gamepads()
        .filter(|(_, p)| p.vendor_id() == Some(VENDOR_ID) && p.is_ff_supported())
        .map(|(id, _)| id)
        .collect();
    if ids.is_empty() {
        return None;
    }
    let scheduling = Replay {
        play_for: Ticks::from_ms(900),
        ..Default::default()
    };
    let effect = EffectBuilder::new()
        .add_effect(BaseEffect {
            kind: BaseEffectType::Strong { magnitude: strong },
            scheduling,
            ..Default::default()
        })
        .add_effect(BaseEffect {
            kind: BaseEffectType::Weak { magnitude: weak },
            scheduling,
            ..Default::default()
        })
        .gamepads(&ids)
        .finish(gilrs)
        .ok()?;
    effect.play().ok()?;
    Some(effect)
}
