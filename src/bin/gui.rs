// No console window on Windows.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() -> eframe::Result {
    padtune::gui::run()
}
