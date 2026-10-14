# TouchDeck (Rust)

Die Rust-Fassung von TouchDeck: gleiches Bedienfeld, gleicher Editor, gleiche Daten.
Server, Aktionen, Events und Integrationen sind komplett in Rust geschrieben;
das Programmfenster nutzt Tauri mit der Web-Ansicht des Systems statt Electron.

| | bisher (Electron) | Rust |
|---|---|---|
| .deb-Paket | 100 MB | 7,2 MB |
| AppImage (bringt WebKitGTK mit) | 126 MB | 89 MB |
| Tastatur, Maus, Fenster | Python-Hilfsskript | direkt in Rust (X11/XTest) |
| Laufzeit | Node.js + Chromium | ein Programm + WebKitGTK |

Die Daten liegen wie bisher in `~/.config/TouchDeck/data`. Beide Fassungen können
dieselbe `config.json` lesen und schreiben, nur nicht gleichzeitig laufen
(beide nutzen Port 8770).

## Aufbau

```
touchdeck-rs/
├── server/                 Bibliothek + Kommandozeilen-Server (touchdeck-server)
│   ├── catalog/            Beschreibung der Aktionen und Auslöser für den Editor
│   └── src/
│       ├── core/           Daten (store), Aktionen, Events, Live-Werte, Icon-Pakete, Plugins, Protokoll, Sicherungen
│       ├── server/         HTTP-Routen, WebSocket, Suche im WLAN
│       ├── system/         Linux: Programme, Audio, Medien (linux.rs), Tastatur/Maus/Fenster (x11.rs)
│       └── integrations/   OBS, Steam, Twitch, Discord, Import aus Touch Portal
├── app/                    Desktop-Programm (Tauri): Fenster, Symbol in der Systemleiste, Autostart
└── plugins/beispiel/       Beispiel-Plugin (Vorlage für eigene Plugins)
```

Editor und Bedienfeld (HTML/JS) kommen unverändert aus `../touchdeck/public`
und werden beim Bauen ins Paket übernommen.

## Icon-Pakete

Eigene Bildersammlungen (Einstellungen › Icon-Pakete): anlegen, Bilder hinzufügen,
als `.tdicons`-Datei exportieren und weitergeben, Pakete anderer importieren.
Abgelegt im Datenordner unter `icons/packs/`.

## Plugins

Erweiterungen in JavaScript, abgeschottet in einer eigenen JS-Engine (QuickJS) mit
festen Grenzen und Berechtigungen; Installation nur aus öffentlichen GitHub-Repos,
dazu ein Marktplatz und ein Testmodus für Entwickler. Anleitung: [PLUGINS.md](PLUGINS.md).

## Entwickeln

Alles liegt im Projektordner (Rust, Cargo-Cache, Werkzeuge):

```bash
source ../env.sh
cargo test                     # Tests
cargo run -p touchdeck         # Programm mit Fenster
cargo run -p touchdeck-server  # nur der Server, Editor im Browser
```

Nützliche Umgebungsvariablen:

| Variable | Bedeutung |
|---|---|
| `TOUCHDECK_PORT` | anderer Port (Standard 8770), z. B. zum Testen neben einer laufenden Fassung |
| `TOUCHDECK_DATA` | anderer Datenordner |
| `TOUCHDECK_PUBLIC` | anderer Ordner für Editor und Bedienfeld |

Zum Bauen des Fensters braucht das System einmalig:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

## Installationspakete bauen

```bash
source ../env.sh
cd app
XDG_CACHE_HOME="$PWD/../../.cache" cargo tauri build
```

Ergebnis in `target/release/bundle/`: ein `.deb` (Firewall wird beim Installieren
fürs Heimnetz geöffnet und beim Entfernen wieder geschlossen) und ein AppImage.
