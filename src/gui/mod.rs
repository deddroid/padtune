//! Desktop GUI (egui/eframe). Thin layer over [`crate::actions`]; every
//! device operation runs on the [`worker`] thread.
//!
//! Environment variables:
//! * `BIGBIGCTL_DEMO=1` — show a built-in Rainbow 2 Pro snapshot instead of a
//!   real device (no USB access at all);
//! * `BIGBIGCTL_SCREENSHOTS=<dir>` — demo mode that saves a PNG of every tab
//!   in every language to `<dir>`, then exits.

mod app;
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

use eframe::egui;

/// Runs the GUI until the window is closed.
pub fn run() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("BIGBIG WON Controller")
        .with_app_id("bigbigctl")
        .with_inner_size([1060.0, 720.0])
        .with_min_inner_size([820.0, 560.0]);
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "bigbigctl",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
