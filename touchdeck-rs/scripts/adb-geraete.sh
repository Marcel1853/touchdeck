#!/usr/bin/env bash
# Android-Geräte per drahtlosem Debugging koppeln, verbinden und die TouchDeck-App aufspielen.
#
#   scripts/adb-geraete.sh koppeln IP:PORT CODE [IP:PORT CODE …]
#       Koppelt Geräte mit dem Code aus „Drahtloses Debugging › Gerät mit Kopplungscode koppeln“
#       (IP:PORT aus genau diesem Dialog, nicht die Adresse oben auf der Seite) und verbindet sie.
#       Ohne Angaben wird gefragt.
#   scripts/adb-geraete.sh verbinden
#       Verbindet alle schon gekoppelten Geräte, die im WLAN drahtloses Debugging an haben.
#   scripts/adb-geraete.sh app
#       Spielt downloads/TouchDeck.apk auf alle verbundenen Geräte (nur Hauptnutzer, --user 0,
#       sonst landet die App auf dem Samsung auch im Zweitprofil / Sicheren Ordner).
#   scripts/adb-geraete.sh liste
#       Zeigt verbundene Geräte mit Modell und installierter App-Version.
#
# Nach einem Neustart des Handys wechselt der Port des drahtlosen Debuggings; die Kopplung bleibt
# aber gespeichert – dann reicht „verbinden“. Ist sie weg, wieder „koppeln“.
set -e
DIR="$(cd "$(dirname "$0")/.." && pwd)"
. "$DIR/../env.sh"
APK="$DIR/downloads/TouchDeck.apk"

geraete() {
  # Ein Gerät kann zweimal verbunden sein (per IP und per mDNS-Name): nur einmal nehmen.
  local seen=" "
  for d in $(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }'); do
    serial=$(adb -s "$d" shell getprop ro.serialno 2>/dev/null | tr -d '\r')
    case "$seen" in *" $serial "*) continue ;; esac
    seen="$seen$serial "
    echo "$d"
  done
}

verbinden() {
  # Gekoppelte Geräte melden sich per mDNS; adb verbindet sie meist von selbst, sonst hier.
  adb mdns services 2>/dev/null | awk '/_adb-tls-connect/ { print $NF }' | while read -r addr; do
    adb connect "$addr" >/dev/null 2>&1 || true
  done
  sleep 1
  liste
}

liste() {
  local n=0
  for d in $(geraete); do
    n=$((n + 1))
    model=$(adb -s "$d" shell getprop ro.product.model 2>/dev/null | tr -d '\r')
    version=$(adb -s "$d" shell dumpsys package de.touchdeck.app 2>/dev/null | awk -F= '/versionName/ { print $2; exit }' | tr -d '\r')
    echo "  $model – TouchDeck-App ${version:-nicht installiert}  ($d)"
  done
  [ "$n" -gt 0 ] || echo "  Kein Gerät verbunden. Drahtloses Debugging an? Sonst: $0 koppeln"
}

koppeln() {
  if [ $# -eq 0 ]; then
    read -rp "IP:PORT aus dem Kopplungsdialog: " addr
    read -rp "Kopplungscode (6 Ziffern): " code
    set -- "$addr" "$code"
  fi
  while [ $# -ge 2 ]; do
    addr="$1"
    code="${2// /}"
    shift 2
    echo "Koppeln: $addr"
    adb pair "$addr" "$code" || echo "  Hat nicht geklappt – Code und Port stehen im Kopplungsdialog und gelten nur, solange er offen ist."
  done
  # Nach dem Koppeln brauchen die Geräte einen Moment, bis sie sich per mDNS melden.
  sleep 3
  verbinden
}

app() {
  [ -f "$APK" ] || { echo "Keine App unter $APK – erst scripts/build-android.sh"; exit 1; }
  for d in $(geraete); do
    model=$(adb -s "$d" shell getprop ro.product.model | tr -d '\r')
    echo "App auf $model …"
    adb -s "$d" install -r --user 0 "$APK" | tail -1
  done
  liste
}

case "${1:-}" in
  koppeln) shift; koppeln "$@" ;;
  verbinden) verbinden ;;
  app) app ;;
  liste) liste ;;
  *) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//' ;;
esac
