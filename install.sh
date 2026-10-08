#!/usr/bin/env bash
# Instaluje/aktualizuje Modbus Dashboard w /opt/modbus-dashboard (binarka Rust + skrót). Uruchom jako root.
# Na X13 robi to automatycznie target/apply.sh z cachyos_sync po każdym nowym commicie w tym repo.
# Od 2.0 jedna binarka z wbudowanym interfejsem (wcześniej Python + venv z pywebview).
set -euo pipefail
SRC="$(cd "$(dirname "$0")" && pwd)"
DEST=/opt/modbus-dashboard
[[ $EUID -eq 0 ]] || { echo "Uruchom przez sudo"; exit 1; }

# Okno to WebKitGTK (wry/tao) - potrzebne do budowania i działania.
pacman -S --needed --asdeps --noconfirm webkit2gtk-4.1 gtk3

# Budowanie jako zwykły użytkownik (build.sh).
BIN=$("$SRC/build.sh" "$SRC")

rm -rf "$DEST/.venv" "$DEST/static" "$DEST"/*.py "$DEST/__pycache__"  # wersja w Pythonie
install -Dm755 "$BIN" "$DEST/modbus-dashboard"
install -Dm644 "$SRC/modbus-dashboard.desktop" /usr/local/share/applications/modbus-dashboard.desktop
install -Dm644 "$SRC/modbus-dashboard.svg" /usr/local/share/icons/hicolor/scalable/apps/modbus-dashboard.svg
gtk-update-icon-cache -qtf /usr/local/share/icons/hicolor 2>/dev/null || true
