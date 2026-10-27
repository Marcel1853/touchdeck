# TouchDeck

[English](README.md) · **Deutsch**

Makro-Fernbedienung für Linux: Button-Seiten am PC gestalten und vom Handy
oder Tablet aus auslösen – Programme starten, Tastenkürzel, Medien und
Lautstärke, Steam-Spiele, OBS Studio, Twitch, Discord und vieles mehr.

| Ordner | Inhalt |
|---|---|
| [`touchdeck-rs/`](touchdeck-rs/) | PC-Programm in Rust (Tauri), Version 0.12.1: Server, Aktionen und Integrationen, Editor und Bedienfeld, Fenster mit Symbol in der Systemleiste |
| [`android/`](android/) | Android-App (Kotlin), Version 0.8.3, auf Deutsch und Englisch: findet den PC im WLAN, koppelt per Bestätigung oder QR-Code, zeigt das Bedienfeld im Vollbild |

Änderungen je Version: [`CHANGELOG.de.md`](CHANGELOG.de.md)

## Funktionen

- **Editor** mit Seiten, Buttons, Live-Anzeigen, Werten (Variablen), Events
  („Wenn …, dann …“), Vorlagen (wiederverwendbare Aktionslisten), Logs, Backups;
  Rechtsklick auf Buttons zum Duplizieren, Kopieren, Ausschneiden, Einfügen,
  Aussehen übertragen, Logik kopieren/einfügen und Verschieben auf andere Seiten
- **Statusleiste** unten im Editor: verbundene Geräte mit Name, Adresse und der
  angezeigten Seite, Zustand von OBS, Twitch und Discord, dazu Adresse und Version
- **Seiten** als **Raster** (mit eigenen Spaltenbreiten und Zeilenhöhen) oder
  **frei** (Buttons beliebig platziert, Position und Größe in Prozent, auf einer
  Fläche mit gewähltem Seitenverhältnis – Handy und Tablet zeigen sie genau in
  diesem Format); jede Seite kann eigene Events haben, z. B. mit dem Auslöser
  „Seite geöffnet / verlassen“. Neue Buttons entstehen per Doppelklick oder
  „＋ Button“ (ein einfacher Klick hebt nur die Auswahl auf); Rechtsklick ›
  „Ordner hier anlegen“ legt eine Unterseite mit Zurück-Button an
- **Profile**: eine oder alle Seiten als `.tdprofile`-Datei exportieren, samt den
  benutzten Bildern, Vorlagen, Werten und Events, und anderswo importieren – der
  Import fügt nur hinzu und überschreibt nie etwas
- **Buttons**: Drücken, Umschalter und Regler, dazu Aktionen beim Halten und beim
  Doppeldruck, eigenes Aussehen „Wenn an“ und Events je Button
- **Fertige Seiten** für System, OBS und Steam, nur aus normalen, sichtbaren
  Aktionen und Events gebaut; Steam-Seiten passen sich automatisch an, wenn Spiele
  installiert oder entfernt werden, und laden fehlende Cover von Steam
- **Rund 80 Aktionen**: Programme, Tastatur und Maus, Fenster, Audio und
  Medien (PipeWire, MPRIS), Soundboard, System, Dateien, HTTP, Timer (Countdown /
  Stoppuhr), Zufallsauswahl, Steam, OBS Studio (obs-websocket 5), Twitch (eigene
  Client-ID, Geräte-Code-Anmeldung), Discord (Sprachsteuerung über das
  Discord-Programm, Webhooks); Auslöser wie „Programm im Vordergrund“ (z. B.
  wechseln Handy und Tablet zur OBS-Seite, sobald OBS vorn ist) oder „Timer
  abgelaufen“
- **Handy und Tablet**: Android-App oder Bedienfeld im Browser; mehrere PCs,
  automatische Suche im WLAN, Kopplung mit Bestätigung am PC, optionale PIN;
  Einstellung „Form der Felder“: „Fläche füllen“ oder „Quadratisch“
- **Deutsch und Englisch**: die ganze Oberfläche (Editor, Bedienfeld, Meldungen
  des Servers, Menü in der Systemleiste) richtet sich nach der Systemsprache und
  lässt sich unter Einstellungen › Allgemein umstellen. Auch die **Android-App**
  gibt es auf Deutsch und Englisch; sie folgt der Sprache, die in TouchDeck am PC
  eingestellt ist – auch im Verbindungsbildschirm und in Dialogen (vor der ersten
  Verbindung: Sprache des gefundenen PCs, sonst die Sprache des Handys). Plugins
  können eigene Übersetzungen mitbringen
- **Icon-Pakete**: eigener Bereich links in der Leiste des Editors (🖼️ Icon-Pakete,
  neben Plugins): eigene Bildersammlungen anlegen, exportieren und weitergeben
- **Plugins**: Erweiterungen in JavaScript, abgeschottet und nur aus öffentlichen
  GitHub-Repos, mit Marktplatz; dazu erweiterte Plugins mit vollen Rechten nur nach
  doppelter Bestätigung ([Anleitung](touchdeck-rs/PLUGINS.de.md))
- **Desktop-Programm** mit Symbol in der Systemleiste (Menü in der gewählten
  Sprache), eigenem Starter-Eintrag mit Symbol, Autostart (zeigt immer auf die
  laufende Version) und Firewall-Freigabe fürs Heimnetz

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

**Windows: ganz frühe Alpha.** TouchDeck lässt sich für Windows 10/11 bauen,
und eine GitHub-Aktion erstellt einen Installer (unter „Artifacts“ der Aktion
„Windows“), aber er ist **noch nie auf einem echten Windows-PC getestet**
worden. Mit Fehlern ist zu rechnen. Unter Windows fehlen bisher: Audiogeräte
wechseln, Nachtlicht, aktueller Medientitel, Netzwerkrate, Auslöser „PC
gesperrt“ und Ton-Auslöser; Töne spielen nur WAV-Dateien. Plugins, die
Linux-Programme brauchen (z. B. xclip), laufen dort nicht. Rückmeldungen gern
als GitHub-Issue.
