//! Main window: sidebar navigation and one page per feature.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Color32, Layout, RichText, Stroke, Vec2};

use super::live::{self, LiveState};
use super::pad::{self, Marks};
use super::theme::{self, ACCENT, ERR, OK, TEXT_DIM, WARN, card, dim};
use super::worker::{Event, Request, Worker};
use crate::actions::Snapshot;
use crate::config::{self, DeviceConfig, NamedProfile, Settings};
use crate::i18n::{Lang, T, tr};
use crate::protocol::commands::{POLLING_CODES, polling_hz};
use crate::protocol::keys::{self, KEYBOARD, MODIFIERS};
use crate::protocol::macros::{Macro, Step as MacroStep};
use crate::protocol::profile::{Curve, KeyMap, Profile, SLOT_NAMES, TURBO_SPEEDS_HZ, Trigger};
use crate::protocol::profile::{NO_KEY, VIBRATION_MOTORS, Vibration};
use crate::protocol::strip::{self, Effect, Strip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Device,
    Buttons,
    Sticks,
    Triggers,
    Lights,
    Vibration,
    Macros,
    Profiles,
    Settings,
}

impl Tab {
    const ALL: [Tab; 9] = [
        Tab::Device,
        Tab::Buttons,
        Tab::Sticks,
        Tab::Triggers,
        Tab::Lights,
        Tab::Vibration,
        Tab::Macros,
        Tab::Profiles,
        Tab::Settings,
    ];

    fn label(self) -> T {
        match self {
            Tab::Device => T::TabDevice,
            Tab::Buttons => T::TabButtons,
            Tab::Sticks => T::TabSticks,
            Tab::Triggers => T::TabTriggers,
            Tab::Lights => T::TabLights,
            Tab::Vibration => T::Vibration,
            Tab::Macros => T::TabMacros,
            Tab::Profiles => T::TabProfiles,
            Tab::Settings => T::TabSettings,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Tab::Device => "▣",
            Tab::Buttons => "⌨",
            Tab::Sticks => "◎",
            Tab::Triggers => "◭",
            Tab::Lights => "☀",
            Tab::Vibration => "≈",
            Tab::Macros => "⟲",
            Tab::Profiles => "☰",
            Tab::Settings => "⚙",
        }
    }

    fn slug(self) -> &'static str {
        match self {
            Tab::Device => "device",
            Tab::Buttons => "buttons",
            Tab::Sticks => "sticks",
            Tab::Triggers => "triggers",
            Tab::Lights => "lights",
            Tab::Vibration => "vibration",
            Tab::Macros => "macros",
            Tab::Profiles => "profiles",
            Tab::Settings => "settings",
        }
    }
}

/// Editable copy of what the controller holds.
#[derive(Debug, Clone, PartialEq)]
struct Edit {
    profile: Option<Profile>,
    polling_code: Option<u8>,
    led_brightness: Option<u8>,
    logo_colors: Option<[[u8; 3]; 3]>,
    macros: Vec<Macro>,
    strip: Option<Strip>,
}

impl Edit {
    fn from_snapshot(s: &Snapshot) -> Self {
        Self {
            profile: s.profile.clone(),
            polling_code: s.info.polling_code,
            led_brightness: s.led_brightness,
            logo_colors: s.logo_colors,
            macros: s.macros.clone(),
            strip: s.strip.clone(),
        }
    }

    fn config(&self) -> DeviceConfig {
        DeviceConfig {
            profile: self.profile.as_ref().map(|p| config::to_hex(&p.to_bytes())),
            polling_code: self.polling_code,
            led_brightness: self.led_brightness,
            logo_colors: self.logo_colors,
            macros: self
                .macros
                .iter()
                .filter_map(|m| m.encode().ok().map(|r| (m.slot, config::to_hex(&r))))
                .collect(),
            strip: self.strip.clone(),
        }
    }

    fn apply_config(&mut self, c: &DeviceConfig) {
        if let Some(Ok(p)) = c.profile() {
            self.profile = Some(p);
        }
        if c.polling_code.is_some() {
            self.polling_code = c.polling_code;
        }
        if c.led_brightness.is_some() {
            self.led_brightness = c.led_brightness;
        }
        if c.logo_colors.is_some() {
            self.logo_colors = c.logo_colors;
        }
        if c.strip.is_some() {
            self.strip = c.strip.clone();
        }
        if let Ok(records) = c.macro_records() {
            for m in records {
                match self.macros.iter_mut().find(|x| x.slot == m.slot) {
                    Some(x) => *x = m,
                    None => self.macros.push(m),
                }
            }
        }
    }
}

enum Status {
    None,
    Info(String, Instant),
    Error(String),
}

/// Automatic screenshot run (`PADTUNE_SCREENSHOTS`).
struct ScreenshotRun {
    dir: PathBuf,
    steps: Vec<(Lang, Tab)>,
    index: usize,
    frames: u32,
    pending: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

pub struct App {
    worker: Option<Worker>,
    settings: Settings,
    tab: Tab,
    snap: Option<Snapshot>,
    edit: Option<Edit>,
    busy: bool,
    connected: bool,
    status: Status,
    stick_right: bool,
    curve_set: usize,
    profiles: Vec<NamedProfile>,
    new_profile_name: String,
    shots: Option<ScreenshotRun>,
    live: Option<live::Live>,
    live_now: LiveState,
    selected_slot: usize,
    /// Buttons from the controller's own key report (covers every button).
    vendor_keys: Option<u32>,
    /// Key test active (controller key reports on; gamepad output paused).
    key_test: bool,
    /// When each button was last seen pressed, so quick taps stay visible.
    pressed_at: [Option<Instant>; 32],
    idle_dongle: bool,
    macro_slot: usize,
    shell: std::sync::Arc<super::Shell>,
    /// Rumble to play once the vibration preview write is acknowledged.
    pending_rumble: Option<(u16, u16)>,
    /// A vibration-only preview write is in flight: keep the other edits.
    preview_inflight: bool,
}

fn demo_snapshot() -> Snapshot {
    use crate::actions::DeviceInfo;
    use crate::protocol::commands::{Mode, Power};
    let profile = Profile::from_bytes(&crate::protocol::profile::RAINBOW2PRO_FACTORY).ok();
    Snapshot {
        info: DeviceInfo {
            path: "demo".into(),
            vid: 0x413d,
            pid: 0x2104,
            product: "XBOX 360 For Windows".into(),
            mode: Some(Mode {
                prefix: [0x56, 0x51, 0x01],
                marker: "ZJ-C1pro".into(),
            }),
            zkm_version: Some(b'6'),
            uuid: Some("2220B21AFF020608".into()),
            profile_size: Some(280),
            polling_code: Some(0),
        },
        profile,
        led_brightness: Some(255),
        logo_colors: Some([[0x10, 0xd0, 0x40], [0xff, 0x30, 0x30], [0x30, 0x60, 0xff]]),
        power: Some(Power {
            battery: 100,
            status: 0,
        }),
        macros: {
            let mut m: Vec<Macro> = (0..4).map(Macro::empty).collect();
            m[0].run_key = 23;
            m[0].steps = vec![
                MacroStep {
                    duration_ms: 160,
                    marker: false,
                    inputs: 1,
                    analog: [0; 4],
                },
                MacroStep {
                    duration_ms: 400,
                    marker: false,
                    inputs: 0,
                    analog: [0; 4],
                },
                MacroStep {
                    duration_ms: 160,
                    marker: false,
                    inputs: 1 << 1,
                    analog: [0; 4],
                },
            ];
            m
        },
        strip: Some(Strip::default()),
        wireless_flag: Some(1),
        connection: Some(crate::actions::Link::Usb),
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shell: std::sync::Arc<super::Shell>) -> Self {
        if let Ok(mut c) = shell.ctx.lock() {
            *c = Some(cc.egui_ctx.clone());
        }
        theme::apply(&cc.egui_ctx);
        let settings = Settings::load();
        let shots = std::env::var_os("PADTUNE_SCREENSHOTS").map(|d| {
            let dir = PathBuf::from(d);
            let _ = std::fs::create_dir_all(&dir);
            let steps = Lang::ALL
                .iter()
                .flat_map(|l| Tab::ALL.iter().map(move |t| (*l, *t)))
                .collect();
            ScreenshotRun {
                dir,
                steps,
                index: 0,
                frames: 0,
                pending: Default::default(),
            }
        });
        let demo = shots.is_some() || std::env::var_os("PADTUNE_DEMO").is_some_and(|v| v != "0");
        let mut app = Self {
            worker: None,
            settings,
            tab: Tab::Device,
            snap: None,
            edit: None,
            busy: false,
            connected: false,
            status: Status::None,
            stick_right: false,
            curve_set: 0,
            profiles: config::list_profiles(),
            new_profile_name: String::new(),
            shots,
            live: None,
            live_now: LiveState::default(),
            selected_slot: 0,
            vendor_keys: None,
            key_test: false,
            pressed_at: [None; 32],
            idle_dongle: false,
            macro_slot: 0,
            shell,
            pending_rumble: None,
            preview_inflight: false,
        };
        if demo {
            app.set_snapshot(demo_snapshot());
            // Plausible live input so screenshots show the live widgets.
            app.live_now = LiveState {
                connected: true,
                name: "demo".into(),
                left: (-0.35, 0.55),
                right: (0.62, -0.18),
                lt: 0.42,
                rt: 0.0,
                pressed: 1 << 0,
                events_per_sec: 0.0,
            };
        } else {
            let ctx = cc.egui_ctx.clone();
            app.worker = Some(Worker::spawn(move || ctx.request_repaint()));
            let ctx = cc.egui_ctx.clone();
            app.live = Some(live::spawn(move || ctx.request_repaint()));
        }
        app
    }

    fn lang(&self) -> Lang {
        self.settings.language
    }

    fn t(&self, key: T) -> &'static str {
        tr(self.lang(), key)
    }

    fn set_snapshot(&mut self, s: Snapshot) {
        self.connected = true;
        self.edit = Some(Edit::from_snapshot(&s));
        self.snap = Some(s);
    }

    fn dirty(&self) -> bool {
        match (&self.snap, &self.edit) {
            (Some(s), Some(e)) => e.config() != s.config(),
            _ => false,
        }
    }

    fn send(&self, r: Request) {
        if let Some(w) = &self.worker {
            w.send(r);
        }
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.status = Status::Info(msg.into(), Instant::now());
    }

    fn poll_worker(&mut self) {
        let Some(w) = &self.worker else { return };
        let events: Vec<Event> = w.rx.try_iter().collect();
        for ev in events {
            match ev {
                Event::Busy(b) => self.busy = b,
                Event::Snapshot(s) if self.preview_inflight => {
                    // Vibration preview: the device now holds the edited
                    // vibration; keep every other unapplied edit.
                    self.preview_inflight = false;
                    self.snap = Some(*s);
                }
                Event::Snapshot(s) => {
                    // Keep unapplied edits only if the device did not change underneath.
                    let keep = self.dirty()
                        && self.snap.as_ref().is_some_and(|o| o.config() == s.config());
                    if keep {
                        self.snap = Some(*s);
                        self.connected = true;
                    } else {
                        self.set_snapshot(*s);
                    }
                }
                Event::IdleDongle(p) => self.idle_dongle = p,
                Event::Keys(k) => {
                    for (bit, at) in self.pressed_at.iter_mut().enumerate() {
                        if k & (1 << bit) != 0 {
                            *at = Some(Instant::now());
                        }
                    }
                    self.vendor_keys = Some(k);
                }
                Event::Battery(b) => {
                    if let Some(p) = self.snap.as_mut().and_then(|s| s.power.as_mut()) {
                        p.battery = b;
                    }
                }
                Event::Disconnected(why) => {
                    let was = self.connected;
                    self.connected = false;
                    if was || why.contains("denied") || why.contains("Permission") {
                        self.status = Status::Error(if why.contains("ermission") {
                            self.t(T::UdevHint).to_string()
                        } else {
                            why
                        });
                    }
                }
                Event::Applied if self.preview_inflight => {
                    if let (Some((strong, weak)), Some(l)) =
                        (self.pending_rumble.take(), &self.live)
                    {
                        l.rumble(strong, weak);
                    }
                }
                Event::Applied => {
                    self.info(self.t(T::Done));
                }
                Event::Message(m) => self.info(format!("{}: {m}", self.t(T::BackupDone))),
                Event::Error(e) => self.status = Status::Error(e),
            }
        }
    }

    // ---------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new("PadTune")
                    .strong()
                    .size(18.0)
                    .color(Color32::WHITE),
            );
        });
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let model = self
                .snap
                .as_ref()
                .and_then(|s| s.info.mode.as_ref())
                .map(|m| model_name(&m.marker))
                .unwrap_or("—");
            ui.label(dim(model));
        });
        ui.add_space(14.0);
        for tab in Tab::ALL {
            let selected = self.tab == tab;
            let text =
                RichText::new(format!("{}   {}", tab.icon(), self.t(tab.label()))).size(15.0);
            let btn = egui::Button::new(if selected {
                text.color(Color32::WHITE)
            } else {
                text.color(TEXT_DIM)
            })
            .fill(if selected {
                theme::ACCENT_SOFT
            } else {
                Color32::TRANSPARENT
            })
            .stroke(if selected {
                Stroke::new(1.0, ACCENT)
            } else {
                Stroke::NONE
            })
            .min_size(Vec2::new(ui.available_width(), 36.0));
            if ui.add(btn).clicked() {
                self.tab = tab;
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(12.0);
            let conn = self.connection_label();
            let (color, text) = if self.busy {
                (WARN, self.t(T::Busy).to_string())
            } else if self.connected {
                (OK, conn)
            } else {
                (ERR, self.t(T::Disconnected).to_string())
            };
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("●").color(color));
                ui.label(RichText::new(text).color(TEXT_DIM));
            });
            if let Some(b) = self.snap.as_ref().and_then(|s| s.power).map(|p| p.battery) {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.add(
                        egui::ProgressBar::new(f32::from(b) / 100.0)
                            .desired_width(ui.available_width() - 12.0)
                            .text(format!("{} {b}%", self.t(T::Battery))),
                    );
                });
            }
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(self.t(self.tab.label()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let dirty = self.dirty();
                let can = dirty && self.connected && !self.busy && self.worker.is_some();
                let apply =
                    egui::Button::new(RichText::new(self.t(T::Apply)).color(Color32::WHITE))
                        .fill(if can { ACCENT } else { Color32::from_gray(60) });
                if ui.add_enabled(can, apply).clicked()
                    && let (Some(e), Some(s)) = (&self.edit, &self.snap)
                {
                    let delta = e.config().changes_from(&s.config());
                    self.send(Request::Apply(delta));
                }
                if ui
                    .add_enabled(dirty, egui::Button::new(self.t(T::Revert)))
                    .clicked()
                    && let Some(s) = &self.snap
                {
                    self.edit = Some(Edit::from_snapshot(s));
                }
                if ui
                    .add_enabled(
                        !self.busy,
                        egui::Button::new(format!("⟳ {}", self.t(T::Refresh))),
                    )
                    .clicked()
                {
                    self.send(Request::Refresh);
                }
                if dirty {
                    ui.label(RichText::new(format!("● {}", self.t(T::Unsaved))).color(WARN));
                }
            });
        });
        match &self.status {
            Status::None => {}
            Status::Info(m, at) => {
                if at.elapsed() < Duration::from_secs(5) {
                    ui.label(RichText::new(m).color(OK));
                }
            }
            Status::Error(m) => {
                let text = format!("{}: {m}", self.t(T::Error));
                let clear = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new(text).color(ERR));
                        ui.small_button("✕").clicked()
                    })
                    .inner;
                if clear {
                    self.status = Status::None;
                }
            }
        }
        ui.add_space(6.0);
    }

    // ---- pages -----------------------------------------------------------

    fn page_device(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let marker = self.marker();
        let Some(s) = self.snap.clone() else {
            card(ui, |ui| {
                ui.label(RichText::new(tr(lang, T::Disconnected)).size(18.0));
                ui.label(dim(tr(lang, T::UdevHint)));
            });
            return;
        };
        card(ui, |ui| {
            egui::Grid::new("info")
                .num_columns(2)
                .spacing([40.0, 10.0])
                .show(ui, |ui| {
                    let marker = s
                        .info
                        .mode
                        .as_ref()
                        .map(|m| m.marker.clone())
                        .unwrap_or_default();
                    row(
                        ui,
                        tr(lang, T::Model),
                        &format!("{} ({marker})", model_name(&marker)),
                    );
                    row(
                        ui,
                        tr(lang, T::Firmware),
                        &s.info
                            .zkm_version
                            .map(|v| format!("ZKM {}", v as char))
                            .unwrap_or("—".into()),
                    );
                    row(ui, tr(lang, T::Uuid), s.info.uuid.as_deref().unwrap_or("—"));
                    row(
                        ui,
                        tr(lang, T::ProfileFormat),
                        &s.profile
                            .as_ref()
                            .map(|p| p.layout().name.to_string())
                            .unwrap_or("—".into()),
                    );
                    row(ui, "USB", &format!("{:04x}:{:04x}", s.info.vid, s.info.pid));
                });
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new(tr(lang, T::Connections)).strong());
            ui.add_space(4.0);
            egui::Grid::new("links")
                .num_columns(3)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    for (link, label) in [
                        (crate::actions::Link::Usb, T::Wired),
                        (crate::actions::Link::Dongle, T::Wireless),
                    ] {
                        let active = s.connection == Some(link);
                        let idle = link == crate::actions::Link::Dongle && self.idle_dongle;
                        ui.label(RichText::new("●").color(if active {
                            OK
                        } else {
                            Color32::from_gray(70)
                        }));
                        ui.label(RichText::new(tr(lang, label)).strong());
                        if active {
                            let hz = s
                                .info
                                .polling_code
                                .and_then(polling_hz)
                                .map(|h| format!(" · {h} Hz"))
                                .unwrap_or_default();
                            ui.label(
                                RichText::new(format!("{}{hz}", tr(lang, T::Active))).color(OK),
                            );
                        } else if idle {
                            ui.label(RichText::new(tr(lang, T::DonglePresent)).color(WARN));
                        } else {
                            ui.label(dim(tr(lang, T::NotConnected)));
                        }
                        ui.end_row();
                    }
                });
        });
        ui.add_space(10.0);
        let Some(edit) = &mut self.edit else { return };
        card(ui, |ui| {
            ui.label(RichText::new(tr(lang, T::PollingRate)).strong());
            ui.label(dim(tr(lang, T::PollingHint)));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // USB full-speed devices poll at most every 1 ms: 2000 Hz is
                // only offered for models that can actually do it.
                for (code, hz) in POLLING_CODES
                    .into_iter()
                    .filter(|(_, hz)| *hz <= max_polling_hz(&marker))
                {
                    let sel = edit.polling_code == Some(code);
                    if ui.selectable_label(sel, format!("{hz} Hz")).clicked() {
                        edit.polling_code = Some(code);
                    }
                }
            });
            if let Some(c) = s.info.polling_code {
                ui.label(dim(format!("→ {} Hz", polling_hz(c).unwrap_or(0))));
            }
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.worker.is_some(),
                        egui::Button::new(tr(lang, T::Backup)),
                    )
                    .clicked()
                {
                    let name = format!(
                        "padtune-backup-{}.bin",
                        s.info.uuid.clone().unwrap_or_else(|| "controller".into())
                    );
                    let path = dirs::document_dir()
                        .unwrap_or_else(config::config_dir)
                        .join(name);
                    self.send(Request::Backup(path));
                }
                if ui.button(tr(lang, T::FactoryResetAll)).clicked()
                    && let Some(e) = &mut self.edit
                {
                    e.apply_config(&DeviceConfig::factory());
                    self.status =
                        Status::Info(tr(lang, T::FactoryResetHint).to_string(), Instant::now());
                }
            });
        });
    }

    fn page_buttons(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let model = self.marker();
        let mut live = self.live_now.clone();
        if self.key_test {
            // Controller reports cover every button; keep quick taps lit briefly.
            let hold = Duration::from_millis(250);
            let latched = self
                .pressed_at
                .iter()
                .enumerate()
                .filter(|(_, t)| t.is_some_and(|t| t.elapsed() < hold))
                .fold(0u32, |m, (b, _)| m | (1 << b));
            live.pressed = self.vendor_keys.unwrap_or(0) | latched;
            if latched != 0 {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
        }
        let key_test_before = self.key_test;
        let self_link = self.snap.as_ref().and_then(|s| s.connection);
        let mut key_test = self.key_test;
        let mut selected = self.selected_slot;
        let Some(lights) = self.edit.as_ref().map(|e| light_marks(e, false)) else {
            return no_profile(ui, lang);
        };
        let Some(profile) = self.edit.as_mut().and_then(|e| e.profile.as_mut()) else {
            return no_profile(ui, lang);
        };
        let slots = visible_slots(&model);
        let remapped = slots
            .iter()
            .filter(|&&i| profile.key(i) != KeyMap::Default)
            .fold(0u32, |m, &i| m | (1 << i));
        let marks = Marks {
            turbo: profile.turbo_mask(),
            remapped,
            ..lights
        };
        // Side by side when there is room, otherwise the editor goes below
        // the drawing (never cut off on the right).
        let wide = ui.available_width() >= 760.0;
        let editor_w = if wide { 300.0 } else { ui.available_width() };
        let pad_w = if wide {
            (ui.available_width() - editor_w - 40.0).max(320.0)
        } else {
            ui.available_width()
        };
        let layout = if wide {
            Layout::left_to_right(Align::Min)
        } else {
            Layout::top_down(Align::Min)
        };
        ui.with_layout(layout, |ui| {
            ui.vertical(|ui| {
                ui.set_max_width(pad_w);
                card(ui, |ui| {
                    let on_dongle = self_link == Some(crate::actions::Link::Dongle);
                    if on_dongle {
                        key_test = false;
                    }
                    // Long hints go on their own wrapping line so they never
                    // widen the card and push the editor out of view.
                    if on_dongle {
                        ui.add(egui::Label::new(dim(tr(lang, T::KeyTestCable))).wrap());
                    } else {
                        ui.toggle_value(
                            &mut key_test,
                            RichText::new(format!("⚡ {}", tr(lang, T::KeyTest))).strong(),
                        );
                        let hint = if key_test {
                            T::KeyTestOn
                        } else {
                            T::KeyTestOff
                        };
                        ui.add(egui::Label::new(dim(tr(lang, hint))).wrap());
                    }
                    if let Some(slot) = pad::pad(ui, Some(selected), &marks, &live) {
                        selected = slot;
                    }
                    ui.label(dim(tr(lang, T::PadHint)));
                })
            });
            ui.vertical(|ui| {
                ui.set_max_width(editor_w);
                card(ui, |ui| {
                    let name = SLOT_NAMES[selected].unwrap_or("?");
                    ui.label(RichText::new(name).size(24.0).strong());
                    ui.add_space(6.0);
                    if let Some(f) = FEATURES.iter().find(|f| f.key_slot == Some(selected)) {
                        ui.label(RichText::new(tr(lang, T::FunctionKey)).color(WARN));
                        ui.add_space(6.0);
                        feature_body(ui, lang, f);
                        return;
                    }
                    ui.label(dim(tr(lang, T::Action)));
                    let mut map = profile.key(selected);
                    if keymap_editor(ui, lang, selected, &mut map, &slots) {
                        let _ = profile.set_key(selected, map);
                    }
                    ui.add_space(12.0);
                    ui.separator();
                    let mut mask = profile.turbo_mask();
                    let mut on = mask & (1 << selected) != 0;
                    let label =
                        format!("{}  {}", tr(lang, T::Turbo), if on { "ON" } else { "OFF" });
                    let btn =
                        egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
                            .fill(if on {
                                WARN.gamma_multiply(0.8)
                            } else {
                                Color32::from_gray(55)
                            })
                            .min_size(Vec2::new(ui.available_width(), 36.0));
                    if ui.add(btn).clicked() {
                        on = !on;
                        mask = if on {
                            mask | (1 << selected)
                        } else {
                            mask & !(1 << selected)
                        };
                        profile.set_turbo_mask(mask);
                    }
                    ui.add_space(8.0);
                    ui.label(dim(tr(lang, T::TurboSpeed)));
                    ui.horizontal(|ui| {
                        let cur = profile.turbo_speed();
                        for (i, hz) in TURBO_SPEEDS_HZ.iter().enumerate() {
                            if ui
                                .selectable_label(usize::from(cur) == i, format!("{hz} Hz"))
                                .clicked()
                            {
                                let _ = profile.set_turbo_speed(i as u8);
                            }
                        }
                    });
                    ui.label(dim(tr(lang, T::TurboHint)));
                })
            });
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new(tr(lang, T::Functions)).strong().size(17.0));
            ui.label(dim(tr(lang, T::FunctionsHint)));
            ui.add_space(6.0);
            let cols = if ui.available_width() > 700.0 { 3 } else { 2 };
            let col_w =
                ((ui.available_width() - 14.0 * (cols as f32 - 1.0)) / cols as f32).max(160.0);
            for row in FEATURES.chunks(cols) {
                ui.horizontal_top(|ui| {
                    for f in row {
                        egui::Frame::new()
                            .fill(theme::BG)
                            .corner_radius(egui::CornerRadius::same(10))
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_width((col_w - 24.0).max(120.0));
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(tr(lang, f.title)).strong());
                                    feature_body(ui, lang, f);
                                });
                            });
                    }
                });
                ui.add_space(8.0);
            }
        });
        self.selected_slot = selected;
        if key_test != key_test_before {
            self.set_key_test(key_test);
        }
    }

    fn set_key_test(&mut self, on: bool) {
        self.key_test = on;
        self.vendor_keys = None;
        self.pressed_at = [None; 32];
        self.send(Request::KeyTest(on));
    }

    fn page_sticks(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let (mut right, mut set) = (self.stick_right, self.curve_set);
        let user_presets: [Option<Curve>; 3] = std::array::from_fn(|i| {
            self.settings
                .curve_presets
                .get(&(i + 1).to_string())
                .copied()
        });
        let mut save_slot: Option<usize> = None;
        let mut copy_to_other = false;
        let live = self.live_now.clone();
        let Some(profile) = self.edit.as_mut().and_then(|e| e.profile.as_mut()) else {
            return no_profile(ui, lang);
        };
        ui.horizontal(|ui| {
            ui.selectable_value(&mut right, false, tr(lang, T::LeftStick));
            ui.selectable_value(&mut right, true, tr(lang, T::RightStick));
            ui.separator();
            ui.label(tr(lang, T::CurveSet));
            for i in 0..3 {
                ui.selectable_value(&mut set, i, format!("{}", i + 1));
            }
        });
        self.stick_right = right;
        self.curve_set = set;
        let index = set * 2 + usize::from(right);
        let mut c = profile.curve(index);
        let before = c;
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                let pos = if right { live.right } else { live.left };
                let mag = live.connected.then(|| pos.0.hypot(pos.1).min(1.0));
                curve_plot(
                    ui,
                    &c,
                    mag,
                    (tr(lang, T::AxisInput), tr(lang, T::AxisOutput)),
                );
                ui.label(dim(tr(lang, T::CurveExplain)));
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(dim(tr(lang, T::Presets)));
                    if ui.small_button(tr(lang, T::FactoryReset)).clicked() {
                        // Factory curve of the Rainbow 2 Pro (read from a new unit).
                        c.deadzone = 12;
                        c.center_comp = 0;
                        c.point_a = (30, 30);
                        c.point_b = (70, 70);
                        c.outer = 95;
                        c.max_out = 100;
                    }
                    for (label, a, b) in [
                        (T::PresetLinear, (30, 30), (70, 70)),
                        (T::PresetDelayed, (30, 30), (70, 50)),
                        (T::PresetPerformance, (30, 30), (70, 80)),
                        (T::PresetInstant, (30, 30), (30, 40)),
                    ] {
                        if ui.small_button(tr(lang, label)).clicked() {
                            c.point_a = a;
                            c.point_b = b;
                        }
                    }
                });
                ui.add_space(6.0);
                // Three presets of the user's own, kept in the app settings.
                ui.horizontal_wrapped(|ui| {
                    ui.label(dim(tr(lang, T::MyPresets)));
                    for (i, slot) in user_presets.iter().enumerate() {
                        let b = ui.add_enabled(
                            slot.is_some(),
                            egui::Button::new(format!("{} {}", tr(lang, T::PresetSlot), i + 1))
                                .small(),
                        );
                        if b.clicked()
                            && let Some(p) = slot
                        {
                            c = *p;
                        }
                        if ui
                            .small_button("✎")
                            .on_hover_text(tr(lang, T::SavePreset))
                            .clicked()
                        {
                            save_slot = Some(i);
                        }
                    }
                });
                ui.add_space(6.0);
                if ui.button(tr(lang, T::CopyToOtherStick)).clicked() {
                    copy_to_other = true;
                }
            });
            card(&mut cols[1], |ui| {
                ui.spacing_mut().slider_width = (ui.available_width() - 190.0).max(80.0);
                egui::Grid::new("curve")
                    .num_columns(2)
                    .spacing([12.0, 10.0])
                    .show(ui, |ui| {
                        let s = |ui: &mut egui::Ui, label: String, v: &mut u8| {
                            ui.label(RichText::new(label).color(TEXT_DIM));
                            ui.add(egui::Slider::new(v, 0..=100).suffix("%"));
                            ui.end_row();
                        };
                        s(ui, tr(lang, T::Deadzone).into(), &mut c.deadzone);
                        s(ui, tr(lang, T::AntiDeadzone).into(), &mut c.center_comp);
                        s(ui, tr(lang, T::OuterLimit).into(), &mut c.outer);
                        s(ui, tr(lang, T::MaxOutput).into(), &mut c.max_out);
                        s(ui, format!("{} · X", tr(lang, T::PointA)), &mut c.point_a.0);
                        s(ui, format!("{} · Y", tr(lang, T::PointA)), &mut c.point_a.1);
                        s(ui, format!("{} · X", tr(lang, T::PointB)), &mut c.point_b.0);
                        s(ui, format!("{} · Y", tr(lang, T::PointB)), &mut c.point_b.1);
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let pos = if right { live.right } else { live.left };
                    pad::stick_view(ui, pos, c.deadzone, c.outer, live.connected);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(tr(lang, T::LiveInput)).strong());
                        if live.connected {
                            ui.label(dim(format!("X {:+.2}  Y {:+.2}", pos.0, pos.1)));
                        } else {
                            ui.label(dim(tr(lang, T::LiveUnavailable)));
                        }
                    });
                });
            });
        });
        if c != before {
            // Keep the curve valid while dragging instead of refusing the edit.
            c.point_b.0 = c.point_b.0.max(c.point_a.0);
            c.point_b.1 = c.point_b.1.max(c.point_a.1);
            // The dead zone must stay below the outer limit.
            c.deadzone = c.deadzone.min(99);
            c.outer = c.outer.max(c.deadzone + 1).min(100);
            if profile.set_curve(index, c).is_err() {
                self.status = Status::Error("invalid curve".into());
            }
        }
        if copy_to_other {
            let other = set * 2 + usize::from(!right);
            let _ = profile.set_curve(other, profile.curve(index));
        }
        if let Some(i) = save_slot {
            self.settings
                .curve_presets
                .insert((i + 1).to_string(), profile.curve(index));
            if self.shots.is_none() {
                let _ = self.settings.save();
            }
        }
    }

    fn page_triggers(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let live = self.live_now.clone();
        let Some(profile) = self.edit.as_mut().and_then(|e| e.profile.as_mut()) else {
            return no_profile(ui, lang);
        };
        ui.columns(2, |cols| {
            for (i, (right, title)) in [(false, T::LeftTrigger), (true, T::RightTrigger)]
                .into_iter()
                .enumerate()
            {
                let ui = &mut cols[i];
                card(ui, |ui| {
                    ui.label(RichText::new(tr(lang, title)).strong());
                    let mut t: Trigger = profile.trigger(right);
                    let before = t;
                    let value = if right { live.rt } else { live.lt };
                    trigger_bar(ui, &t, live.connected.then_some(value));
                    ui.spacing_mut().slider_width = (ui.available_width() - 190.0).max(80.0);
                    egui::Grid::new(("trig", right))
                        .num_columns(2)
                        .spacing([12.0, 10.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new(tr(lang, T::TriggerStart)).color(TEXT_DIM));
                            ui.add(egui::Slider::new(&mut t.start, 0..=99).suffix("%"));
                            ui.end_row();
                            ui.label(RichText::new(tr(lang, T::TriggerEnd)).color(TEXT_DIM));
                            ui.add(egui::Slider::new(&mut t.end, 1..=100).suffix("%"));
                            ui.end_row();
                        });
                    ui.checkbox(&mut t.hair, tr(lang, T::HairTrigger));
                    if t != before {
                        t.end = t.end.max(t.start + 1);
                        let _ = profile.set_trigger(right, t);
                    }
                });
            }
        });
    }

    fn page_vibration(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let mut preview: Option<(u16, u16)> = None;
        let Some(profile) = self.edit.as_mut().and_then(|e| e.profile.as_mut()) else {
            return no_profile(ui, lang);
        };
        card(ui, |ui| {
            ui.label(dim(tr(lang, T::VibHint)));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr(lang, T::Presets)).color(TEXT_DIM));
                for (label, v) in [
                    (T::VibOff, Vibration::OFF),
                    (T::VibWeak, Vibration { min: 50, max: 100 }),
                    (T::VibStandard, Vibration { min: 50, max: 150 }),
                    (T::VibStrong, Vibration { min: 50, max: 200 }),
                    (T::FactoryReset, Vibration::FACTORY),
                ] {
                    if ui.button(tr(lang, label)).clicked() {
                        for m in 0..VIBRATION_MOTORS {
                            let _ = profile.set_vibration(m, v);
                        }
                        preview = Some((u16::MAX, u16::MAX));
                    }
                }
            });
        });
        ui.add_space(10.0);
        // Laid out like the controller: triggers on top, grips below.
        // Motor index, name, and the OS rumble that reaches it (strong = left
        // grip, weak = right grip; trigger motors are not reachable).
        let rows = [
            [
                (2usize, T::VibMotor3, Some((u16::MAX, u16::MAX))),
                (3, T::VibMotor4, Some((u16::MAX, u16::MAX))),
            ],
            [
                (0, T::VibGripL, Some((u16::MAX, 0u16))),
                (1, T::VibGripR, Some((0u16, u16::MAX))),
            ],
        ];
        let mut motor_card =
            |ui: &mut egui::Ui, (m, name, rumble): (usize, T, Option<(u16, u16)>)| {
                card(ui, |ui| {
                    ui.label(RichText::new(tr(lang, name)).strong());
                    if m >= 2 {
                        ui.label(dim(tr(lang, T::VibNoTest)));
                    }
                    let mut v = profile.vibration(m);
                    let before = v;
                    ui.spacing_mut().slider_width = (ui.available_width() - 150.0).max(60.0);
                    let slider = |ui: &mut egui::Ui, label: T, val: &mut u8| {
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [70.0, 20.0],
                                egui::Label::new(RichText::new(tr(lang, label)).color(TEXT_DIM)),
                            );
                            ui.add(egui::Slider::new(val, 0..=255))
                        })
                        .inner
                    };
                    let rmin = slider(ui, T::VibMin, &mut v.min);
                    let rmax = slider(ui, T::VibMax, &mut v.max);
                    if v != before {
                        // The controller needs max > min: push the other end.
                        if v.min != before.min {
                            v.min = v.min.min(254);
                            v.max = v.max.max(v.min + 1);
                        } else {
                            v.max = v.max.max(1);
                            v.min = v.min.min(v.max - 1);
                        }
                        let _ = profile.set_vibration(m, v);
                    }
                    // Preview when a slider is clicked or released: write the
                    // vibration, then rumble at the weakest (min) or strongest
                    // (max) game level on this motor only.
                    if let Some((s, w)) = rumble {
                        let level = |full: u16, top: bool| match (full, top) {
                            (0, _) => 0,
                            (_, true) => u16::MAX,
                            (_, false) => 300,
                        };
                        if rmax.drag_stopped() || rmax.clicked() {
                            preview = Some((level(s, true), level(w, true)));
                        } else if rmin.drag_stopped() || rmin.clicked() {
                            preview = Some((level(s, false), level(w, false)));
                        }
                    }
                });
            };
        let wide = ui.available_width() >= 700.0;
        for row in rows {
            if wide {
                ui.columns(2, |cols| {
                    for (col, motor) in cols.iter_mut().zip(row) {
                        motor_card(col, motor);
                    }
                });
            } else {
                for motor in row {
                    motor_card(ui, motor);
                    ui.add_space(8.0);
                }
            }
            ui.add_space(8.0);
        }
        if let Some(r) = preview {
            self.preview_vibration(r);
        }
    }

    /// Writes only the edited vibration to the controller, then rumbles.
    fn preview_vibration(&mut self, rumble: (u16, u16)) {
        let (Some(snap), Some(edit)) = (&self.snap, &self.edit) else {
            return;
        };
        let (Some(current), Some(edited)) = (&snap.profile, &edit.profile) else {
            return;
        };
        let mut p = current.clone();
        for m in 0..VIBRATION_MOTORS {
            let _ = p.set_vibration(m, edited.vibration(m));
        }
        if p == *current {
            if let Some(l) = &self.live {
                l.rumble(rumble.0, rumble.1);
            }
            return;
        }
        let delta = DeviceConfig {
            profile: Some(config::to_hex(&p.to_bytes())),
            ..Default::default()
        };
        self.pending_rumble = Some(rumble);
        self.preview_inflight = true;
        self.send(Request::Apply(delta));
    }

    fn page_lights(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let live = self.live_now.clone();
        let Some(edit) = &mut self.edit else {
            return no_profile(ui, lang);
        };
        card(ui, |ui| {
            let marks = light_marks(edit, true);
            ui.vertical_centered(|ui| {
                ui.set_max_width(380.0);
                pad::pad(ui, None, &marks, &live);
            });
            ui.label(dim(tr(lang, T::ModeLightHint)));
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr(lang, T::LogoColors)).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(tr(lang, T::FactoryReset)).clicked() {
                        edit.logo_colors = Some([[0; 3]; 3]);
                    }
                });
            });
            ui.label(dim(tr(lang, T::LogoHint)));
            if let Some(colors) = &mut edit.logo_colors {
                use crate::protocol::commands::LOGO_SLOT_XINPUT as X;
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgb(&mut colors[X]);
                    if colors[X] == [0; 3] {
                        ui.label(dim(tr(lang, T::FollowsMode)));
                    }
                });
                // Intensity: scale the colour, keeping its hue.
                let max = colors[X].iter().copied().max().unwrap_or(0);
                let mut pct = u32::from(max) * 100 / 255;
                ui.horizontal(|ui| {
                    ui.label(tr(lang, T::LogoIntensity));
                    if ui
                        .add_enabled(max > 0, egui::Slider::new(&mut pct, 1..=100).suffix("%"))
                        .changed()
                        && max > 0
                    {
                        let target = (pct * 255 / 100).max(1);
                        for c in colors[X].iter_mut() {
                            *c = ((u32::from(*c) * target + u32::from(max) / 2) / u32::from(max))
                                .min(255) as u8;
                        }
                    }
                });
                ui.add_space(4.0);
                egui::CollapsingHeader::new(tr(lang, T::Advanced))
                    .id_salt("logo_adv")
                    .show(ui, |ui| {
                        ui.label(dim(tr(lang, T::LogoOtherModes)));
                        ui.horizontal(|ui| {
                            for i in (0..3).filter(|i| *i != X) {
                                ui.vertical(|ui| {
                                    ui.label(format!("{} {}", tr(lang, T::ModeSlot), i + 1));
                                    ui.color_edit_button_srgb(&mut colors[i]);
                                });
                                ui.add_space(24.0);
                            }
                        });
                    });
            }
        });
        ui.add_space(10.0);
        let follows_before = self.settings.strip_follows_logo;
        let mut resend: Option<DeviceConfig> = None;
        let resend_ok = self.connected && !self.busy && self.worker.is_some();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr(lang, T::StripTitle)).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(tr(lang, T::FactoryReset)).clicked() {
                        edit.strip = Some(Strip::default());
                        edit.led_brightness = Some(0xff);
                        self.settings.strip_follows_logo = false;
                    }
                });
            });
            // One brightness for every light mode: 72 (mode light) and the
            // custom effect's own brightness move together.
            if let Some(b) = &mut edit.led_brightness {
                let mut pct = (u32::from(*b) * 100 / 255) as u8;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr(lang, T::Brightness)).color(TEXT_DIM));
                    if ui
                        .add(egui::Slider::new(&mut pct, 0..=100).suffix("%"))
                        .changed()
                    {
                        *b = (u32::from(pct) * 255 / 100) as u8;
                        if let Some(st) = &mut edit.strip {
                            st.bright = (*b).max(strip::MIN_BRIGHT);
                        }
                    }
                });
                ui.label(dim(tr(lang, T::BrightnessHint)));
            }
            ui.add_space(6.0);
            ui.checkbox(
                &mut self.settings.strip_follows_logo,
                tr(lang, T::StripFollowsLogo),
            );
            ui.add_space(4.0);
            // The strip state cannot be read back and can change on the
            // controller (FN): allow sending it again unconditionally.
            if ui
                .add_enabled(resend_ok, egui::Button::new(tr(lang, T::StripResend)))
                .on_hover_text(tr(lang, T::StripResendHint))
                .clicked()
            {
                resend = Some(DeviceConfig {
                    led_brightness: edit.led_brightness,
                    strip: edit.strip.clone(),
                    ..Default::default()
                });
            }
            ui.add_space(6.0);
            ui.separator();
            ui.label(RichText::new(tr(lang, T::FnCycle)).strong());
            ui.label(dim(tr(lang, T::FnCycleHint)));
            ui.add_space(4.0);
            ui.label(RichText::new(tr(lang, T::FnSlot1)).color(TEXT_DIM));
            ui.label(RichText::new(tr(lang, T::FnSlot3)).color(TEXT_DIM));
            ui.add_space(6.0);
            ui.label(RichText::new(tr(lang, T::StripCustomTitle)).strong());
            ui.label(dim(tr(lang, T::StripHint)));
            let locked = self.settings.strip_follows_logo;
            let Some(st) = &mut edit.strip else {
                ui.horizontal(|ui| {
                    ui.label(dim(tr(lang, T::StripUnknown)));
                    if ui.button(tr(lang, T::StripCustomize)).clicked() {
                        edit.strip = Some(Strip::default());
                    }
                });
                return;
            };
            ui.add_enabled_ui(!locked, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr(lang, T::StripEffect)).color(TEXT_DIM));
                    for (e, t) in Effect::ALL.into_iter().zip([
                        T::StripStatic,
                        T::StripBreathing,
                        T::StripGradient,
                        T::StripFlicker,
                    ]) {
                        ui.selectable_value(&mut st.effect, e, tr(lang, t));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr(lang, T::StripSpeed)).color(TEXT_DIM));
                    ui.add_enabled(
                        st.effect != Effect::Static,
                        egui::Slider::new(&mut st.speed, 0..=255),
                    );
                });
                ui.checkbox(&mut st.same, tr(lang, T::StripSame));
                let sides: &[(bool, T)] = if st.same {
                    &[(false, T::StripColors)]
                } else {
                    &[(false, T::StripLeft), (true, T::StripRight)]
                };
                for &(right, label) in sides {
                    let list = if right { &mut st.right } else { &mut st.left };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(tr(lang, label)).color(TEXT_DIM));
                        let mut remove = None;
                        for (i, c) in list.iter_mut().enumerate() {
                            ui.color_edit_button_srgb(c);
                            if i > 0 && ui.small_button("✕").clicked() {
                                remove = Some(i);
                            }
                        }
                        if let Some(i) = remove {
                            list.remove(i);
                        }
                        if list.len() < strip::MAX_COLORS && ui.small_button("+").clicked() {
                            let last = *list.last().unwrap_or(&[255, 255, 255]);
                            list.push(last);
                        }
                    });
                }
            });
        });
        if self.settings.strip_follows_logo != follows_before && self.shots.is_none() {
            let _ = self.settings.save();
        }
        if let Some(c) = resend {
            self.send(Request::Apply(c));
        }
    }

    fn page_macros(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let model = self.marker();
        let back_keys: Vec<usize> = visible_slots(&model)
            .into_iter()
            .filter(|&i| matches!(SLOT_NAMES[i], Some("M1" | "M2" | "M3" | "M4")))
            .collect();
        let input_keys: Vec<usize> = visible_slots(&model)
            .into_iter()
            .filter(|i| !back_keys.contains(i))
            .collect();
        let mut sel = self.macro_slot;
        let Some(edit) = &mut self.edit else {
            return no_profile(ui, lang);
        };
        if edit.macros.is_empty() {
            card(ui, |ui| {
                ui.label(dim(tr(lang, T::Disconnected)));
            });
            return;
        }
        sel = sel.min(edit.macros.len() - 1);
        card(ui, |ui| {
            ui.label(dim(tr(lang, T::MacroHint)));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (i, m) in edit.macros.iter().enumerate() {
                    let key = macro_key_name(m.run_key);
                    let total: u32 = m.steps.iter().map(|s| s.duration_ms).sum();
                    let text = if m.steps.is_empty() {
                        format!(
                            "{} {}\n{}",
                            tr(lang, T::MacroSlot),
                            i + 1,
                            tr(lang, T::MacroEmpty)
                        )
                    } else {
                        format!(
                            "{} {}  ·  {key}\n{} {} · {:.1} s",
                            tr(lang, T::MacroSlot),
                            i + 1,
                            m.steps.len(),
                            tr(lang, T::MacroSteps),
                            total as f32 / 1000.0
                        )
                    };
                    let b = egui::Button::new(RichText::new(text))
                        .min_size(Vec2::new(170.0, 48.0))
                        .fill(if i == sel {
                            theme::ACCENT_SOFT
                        } else {
                            theme::BG
                        })
                        .stroke(if i == sel {
                            Stroke::new(1.0, ACCENT)
                        } else {
                            Stroke::NONE
                        });
                    if ui.add(b).clicked() {
                        sel = i;
                    }
                }
            });
        });
        ui.add_space(10.0);
        let m = &mut edit.macros[sel];
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} {}", tr(lang, T::MacroSlot), sel + 1))
                        .size(20.0)
                        .strong(),
                );
                ui.add_space(20.0);
                ui.label(tr(lang, T::MacroBoundTo));
                egui::ComboBox::from_id_salt("macro_key")
                    .selected_text(macro_key_name(m.run_key))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut m.run_key, crate::protocol::macros::UNBOUND, "—");
                        for &k in &back_keys {
                            ui.selectable_value(
                                &mut m.run_key,
                                k as u8,
                                SLOT_NAMES[k].unwrap_or("?"),
                            );
                        }
                    });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(tr(lang, T::MacroClear)).clicked() {
                        *m = Macro::empty(m.slot);
                    }
                });
            });
            ui.add_space(6.0);
            macro_timeline(ui, m);
            ui.add_space(8.0);
            let mut remove = None;
            let mut swap = None;
            egui::Grid::new("steps")
                .num_columns(4)
                .striped(true)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(dim("#"));
                    ui.label(dim(tr(lang, T::MacroDuration)));
                    ui.label(dim(tr(lang, T::MacroButtons)));
                    ui.label("");
                    ui.end_row();
                    let n = m.steps.len();
                    for (i, st) in m.steps.iter_mut().enumerate() {
                        ui.label(RichText::new(format!("{}", i + 1)).monospace());
                        ui.add(
                            egui::DragValue::new(&mut st.duration_ms)
                                .range(8..=32_760)
                                .speed(8.0)
                                .suffix(" ms"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.set_max_width(420.0);
                            let mut any = false;
                            for &k in &input_keys {
                                if st.inputs & (1 << k) != 0 {
                                    any = true;
                                    let chip = egui::Button::new(
                                        RichText::new(format!(
                                            "{} ✕",
                                            SLOT_NAMES[k].unwrap_or("?")
                                        ))
                                        .color(Color32::WHITE),
                                    )
                                    .fill(theme::ACCENT_SOFT)
                                    .small();
                                    if ui.add(chip).clicked() {
                                        st.inputs &= !(1 << k);
                                    }
                                }
                            }
                            if !any {
                                ui.label(dim(tr(lang, T::MacroPause)));
                            }
                            egui::ComboBox::from_id_salt(("add", i))
                                .selected_text("+")
                                .width(40.0)
                                .show_ui(ui, |ui| {
                                    for &k in &input_keys {
                                        if st.inputs & (1 << k) == 0
                                            && ui
                                                .selectable_label(
                                                    false,
                                                    SLOT_NAMES[k].unwrap_or("?"),
                                                )
                                                .clicked()
                                        {
                                            st.inputs |= 1 << k;
                                        }
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(i > 0, egui::Button::new("▲").small())
                                .clicked()
                            {
                                swap = Some((i, i - 1));
                            }
                            if ui
                                .add_enabled(i + 1 < n, egui::Button::new("▼").small())
                                .clicked()
                            {
                                swap = Some((i, i + 1));
                            }
                            if ui.small_button("✕").clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.end_row();
                    }
                });
            if let Some(i) = remove {
                m.steps.remove(i);
            }
            if let Some((a, b)) = swap {
                m.steps.swap(a, b);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let full = m.steps.len() >= crate::protocol::macros::MAX_STEPS;
                for (label, inputs) in [(T::MacroAddPress, 1u32), (T::MacroAddPause, 0u32)] {
                    if ui
                        .add_enabled(!full, egui::Button::new(tr(lang, label)))
                        .clicked()
                    {
                        if m.steps.is_empty() && m.repeat == 0 && !m.run_after_release {
                            // Header values the controller itself writes when recording
                            // (meaning unknown; a recorded macro plays with them).
                            m.run_after_release = true;
                            m.repeat = 0x000d;
                        }
                        m.steps.push(MacroStep {
                            duration_ms: 160,
                            marker: false,
                            inputs,
                            analog: [0; 4],
                        });
                    }
                }
                ui.label(dim(format!(
                    "{}/{}",
                    m.steps.len(),
                    crate::protocol::macros::MAX_STEPS
                )));
            });
        });
        self.macro_slot = sel;
    }

    fn page_profiles(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_profile_name)
                        .hint_text(tr(lang, T::ProfileName)),
                );
                let ok = !self.new_profile_name.trim().is_empty() && self.edit.is_some();
                if ui
                    .add_enabled(ok, egui::Button::new(tr(lang, T::SaveAsProfile)))
                    .clicked()
                    && let Some(e) = &self.edit
                {
                    let p = NamedProfile {
                        name: self.new_profile_name.trim().to_string(),
                        model: self.marker(),
                        config: e.config(),
                    };
                    match config::save_profile(&p) {
                        Ok(_) => {
                            self.new_profile_name.clear();
                            self.profiles = config::list_profiles();
                        }
                        Err(err) => self.status = Status::Error(err.to_string()),
                    }
                }
            });
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            if self.profiles.is_empty() {
                ui.label(dim(tr(lang, T::NoProfiles)));
            }
            let mut apply = None;
            let mut delete = None;
            for p in &self.profiles {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&p.name).strong());
                    ui.label(dim(model_name(&p.model)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button(tr(lang, T::Delete)).clicked() {
                            delete = Some(p.name.clone());
                        }
                        if ui.button(tr(lang, T::ApplyProfile)).clicked() {
                            apply = Some(p.config.clone());
                        }
                    });
                });
            }
            if let (Some(c), Some(e)) = (apply, &mut self.edit) {
                // Load into the editor; "Apply to controller" then sends only the differences.
                e.apply_config(&c);
            }
            if let Some(name) = delete {
                let _ = config::delete_profile(&name);
                self.profiles = config::list_profiles();
            }
        });
    }

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let before = self.settings.clone();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr(lang, T::Language));
                for l in Lang::ALL {
                    ui.selectable_value(&mut self.settings.language, l, l.name());
                }
            });
            ui.checkbox(&mut self.settings.autostart, tr(lang, T::Autostart));
            ui.checkbox(
                &mut self.settings.close_to_tray,
                tr(lang, T::MinimizeToTray),
            );
            ui.checkbox(&mut self.settings.start_hidden, tr(lang, T::StartHidden));
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new(tr(lang, T::Info)).strong());
            ui.add_space(4.0);
            egui::Grid::new("info")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(tr(lang, T::Version)).color(TEXT_DIM));
                    ui.label(format!("PadTune {}", env!("CARGO_PKG_VERSION")));
                    ui.end_row();
                    ui.label(RichText::new(tr(lang, T::ConfigFolder)).color(TEXT_DIM));
                    let dir = config::config_dir();
                    if ui
                        .link(dir.display().to_string())
                        .on_hover_text(tr(lang, T::OpenFolder))
                        .clicked()
                    {
                        let _ = std::fs::create_dir_all(&dir);
                        super::desktop::open_external(&dir.display().to_string());
                    }
                    ui.end_row();
                    ui.label(RichText::new(tr(lang, T::Repository)).color(TEXT_DIM));
                    let repo = env!("CARGO_PKG_REPOSITORY");
                    if ui.link(repo).clicked() {
                        super::desktop::open_external(repo);
                    }
                    ui.end_row();
                    ui.label(RichText::new(tr(lang, T::LicenseLabel)).color(TEXT_DIM));
                    ui.label("MIT");
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.label(dim(tr(lang, T::Unofficial)));
        });
        if self.settings != before && self.shots.is_none() {
            if self.settings.autostart != before.autostart
                && let Err(e) = super::desktop::set_autostart(self.settings.autostart)
            {
                self.status = Status::Error(e.to_string());
                self.settings.autostart = before.autostart;
            }
            let _ = self.settings.save();
        }
    }

    /// "USB · 1000 Hz" / "Dongle · 1000 Hz".
    fn connection_label(&self) -> String {
        let Some(s) = &self.snap else {
            return self.t(T::Connected).to_string();
        };
        let link = match s.connection {
            Some(crate::actions::Link::Usb) => self.t(T::Wired),
            Some(crate::actions::Link::Dongle) => self.t(T::Wireless),
            None => self.t(T::Connected),
        };
        match s.info.polling_code.and_then(polling_hz) {
            Some(hz) => format!("{link} · {hz} Hz"),
            None => link.to_string(),
        }
    }

    /// With "strip follows logo", keep the strip a solid copy of the HOME
    /// logo colour (or of the mode colour when the logo follows the mode).
    fn sync_strip_with_logo(&mut self) {
        if !self.settings.strip_follows_logo {
            return;
        }
        let Some(e) = &mut self.edit else { return };
        // Only the hue is shared: the logo's intensity is scaled up to full,
        // the strip keeps its own brightness.
        let c = logo_color(e.logo_colors).unwrap_or(MODE_LIGHT_XINPUT);
        let rgb = [c.r(), c.g(), c.b()];
        let peak = rgb.iter().copied().max().unwrap_or(0).max(1);
        let full = rgb.map(|v| (u32::from(v) * 255 / u32::from(peak)) as u8);
        let bright = e.led_brightness.unwrap_or(255).max(strip::MIN_BRIGHT);
        let want = Strip::solid(full, bright);
        if e.strip.as_ref() != Some(&want) {
            e.strip = Some(want);
        }
    }

    fn marker(&self) -> String {
        self.snap
            .as_ref()
            .and_then(|s| s.info.mode.as_ref())
            .map(|m| m.marker.clone())
            .unwrap_or_default()
    }

    fn drive_screenshots(&mut self, ctx: &egui::Context) {
        use std::sync::atomic::Ordering;
        let Some(run) = &mut self.shots else { return };
        if run.pending.load(Ordering::SeqCst) {
            ctx.request_repaint();
            return;
        }
        let Some(&(lang, tab)) = run.steps.get(run.index) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        self.settings.language = lang;
        self.tab = tab;
        run.frames += 1;
        if run.frames < 4 {
            ctx.request_repaint();
            return;
        }
        run.frames = 0;
        run.index += 1;
        let path = run.dir.join(format!(
            "{}-{}.png",
            tab.slug(),
            if lang == Lang::It { "it" } else { "en" }
        ));
        let pending = run.pending.clone();
        pending.store(true, Ordering::SeqCst);
        let repaint = ctx.clone();
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |img| {
                let _ = save_png(&path, &img);
                pending.store(false, Ordering::SeqCst);
                repaint.request_repaint();
            }),
        ));
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Ok(mut c) = self.shell.ctx.lock() {
            *c = None;
        }
        if let Some(w) = &self.worker {
            w.send(Request::Shutdown);
            // Give the device thread a moment to switch key reports off.
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
        self.sync_strip_with_logo();
        if let Some(l) = &self.live
            && let Ok(st) = l.state.lock()
        {
            self.live_now = st.clone();
        }
        self.drive_screenshots(ctx);
        if matches!(self.status, Status::Info(..)) {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The key test pauses the gamepad output: never leave it on elsewhere.
        if self.key_test && self.tab != Tab::Buttons {
            self.set_key_test(false);
        }
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(210.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| {
            self.top_bar(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| match self.tab {
                    Tab::Device => self.page_device(ui),
                    Tab::Buttons => self.page_buttons(ui),
                    Tab::Sticks => self.page_sticks(ui),
                    Tab::Triggers => self.page_triggers(ui),
                    Tab::Lights => self.page_lights(ui),
                    Tab::Vibration => self.page_vibration(ui),
                    Tab::Macros => self.page_macros(ui),
                    Tab::Profiles => self.page_profiles(ui),
                    Tab::Settings => self.page_settings(ui),
                });
        });
    }
}

// ---- helpers --------------------------------------------------------------

fn model_name(marker: &str) -> &'static str {
    match marker {
        "ZJ-C1pro" => "Rainbow 2 Pro",
        "" => "—",
        _ => "BIGBIG WON",
    }
}

/// Highest polling rate a model supports (Rainbow 2 Pro: USB full speed,
/// `bInterval` 1 ms).
fn max_polling_hz(marker: &str) -> u32 {
    if marker == "ZJ-C1pro" { 1000 } else { 2000 }
}

/// Key slots shown for a model (hides unused/reserved slots).
/// The lights to draw, computed once from the editor state for every tab:
/// light-strip brightness, the custom strip effect when PadTune knows it
/// (otherwise the mode colour) and the HOME logo colour.
fn light_marks(e: &Edit, lights_only: bool) -> Marks {
    let rgb = |c: &[u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
    Marks {
        turbo: 0,
        remapped: 0,
        strip: e.led_brightness,
        light: MODE_LIGHT_XINPUT,
        logo: logo_color(e.logo_colors),
        strip_custom: e.strip.as_ref().map(|st| {
            let left = st.left.first().map_or(MODE_LIGHT_XINPUT, rgb);
            let right = if st.same {
                left
            } else {
                st.right.first().map_or(left, rgb)
            };
            (left, right)
        }),
        profile: None,
        lights_only,
    }
}

/// Custom XInput-mode logo colour, if one is set (black = follows the mode).
fn logo_color(colors: Option<[[u8; 3]; 3]>) -> Option<Color32> {
    colors
        .map(|c| c[crate::protocol::commands::LOGO_SLOT_XINPUT])
        .filter(|c| *c != [0; 3])
        .map(|[r, g, b]| Color32::from_rgb(r, g, b))
}

fn macro_key_name(k: u8) -> &'static str {
    if k == crate::protocol::macros::UNBOUND {
        "—"
    } else {
        SLOT_NAMES
            .get(usize::from(k))
            .copied()
            .flatten()
            .unwrap_or("?")
    }
}

/// Proportional timeline of a macro: pressed steps in accent, pauses dim.
fn macro_timeline(ui: &mut egui::Ui, m: &Macro) {
    let (resp, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 18.0), egui::Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 4.0, theme::BG);
    let total: u32 = m.steps.iter().map(|s| s.duration_ms).sum::<u32>().max(1);
    let mut x = r.left();
    for s in &m.steps {
        let w = r.width() * s.duration_ms as f32 / total as f32;
        let rect = egui::Rect::from_min_size(
            egui::pos2(x, r.top()),
            Vec2::new((w - 1.0).max(1.0), r.height()),
        );
        if s.inputs != 0 {
            painter.rect_filled(rect, 3.0, ACCENT);
        }
        x += w;
    }
}

/// Colour of the strip/Home light in "mode light" while in XInput mode (the
/// only mode in which the controller enumerates as `413d:2104`).
const MODE_LIGHT_XINPUT: Color32 = Color32::from_rgb(0x2e, 0xe0, 0x5a);

/// An on-board feature with the button combination that starts it.
struct Feature {
    title: T,
    desc: T,
    combo: T,
    /// Front function key that starts it, if any (drawn on the pad).
    key_slot: Option<usize>,
    /// The key assignment comes from icons/manual and is not yet confirmed.
    tentative: bool,
}

const FEATURES: &[Feature] = &[
    Feature {
        title: T::FTurboT,
        desc: T::FTurboD,
        combo: T::FTurboC,
        key_slot: Some(27),
        tentative: false,
    },
    Feature {
        title: T::FMapT,
        desc: T::FMapD,
        combo: T::FMapC,
        key_slot: Some(29),
        tentative: false,
    },
    Feature {
        title: T::FMacroT,
        desc: T::FMacroD,
        combo: T::FMacroC,
        key_slot: Some(22),
        tentative: false,
    },
    Feature {
        title: T::FProfileT,
        desc: T::FProfileD,
        combo: T::FProfileC,
        key_slot: Some(28),
        tentative: false,
    },
    Feature {
        title: T::FLightT,
        desc: T::FLightD,
        combo: T::FLightC,
        key_slot: Some(30),
        tentative: false,
    },
    Feature {
        title: T::FVibT,
        desc: T::FVibD,
        combo: T::FVibC,
        key_slot: None,
        tentative: false,
    },
    Feature {
        title: T::FVolT,
        desc: T::FVolD,
        combo: T::FVolC,
        key_slot: None,
        tentative: false,
    },
    Feature {
        title: T::FGyroT,
        desc: T::FGyroD,
        combo: T::FGyroC,
        key_slot: None,
        tentative: false,
    },
    Feature {
        title: T::FModeT,
        desc: T::FModeD,
        combo: T::FModeC,
        key_slot: None,
        tentative: false,
    },
    Feature {
        title: T::FCalT,
        desc: T::FCalD,
        combo: T::FCalC,
        key_slot: None,
        tentative: false,
    },
];

fn feature_body(ui: &mut egui::Ui, lang: Lang, f: &Feature) {
    ui.label(RichText::new(tr(lang, f.desc)).color(TEXT_DIM));
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!("{}:", tr(lang, T::HowToStart)))
                .small()
                .color(TEXT_DIM),
        );
        ui.label(RichText::new(tr(lang, f.combo)).strong().color(ACCENT));
    });
    if f.tentative {
        ui.label(
            RichText::new(format!("({})", tr(lang, T::ToConfirm)))
                .small()
                .color(WARN),
        );
    }
}

fn visible_slots(marker: &str) -> Vec<usize> {
    (0..SLOT_NAMES.len())
        .filter(|&i| SLOT_NAMES[i].is_some())
        // Front function keys are not remappable.
        .filter(|i| !crate::protocol::profile::FUNCTION_SLOTS.contains(i))
        .filter(|&i| {
            let name = SLOT_NAMES[i].unwrap_or_default();
            // Rainbow 2 Pro has only M1/M2 at the back.
            !(marker == "ZJ-C1pro" && (name == "M3" || name == "M4"))
        })
        .collect()
}

fn row(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.label(RichText::new(k).color(TEXT_DIM));
    ui.label(RichText::new(v).strong());
    ui.end_row();
}

fn no_profile(ui: &mut egui::Ui, lang: Lang) {
    card(ui, |ui| {
        ui.label(tr(lang, T::Disconnected));
    });
}

fn keymap_editor(
    ui: &mut egui::Ui,
    lang: Lang,
    slot: usize,
    map: &mut KeyMap,
    slots: &[usize],
) -> bool {
    let before = *map;
    let kind = match map {
        KeyMap::Default => T::MapDefault,
        KeyMap::Disabled => T::MapDisabled,
        KeyMap::Button { .. } => T::MapButton,
        KeyMap::Keyboard { .. } => T::MapKeyboard,
        KeyMap::Raw { .. } => T::MapRaw,
    };
    egui::ComboBox::from_id_salt(("kind", slot))
        .selected_text(tr(lang, kind))
        .width(170.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(kind == T::MapDefault, tr(lang, T::MapDefault))
                .clicked()
            {
                *map = KeyMap::Default;
            }
            if ui
                .selectable_label(kind == T::MapDisabled, tr(lang, T::MapDisabled))
                .clicked()
            {
                *map = KeyMap::Disabled;
            }
            if ui
                .selectable_label(kind == T::MapButton, tr(lang, T::MapButton))
                .clicked()
            {
                *map = KeyMap::button(slot as u8);
            }
            if ui
                .selectable_label(kind == T::MapKeyboard, tr(lang, T::MapKeyboard))
                .clicked()
            {
                *map = KeyMap::Keyboard {
                    modifiers: 0,
                    key1: 0x2c,
                    key2: 0,
                };
            }
        });
    ui.horizontal(|ui| match map {
        KeyMap::Button { targets } => {
            // First target required, two more optional for a combination.
            let name = |t: u8| {
                if t == NO_KEY {
                    "—"
                } else {
                    SLOT_NAMES
                        .get(usize::from(t))
                        .copied()
                        .flatten()
                        .unwrap_or("?")
                }
            };
            ui.vertical(|ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, t) in targets.iter_mut().enumerate() {
                        if i > 0 {
                            ui.label("+");
                        }
                        egui::ComboBox::from_id_salt(("target", slot, i))
                            .selected_text(name(*t))
                            .width(70.0)
                            .show_ui(ui, |ui| {
                                if i > 0 {
                                    ui.selectable_value(t, NO_KEY, "—");
                                }
                                for &s in slots {
                                    ui.selectable_value(t, s as u8, SLOT_NAMES[s].unwrap_or("?"));
                                }
                            });
                    }
                });
            });
        }
        KeyMap::Keyboard {
            modifiers, key1, ..
        } => {
            egui::ComboBox::from_id_salt(("mod", slot))
                .selected_text(keys::modifier_name(*modifiers).unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (bits, n) in MODIFIERS {
                        ui.selectable_value(modifiers, *bits, *n);
                    }
                });
            ui.label("+");
            egui::ComboBox::from_id_salt(("key", slot))
                .selected_text(keys::key_name(*key1).unwrap_or("?"))
                .height(300.0)
                .show_ui(ui, |ui| {
                    for (u, n) in KEYBOARD {
                        ui.selectable_value(key1, *u, *n);
                    }
                });
        }
        KeyMap::Raw { bytes } => {
            ui.label(dim(format!("{bytes:02x?}")));
        }
        _ => {}
    });
    *map != before
}

/// Draws the stick response curve.
fn curve_plot(ui: &mut egui::Ui, c: &Curve, live: Option<f32>, axes: (&str, &str)) {
    let size = Vec2::splat(ui.available_width().min(320.0));
    let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 8.0, theme::BG);
    let grid = Stroke::new(1.0, Color32::from_gray(45));
    for i in 1..4 {
        let f = i as f32 / 4.0;
        painter.line_segment(
            [
                r.lerp_inside(Vec2::new(f, 0.0)),
                r.lerp_inside(Vec2::new(f, 1.0)),
            ],
            grid,
        );
        painter.line_segment(
            [
                r.lerp_inside(Vec2::new(0.0, f)),
                r.lerp_inside(Vec2::new(1.0, f)),
            ],
            grid,
        );
    }
    let p =
        |x: u8, y: u8| r.lerp_inside(Vec2::new(f32::from(x) / 100.0, 1.0 - f32::from(y) / 100.0));
    let pts = vec![
        p(0, 0),
        p(c.deadzone, 0),
        p(c.deadzone, c.center_comp),
        p(c.point_a.0.max(c.deadzone), c.point_a.1),
        p(c.point_b.0.max(c.deadzone), c.point_b.1),
        p(c.outer, c.max_out),
        p(100, c.max_out),
    ];
    painter.line_segment(
        [p(0, 0), p(100, 100)],
        Stroke::new(1.0, Color32::from_gray(70)),
    );
    painter.line(pts, Stroke::new(3.0, ACCENT));
    for (x, y) in [c.point_a, c.point_b] {
        painter.circle_filled(p(x, y), 6.0, Color32::WHITE);
    }
    if let Some(m) = live {
        let x = r.left() + r.width() * m;
        painter.line_segment(
            [egui::pos2(x, r.top()), egui::pos2(x, r.bottom())],
            Stroke::new(1.5, OK),
        );
    }
    // Axis captions: X = how far the stick is pushed, Y = what the game gets.
    let font = egui::FontId::proportional(11.0);
    painter.text(
        r.left_bottom() + Vec2::new(6.0, -4.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{} →", axes.0),
        font.clone(),
        TEXT_DIM,
    );
    painter.text(
        r.left_top() + Vec2::new(6.0, 4.0),
        egui::Align2::LEFT_TOP,
        format!("↑ {}", axes.1),
        font,
        TEXT_DIM,
    );
}

fn trigger_bar(ui: &mut egui::Ui, t: &Trigger, live: Option<f32>) {
    let (resp, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), 22.0), egui::Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 6.0, theme::BG);
    let x = |v: u8| r.left() + r.width() * f32::from(v) / 100.0;
    let active = egui::Rect::from_x_y_ranges(x(t.start)..=x(t.end), r.y_range());
    painter.rect_filled(active, 6.0, ACCENT);
    if let Some(v) = live {
        let lx = r.left() + r.width() * v.clamp(0.0, 1.0);
        painter.line_segment(
            [
                egui::pos2(lx, r.top() - 3.0),
                egui::pos2(lx, r.bottom() + 3.0),
            ],
            Stroke::new(3.0, OK),
        );
        painter.text(
            r.right_center() + egui::vec2(-6.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            format!("{:.0}%", v * 100.0),
            egui::FontId::monospace(12.0),
            Color32::WHITE,
        );
    }
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        img.size[0] as u32,
        img.size[1] as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    w.write_image_data(&bytes).map_err(std::io::Error::other)?;
    Ok(())
}
