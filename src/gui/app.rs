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
use crate::protocol::profile::{Curve, KeyMap, Profile, SLOT_NAMES, TURBO_SPEEDS_HZ, Trigger};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Device,
    Buttons,
    Sticks,
    Triggers,
    Lights,
    Macros,
    Profiles,
    Settings,
}

impl Tab {
    const ALL: [Tab; 8] = [
        Tab::Device,
        Tab::Buttons,
        Tab::Sticks,
        Tab::Triggers,
        Tab::Lights,
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
}

impl Edit {
    fn from_snapshot(s: &Snapshot) -> Self {
        Self {
            profile: s.profile.clone(),
            polling_code: s.info.polling_code,
            led_brightness: s.led_brightness,
            logo_colors: s.logo_colors,
        }
    }

    fn config(&self) -> DeviceConfig {
        DeviceConfig {
            profile: self.profile.as_ref().map(|p| config::to_hex(&p.to_bytes())),
            polling_code: self.polling_code,
            led_brightness: self.led_brightness,
            logo_colors: self.logo_colors,
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
    }
}

enum Status {
    None,
    Info(String, Instant),
    Error(String),
}

/// Automatic screenshot run (`BIGBIGCTL_SCREENSHOTS`).
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
    live: Option<live::Shared>,
    live_now: LiveState,
    selected_slot: usize,
    /// Buttons from the controller's own key report (covers every button).
    vendor_keys: Option<u32>,
    /// Key test active (controller key reports on; gamepad output paused).
    key_test: bool,
    /// When each button was last seen pressed, so quick taps stay visible.
    pressed_at: [Option<Instant>; 32],
    idle_dongle: bool,
}

fn demo_snapshot() -> Snapshot {
    use crate::actions::DeviceInfo;
    use crate::protocol::commands::{MacroSlotInfo, Mode, Power};
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
        macros: vec![
            MacroSlotInfo {
                crc: 0xcbe1,
                len: 10
            };
            4
        ],
        wireless_flag: Some(1),
        connection: Some(crate::actions::Link::Usb),
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let settings = Settings::load();
        let shots = std::env::var_os("BIGBIGCTL_SCREENSHOTS").map(|d| {
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
        let demo = shots.is_some() || std::env::var_os("BIGBIGCTL_DEMO").is_some_and(|v| v != "0");
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
                Event::Applied => {
                    if let (Some(e), Some(s)) = (&self.edit, &self.snap) {
                        let mut state = config::State::load();
                        if let Some(uuid) = &s.info.uuid {
                            state.devices.insert(uuid.clone(), e.config());
                            let _ = state.save();
                        }
                    }
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
                RichText::new("BIGBIG WON")
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
                for (code, hz) in POLLING_CODES {
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
                        "bigbigctl-backup-{}.bin",
                        s.info.uuid.clone().unwrap_or_else(|| "controller".into())
                    );
                    let path = dirs::document_dir()
                        .unwrap_or_else(config::config_dir)
                        .join(name);
                    self.send(Request::Backup(path));
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
        let strip = self.edit.as_ref().and_then(|e| e.led_brightness);
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
            strip,
            light: MODE_LIGHT_XINPUT,
            profile: None,
            lights_only: false,
        };
        let editor_w = 300.0;
        let pad_w = (ui.available_width() - editor_w - 40.0).max(320.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(pad_w);
                card(ui, |ui| {
                    let on_dongle = self_link == Some(crate::actions::Link::Dongle);
                    if on_dongle {
                        key_test = false;
                    }
                    ui.horizontal(|ui| {
                        if on_dongle {
                            ui.label(dim(tr(lang, T::KeyTestCable)));
                            return;
                        }
                        ui.toggle_value(
                            &mut key_test,
                            RichText::new(format!("⚡ {}", tr(lang, T::KeyTest))).strong(),
                        );
                        ui.label(dim(tr(
                            lang,
                            if key_test {
                                T::KeyTestOn
                            } else {
                                T::KeyTestOff
                            },
                        )));
                    });
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
                curve_plot(ui, &c, mag);
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(dim(tr(lang, T::Presets)));
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
            c.outer = c.outer.max(c.deadzone + 1).min(100);
            if profile.set_curve(index, c).is_err() {
                self.status = Status::Error("invalid curve".into());
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

    fn page_lights(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let live = self.live_now.clone();
        let Some(edit) = &mut self.edit else {
            return no_profile(ui, lang);
        };
        card(ui, |ui| {
            let marks = Marks {
                turbo: 0,
                remapped: 0,
                strip: edit.led_brightness,
                light: MODE_LIGHT_XINPUT,
                profile: None,
                lights_only: true,
            };
            ui.vertical_centered(|ui| {
                ui.set_max_width(520.0);
                pad::pad(ui, None, &marks, &live);
            });
            ui.label(dim(tr(lang, T::ModeLightHint)));
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new(tr(lang, T::Brightness)).strong());
            ui.label(dim(tr(lang, T::BrightnessHint)));
            if let Some(b) = &mut edit.led_brightness {
                let mut pct = (u32::from(*b) * 100 / 255) as u8;
                if ui
                    .add(egui::Slider::new(&mut pct, 0..=100).suffix("%"))
                    .changed()
                {
                    *b = (u32::from(pct) * 255 / 100) as u8;
                }
            } else {
                ui.label(dim("—"));
            }
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new(tr(lang, T::LogoColors)).strong());
            ui.label(dim(tr(lang, T::NotVerified)));
            if let Some(colors) = &mut edit.logo_colors {
                ui.horizontal(|ui| {
                    for (i, t) in [T::ModeXbox, T::ModeSwitch, T::ModePs]
                        .into_iter()
                        .enumerate()
                    {
                        ui.vertical(|ui| {
                            ui.label(tr(lang, t));
                            ui.color_edit_button_srgb(&mut colors[i]);
                        });
                        ui.add_space(20.0);
                    }
                });
            }
        });
    }

    fn page_macros(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang();
        let Some(s) = &self.snap else {
            return no_profile(ui, lang);
        };
        card(ui, |ui| {
            ui.label(dim(tr(lang, T::NotVerified)));
            for (i, m) in s.macros.iter().enumerate() {
                let steps = m.len.saturating_sub(10) / 10;
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} {}", tr(lang, T::MacroSlot), i + 1)).strong(),
                    );
                    if steps == 0 {
                        ui.label(dim(tr(lang, T::MacroEmpty)));
                    } else {
                        ui.label(format!("{steps} {}", tr(lang, T::MacroSteps)));
                    }
                });
            }
        });
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
            ui.label(dim(format!("bigbigctl {}", env!("CARGO_PKG_VERSION"))));
            ui.label(dim(config::config_dir().display().to_string()));
        });
        if self.settings != before && self.shots.is_none() {
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
        if let Some(w) = &self.worker {
            w.send(Request::Shutdown);
            // Give the device thread a moment to switch key reports off.
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
        if let Some(l) = &self.live
            && let Ok(st) = l.lock()
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

/// Key slots shown for a model (hides unused/reserved slots).
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
                *map = KeyMap::Button { target: slot as u8 };
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
        KeyMap::Button { target } => {
            let name = SLOT_NAMES
                .get(usize::from(*target))
                .copied()
                .flatten()
                .unwrap_or("?");
            egui::ComboBox::from_id_salt(("target", slot))
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for &s in slots {
                        ui.selectable_value(target, s as u8, SLOT_NAMES[s].unwrap_or("?"));
                    }
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
fn curve_plot(ui: &mut egui::Ui, c: &Curve, live: Option<f32>) {
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
