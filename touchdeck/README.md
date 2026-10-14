# TouchDeck

Makro-Fernbedienung für Linux: Button-Seiten am PC gestalten und vom Handy
(oder Tablet) aus auslösen – mit modernem Editor, Android-App und
Bedienfeld im Browser.

## Installieren

TouchDeck gibt es als Programm mit eigenem Fenster und Symbol in der
Systemleiste. Die Pakete liegen nach dem Bauen in `dist/`:

```sh
sudo apt install ./dist/touchdeck_<version>_amd64.deb   # Installation, mit Menüeintrag
# oder ohne Installation:
./dist/TouchDeck-<version>.AppImage
```

- Fenster schließen = TouchDeck läuft im Hintergrund weiter (Handys bleiben
  verbunden). Über das Symbol in der Systemleiste: Editor öffnen,
  Handy verbinden, Vorschau, **Beim Anmelden starten**, Beenden.
- Ein zweiter Start holt nur das vorhandene Fenster nach vorn.
- Der Server läuft in einem eigenen Prozess: Live-Abfragen und Aktionen
  bremsen das Fenster nicht aus; stürzt er ab, wird er neu gestartet.
- Daten liegen in `~/.config/TouchDeck/data/`.
- Deinstallieren: `sudo apt remove touchdeck`

## Ohne Installation starten (Entwicklung)

```sh
./start.sh        # nur Server, Editor im Browser
npm run app       # als Desktop-Programm aus dem Quellcode
npm run dist      # Pakete bauen (AppImage und .deb)
```

Der Editor ist dann unter <http://localhost:8770/editor/> erreichbar.
Optionen von `start.sh`: `--port 8770`, `--data <ordner>`, `--no-open`.
Ohne Installation liegen die Daten in `data/`.

Voraussetzung ist Node.js ab Version 22. Liegt im übergeordneten Ordner
`.node/`, wird dieses verwendet, sonst das System-Node.

## Handy verbinden

**Mit der Android-App (empfohlen):**

1. Im Editor **📱 Handy verbinden** öffnen und den QR-Code unter
   „Android-App“ scannen – die App (`TouchDeck.apk`, ca. 180 KB) wird
   vom PC heruntergeladen. Beim ersten Mal „Unbekannte Apps installieren“
   für den Browser erlauben.
2. App öffnen: Sie findet den PC im WLAN von selbst. Antippen →
   am PC erscheint „… möchte sich verbinden“ → **Erlauben**.
3. Fertig: Bedienfeld im Vollbild, Bildschirm bleibt an, Vibration,
   Zurück-Taste blättert Seiten zurück. Über das Seitenmenü (Seitenname
   oben antippen) → „Verbindung ändern“.

Alternativ in der App „QR-Code vom PC scannen“ oder die Adresse von Hand
eingeben.

**Ohne App:** den QR-Code im Dialog „Handy verbinden“ mit der Handykamera
scannen – das Bedienfeld öffnet sich im Browser.

Der Editor ist nur am PC selbst erreichbar; Handys brauchen den Schlüssel
(per Bestätigung oder QR-Code) und – falls in den Einstellungen gesetzt –
die PIN. **Neuen Schlüssel erzeugen** meldet alle Geräte ab.

**Firewall:** Ist `ufw` aktiv, zeigt der Dialog den Befehl, der TCP-Port
8770 (Bedienfeld) und UDP-Port 8769 (automatisches Finden) fürs Heimnetz
freigibt, z. B.:

```sh
sudo ufw allow from 192.168.178.0/24 to any port 8770 proto tcp
sudo ufw allow from 192.168.178.0/24 to any port 8769 proto udp
```

## Aktionen

| Aktion | Umsetzung unter Linux |
|---|---|
| Webseite / Datei / Ordner öffnen | `xdg-open` |
| Programm starten | `.desktop`-Einträge über `gtk-launch` |
| Befehl ausführen | `sh -c`, wahlweise mit Ausgabe |
| Tastenkürzel, Text tippen | X11-XTest über `helpers/xinput.py` (nur X11, nicht Wayland) |
| Mediensteuerung | MPRIS über D-Bus (`gdbus`) – Spotify, Browser, VLC … |
| Lautstärke, Mikrofon stumm | PipeWire über `wpctl` |
| Seite wechseln, Warten | im Bedienfeld |
| Benachrichtigung | `notify-send` |
| HTTP-Anfrage | eingebautes `fetch` |

Dazu kommen:

| Bereich | Aktionen |
|---|---|
| System | Bildschirmfoto, Bildschirm sperren, Bereitschaft/Abmelden/Neustart/Herunterfahren, Nachtlicht, Hell/Dunkel-Design |
| Audio | Audioausgang und Mikrofon wechseln (fest oder „immer zum nächsten“) |
| Netzwerk | Rechner per Wake-on-LAN aufwecken |
| Steam | Spiel starten (Auswahl aus allen Bibliotheken), Bibliothek, Downloads, Big Picture, Freunde, Shop |
| OBS Studio | Szene wechseln, Vorschauszene, Übergang auslösen/auswählen, Stream, Aufnahme (inkl. Pause), Clip aus dem Wiederholungspuffer, Audioquelle stumm/lauter/leiser/setzen, Medienquelle steuern, Quelle ein-/ausblenden, Filter ein/aus, Text-, Bild- und Browserquelle ändern, Profil und Szenensammlung, OBS-Tastenkürzel, Bildschirmfoto einer Quelle, Studio-Modus, virtuelle Kamera, eigene Anfrage |
| Tastatur & Maus | Tastenkürzel, Text tippen, Taste gedrückt halten, Mausklick (inkl. Doppelklick, Mausrad), Maus bewegen |
| Programme | Fenster nach vorn holen (Auswahl aus offenen Fenstern), Programm beenden |
| Sound | Audiodateien abspielen (Soundboard, mehrere gleichzeitig), alle Sounds stoppen |
| Dateien | Text in Datei schreiben oder als Zeile anhängen |
| Logik & Variablen | Variable setzen, Zähler erhöhen/verringern, an/aus umschalten, Bedingung „nur weiter, wenn …“ |

Variablen erscheinen in Beschriftungen über `{Name}` (z. B. „Tore: {Tore}“),
als Live-Anzeige und in OBS-Texten. `{Zeit}` und `{Datum}` gibt es immer.

Buttons gibt es in drei Arten:

- **Drücken** – führt die Aktionen nacheinander aus.
- **Umschalter** – an/aus mit eigener Farbe und eigenen Aktionen für beide Zustände.
- **Regler** – zum Wischen: Lautstärke, Mikrofon-Pegel, OBS-Audioquelle oder
  ein eigener Befehl mit `{value}` (0–100).

Jeder Button kann eine **Live-Anzeige** haben: Uhrzeit, Datum, CPU, RAM,
CPU-Temperatur, Lautstärke, Mikrofon, aktueller Musiktitel, Steam-Updates,
„Spiel läuft“, OBS-Stream/-Aufnahme/-Szene oder die Ausgabe eines Befehls.
Je nach Wert leuchtet der Button (z. B. rot bei laufendem Stream).

## Bereiche im Editor

Links in der Leiste (Strg+1 … Strg+6):

- **Seiten** – Raster gestalten. Jeder Button hat die Tabs **Aussehen**,
  **Drücken** (bzw. **Regler**), **Event** (der Button reagiert selbst auf
  Ereignisse, z. B. Farbe ändern, wenn ein Wert sich ändert) und **Halten**
  (Aktionen bei langem Drücken, auf Wunsch wiederholt, plus „beim Loslassen“).
- **Werte** – eigene Variablen mit aktuellem Inhalt, Startwert, Ordner,
  Beschreibung, „beim Start zurücksetzen“ und Übersicht, wo sie benutzt
  werden. Umbenennen passt alle Verwendungen an.
- **Events** – „Wenn …, dann …“ ohne Button: Wert ändert sich, TouchDeck
  startet, Zeitgeber, Uhrzeit, Datei ändert sich, Handy verbindet sich,
  Umschalter, Audioausgang, Steam-Spiel startet/endet, OBS (Szene, Vorschau,
  Stream, Aufnahme, Clip, Stummschaltung, Quelle, Filter, Studio-Modus,
  Profil, Verbindung). Mit Abklingzeit; Endlosschleifen werden abgefangen.
  Daten des Ereignisses stehen als `{Event.Szene}`, `{Event.Wert}` … bereit.
- **Vorlagen** – wiederverwendbare Aktionslisten, aufrufbar mit
  „Vorlage ausführen“ (auch mehrfach = Wiederholen).
- **Logs** – alles, was passiert, live; mit Filter und Suche.
- **Einstellungen** – Allgemein, Handy & Sicherheit (WLAN-Zugriff, Schlüssel,
  **PIN**), Bedienfeld, Aussehen (hell/dunkel/System, Akzentfarbe),
  Integrationen (OBS), Backups (automatisch täglich, wiederherstellen,
  herunterladen), Protokoll, Information.

Die Aktionen sind in Gruppen sortiert: **Allgemein** (Ausführen,
Navigation, Eingabe, Dienste, Logik, Werte, Dateien, HTTP, Visuelles),
**Audio, Musik & Medien**, **Streaming** (OBS) und **Spiele** (Steam).

## Fertige Seiten

In der Seitenleiste unter **Fertige Seiten**:

- **Steam-Spiele** – alle installierten Spiele mit Cover; ein Spiel leuchtet,
  solange es läuft. Unten: Bibliothek, Downloads (mit Update-Anzeige),
  Big Picture, Freunde.
- **System** – Uhr, CPU, RAM, Temperatur, Musik, Ton, Mikrofon,
  Audioausgang, Bildschirmfoto, Nachtlicht, Sperren und ein Lautstärkeregler.
- **OBS Studio** – ein Button pro Szene (leuchtet, wenn live), Stream,
  Aufnahme, Pause, Clip.

## Twitch einrichten

Jeder verbindet sein eigenes Konto über eine eigene Twitch-Anwendung:

1. Auf <https://dev.twitch.tv/console> eine Anwendung registrieren:
   Redirect-URL `http://localhost`, Kategorie „Application Integration“,
   Client-Typ **Öffentlich**.
2. In TouchDeck: **Einstellungen → Integrationen → Twitch** die Client-ID
   eintragen und **Mit Twitch verbinden** klicken.
3. Den angezeigten Code auf <https://www.twitch.tv/activate> eingeben.

Danach gibt es Aktionen (Chat-Nachricht, Ankündigung, Clip, Marker, Titel
und Kategorie, Werbung, Chat-Modi, Shoutout, Raid, Kanalpunkte-Belohnung,
Kanal-Infos in Werte), Events (Follower, Abo, Bits, Raid, Kanalpunkte,
Chat-Befehl wie `!szene`, Stream online/offline, Werbepause) und
Live-Anzeigen (live?, Zuschauer, Follower). Die Anmeldung bleibt nur auf
dem PC und wird automatisch erneuert.

## OBS einrichten

1. In OBS: **Werkzeuge → WebSocket-Servereinstellungen → WebSocket-Server aktivieren**.
2. In TouchDeck: **Einstellungen → Aus OBS übernehmen** – Port und Passwort
   werden aus der OBS-Konfiguration gelesen (auch bei der Flatpak-Version).
3. **Verbindung testen**.

## Touch-Portal-Seiten übernehmen

In der Seitenleiste **Fertige Seiten → Touch Portal** liest `~/.config/TouchPortal/pages/*.tml` ein:
Raster, Farben, Beschriftungen, Bilder, Seitenwechsel, Webseiten, Programme,
Tastenkürzel und Texteingabe. Aktionen ohne Gegenstück (z. B. OBS, Twitch)
werden als „Nicht unterstützt“ markiert und lassen sich ersetzen.

## Ohne Paket ins Menü aufnehmen

Für die Variante ohne Installation (`start.sh`):

```sh
install/install.sh              # Menüeintrag
install/install.sh --autostart  # zusätzlich beim Anmelden im Hintergrund starten
install/install.sh --remove     # beides wieder entfernen
```

Es wird nur im Benutzerordner (`~/.local/share/applications`,
`~/.config/autostart`) etwas angelegt. Mit dem installierten Paket ist das
nicht nötig.

## Daten

Seiten, Einstellungen und Bilder liegen in `config.json` und `icons/` –
beim installierten Programm in `~/.config/TouchDeck/data/`, sonst in `data/`.
Im Editor unter **Einstellungen → Sicherung** lassen sie sich als JSON
herunterladen und wieder laden.

## Entwicklung

```sh
npm install
npm test
npm run android   # Android-App bauen (Projekt in ../android) und nach downloads/ legen
```

Aufbau:

```
src/main.js               Start ohne Desktop-Programm
src/core/                 store (Daten), actions, events, live, log, backup
src/integrations/         obs, steam, steam-pages, touchportal
src/system/linux.js       Audio, Fenster, Tastatur, Medien, System
src/server/               server (HTTP + WebSocket), http (Hilfen)
electron/                 Desktop-Programm (Fenster, Systemleiste, Server-Prozess)
public/editor/js/         main, state, dom, actions-ui, views/*
public/deck/              Bedienfeld fürs Handy
public/shared/            gemeinsame Button-Darstellung
helpers/xinput.py         X11: Tastatur, Maus, Fenster
src/server/discovery.js   automatisches Finden (UDP 8769)
src/integrations/twitch.js  Twitch (Geräte-Code-Anmeldung, Helix, EventSub)
../android/               Android-App (Kotlin, ohne AndroidX)
```
