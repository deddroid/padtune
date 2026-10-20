#!/bin/sh
# Install bigbigctl for the current user in ~/.local (no root needed),
# except the optional udev rule (--udev, uses sudo).
#
#   ./install.sh            install binaries, desktop entry and icons
#   ./install.sh --udev     also install the udev rule (asks for sudo)
#   ./install.sh --uninstall
set -eu

here=$(cd "$(dirname "$0")" && pwd)
prefix="${PREFIX:-$HOME/.local}"
rule=/etc/udev/rules.d/70-bigbigctl.rules
udev=0
uninstall=0
for arg in "$@"; do
    case "$arg" in
        --udev) udev=1 ;;
        --uninstall) uninstall=1 ;;
        -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

# Files may come from a release archive (flat) or a source checkout.
find_file() {
    for p in "$@"; do [ -e "$here/$p" ] && { echo "$here/$p"; return; }; done
    return 1
}

if [ "$uninstall" = 1 ]; then
    rm -f "$prefix/bin/bigbigctl" "$prefix/bin/bigbigctl-gui" \
          "$prefix/share/applications/bigbigctl.desktop" \
          "$HOME/.config/autostart/bigbigctl.desktop"
    for s in 16 32 48 64 128 256 512; do
        rm -f "$prefix/share/icons/hicolor/${s}x${s}/apps/bigbigctl.png"
    done
    rm -f "$prefix/share/icons/hicolor/scalable/apps/bigbigctl.svg"
    if [ -e "$rule" ]; then
        echo "Removing $rule (sudo)"
        sudo rm -f "$rule" && sudo udevadm control --reload
    fi
    echo "Uninstalled. Settings are kept in ${XDG_CONFIG_HOME:-$HOME/.config}/bigbigctl."
    exit 0
fi

cli=$(find_file bigbigctl target/release/bigbigctl) || { echo "bigbigctl binary not found (run cargo build --release)" >&2; exit 1; }
gui=$(find_file bigbigctl-gui target/release/bigbigctl-gui) || gui=""
install -Dm755 "$cli" "$prefix/bin/bigbigctl"
if [ -n "$gui" ]; then
    install -Dm755 "$gui" "$prefix/bin/bigbigctl-gui"
    desktop=$(find_file bigbigctl.desktop packaging/linux/bigbigctl.desktop)
    install -Dm644 "$desktop" "$prefix/share/applications/bigbigctl.desktop"
    for s in 16 32 48 64 128 256 512; do
        icon=$(find_file "icons/icon-$s.png" "assets/icon-$s.png") && \
            install -Dm644 "$icon" "$prefix/share/icons/hicolor/${s}x${s}/apps/bigbigctl.png"
    done
    svg=$(find_file icons/icon.svg assets/icon.svg) && \
        install -Dm644 "$svg" "$prefix/share/icons/hicolor/scalable/apps/bigbigctl.svg"
    command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" 2>/dev/null || true
fi

if [ "$udev" = 1 ]; then
    src=$(find_file 70-bigbigctl.rules packaging/linux/70-bigbigctl.rules)
    echo "Installing $rule (sudo)"
    sudo install -Dm644 "$src" "$rule"
    sudo udevadm control --reload
    sudo udevadm trigger
else
    echo "Note: without the udev rule the controller is only accessible as root. Run: $0 --udev"
fi
echo "Installed in $prefix. Make sure $prefix/bin is in your PATH."
