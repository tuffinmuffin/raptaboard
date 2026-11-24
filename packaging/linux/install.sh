#!/bin/sh
# Install raptaboard for this user only (no root): the programs (raptaboard,
# and raptaboard-cli for its commands, such as `raptaboard-cli packs list`) in
# ~/.local/bin, the menu entry and icon in ~/.local/share.
#   ./install.sh             install (or update)
#   ./install.sh --uninstall remove it again (settings stay in ~/.raptaboard)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$bin/raptaboard" "$bin/raptaboard-cli" "$apps/raptaboard.desktop" "$icons/raptaboard.png"
    echo "raptaboard removed. Settings and images stay in ~/.raptaboard."
    exit 0
fi

mkdir -p "$bin" "$apps" "$icons"
install -m 755 "$here/raptaboard" "$bin/raptaboard"
install -m 755 "$here/raptaboard-cli" "$bin/raptaboard-cli"
install -m 644 "$here/raptaboard.png" "$icons/raptaboard.png"
sed "s|^Exec=raptaboard|Exec=$bin/raptaboard|" "$here/raptaboard.desktop" > "$apps/raptaboard.desktop"
chmod 644 "$apps/raptaboard.desktop"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps" || true
echo "raptaboard installed: $bin/raptaboard (and raptaboard-cli), and in your applications menu."
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "To run it from a terminal by name, add $bin to your PATH." ;;
esac
