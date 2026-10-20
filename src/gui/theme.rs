//! Visual style: a dark, rounded theme and fallback fonts for symbols.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontFamily, Stroke};
use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

pub const ACCENT: Color32 = Color32::from_rgb(0x7c, 0x5c, 0xff);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x2a, 0x24, 0x4a);
pub const BG: Color32 = Color32::from_rgb(0x12, 0x13, 0x18);
pub const PANEL: Color32 = Color32::from_rgb(0x19, 0x1b, 0x22);
pub const CARD: Color32 = Color32::from_rgb(0x21, 0x23, 0x2c);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0x9e, 0xb0);
pub const OK: Color32 = Color32::from_rgb(0x3d, 0xd6, 0x8c);
pub const WARN: Color32 = Color32::from_rgb(0xff, 0xb4, 0x4d);
pub const ERR: Color32 = Color32::from_rgb(0xff, 0x5c, 0x6c);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = CARD;
    v.extreme_bg_color = BG;
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    let r = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x2a, 0x2d, 0x38);
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x2a, 0x2d, 0x38);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x34, 0x37, 0x45);
    v.widgets.active.weak_bg_fill = ACCENT;
    v.window_corner_radius = CornerRadius::same(12);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.slider_width = 220.0;
        s.spacing.interact_size.y = 26.0;
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
    });
    install_fallback_fonts(ctx);
}

/// System fonts that cover arrows, dots and other symbols egui's bundled
/// fonts lack. The first one found is added as a low-priority fallback.
fn install_fallback_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSansSymbols2-Regular.ttf",
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ];
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            ctx.add_font(FontInsert::new(
                "symbols-fallback",
                FontData::from_owned(bytes),
                vec![
                    InsertFontFamily { family: FontFamily::Proportional, priority: FontPriority::Lowest },
                    InsertFontFamily { family: FontFamily::Monospace, priority: FontPriority::Lowest },
                ],
            ));
            return;
        }
    }
}

/// A rounded card container.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // The margin is added around the content, so reserve it up front.
    let width = (ui.available_width() - 32.0).max(0.0);
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(width);
            add(ui)
        })
        .inner
}

/// Small grey caption.
pub fn dim(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(TEXT_DIM).small()
}
