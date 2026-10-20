//! Desktop integration: tray icon, single instance and autostart.
//!
//! * Linux: tray via `ksni` (StatusNotifierItem, tokio backend), autostart via
//!   an XDG `.desktop` file in `~/.config/autostart`.
//! * Windows: tray via `tray-icon` on its own thread with a Win32 message
//!   loop, autostart via `HKCU\…\Run`.
//! * Both: a second launch only asks the running instance to show itself
//!   (local socket).

use std::sync::mpsc::Sender;

/// Commands coming from the tray or from a second instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Show,
    Quit,
}

/// Menu labels in the user's language.
#[derive(Clone)]
pub struct Labels {
    pub show: String,
    pub quit: String,
    pub tooltip: String,
}

// ---- single instance --------------------------------------------------------

const SOCKET_NAME: &str = "bigbigctl-gui.sock";

/// Returns `false` if another instance is running (it has been asked to show
/// its window); otherwise starts listening for later launches.
pub fn claim_single_instance(tx: Sender<Cmd>) -> bool {
    use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream, prelude::*};
    use std::io::{BufRead, BufReader, Write};

    let Ok(name) = SOCKET_NAME.to_ns_name::<GenericNamespaced>() else {
        return true;
    };
    if let Ok(mut conn) = Stream::connect(name.clone()) {
        let _ = conn.write_all(b"show\n");
        return false;
    }
    let Ok(listener) = ListenerOptions::new().name(name).create_sync() else {
        // Could not listen (unusual): run anyway rather than refuse to start.
        return true;
    };
    std::thread::Builder::new()
        .name("bigbigctl-instance".into())
        .spawn(move || {
            for conn in listener.incoming().flatten() {
                let mut line = String::new();
                let _ = BufReader::new(conn).read_line(&mut line);
                if line.trim() == "show" && tx.send(Cmd::Show).is_err() {
                    return;
                }
            }
        })
        .ok();
    true
}

// ---- icon --------------------------------------------------------------------

/// RGBA pixels of the embedded tray icon.
fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(std::io::Cursor::new(super::TRAY_PNG));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    buf.truncate(info.buffer_size());
    Some((buf, info.width, info.height))
}

// ---- tray: Linux -------------------------------------------------------------

#[cfg(target_os = "linux")]
pub fn spawn_tray(tx: Sender<Cmd>, labels: Labels) {
    use ksni::TrayMethods;

    struct Tray {
        tx: Sender<Cmd>,
        labels: Labels,
        icon: Vec<ksni::Icon>,
    }

    impl ksni::Tray for Tray {
        fn id(&self) -> String {
            "bigbigctl".into()
        }
        fn title(&self) -> String {
            self.labels.tooltip.clone()
        }
        fn icon_name(&self) -> String {
            "bigbigctl".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.clone()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: self.labels.tooltip.clone(),
                ..Default::default()
            }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.tx.send(Cmd::Show);
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem {
                    label: self.labels.show.clone(),
                    activate: Box::new(|t: &mut Self| {
                        let _ = t.tx.send(Cmd::Show);
                    }),
                    ..Default::default()
                }
                .into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: self.labels.quit.clone(),
                    activate: Box::new(|t: &mut Self| {
                        let _ = t.tx.send(Cmd::Quit);
                    }),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    // StatusNotifierItem wants ARGB32 in network byte order.
    let icon = icon_rgba()
        .map(|(rgba, w, h)| {
            let data = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[3], p[0], p[1], p[2]])
                .collect();
            vec![ksni::Icon {
                width: w as i32,
                height: h as i32,
                data,
            }]
        })
        .unwrap_or_default();
    std::thread::Builder::new()
        .name("bigbigctl-tray".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async move {
                // No tray host (e.g. a bare compositor): keep running without one.
                if let Ok(_handle) = (Tray { tx, labels, icon }).spawn().await {
                    std::future::pending::<()>().await;
                }
            });
        })
        .ok();
}

// ---- tray: Windows -----------------------------------------------------------

#[cfg(windows)]
pub fn spawn_tray(tx: Sender<Cmd>, labels: Labels) {
    std::thread::Builder::new()
        .name("bigbigctl-tray".into())
        .spawn(move || {
            use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
            use tray_icon::{MouseButton, TrayIconBuilder, TrayIconEvent};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
            };

            let show = MenuItem::new(&labels.show, true, None);
            let quit = MenuItem::new(&labels.quit, true, None);
            let menu = Menu::new();
            let _ = menu.append_items(&[&show, &PredefinedMenuItem::separator(), &quit]);
            let mut builder = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip(&labels.tooltip);
            if let Some((rgba, w, h)) = icon_rgba()
                && let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h)
            {
                builder = builder.with_icon(icon);
            }
            let Ok(_tray) = builder.build() else { return };
            // The tray icon lives on this thread, which therefore needs its
            // own Win32 message loop.
            loop {
                unsafe {
                    let mut msg: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                while let Ok(ev) = MenuEvent::receiver().try_recv() {
                    let cmd = if ev.id == *show.id() {
                        Cmd::Show
                    } else if ev.id == *quit.id() {
                        Cmd::Quit
                    } else {
                        continue;
                    };
                    if tx.send(cmd).is_err() {
                        return;
                    }
                }
                while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        ..
                    } = ev
                        && tx.send(Cmd::Show).is_err()
                    {
                        return;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        })
        .ok();
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn spawn_tray(_tx: Sender<Cmd>, _labels: Labels) {}

// ---- autostart ---------------------------------------------------------------

/// Enables or disables starting the GUI (hidden, in the tray) at login.
pub fn set_autostart(enabled: bool) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    platform_autostart(enabled, &exe)
}

#[cfg(target_os = "linux")]
fn platform_autostart(enabled: bool, exe: &std::path::Path) -> std::io::Result<()> {
    let dir = dirs::config_dir().unwrap_or_default().join("autostart");
    let file = dir.join("bigbigctl.desktop");
    if !enabled {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        &file,
        format!(
            "[Desktop Entry]\nType=Application\nName=BIGBIG WON Controller\n\
             Exec=\"{}\" --hidden\nIcon=bigbigctl\nX-GNOME-Autostart-enabled=true\n\
             NoDisplay=true\n",
            exe.display()
        ),
    )
}

#[cfg(windows)]
fn platform_autostart(enabled: bool, exe: &std::path::Path) -> std::io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
    if enabled {
        run.set_value("bigbigctl", &format!("\"{}\" --hidden", exe.display()))
    } else {
        match run.delete_value("bigbigctl") {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn platform_autostart(_enabled: bool, _exe: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
