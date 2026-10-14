#!/bin/sh
# Richtet TouchDeck für den aktuellen Benutzer ein: Menüeintrag und optional Autostart beim Anmelden.
# Es wird nichts systemweit installiert. Rückgängig: install.sh --remove
set -e
DIR="$(cd "$(dirname "$0")/.." && pwd)"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
AUTOSTART="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
ICON="$DIR/public/shared/icon.svg"

if [ "$1" = "--remove" ]; then
  rm -f "$APPS/touchdeck.desktop" "$AUTOSTART/touchdeck.desktop"
  echo "TouchDeck-Menüeintrag und Autostart entfernt."
  exit 0
fi

mkdir -p "$APPS"
cat > "$APPS/touchdeck.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=TouchDeck
Comment=Makro-Fernbedienung: Buttons am PC gestalten, vom Handy auslösen
Exec="$DIR/start.sh"
Icon=$ICON
Terminal=false
Categories=Utility;
DESKTOP
echo "Menüeintrag angelegt: $APPS/touchdeck.desktop"

if [ "$1" = "--autostart" ]; then
  mkdir -p "$AUTOSTART"
  sed 's|^Exec=.*|Exec="'"$DIR"'/start.sh" --no-open|' "$APPS/touchdeck.desktop" > "$AUTOSTART/touchdeck.desktop"
  echo "Autostart eingerichtet: TouchDeck startet beim Anmelden im Hintergrund."
fi
