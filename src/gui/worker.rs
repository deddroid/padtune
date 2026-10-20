//! Background thread that owns the HID device. The UI sends [`Request`]s and
//! receives [`Event`]s, so it never blocks on USB I/O.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::actions::{self, Snapshot};
use crate::config::DeviceConfig;
use crate::device::Device;
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
    /// Switch live key reports off and stop the thread.
    Shutdown,
}

#[derive(Debug)]
pub enum Event {
    Busy(bool),
    Snapshot(Box<Snapshot>),
    Battery(u8),
    /// Pressed buttons from the controller's live key report (slot bits).
    Keys(u32),
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
            .name("bigbigctl-device".into())
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
    let mut keys_on = false;
    let mut last_keys = None;
    let mut last_poll = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(15)) {
            Ok(Request::Shutdown) => {
                if let (Some(d), true) = (&dev, keys_on) {
                    let _ = d.request(&commands::set_key_reports(false));
                }
                return;
            }
            Ok(req) => {
                emit(Event::Busy(true));
                handle(req, &mut dev, &emit);
                emit(Event::Busy(false));
                // Reconnects reset the controller's report switch: re-enable.
                keys_on = dev
                    .as_ref()
                    .is_some_and(|d| d.request(&commands::set_key_reports(true)).is_ok());
                last_poll = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {
                // Live key reports pushed by the controller.
                if let Some(d) = &dev {
                    while let Ok(Some(r)) = d.recv(Duration::ZERO) {
                        if let Ok(Frame::Short {
                            cmd: op::KEY_REPORT,
                            payload,
                        }) = frame::parse(&r)
                            && let Ok(k) = commands::decode_key_report(payload)
                            && last_keys != Some(k.keys)
                        {
                            last_keys = Some(k.keys);
                            emit(Event::Keys(k.keys));
                        }
                    }
                }
                if last_poll.elapsed() < Duration::from_secs(3) {
                    continue;
                }
                last_poll = Instant::now();
                // Periodic poll: battery when connected, hot-plug when not.
                match &dev {
                    Some(d) => match actions::read_power(d) {
                        Ok(p) => emit(Event::Battery(p.battery)),
                        Err(e) => {
                            dev = None;
                            keys_on = false;
                            emit(Event::Disconnected(e.to_string()));
                        }
                    },
                    None => {
                        if crate::device::list().is_ok_and(|l| !l.is_empty()) {
                            handle(Request::Refresh, &mut dev, &emit);
                            keys_on = dev.as_ref().is_some_and(|d| {
                                d.request(&commands::set_key_reports(true)).is_ok()
                            });
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if let (Some(d), true) = (&dev, keys_on) {
                    let _ = d.request(&commands::set_key_reports(false));
                }
                return;
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
                emit(Event::Applied);
                emit(Event::Snapshot(Box::new(actions::read_snapshot(d)?)));
            }
            Request::Shutdown => {}
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
