//! Background thread that owns the HID device. The UI sends [`Request`]s and
//! receives [`Event`]s, so it never blocks on USB I/O.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::actions::{self, Snapshot};
use crate::config::DeviceConfig;
use crate::device::Device;
use crate::protocol::calibration::{self, Progress, Step, Target};
use crate::protocol::commands::{self, op};
use crate::protocol::frame::{self, Frame};

#[derive(Debug)]
pub enum Request {
    /// (Re)connect and read everything.
    Refresh,
    /// Write only these parts, then re-read.
    Apply(DeviceConfig),
    /// Save the current profile bytes to a file.
    Backup(std::path::PathBuf),
    /// Turn the controller's key reports on/off (key test). While on, the
    /// controller pauses its normal gamepad output.
    KeyTest(bool),
    /// Start (`true`) or end a calibration run. Ending the gyroscope run
    /// cancels it: it finishes by itself.
    Calibrate(Target, bool),
    /// Switch live key reports off and stop the thread.
    Shutdown,
}

#[derive(Debug)]
pub enum Event {
    Busy(bool),
    Snapshot(Box<Snapshot>),
    Battery(u8),
    /// Active on-board configuration, read every few seconds: it changes
    /// with the controller's Configuration key.
    ActiveConfig(u8),
    /// Whether an idle 2.4G dongle (no linked controller) is plugged in.
    IdleDongle(bool),
    /// Pressed buttons from the controller's live key report (slot bits).
    Keys(u32),
    /// A calibration reply; [`Step::outcome`] tells when the run is over.
    CalibrationStep(Step),
    /// Stick and trigger positions pushed during the sticks run.
    CalibrationProgress(Progress),
    /// The run got no closing reply in time.
    CalibrationLost,
    Disconnected(String),
    Applied,
    Message(String),
    Error(String),
}

/// Handle owned by the UI.
pub struct Worker {
    tx: Sender<Request>,
    pub rx: Receiver<Event>,
}

impl Worker {
    /// Starts the worker; `wake` is called after every event (repaint).
    pub fn spawn(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, req_rx) = mpsc::channel();
        let (ev_tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("padtune-device".into())
            .spawn(move || run(req_rx, ev_tx, wake))
            .expect("spawn device thread");
        let w = Self { tx, rx };
        w.send(Request::Refresh);
        w
    }

    pub fn send(&self, r: Request) {
        let _ = self.tx.send(r);
    }
}

fn run(rx: Receiver<Request>, tx: Sender<Event>, wake: impl Fn()) {
    let emit = |e: Event| {
        let _ = tx.send(e);
        wake();
    };
    let mut dev: Option<Device> = None;
    // Key test: the controller pushes key reports but pauses its gamepad
    // output, so it is only on while the user asks for it.
    let mut keys_wanted = false;
    let mut last_keys = None;
    let mut last_poll = Instant::now();
    // Calibration run in progress, with the time by which it must end.
    let mut calibrating: Option<(Target, Instant)> = None;
    let set_keys = |dev: &Option<Device>, on: bool| {
        if let Some(d) = dev {
            let _ = d.request(&commands::set_key_reports(on));
        }
    };
    loop {
        // Tick fast only while live key reports must be drained; otherwise an
        // idle tick is plenty (battery/hot-plug polls are gated to 3 s below,
        // and a request wakes the loop immediately regardless of the tick).
        let tick = if keys_wanted || calibrating.is_some() {
            Duration::from_millis(15)
        } else {
            Duration::from_millis(200)
        };
        match rx.recv_timeout(tick) {
            Ok(Request::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                if keys_wanted {
                    set_keys(&dev, false);
                }
                return;
            }
            Ok(Request::KeyTest(on)) => {
                keys_wanted = on;
                set_keys(&dev, on);
                last_keys = None;
                emit(Event::Keys(0));
            }
            Ok(Request::Calibrate(target, begin)) => {
                // Generous limits: the user moves the sticks for as long as
                // needed; the gyroscope ends in about 5 s; the end of the
                // sticks run is answered within milliseconds.
                let limit = match (target, begin) {
                    (Target::Sticks, true) => Duration::from_secs(600),
                    (Target::Gyro, true) => Duration::from_secs(30),
                    (_, false) => Duration::from_secs(3),
                };
                let frame = if begin {
                    calibration::start(target)
                } else {
                    calibration::finish(target)
                };
                let sent = connect(&mut dev).and_then(|d| {
                    if begin {
                        d.drain()?;
                    }
                    d.send(&frame)
                });
                match sent {
                    Ok(()) => calibrating = Some((target, Instant::now() + limit)),
                    Err(e) => {
                        calibrating = None;
                        emit(Event::CalibrationLost);
                        emit(Event::Error(e.to_string()));
                    }
                }
            }
            Ok(req) => {
                emit(Event::Busy(true));
                handle(req, &mut dev, &emit);
                emit(Event::Busy(false));
                // A reconnect resets the controller's report switch.
                if keys_wanted {
                    set_keys(&dev, true);
                }
                last_poll = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {
                if (keys_wanted || calibrating.is_some())
                    && let Some(d) = &dev
                {
                    while let Ok(Some(r)) = d.recv(Duration::ZERO) {
                        let Ok(Frame::Short { cmd, payload }) = frame::parse(&r) else {
                            continue;
                        };
                        match cmd {
                            // During the sticks run `02` carries progress
                            // flags, not keys.
                            op::KEY_REPORT if matches!(calibrating, Some((Target::Sticks, _))) => {
                                if let Ok(p) = calibration::decode_progress(payload) {
                                    emit(Event::CalibrationProgress(p));
                                }
                            }
                            op::KEY_REPORT if keys_wanted => {
                                if let Ok(k) = commands::decode_key_report(payload)
                                    && last_keys != Some(k.keys)
                                {
                                    last_keys = Some(k.keys);
                                    emit(Event::Keys(k.keys));
                                }
                            }
                            calibration::OP => {
                                if let Ok(step) = calibration::decode_step(payload) {
                                    // Diagnostics: the meaning of steps other
                                    // than the logged ones is still unknown.
                                    eprintln!(
                                        "padtune: calibration {:?} step {:02x}",
                                        step.target, step.step
                                    );
                                    if step.outcome().is_some() {
                                        calibrating = None;
                                    }
                                    emit(Event::CalibrationStep(step));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if let Some((_, by)) = calibrating {
                    if Instant::now() > by {
                        eprintln!("padtune: calibration got no closing reply in time");
                        calibrating = None;
                        emit(Event::CalibrationLost);
                    }
                    // A poll would drain the replies of the run.
                    continue;
                }
                if last_poll.elapsed() < Duration::from_secs(3) {
                    continue;
                }
                last_poll = Instant::now();
                emit(Event::IdleDongle(crate::device::idle_dongle_present()));
                // Periodic poll: battery when connected, hot-plug when not.
                match &dev {
                    Some(d) => match actions::read_power(d) {
                        Ok(p) => {
                            emit(Event::Battery(p.battery));
                            if let Some(c) = actions::read_active_config(d) {
                                emit(Event::ActiveConfig(c));
                            }
                        }
                        Err(e) => {
                            dev = None;
                            emit(Event::Disconnected(e.to_string()));
                        }
                    },
                    None => {
                        if crate::device::list().is_ok_and(|l| !l.is_empty()) {
                            handle(Request::Refresh, &mut dev, &emit);
                            if keys_wanted {
                                set_keys(&dev, true);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn connect(dev: &mut Option<Device>) -> crate::Result<&Device> {
    if dev.is_none() {
        *dev = Some(Device::open(None)?);
    }
    Ok(dev.as_ref().expect("just set"))
}

fn handle(req: Request, dev: &mut Option<Device>, emit: &impl Fn(Event)) {
    let result = (|| -> crate::Result<()> {
        match req {
            Request::Refresh => {
                *dev = None; // reopen: the path may have changed after a replug
                let d = connect(dev)?;
                emit(Event::Snapshot(Box::new(actions::read_snapshot(d)?)));
            }
            Request::Apply(delta) => {
                let d = connect(dev)?;
                actions::apply(d, &delta)?;
                // Remember what was applied before re-reading: the strip
                // effect cannot be read back and comes from this state.
                actions::remember_applied(d, &delta);
                emit(Event::Applied);
                emit(Event::Snapshot(Box::new(actions::read_snapshot(d)?)));
            }
            Request::Shutdown | Request::KeyTest(_) | Request::Calibrate(..) => {}
            Request::Backup(path) => {
                let d = connect(dev)?;
                let p = actions::read_profile(d)?;
                std::fs::write(&path, p.to_bytes())?;
                emit(Event::Message(path.display().to_string()));
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        let msg = e.to_string();
        if matches!(e, crate::Error::NotFound | crate::Error::Hid(_)) {
            *dev = None;
            emit(Event::Disconnected(msg));
        } else {
            emit(Event::Error(msg));
        }
    }
}
