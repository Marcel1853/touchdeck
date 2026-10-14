# TouchDeck

Makro-Fernbedienung für Linux: Button-Seiten am PC gestalten und vom Handy
oder Tablet aus auslösen – Programme starten, Tastenkürzel, Medien und
Lautstärke, Steam-Spiele, OBS Studio, Twitch und vieles mehr.

| Ordner | Inhalt |
|---|---|
| [`touchdeck/`](touchdeck/) | PC-Programm (Node.js + Electron): Server, Editor, Bedienfeld fürs Handy, Pakete als `.deb` und AppImage |
| [`android/`](android/) | Android-App (Kotlin): findet den PC im WLAN, koppelt per Bestätigung oder QR-Code, zeigt das Bedienfeld im Vollbild |

## Funktionen

- **Editor** mit Seiten, Buttons (Drücken, Umschalter, Regler), Live-Anzeigen,
  Werten (Variablen), Events („Wenn …, dann …“), Vorlagen, Logs, Backups
- **Über 60 Aktionen**: Programme, Tastatur und Maus, Fenster, Audio und
  Medien (PipeWire, MPRIS), Soundboard, System, Dateien, HTTP, Steam,
  OBS Studio (obs-websocket 5), Twitch (eigene Client-ID, Geräte-Code-Anmeldung)
- **Handy und Tablet**: Android-App oder Bedienfeld im Browser; mehrere PCs,
  automatische Suche im WLAN, Kopplung mit Bestätigung am PC, optionale PIN
- **Desktop-Programm** mit Symbol in der Systemleiste, Autostart und
  Firewall-Freigabe fürs Heimnetz

Ausführliche Beschreibung: [`touchdeck/README.md`](touchdeck/README.md)

## Bauen

Voraussetzungen: Node.js ab 22, Java 17+ und ein Android SDK (für die App).
`env.sh` setzt alle Pfade auf Ordner im Projekt (`.node/`, `.android-sdk/`,
`.cache/`), damit nichts systemweit installiert werden muss.

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
