# TouchDeck

Makro-Fernbedienung für Linux: Button-Seiten am PC gestalten und vom Handy
oder Tablet aus auslösen – Programme starten, Tastenkürzel, Medien und
Lautstärke, Steam-Spiele, OBS Studio, Twitch, Discord und vieles mehr.

| Ordner | Inhalt |
|---|---|
| [`touchdeck-rs/`](touchdeck-rs/) | PC-Programm in Rust (Tauri): Server, Aktionen und Integrationen, Fenster mit Symbol in der Systemleiste – kleineres Paket, kein Node.js oder Python nötig |
| [`touchdeck/`](touchdeck/) | Bisherige Fassung (Node.js + Electron) sowie Editor und Bedienfeld (HTML/JS), die auch die Rust-Fassung nutzt |
| [`android/`](android/) | Android-App (Kotlin): findet den PC im WLAN, koppelt per Bestätigung oder QR-Code, zeigt das Bedienfeld im Vollbild |

## Funktionen

- **Editor** mit Seiten, Buttons (Drücken, Umschalter, Regler), Live-Anzeigen,
  Werten (Variablen), Events („Wenn …, dann …“), Vorlagen, Logs, Backups;
  Rechtsklick auf Buttons zum Duplizieren, Kopieren, Ausschneiden, Einfügen,
  Aussehen übertragen und Verschieben auf andere Seiten
- **Über 60 Aktionen**: Programme, Tastatur und Maus, Fenster, Audio und
  Medien (PipeWire, MPRIS), Soundboard, System, Dateien, HTTP, Steam,
  OBS Studio (obs-websocket 5), Twitch (eigene Client-ID, Geräte-Code-Anmeldung),
  Discord (Sprachsteuerung über das Discord-Programm, Webhooks)
- **Handy und Tablet**: Android-App oder Bedienfeld im Browser; mehrere PCs,
  automatische Suche im WLAN, Kopplung mit Bestätigung am PC, optionale PIN
- **Icon-Pakete**: eigene Bildersammlungen anlegen, exportieren und weitergeben
- **Plugins**: Erweiterungen in JavaScript, abgeschottet und nur aus öffentlichen
  GitHub-Repos, mit Marktplatz; dazu erweiterte Plugins mit vollen Rechten nur nach
  doppelter Bestätigung ([Anleitung](touchdeck-rs/PLUGINS.md))
- **Desktop-Programm** mit Symbol in der Systemleiste, Autostart und
  Firewall-Freigabe fürs Heimnetz

Ausführliche Beschreibung: [`touchdeck/README.md`](touchdeck/README.md),
Rust-Fassung: [`touchdeck-rs/README.md`](touchdeck-rs/README.md)

## Bauen

Voraussetzungen: Rust (für die Rust-Fassung), Node.js ab 22 (bisherige Fassung),
Java 17+ und ein Android SDK (für die App). `env.sh` setzt alle Pfade auf Ordner
im Projekt (`.rustup/`, `.cargo/`, `.node/`, `.android-sdk/`, `.cache/`), damit
nichts systemweit installiert werden muss.

```sh
source env.sh
cd touchdeck-rs
cargo test                                            # automatische Tests
cd app && XDG_CACHE_HOME="$PWD/../../.cache" cargo tauri build   # .deb und AppImage
```

Bisherige Fassung und Android-App:

```sh
source env.sh
cd touchdeck
npm install
npm test             # automatische Tests
npm run android      # Android-App bauen (signiert mit android/keystore/, siehe unten)
npm run dist         # PC-Pakete bauen: dist/*.deb und dist/*.AppImage
```

Die Android-App wird mit einem eigenen Schlüssel signiert. Er liegt **nicht**
im Repository: `android/keystore/keystore.properties` mit `storeFile`,
`storePassword`, `keyAlias` und `keyPassword` anlegen (Schlüssel mit
`keytool -genkeypair` erzeugen). Ohne ihn lässt sich `assembleDebug` bauen.

## Plattform

Linux mit X11 (Tastenkürzel und Mausaktionen nutzen XTest), PipeWire,
D-Bus/MPRIS. Getestet unter Linux Mint 22 (Cinnamon) und Android 16.
