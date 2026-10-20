#!/bin/sh
# Install padtune for the current user in ~/.local (no root needed),
# except the optional udev rule (--udev, uses sudo).
#
#   ./install.sh            install binaries, desktop entry and icons
#   ./install.sh --udev     also install the udev rule (asks for sudo)
#   ./install.sh --uninstall
set -eu

here=$(cd "$(dirname "$0")" && pwd)
prefix="${PREFIX:-$HOME/.local}"
rule=/etc/udev/rules.d/70-padtune.rules
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
    rm -f "$prefix/bin/padtune" "$prefix/bin/padtune-gui" \
          "$prefix/share/applications/padtune.desktop" \
          "$HOME/.config/autostart/padtune.desktop"
    for s in 16 32 48 64 128 256 512; do
        rm -f "$prefix/share/icons/hicolor/${s}x${s}/apps/padtune.png"
    done
    rm -f "$prefix/share/icons/hicolor/scalable/apps/padtune.svg"
    if [ -e "$rule" ]; then
        echo "Removing $rule (sudo)"
        sudo rm -f "$rule" && sudo udevadm control --reload
    fi
    echo "Uninstalled. Settings are kept in ${XDG_CONFIG_HOME:-$HOME/.config}/padtune."
    exit 0
fi

cli=$(find_file padtune target/release/padtune) || { echo "padtune binary not found (run cargo build --release)" >&2; exit 1; }
gui=$(find_file padtune-gui target/release/padtune-gui) || gui=""
install -Dm755 "$cli" "$prefix/bin/padtune"
if [ -n "$gui" ]; then
    install -Dm755 "$gui" "$prefix/bin/padtune-gui"
    desktop=$(find_file padtune.desktop packaging/linux/padtune.desktop)
    install -Dm644 "$desktop" "$prefix/share/applications/padtune.desktop"
    for s in 16 32 48 64 128 256 512; do
        icon=$(find_file "icons/icon-$s.png" "assets/icon-$s.png") && \
            install -Dm644 "$icon" "$prefix/share/icons/hicolor/${s}x${s}/apps/padtune.png"
    done
    svg=$(find_file icons/icon.svg assets/icon.svg) && \
        install -Dm644 "$svg" "$prefix/share/icons/hicolor/scalable/apps/padtune.svg"
    command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$prefix/share/icons/hicolor" 2>/dev/null || true
fi

if [ "$udev" = 1 ]; then
    src=$(find_file 70-padtune.rules packaging/linux/70-padtune.rules)
    echo "Installing $rule (sudo)"
    sudo install -Dm644 "$src" "$rule"
    sudo udevadm control --reload
    sudo udevadm trigger
else
    echo "Note: without the udev rule the controller is only accessible as root. Run: $0 --udev"
fi
echo "Installed in $prefix. Make sure $prefix/bin is in your PATH."
