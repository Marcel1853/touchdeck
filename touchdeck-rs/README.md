# TouchDeck

Makro-Fernbedienung für Linux: Button-Seiten am PC gestalten und vom Handy oder Tablet
aus auslösen – mit Editor, Android-App und Bedienfeld im Browser. Geschrieben in Rust,
das Programmfenster nutzt Tauri mit der Web-Ansicht des Systems.

## Installieren

```sh
sudo apt install ./TouchDeck_<version>_amd64.deb   # mit Menüeintrag
# oder ohne Installation: AppImage ausführbar machen und doppelklicken
./TouchDeck_<version>_amd64.AppImage
```

- Das `.deb` öffnet beim Installieren die Firewall (ufw) fürs Heimnetz und schließt
  sie beim Entfernen wieder. Python oder Node.js werden nicht gebraucht.
- Fenster schließen = TouchDeck läuft im Hintergrund weiter (Handys bleiben
  verbunden). Über das Symbol in der Systemleiste: Editor öffnen, Handy verbinden,
  Vorschau, **Beim Anmelden starten**, Beenden.
- Ein zweiter Start holt das vorhandene Fenster nach vorn.
- Sprache: Deutsch oder Englisch, voreingestellt nach der Systemsprache.
- Daten liegen in `~/.config/TouchDeck/data/`.
- Deinstallieren: `sudo apt remove touchdeck`

## Handy verbinden

**Mit der Android-App (empfohlen):**

1. Im Editor **📱 Handy verbinden** öffnen und den QR-Code unter „Android-App“
   scannen – die App wird vom PC heruntergeladen. Beim ersten Mal „Unbekannte Apps
   installieren“ für den Browser erlauben.
2. App öffnen: Sie findet den PC im WLAN von selbst. Antippen → am PC erscheint
   „… möchte sich verbinden“ → **Erlauben**.
3. Fertig: Bedienfeld im Vollbild, Bildschirm bleibt an, Vibration, die Zurück-Taste
   blättert Seiten zurück. Die App merkt sich mehrere PCs.

Alternativ in der App „QR-Code vom PC scannen“ oder die Adresse von Hand eingeben.
**Ohne App:** den QR-Code im Dialog „Handy verbinden“ mit der Handykamera scannen –
das Bedienfeld öffnet sich im Browser.

Der Editor ist nur am PC selbst erreichbar; Handys brauchen den Schlüssel (per
Bestätigung oder QR-Code) und – falls gesetzt – die PIN. **Neuen Schlüssel erzeugen**
meldet alle Geräte ab.

## Aktionen

Rund 80 Aktionen in Gruppen:

| Gruppe | Aktionen |
|---|---|
| Allgemein | Webseiten, Dateien, Programme und Befehle, Seiten wechseln, Tastenkürzel, Text tippen, Tasten halten, Maus, Fenster nach vorn holen, Programme beenden, Benachrichtigungen, Bildschirmfoto, Sperren, Energie, Nachtlicht, Hell/Dunkel, Wake-on-LAN, HTTP, Dateien schreiben, Werte, Bedingungen, Vorlagen, Aussehen von Buttons ändern |
| Audio, Musik & Medien | Mediensteuerung (MPRIS), Lautstärke und Mikrofon (PipeWire), Audioausgang/-eingang wechseln, Soundboard |
| Streaming | OBS Studio (Szenen, Stream, Aufnahme, Clips, Audioquellen, Quellen, Filter, Texte, Übergänge, Profile …), Twitch |
| Kommunikation | Discord (stumm, taub, Sprachkanal, Lautstärke, Push-to-Talk, Webhooks) |
| Spiele | Steam (Spiele starten, Bibliothek, Downloads, Big Picture …) |
| Plugins | Aktionen installierter Plugins |

Werte erscheinen in Beschriftungen über `{Name}` (z. B. „Tore: {Tore}“), als
Live-Anzeige und in OBS-Texten. `{Zeit}` und `{Datum}` gibt es immer.

Buttons gibt es als **Drücken**, **Umschalter** (an/aus mit eigener Farbe und
eigenen Aktionen) und **Regler** (Lautstärke, Mikrofon, OBS-Audioquelle oder eigener
Befehl mit `{value}`). Jeder Button kann eine **Live-Anzeige** haben (Uhrzeit, CPU,
RAM, Temperatur, Lautstärke, Musiktitel, Steam, OBS, Twitch, Discord, Werte,
Befehlsausgabe), auf **Events** reagieren und beim **Halten** eigene Aktionen
ausführen. Rechtsklick auf einen Button: Duplizieren, Kopieren, Ausschneiden,
Einfügen, Aussehen übertragen, auf andere Seiten verschieben.

## Bereiche im Editor

Links in der Leiste (Strg+1 … Strg+7):

- **Seiten** – Raster gestalten; fertige Seiten für Steam, System, OBS und den Import
  von Touch-Portal-Seiten.
- **Werte** – eigene Variablen mit Startwert, Ordner und Beschreibung.
- **Events** – „Wenn …, dann …“: Wert ändert sich, Start, Zeitgeber, Uhrzeit, Datei,
  Handy, Audio, Steam, OBS, Twitch, Discord, Plugins. Mit Abklingzeit; Endlosschleifen
  werden abgefangen.
- **Vorlagen** – wiederverwendbare Aktionslisten.
- **Logs** – alles, was passiert, live.
- **Plugins** – installierte Plugins, Marktplatz, Entwickeln, erweiterter Modus.
- **Einstellungen** – Allgemein, Handy & Sicherheit, Bedienfeld, Aussehen,
  Integrationen, Icon-Pakete, Backups, Protokoll, Information.

## Twitch einrichten

1. Auf <https://dev.twitch.tv/console> eine Anwendung registrieren: Redirect-URL
   `http://localhost`, Kategorie „Application Integration“, Client-Typ **Öffentlich**.
2. In TouchDeck: **Einstellungen → Integrationen → Twitch** die Client-ID eintragen und
   **Mit Twitch verbinden** klicken.
3. Den angezeigten Code auf <https://www.twitch.tv/activate> eingeben.

Die Anmeldung bleibt nur auf dem PC und wird automatisch erneuert.

## Discord einrichten

1. Auf <https://discord.com/developers/applications> „New Application“ anlegen.
2. Unter **OAuth2 → Redirects** `http://localhost` eintragen.
3. **Client ID** und **Client Secret** (über „Reset Secret“) kopieren.
4. In TouchDeck: **Einstellungen → Integrationen → Discord** eintragen, **Mit Discord
   verbinden** klicken und in Discord **Autorisieren**.

Nachrichten in einen Kanal gehen ohne Anmeldung per Webhook (in Discord: Kanal
bearbeiten → Integrationen → Webhooks → Webhook-URL kopieren).

## OBS einrichten

1. In OBS: **Werkzeuge → WebSocket-Servereinstellungen → WebSocket-Server aktivieren**.
2. In TouchDeck: **Einstellungen → Integrationen → Aus OBS übernehmen** – Port und
   Passwort werden aus der OBS-Konfiguration gelesen (auch Flatpak).
3. **Verbindung testen**.

## Icon-Pakete

Eigene Bildersammlungen (Einstellungen › Icon-Pakete): anlegen, Bilder hinzufügen,
als `.tdicons`-Datei exportieren und weitergeben, Pakete anderer importieren.
Abgelegt im Datenordner unter `icons/packs/`.

## Plugins

Normale Plugins sind JavaScript und laufen abgeschottet (QuickJS) mit festen Grenzen
und Berechtigungen. Erweiterte Plugins sind eigene Programme mit vollen Rechten und
laufen nur nach doppelter Bestätigung auf eigene Gefahr. Installiert wird nur aus
öffentlichen GitHub-Repos; dazu gibt es einen Marktplatz und einen Testmodus.
Anleitung: [Plugin-Wiki](https://github.com/Marcel1853/touchdeck-plugins/wiki),
Beispiele: [touchdeck-plugins](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples),
Kurzfassung: [PLUGINS.md](PLUGINS.md).

## Entwickeln

Alles liegt im Projektordner (Rust, Cargo-Cache, Android-SDK, Werkzeuge):

```bash
source ../env.sh
cargo test                     # Tests
cargo run -p touchdeck         # Programm mit Fenster
cargo run -p touchdeck-server  # nur der Server, Editor im Browser
scripts/build-android.sh       # Android-App bauen und nach downloads/ legen
```

Aufbau:

```
touchdeck-rs/
├── server/                 Bibliothek + Kommandozeilen-Server (touchdeck-server)
│   ├── catalog/            Beschreibung der Aktionen und Auslöser für den Editor
│   └── src/
│       ├── core/           Daten (store), Aktionen, Events, Live-Werte, Icon-Pakete, Plugins, Protokoll, Sicherungen
│       ├── server/         HTTP-Routen, WebSocket, Suche im WLAN
│       ├── system/         Linux: Programme, Audio, Medien (linux.rs), Tastatur/Maus/Fenster (x11.rs)
│       └── integrations/   OBS, Steam, Twitch, Discord, Import von Touch-Portal-Seiten
├── app/                    Desktop-Programm (Tauri): Fenster, Symbol in der Systemleiste, Autostart
├── web/                    Editor (editor/), Bedienfeld fürs Handy (deck/), gemeinsame Teile (shared/)
├── downloads/              Android-App, die TouchDeck den Handys anbietet (wird gebaut, nicht im Repo)
└── scripts/                Hilfsskripte
```

Nützliche Umgebungsvariablen:

| Variable | Bedeutung |
|---|---|
| `TOUCHDECK_PORT` | anderer Port (Standard 8770), z. B. zum Testen neben einer laufenden Version |
| `TOUCHDECK_DATA` | anderer Datenordner |
| `TOUCHDECK_PUBLIC` | anderer Ordner für Editor und Bedienfeld |

Zum Bauen des Fensters braucht das System einmalig:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

## Installationspakete bauen

```bash
source ../env.sh
scripts/build-android.sh       # einmal: Android-App, die ins Paket kommt
cd app
XDG_CACHE_HOME="$PWD/../../.cache" cargo tauri build
```

Ergebnis in `target/release/bundle/`: ein `.deb` (ca. 7 MB) und ein AppImage
(ca. 85 MB, bringt WebKitGTK mit).
