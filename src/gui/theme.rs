//! Visual style: follows the system theme with two palettes ("Carta calda"
//! for light, "Viola notte" for dark), concentric corner radii, embedded UI
//! fonts (Inter for text, Rajdhani for titles) and fallback fonts for symbols.
//!
//! Colours that depend on the theme are read through the accessor functions
//! ([`bg`], [`surface`], [`accent`], …), which return the palette egui is
//! drawing with in the current frame; [`sync`] records it once per frame.
//! Status colours ([`ok`], [`warn`], [`err`]) are darker in the light theme
//! so text keeps its contrast, and are always paired with text or an icon.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Theme,
};
use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};

use crate::config::ThemeMode;

/// Font family of titles and headings (Rajdhani).
pub const TITLE_FAMILY: &str = "titoli";

/// Title font at `size`.
pub fn title(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(TITLE_FAMILY.into()))
}

/// Colour tokens of one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Window and panel background.
    pub bg: Color32,
    /// Cards, sidebar, popovers: one small step above the background.
    pub surface: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    /// Selection, active value and the one primary action per view.
    pub accent: Color32,
    /// Background of unfilled controls: slider rail, off toggle, plot areas.
    pub track: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub err: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// "Viola notte": the app's original violet identity, on the approved tokens.
pub const DARK: Palette = Palette {
    bg: rgb(0x15151c),
    surface: rgb(0x1e1e28),
    text: rgb(0xececf3),
    text_dim: rgb(0x8b8ba0),
    accent: rgb(0x8b5cf6),
    track: rgb(0x3a3a4c),
    ok: rgb(0x3dd68c),
    warn: rgb(0xffb44d),
    err: rgb(0xff5c6c),
};

/// "Carta calda": the light counterpart of [`DARK`].
pub const LIGHT: Palette = Palette {
    bg: rgb(0xf3ede3),
    surface: rgb(0xfbf8f2),
    text: rgb(0x2b2520),
    text_dim: rgb(0x857767),
    accent: rgb(0xc2410c),
    track: rgb(0xddd1bd),
    ok: rgb(0x1f8a5b),
    warn: rgb(0xb45309),
    err: rgb(0xc0392b),
};

/// Corner radius of rows, buttons and cards.
pub const ROUND: CornerRadius = CornerRadius::same(12);
/// Capsule corners for small controls, chips and values: egui clamps the
/// radius to half the height, so a large value always yields a pill.
pub const CAPSULE: CornerRadius = CornerRadius::same(64);

static DARK_ACTIVE: AtomicBool = AtomicBool::new(true);

/// Records which theme egui resolved for this frame; the system preference
/// can change while the app runs.
pub fn sync(ctx: &egui::Context) {
    DARK_ACTIVE.store(ctx.theme() == Theme::Dark, Ordering::Relaxed);
}

/// Palette of the theme being drawn.
pub fn palette() -> &'static Palette {
    if DARK_ACTIVE.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn bg() -> Color32 {
    palette().bg
}
pub fn surface() -> Color32 {
    palette().surface
}
pub fn text() -> Color32 {
    palette().text
}
pub fn text_dim() -> Color32 {
    palette().text_dim
}
pub fn accent() -> Color32 {
    palette().accent
}
pub fn track() -> Color32 {
    palette().track
}
pub fn ok() -> Color32 {
    palette().ok
}
pub fn warn() -> Color32 {
    palette().warn
}
pub fn err() -> Color32 {
    palette().err
}

/// Translucent accent for selected rows and chips (readable with [`text`]
/// on top in both themes).
pub fn accent_soft() -> Color32 {
    let a = palette().accent;
    Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 56)
}

/// Linear blend of two opaque colours.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

/// Applies the user's light/dark choice. `PADTUNE_THEME=light|dark` still
/// overrides it (screenshots and documentation).
pub fn set_mode(ctx: &egui::Context, mode: ThemeMode) {
    let mode = match std::env::var("PADTUNE_THEME").ok().as_deref() {
        Some("light") => ThemeMode::Light,
        Some("dark") => ThemeMode::Dark,
        _ => mode,
    };
    ctx.set_theme(match mode {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark => egui::ThemePreference::Dark,
    });
}

pub fn apply(ctx: &egui::Context, mode: ThemeMode) {
    set_mode(ctx, mode);
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, egui::Visuals::dark()));
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, egui::Visuals::light()));
    ctx.all_styles_mut(|s| {
        // 4/8 px grid; rows tall enough to hit (≥ 32).
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 8.0);
        s.spacing.slider_width = 220.0;
        s.spacing.interact_size.y = 32.0;
        use egui::TextStyle;
        s.text_styles.insert(TextStyle::Heading, title(26.0));
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(14.5));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
    });
    install_fonts(ctx);
    install_fallback_fonts(ctx);
    sync(ctx);
}

/// Embedded UI fonts (SIL OFL, licences in `assets/fonts/`): Inter for text
/// and controls, Rajdhani for titles. Inter ships as a variable font; egui
/// renders its default instance (Regular, optical size 14).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Inter-Variable.ttf"
        ))),
    );
    fonts.font_data.insert(
        "rajdhani".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/Rajdhani-Medium.ttf"
        ))),
    );
    let body = fonts.families.entry(FontFamily::Proportional).or_default();
    body.insert(0, "inter".into());
    // Titles fall back to the body fonts for glyphs Rajdhani lacks.
    let mut titles = vec!["rajdhani".to_owned()];
    titles.extend(body.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name(TITLE_FAMILY.into()), titles);
    ctx.set_fonts(fonts);
}

/// Maps a palette onto egui's visuals so the stock widgets follow it.
fn visuals(p: &Palette, mut v: egui::Visuals) -> egui::Visuals {
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.faint_bg_color = p.surface;
    // Slider rails and text fields sit on the track colour.
    v.extreme_bg_color = p.track;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = p.accent;
    v.slider_trailing_fill = true;
    // Concentric radii: rows/buttons 12, menus 16, window 20.
    v.window_corner_radius = CornerRadius::same(20);
    v.menu_corner_radius = CornerRadius::same(16);
    v.window_stroke = Stroke::new(1.0, p.track);
    let hover = mix(p.track, p.text, 0.12);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = ROUND;
        w.fg_stroke.color = p.text;
    }
    // Borders and separators: barely visible.
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.track);
    v.widgets.inactive.bg_fill = p.track;
    v.widgets.inactive.weak_bg_fill = p.track;
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.weak_bg_fill = p.accent;
    v.widgets.open.weak_bg_fill = hover;
    v
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
                    InsertFontFamily {
                        family: FontFamily::Proportional,
                        priority: FontPriority::Lowest,
                    },
                    InsertFontFamily {
                        family: FontFamily::Monospace,
                        priority: FontPriority::Lowest,
                    },
                ],
            ));
            return;
        }
    }
}

/// A rounded card: surface colour, radius 12, padding 12.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    const PAD: i8 = 12;
    // The margin is added around the content, so reserve it up front.
    let width = (ui.available_width() - 2.0 * f32::from(PAD)).max(0.0);
    egui::Frame::new()
        .fill(surface())
        .corner_radius(ROUND)
        .inner_margin(egui::Margin::same(PAD))
        .show(ui, |ui| {
            ui.set_width(width);
            add(ui)
        })
        .inner
}

/// Small secondary caption.
pub fn dim(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(text_dim()).small()
}
