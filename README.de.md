# TouchDeck

[English](README.md) · **Deutsch**

Makro-Fernbedienung für Linux: Button-Seiten am PC gestalten und vom Handy
oder Tablet aus auslösen – Programme starten, Tastenkürzel, Medien und
Lautstärke, Steam-Spiele, OBS Studio, Twitch, Discord und vieles mehr.

| Ordner | Inhalt |
|---|---|
| [`touchdeck-rs/`](touchdeck-rs/) | PC-Programm in Rust (Tauri): Server, Aktionen und Integrationen, Editor und Bedienfeld, Fenster mit Symbol in der Systemleiste |
| [`android/`](android/) | Android-App (Kotlin): findet den PC im WLAN, koppelt per Bestätigung oder QR-Code, zeigt das Bedienfeld im Vollbild |

## Funktionen

- **Editor** mit Seiten, Buttons, Live-Anzeigen, Werten (Variablen), Events
  („Wenn …, dann …“), Vorlagen, Logs, Backups; Rechtsklick auf Buttons zum
  Duplizieren, Kopieren, Ausschneiden, Einfügen, Aussehen übertragen und
  Verschieben auf andere Seiten
- **Seiten** als **Raster** (mit eigenen Spaltenbreiten und Zeilenhöhen) oder
  **frei** (Buttons beliebig platziert, Position und Größe in Prozent, auf einer
  Fläche mit gewähltem Seitenverhältnis); jede Seite kann eigene Events haben,
  z. B. mit dem Auslöser „Seite geöffnet / verlassen“
- **Buttons**: Drücken, Umschalter und Regler, dazu Aktionen beim Halten und beim
  Doppeldruck, eigenes Aussehen „Wenn an“ und Events je Button
- **Fertige Seiten** für System, OBS und Steam, nur aus normalen, sichtbaren
  Aktionen und Events gebaut; Steam-Seiten passen sich automatisch an, wenn Spiele
  installiert oder entfernt werden, und laden fehlende Cover von Steam
- **Rund 80 Aktionen**: Programme, Tastatur und Maus, Fenster, Audio und
  Medien (PipeWire, MPRIS), Soundboard, System, Dateien, HTTP, Steam,
  OBS Studio (obs-websocket 5), Twitch (eigene Client-ID, Geräte-Code-Anmeldung),
  Discord (Sprachsteuerung über das Discord-Programm, Webhooks)
- **Handy und Tablet**: Android-App oder Bedienfeld im Browser; mehrere PCs,
  automatische Suche im WLAN, Kopplung mit Bestätigung am PC, optionale PIN;
  Form der Felder „Fläche füllen“ oder „Quadratisch“
- **Deutsch und Englisch**: die ganze Oberfläche (Editor, Bedienfeld, Android-App,
  Meldungen des Servers) richtet sich nach der Systemsprache und lässt sich unter
  Einstellungen › Allgemein umstellen; Plugins können eigene Übersetzungen mitbringen
- **Icon-Pakete**: eigene Bildersammlungen anlegen, exportieren und weitergeben
- **Plugins**: Erweiterungen in JavaScript, abgeschottet und nur aus öffentlichen
  GitHub-Repos, mit Marktplatz; dazu erweiterte Plugins mit vollen Rechten nur nach
  doppelter Bestätigung ([Anleitung](touchdeck-rs/PLUGINS.de.md))
- **Desktop-Programm** mit Symbol in der Systemleiste, eigenem Starter-Eintrag mit
  Symbol, Autostart (zeigt immer auf die laufende Version) und Firewall-Freigabe
  fürs Heimnetz

Ausführliche Beschreibung: [`touchdeck-rs/README.de.md`](touchdeck-rs/README.de.md)

## Bauen

Voraussetzungen: Rust, Java 17+ und ein Android SDK (für die App). `env.sh` setzt alle
Pfade auf Ordner im Projekt (`.rustup/`, `.cargo/`, `.android-sdk/`, `.cache/`), damit
nichts systemweit installiert werden muss.

```sh
source env.sh
cd touchdeck-rs
cargo test                     # automatische Tests
scripts/build-android.sh       # Android-App bauen (signiert mit android/keystore/, siehe unten)
cd app && XDG_CACHE_HOME="$PWD/../../.cache" cargo tauri build   # .deb und AppImage
```

Die Android-App wird mit einem eigenen Schlüssel signiert. Er liegt **nicht**
im Repository: `android/keystore/keystore.properties` mit `storeFile`,
`storePassword`, `keyAlias` und `keyPassword` anlegen (Schlüssel mit
`keytool -genkeypair` erzeugen). Ohne ihn lässt sich `assembleDebug` bauen.

## Plattform

Linux mit X11 (Tastenkürzel und Mausaktionen nutzen XTest), PipeWire,
D-Bus/MPRIS. Getestet unter Linux Mint 22 (Cinnamon) und Android 16.
