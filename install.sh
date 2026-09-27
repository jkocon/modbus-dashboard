#!/usr/bin/env bash
# Instaluje/aktualizuje Modbus Dashboard w /opt/modbus-dashboard (venv + skrót). Uruchom jako root.
# Na X13 robi to automatycznie target/apply.sh po każdej zmianie w katalogu modbus-dashboard/.
# Na X16 aplikacja działa prosto z repo (webapp/.venv), tego skryptu tam nie trzeba.
set -euo pipefail
SRC="$(cd "$(dirname "$0")" && pwd)"
DEST=/opt/modbus-dashboard
[[ $EUID -eq 0 ]] || { echo "Uruchom przez sudo"; exit 1; }

# pywebview na Linuksie używa backendu GTK (WebKitGTK).
pacman -S --needed --asdeps --noconfirm python-gobject webkit2gtk-4.1

mkdir -p "$DEST"
rsync -a --delete --exclude .venv --exclude __pycache__ --exclude dist --exclude build \
    --exclude tests "$SRC/webapp/" "$DEST/"
[[ -x $DEST/.venv/bin/python3 ]] || python3 -m venv --system-site-packages "$DEST/.venv"
# Tylko to, czego nie ma w pakietach systemowych (requirements.txt zawiera też pyinstaller).
"$DEST/.venv/bin/pip" install --quiet --upgrade pywebview pyserial

install -Dm644 "$SRC/modbus-dashboard.desktop" /usr/local/share/applications/modbus-dashboard.desktop
