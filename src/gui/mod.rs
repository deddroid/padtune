//! Desktop GUI (egui/eframe). Thin layer over [`crate::actions`]; every
//! device operation runs on the [`worker`] thread.
//!
//! Environment variables:
//! * `BIGBIGCTL_DEMO=1` — show a built-in Rainbow 2 Pro snapshot instead of a
//!   real device (no USB access at all);
//! * `BIGBIGCTL_SCREENSHOTS=<dir>` — demo mode that saves a PNG of every tab
//!   in every language to `<dir>`, then exits.

mod app;
pub mod desktop;
pub mod live;
mod pad;
mod theme;
pub mod worker;

pub use app::App;

/// Window icon (embedded PNG).
pub const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-256.png");

fn icon() -> Option<egui::IconData> {
    let decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some(egui::IconData {
        rgba: buf,
        width: info.width,
        height: info.height,
    })
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::config::Settings;
use crate::i18n::{T, tr};
use desktop::Cmd;

/// Tray icon (embedded PNG).
pub const TRAY_PNG: &[u8] = include_bytes!("../../assets/icon-64.png");

/// State shared between the window, the tray and the single-instance listener.
#[derive(Default)]
pub struct Shell {
    /// Context of the open window, if any.
    pub ctx: Mutex<Option<egui::Context>>,
    /// Quit requested from the tray.
    pub quit: AtomicBool,
}

fn native_options() -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("BIGBIG WON Controller")
        .with_app_id("bigbigctl")
        .with_inner_size([1060.0, 720.0])
        .with_min_inner_size([820.0, 560.0]);
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    eframe::NativeOptions {
        viewport,
        ..Default::default()
    }
}

/// Runs the GUI. With the tray enabled, closing the window keeps the program
/// in the tray: the window is destroyed and created again on "Show" (Wayland
/// cannot hide a window, so this works the same everywhere).
pub fn run() -> eframe::Result {
    let hidden_arg = std::env::args().any(|a| a == "--hidden");
    let screenshots = std::env::var_os("BIGBIGCTL_SCREENSHOTS").is_some();
    let settings = Settings::load();
    let lang = settings.language;

    let (tx, rx) = mpsc::channel::<Cmd>();
    if !screenshots && !desktop::claim_single_instance(tx.clone()) {
        return Ok(()); // the running instance shows its window
    }
    let tray = !screenshots && (settings.close_to_tray || settings.start_hidden || hidden_arg);
    if tray {
        desktop::spawn_tray(
            tx.clone(),
            desktop::Labels {
                show: tr(lang, T::Show).into(),
                quit: tr(lang, T::Quit).into(),
                tooltip: tr(lang, T::AppTitle).into(),
            },
        );
    }

    // Commands arriving while the window is open act on it directly; the
    // others wake the main loop below.
    let shell = Arc::new(Shell::default());
    let (main_tx, main_rx) = mpsc::channel::<Cmd>();
    {
        let shell = shell.clone();
        std::thread::spawn(move || {
            for cmd in rx {
                let ctx = shell.ctx.lock().ok().and_then(|c| c.clone());
                match (cmd, ctx) {
                    (Cmd::Show, Some(ctx)) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    (Cmd::Quit, Some(ctx)) => {
                        shell.quit.store(true, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    (cmd, None) => {
                        if main_tx.send(cmd).is_err() {
                            return;
                        }
                    }
                }
            }
        });
    }

    let mut visible = !tray || !(hidden_arg || settings.start_hidden);
    loop {
        if visible {
            let sh = shell.clone();
            eframe::run_native(
                "bigbigctl",
                native_options(),
                Box::new(move |cc| Ok(Box::new(App::new(cc, sh)))),
            )?;
        }
        // The tray may have been switched off in the settings meanwhile.
        let keep = tray && Settings::load().close_to_tray;
        if shell.quit.load(Ordering::SeqCst) || !keep {
            return Ok(());
        }
        match main_rx.recv() {
            Ok(Cmd::Show) => visible = true,
            Ok(Cmd::Quit) | Err(_) => return Ok(()),
        }
    }
}
