//! Embeds the application icon and version info in the Windows executables.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "PadTune (padtune)");
        res.set("FileDescription", "Configurator for BIGBIG WON controllers");
        if let Err(e) = res.compile() {
            // A missing resource compiler must not break the build.
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
