# TouchDeck

[English](README.md) · **Deutsch**

Makro-Fernbedienung für Linux: Button-Seiten am PC gestalten und vom Handy oder Tablet
aus auslösen – mit Editor, Android-App und Bedienfeld im Browser. Geschrieben in Rust,
das Programmfenster nutzt Tauri mit der Web-Ansicht des Systems.

Aktuelle Version: **0.9.0** (PC-Programm), Android-App **0.6.2**. Änderungen je
Version: [`CHANGELOG.de.md`](../CHANGELOG.de.md)

## Installieren

```sh
sudo apt install ./TouchDeck_<version>_amd64.deb   # mit Menüeintrag
# oder ohne Installation: AppImage ausführbar machen und doppelklicken
./TouchDeck_<version>_amd64.AppImage
```

- Das `.deb` öffnet beim Installieren die Firewall (ufw) fürs Heimnetz und schließt
  sie beim Entfernen wieder. Python oder Node.js werden nicht gebraucht.
- Fenster schließen = TouchDeck läuft im Hintergrund weiter (Handys bleiben
  verbunden). Über das Symbol in der Systemleiste (Menü in der gewählten Sprache):
  Editor öffnen, Handy verbinden,
  Vorschau, **Beim Anmelden starten**, Beenden.
- Ein zweiter Start holt das vorhandene Fenster nach vorn.
- Ohne `.deb` (z. B. beim AppImage) legt TouchDeck selbst einen Starter-Eintrag mit
  Symbol im Benutzerordner an, damit Startmenü und Taskleiste es richtig zeigen. Der
  Autostart-Eintrag zeigt immer auf die gerade laufende Version.
- Daten liegen in `~/.config/TouchDeck/data/`.
- Deinstallieren: `sudo apt remove touchdeck`

## Sprache

Die ganze Oberfläche – Editor, Bedienfeld, Android-App und Meldungen des Servers –
gibt es auf Deutsch und Englisch. Voreingestellt ist die Systemsprache; umstellen
lässt sie sich unter **Einstellungen › Allgemein**. Die Übersetzungen liegen in
`web/shared/lang/de.json` und `en.json` (Editor und Bedienfeld), `server/lang/*.json`
(Meldungen des Servers) und `server/catalog/lang/*.json` (Namen der Aktionen und
Auslöser). Plugins können eigene Übersetzungen mitbringen.

Auch die **Android-App** gibt es auf Deutsch und Englisch. Ihre Texte –
Verbindungsbildschirm, Dialoge und Bedienfeld – folgen der Sprache, die in TouchDeck
am PC eingestellt ist, und wechseln mit, wenn man sie dort umstellt. Vor der ersten
Verbindung nimmt die App die Sprache des gefundenen PCs, sonst die Sprache des Handys.

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

## Seiten

- **Raster**-Seiten ordnen Buttons in Zeilen und Spalten. Spaltenbreiten und
  Zeilenhöhen lassen sich einzeln einstellen. Ein Doppelklick auf ein leeres Feld
  (oder „＋ Button“ in der Werkzeugleiste) legt einen Button an; ein einfacher Klick
  hebt nur die Auswahl auf.
- **Freie** Seiten platzieren Buttons beliebig auf einer Fläche mit gewähltem
  Seitenverhältnis (z. B. 16:10, am besten passend zum Gerät). Position und Größe
  werden in Prozent der Seite gespeichert, so zeigen Handy und Tablet die Seite genau
  in ihrem Format, wie die Vorschau. Neue Buttons entstehen per Doppelklick auf die
  Fläche, mit „＋ Button“ oder per Rechtsklick.
- **Ordner:** Rechtsklick auf ein leeres Feld oder die freie Fläche › **Ordner hier
  anlegen** erzeugt eine Unterseite mit Zurück-Button und an dieser Stelle einen
  Button, der sie öffnet.
- **Logik für Seiten:** Jede Seite kann eigene Events haben, z. B. mit dem Auslöser
  „Seite geöffnet / verlassen“, um eine OBS-Szene zu wechseln oder einen Wert zu setzen.
- **Fertige Seiten** für System, OBS und Steam bestehen nur aus normalen, sichtbaren
  Aktionen und Events – alles lässt sich danach ändern. Steam-Seiten passen sich
  automatisch an, wenn Spiele installiert oder entfernt werden, und laden fehlende
  Cover von Steam.
- **Profile:** Über „⋯“ neben der Seitenliste oder per Rechtsklick auf eine Seite
  lassen sich eine oder alle Seiten als `.tdprofile`-Datei exportieren – samt den
  benutzten Bildern, Vorlagen, Werten und Events. Beim Import eines Profils kommen
  nur Seiten hinzu, überschrieben wird nie etwas.
- In den Einstellungen des Bedienfelds wählt **Form der Felder** zwischen „Fläche
  füllen“ (Felder dürfen etwas breiter oder höher als quadratisch werden) und
  „Quadratisch“.

## Buttons

Buttons gibt es als **Drücken**, **Umschalter** (an/aus mit eigenem Aussehen
„Wenn an“ und eigenen Aktionen) und **Regler** (Lautstärke, Mikrofon, OBS-Audioquelle
oder eigener Befehl mit `{value}`). Jeder Button kann eine **Live-Anzeige** haben
(Uhrzeit, CPU, RAM, Temperatur, Lautstärke, Musiktitel, Steam, OBS, Twitch, Discord,
Werte, Befehlsausgabe), auf **Events** reagieren und beim **Halten** und beim
**Doppeldruck** eigene Aktionen ausführen (nur Buttons mit Doppeldruck-Aktionen
warten kurz, 0,3 s, auf ein zweites Tippen). Rechtsklick auf einen Button:
Duplizieren, Kopieren, Ausschneiden, Einfügen, Aussehen übertragen, **Logik
kopieren** / **Logik einfügen** (alle Aktionen und Events des Buttons), auf andere
Seiten verschieben.

## Aktionen

Rund 80 Aktionen in Gruppen:

| Gruppe | Aktionen |
|---|---|
| Allgemein | Webseiten, Dateien, Programme und Befehle, Seiten wechseln, Tastenkürzel, Text tippen, Tasten halten, Maus, Fenster nach vorn holen, Programme beenden, Benachrichtigungen, Bildschirmfoto, Sperren, Energie, Nachtlicht, Hell/Dunkel, Wake-on-LAN, HTTP, Dateien schreiben, Werte, Bedingungen, Zufall (eine der nächsten Aktionen), Timer (Countdown / Stoppuhr), Seite auf allen Geräten anzeigen, Vorlagen, Aussehen von Buttons ändern |
| Audio, Musik & Medien | Mediensteuerung (MPRIS), Lautstärke und Mikrofon (PipeWire), Audioausgang/-eingang wechseln, Soundboard |
| Streaming | OBS Studio (Szenen, Stream, Aufnahme, Clips, Audioquellen, Quellen, Filter, Texte, Übergänge, Profile …), Twitch |
| Kommunikation | Discord (stumm, taub, Sprachkanal, Lautstärke, Push-to-Talk, Webhooks) |
| Spiele | Steam (Spiele starten, Bibliothek, Downloads, Big Picture …) |
| Plugins | Aktionen installierter Plugins |

Werte erscheinen in Beschriftungen über `{Name}` (z. B. „Tore: {Tore}“), als
Live-Anzeige und in OBS-Texten. `{Zeit}` und `{Datum}` gibt es immer.

Beispiele für die neueren Aktionen und Auslöser:

- **Wenn / Sonst wenn / Sonst**, **Wiederholen (n-mal)**, **Solange** und **Schleife
  verlassen** bilden Blöcke bis **Ende**; der Editor rückt sie ein. Der aktuelle
  Durchlauf ist `{Event.Durchlauf}`.
- **Zufällig: eine der nächsten Aktionen** führt genau eine zufällig gewählte der
  folgenden Aktionen aus (Anzahl einstellbar, 0 = alle folgenden).
- **Drehregler und Regler-Formen:** ein Regler kann Fläche, Schiene mit Griff,
  Drehregler (Ring) oder Drehregler mit Zeiger sein, mit eigener Farbe und Dicke.
- **Timer (Countdown / Stoppuhr)** mit Starten, Anhalten und Zurücksetzen; die
  Live-Anzeige **Timer** zeigt die Restzeit, der Auslöser **Timer abgelaufen**
  reagiert, wenn er abläuft.
- **Programm im Vordergrund** + **Seite auf allen Geräten anzeigen**: z. B. wechseln
  Handy und Tablet von selbst zur OBS-Seite, sobald OBS vorn ist.
- **Seitenanzeige (●○○)** als Live-Anzeige zeigt die Position der Seite in der
  Seitenliste; **Gehe zu Seite** kann auch zur nächsten / vorherigen Seite springen.

## Bereiche im Editor

Links in der Leiste (Strg+1 … Strg+8):

- **Seiten** – Raster- oder freie Seiten gestalten, Logik für Seiten; fertige Seiten
  für Steam, System und OBS.
- **Werte** – eigene Variablen mit Startwert, Ordner und Beschreibung.
- **Events** – „Wenn …, dann …“: Wert ändert sich, Start, Zeitgeber, Uhrzeit, Datei,
  Handy, Umschalter, Seite geöffnet / verlassen, Programm im Vordergrund, Timer
  abgelaufen, Audio (auch Ton oder Mikrofon stumm), Steam, OBS, Twitch, Discord,
  Plugins. Mit Abklingzeit; Endlosschleifen werden abgefangen.
- **Vorlagen** – wiederverwendbare Aktionslisten.
- **Logs** – alles, was passiert, live.
- **Plugins** – installierte Plugins, Marktplatz, Entwickeln, erweiterter Modus.
- **🖼️ Icon-Pakete** – eigene Bildersammlungen (siehe unten).
- **Einstellungen** – Allgemein, Handy & Sicherheit, Bedienfeld, Aussehen,
  Integrationen, Backups, Protokoll, Information.

Die **Statusleiste** unten zeigt die verbundenen Geräte mit Name, Adresse und der
gerade angezeigten Seite, den Zustand von OBS, Twitch und Discord (ein Klick öffnet
die Einstellungen) sowie die Adresse des PCs und die Version.

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

Eigene Bildersammlungen, in einem eigenen Bereich **🖼️ Icon-Pakete** links in der
Leiste des Editors (neben Plugins): anlegen, Bilder hinzufügen,
als `.tdicons`-Datei exportieren und weitergeben, Pakete anderer importieren.
Abgelegt im Datenordner unter `icons/packs/`.

- Jedes Paket hat Name, Autor, Link, Lizenz-Kurzname, **Beschreibung, Version,
  Titelbild, Lizenztext** und optional eine **passende Button-Farbe** – wählt man ein
  Bild aus dem Paket für einen Button mit Standardfarbe, bekommt er diese Farbe.
- Jedes Bild kann einen **Namen und Schlagwörter** bekommen (im Paket anklicken). Die
  Bildauswahl beim Button sucht in Dateinamen, Namen und Schlagwörtern.
- **Icon erstellen** baut aus Hintergrund, Emoji und Text ein PNG.
- Der Import erkennt den Inhalt, nicht die Dateiendung: `.tdicons`, ZIP-Dateien mit
  Bildern (auch in Unterordnern) und übliche Beschreibungsdateien wie eine `info.txt`
  mit „schlüssel=wert“ oder eine `manifest.json` mit `icons.json` (Namen und
  Schlagwörter) und Lizenzdatei. Bitte nur Bilder importieren, die man nutzen darf.

## Plugins

Normale Plugins sind JavaScript und laufen abgeschottet (QuickJS) mit festen Grenzen
und Berechtigungen. Erweiterte Plugins sind eigene Programme mit vollen Rechten und
laufen nur nach doppelter Bestätigung auf eigene Gefahr. Installiert wird nur aus
öffentlichen GitHub-Repos; dazu gibt es einen Marktplatz und einen Testmodus.
Plugins können eigene Übersetzungen mitbringen.
Anleitung: [Plugin-Wiki](https://github.com/Marcel1853/touchdeck-plugins/wiki),
Beispiele: [touchdeck-plugins](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples),
Kurzfassung: [PLUGINS.de.md](PLUGINS.de.md).

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
│   ├── catalog/            Beschreibung der Aktionen und Auslöser für den Editor (mit lang/)
│   ├── lang/               Übersetzungen der Meldungen des Servers
│   └── src/
│       ├── core/           Daten (store), Aktionen, Events, Live-Werte, Timer, Profile, Icon-Pakete, Plugins, Protokoll, Sicherungen
│       ├── server/         HTTP-Routen, WebSocket, Suche im WLAN
│       ├── system/         Linux: Programme, Audio, Medien (linux.rs), Tastatur/Maus/Fenster (x11.rs)
│       └── integrations/   OBS, Steam, Twitch, Discord, Import von Seiten
├── app/                    Desktop-Programm (Tauri): Fenster, Symbol in der Systemleiste, Autostart, Starter-Eintrag
├── web/                    Editor (editor/), Bedienfeld fürs Handy (deck/), gemeinsame Teile und Übersetzungen (shared/)
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
