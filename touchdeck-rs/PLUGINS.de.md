# TouchDeck-Plugins schreiben

[English](PLUGINS.md) · **Deutsch**

> Die ausführliche, öffentliche Anleitung steht im
> [Plugin-Wiki](https://github.com/Marcel1853/touchdeck-plugins/wiki), Beispiele zum
> Kopieren in [touchdeck-plugins](https://github.com/Marcel1853/touchdeck-plugins).
> Diese Datei ist die Kurzfassung für Entwickler von TouchDeck selbst.

Es gibt zwei Arten: **normale Plugins** (JavaScript, abgeschottet – der Standard) und
**erweiterte Plugins** (eigene Programme mit vollen Rechten, nur auf eigene Gefahr,
siehe unten).

Ein Plugin bringt neue **Aktionen** (für Buttons, Events und Vorlagen) und neue
**Auslöser** (für Events) in TouchDeck. Plugins sind JavaScript und laufen
abgeschottet in einer eigenen kleinen JS-Engine. Sie können nur das, was TouchDeck
ihnen über das Objekt `td` anbietet, und nur, was sie in `plugin.json` als
Berechtigung angeben.

## Regeln

- **Öffentlich auf GitHub:** Installiert wird nur aus öffentlichen GitHub-Repos, damit
  jeder den Quelltext prüfen kann. TouchDeck lädt die neueste Veröffentlichung
  (Release) oder, wenn es keine gibt, den Hauptzweig.
- **Alles im Hauptordner:** `plugin.json`, das Skript (`plugin.js`) und optional
  `README.md` und Bilder. Unterordner und andere Dateitypen werden ignoriert.
- **Grenzen:** 32 MB Speicher, 5 Sekunden Rechenzeit je Aufruf, 10 MB Plugin-Größe,
  Antworten aus dem Netz bis 1 MB. Weiterleitungen werden nicht verfolgt.
- **Kein Zugriff** auf Dateien, Programme, Tastatur oder Maus. `require`, `fetch`,
  `process` und Ähnliches gibt es nicht.

## Entwickeln

1. Ordner mit `plugin.json` und `plugin.js` anlegen (Vorlage: [`examples/example`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example)).
2. In TouchDeck: **Plugins › Entwickeln** den Ordner angeben und **Laden**.
   Nach Änderungen bei dem Plugin **Neu laden** klicken. Ausgaben von `td.log`
   erscheinen unter **Logs**.
3. Fertig? Als öffentliches GitHub-Repo veröffentlichen, am besten mit einem
   Release (z. B. `v1.0.0`). Dann kann jeder es über **Plugins › Marktplatz**
   mit `besitzer/repo` installieren.

## plugin.json

```json
{
  "id": "mein-plugin",
  "name": "Mein Plugin",
  "version": "1.0.0",
  "author": "Dein Name",
  "description": "Was das Plugin macht.",
  "main": "plugin.js",
  "permissions": {
    "http": ["api.example.com", "*.example.org", "lan"],
    "values": true,
    "notify": true,
    "events": true,
    "deck": true,
    "status": true
  },
  "settings": [
    { "key": "token", "label": "API-Schlüssel", "type": "password" }
  ],
  "actions": [
    { "id": "senden", "label": "Nachricht senden", "icon": "📨",
      "fields": [{ "key": "text", "label": "Text", "type": "text" }] }
  ],
  "triggers": [
    { "id": "neu", "label": "Neue Nachricht", "icon": "🔔",
      "fields": [{ "key": "kanal", "label": "Kanal (leer = jeder)", "type": "text" }] }
  ],
  "lives": [
    { "id": "ungelesen", "label": "Ungelesene Nachrichten" }
  ]
}
```

| Feld | Bedeutung |
|---|---|
| `id` | 1–32 Zeichen: Kleinbuchstaben, Ziffern, Bindestrich. Muss eindeutig sein. |
| `license` | Lizenz des Plugins, z. B. `MIT` oder `Alle Rechte vorbehalten`. Öffentlich heißt nicht frei: Ohne Lizenz gehört der Code allein dem Autor. |
| `platforms` | Optional: Betriebssysteme, auf denen das Plugin läuft, `["linux"]`, `["windows"]` oder beides. Fehlt es, gilt: überall. Auf anderen Systemen zeigt der Marktplatz „nur Linux“, und TouchDeck startet es nicht. Windows ist eine ganz frühe Alpha. |
| Texte | `name`, `description`, `label` usw. dürfen statt eines Texts eine Übersetzung sein: `{ "en": "Light", "de": "Licht" }`. TouchDeck nimmt die eingestellte Sprache, sonst Englisch, sonst Deutsch. So kann ein Plugin seine Übersetzungen gleich mitbringen. |
| `permissions.http` | Erlaubte Adressen: genauer Name, `*.domain` für alle Unteradressen, `lan` für Geräte im Heimnetz. Ohne Eintrag kein Netz. |
| `permissions.values` | TouchDeck-Werte lesen und setzen. |
| `permissions.notify` | Benachrichtigungen am PC zeigen. |
| `permissions.events` | Mit `td.on` auf TouchDeck-Ereignisse reagieren (Seite geöffnet, stummgeschaltet, Programm im Vordergrund, Timer abgelaufen …). |
| `permissions.deck` | Das Bedienfeld steuern: Seiten auflisten, eine Seite auf allen Geräten anzeigen, Aussehen von Buttons ändern und zurücksetzen, Timer steuern. |
| `permissions.status` | Verbundene Geräte und den Zustand von OBS, Twitch und Discord lesen. |
| `lives` | Live-Anzeigen des Plugins. Sie erscheinen im Editor bei der Live-Anzeige eines Buttons als „Plugin › Bezeichnung“ und können Text, einen Fortschrittsbalken und das Aussehen „an“ zeigen. Keine Berechtigung nötig. |
| `settings` | Felder, die der Nutzer beim Plugin ausfüllt (`text`, `password`, `number`, `checkbox`). |
| `actions[].fields` / `triggers[].fields` | Eingabefelder wie bei den eingebauten Aktionen: `text`, `textarea`, `number`, `checkbox`, `select` (mit `options`), `dynamic` (z. B. `"source": "variables"`). |

Platzhalter wie `{Wert}` oder `{Event.Nutzer}` in Aktionsfeldern ersetzt TouchDeck,
bevor das Plugin sie bekommt.

Ein Auslöser passt, wenn jedes ausgefüllte Feld gleich dem gleichnamigen Eintrag in
den Daten von `td.fire` ist. Leere Felder passen immer. Alle Daten stehen in Events
als `{Event.Name}` zur Verfügung.

## Fertige Seiten

Lege eine oder mehrere `.tdprofile`-Dateien (in TouchDeck über das Seitenmenü „⋯“
exportiert) in dein Plugin – in den Hauptordner oder in Unterordner (bis 3 Ebenen, z. B.
`pages/strom/Übersicht.tdprofile`, angezeigt als „strom › Übersicht“). Eine Datei darf
mehrere Seiten enthalten; ihre Ordner in der Seitenliste bleiben erhalten. TouchDeck zeigt sie in der Plugin-Karte
und unter **Fertige Seiten** in der Seitenliste; ein Klick importiert die Seite samt
Vorlagen, Werten und Bildern. Der Import fügt nur hinzu und überschreibt nichts.

## Designs und Icon-Pakete

Ein Plugin kann Designs für Buttons und Regler mitbringen (`looks` in plugin.json). Sie
erscheinen im Editor unter **Aussehen › Designs**; ein Klick übernimmt sie. Bilder sind
Pfade im Plugin (`icons/…`, png/svg/jpg/webp/gif).

```json
"looks": [
  { "id": "rost", "name": { "en": "Rust", "de": "Rost" },
    "style": { "color": "#7a3b1e", "textColor": "#ffe2c4", "bgImage": "icons/rost.jpg",
               "texFit": "cover", "texVeil": 20, "overlay": "frame", "iconSize": 120 },
    "slider": { "shape": "mixer", "color": "#ff8a3d", "thickness": 12 } }
],
"iconPacks": ["packs/factorio.tdicons"]
```

`style` kennt color, colorOn, textColor, bgOpacity, fontSize, fontFamily, bold, italic,
textOutline, titlePos, iconSize, iconFit, overlay, texFit, texVeil, bgImage und icon;
`slider` kennt shape (fill, rail, fader, knob, dial, mixer, combo), color und thickness.

`iconPacks` nennt Icon-Pakete (`.tdicons`, aus TouchDeck exportiert), die das Plugin braucht.
TouchDeck importiert sie beim Installieren – nur wenn es noch kein Paket mit diesem Namen gibt.
Ein Eintrag darf auch ein Link zu einer `.tdicons`-Datei auf `https://github.com/…` sein (z. B.
eine Release-Datei); TouchDeck lädt sie einmal herunter.
Icon-Pakete können selbst Designs mitbringen: `looks` in ihrer `pack.json`, Bilder per Dateiname.

## Das Objekt `td`

```js
td.action('senden', (params, info) => {
  // params: Felder der Aktion; info: { buttonId, event }
  // Rückgabe (optional): Text, der als Meldung erscheint. Fehler mit throw.
  return 'Gesendet';
});

td.every(30000, () => { /* alle 30 s (mindestens 1 s) */ });

td.fire('neu', { kanal: 'allgemein', Nutzer: 'anna' });   // eigenen Auslöser melden

const res = td.http({ method: 'POST', url: 'https://api.example.com/x',
  headers: { Authorization: `Bearer ${td.settings.token}` }, json: { a: 1 } });
res.status; res.ok; res.body; res.json();

td.values.get('Name'); td.values.set('Name', 'Wert');      // braucht "values"
td.notify('Titel', 'Text');                                 // braucht "notify"
td.log('Info'); td.warn('Warnung'); console.log('geht auch');
td.settings.token;                                          // Einstellungen des Nutzers
td.plugin.id; td.plugin.version;

// braucht "events": dieselben Ereignisse wie die Auslöser der Ereignisse; '*' = alle
td.on('page_view', (data) => {          // data: { pageId, state: 'open'|'close', Seite, Page }
  if (data.state === 'open') td.log(`Seite ${data.Seite} geöffnet`);
});

// eigene Live-Anzeige aus "lives": Text, Zahl 0–100 (Balken), Aussehen „an“
td.live.set('ungelesen', { text: '3 neu', value: 30, on: true });
td.live.set('ungelesen', 'nur ein Text');
// echte Zahl für Graphen (nicht auf 0–100 begrenzt), z. B. Strom in MW:
td.live.set('strom', { text: '35,2 MW', number: 35.2 });
// mehrere Datenreihen für einen Graphen (bis 12); neue Namen werden automatisch zu neuen Linien:
td.live.set('strom', { text: '66 MW', number: 66, series: { Dampf: 30.5, Solar: 12, Kernkraft: 23.5 } });

// braucht "deck"
td.deck.pages();                        // [{ id, name, buttons: [{ id, title }] }]
td.deck.showPage('OBS');                // ID oder Name, auf allen Geräten
td.deck.setLook(buttonId, { title: 'Live', color: '#c0392b', textColor: '#fff', icon: '🔴' });
td.deck.resetLook(buttonId);            // ursprüngliches Aussehen
td.deck.timer('Pause', 'start', 300);   // start | stop | reset | toggle, Sekunden
// Karte auf dem Bildschirmschoner (unter der Uhr), auch mit „deck“; clear() entfernt sie
td.saver.set({ title: 'Factorio', color: '#e6b45a', items: [{ label: 'Forschung', text: 'Logistik 2 · 42 %' }, { label: 'Strom', text: '35,2 MW' }] });
td.saver.clear();

// braucht "status"
td.status();  // { version, devices: [{ name, ip, page }], obs, twitch, discord }
```

Die Ereignisnamen sind die Auslöser-Typen des Editors, z. B. `page_view`,
`audio_mute`, `app_focus`, `timer_done`, `value_change`, `obs_scene`. Die Daten
enthalten die deutschen und englischen Feldnamen (`Seite` und `Page`).
Plugin-Auslöser, die mit `td.fire` gemeldet werden, gehen nicht an Plugins.

Plugin-Aktionen und -Auslöser funktionieren überall in TouchDeck: beim Drücken,
Halten und Doppelt-Drücken, in der Seiten-Logik („Seite geöffnet / verlassen“), in
Abläufen und in Ereignissen.

Alle Aufrufe sind synchron (kein `await` nötig). Ein Plugin läuft in einem eigenen
Thread; lange Netzanfragen halten TouchDeck nicht auf.

## Aktionen mit Rückgabe als Daten

Eine Aktion kann statt einer kurzen Meldung **Daten** zurückgeben. TouchDeck baut daraus selbst die
Anzeige – das Plugin muss keine eigene Oberfläche dafür haben.

```js
td.action('status', () => ({ players: 7, memory: 4210, uptime: 50432 }));
```

In `plugin.json` sagt `returns`, wie die Werte heißen und in welcher Einheit sie kommen:

```json
{ "id": "status", "label": "Server-Status", "fields": [],
  "returns": [
    { "key": "players", "label": { "en": "Players", "de": "Spieler" }, "unit": "number" },
    { "key": "memory",  "label": "RAM", "unit": "mb" },
    { "key": "uptime",  "label": { "en": "Uptime", "de": "Laufzeit" }, "unit": "seconds" }
  ] }
```

Einheiten: `number`, `bytes`, `kb`, `mb`, `gb`, `seconds`, `ms`, `percent`, `text`.
TouchDeck macht daraus lesbare Werte (4210 MB → „4,1 GB“, 50432 s → „14 h“).

- **Anzeige:** Ein Button mit der Live-Anzeige „Plugin › Aktion (Ergebnis)“ zeigt nach jedem Aufruf
  eine Liste aus Bezeichnung und Wert (bis 8 Zeilen). Ohne `returns` erscheinen die Schlüssel wie geliefert.
- **Weiterverwenden:** Folgende Aktionen in derselben Liste bekommen die Werte als
  `{Ergebnis.players}` (englisch `{Result.players}`), z. B. für „Wert setzen“ oder eine Bedingung.
- Erweiterte Plugins schicken dazu `"data": { … }` in ihrer `result`-Antwort mit.

## Erweiterte Plugins (volle Rechte)

Wer mehr braucht, als die Abschottung erlaubt, kann ein **eigenes Programm** als
Plugin schreiben, wie man es von anderen Makro- und Streaming-Programmen kennt: in Python, Node.js, Rust oder einer
anderen Sprache. Solche Plugins laufen **nicht** abgeschottet und können alles, was
der Nutzer am PC kann.

- Sie laufen nur, wenn der Nutzer unter **Plugins › Erweitert** den erweiterten Modus
  einschaltet. Dafür gibt es zwei Bestätigungen: eine Warnung und dann das Eintippen
  von „AUF EIGENE GEFAHR“ (in der englischen Oberfläche „AT MY OWN RISK“).
- Jedes erweiterte Plugin muss beim Installieren noch einmal bestätigt werden.
- Auch sie gibt es nur aus öffentlichen GitHub-Repos (oder im Testmodus aus einem
  eigenen Ordner). Hier dürfen Unterordner und beliebige Dateien dabei sein (bis 80 MB).
- Ist der Modus aus, sind erweiterte Plugins gesperrt, und ihre Aktionen erscheinen
  nicht im Editor.

In `plugin.json` statt `main` einen Startbefehl angeben. Er wird ohne Shell im
Plugin-Ordner ausgeführt; `./name` meint eine Datei im Plugin-Ordner.

```json
{
  "id": "mein-tool",
  "name": "Mein Tool",
  "native": { "command": ["python3", "plugin.py"] },
  "actions": [{ "id": "los", "label": "Los", "fields": [] }],
  "triggers": [{ "id": "fertig", "label": "Fertig" }],
  "settings": [{ "key": "ziel", "label": "Ziel", "type": "text" }]
}
```

TouchDeck startet das Programm und spricht mit ihm über **JSON-Zeilen**: eine
Nachricht pro Zeile auf stdin (von TouchDeck) und stdout (an TouchDeck). Alles auf
stderr erscheint im Protokoll. Es gibt keinen offenen Netzwerk-Port. Stürzt das
Programm ab, startet TouchDeck es neu, höchstens dreimal pro Minute.

| Von TouchDeck | Bedeutung |
|---|---|
| `{"type":"start","plugin":{…},"settings":{…},"touchdeck":"0.10.0"}` | Programm ist gestartet |
| `{"type":"action","requestId":"r_1","id":"los","params":{…},"info":{…}}` | Aktion ausführen, mit `result` antworten |
| `{"type":"value","requestId":"…","value":"…"}` | Antwort auf `getValue` |
| `{"type":"event","kind":"page_view","data":{…}}` | Ein TouchDeck-Ereignis (wird immer gesendet, ohne Berechtigung) |
| `{"type":"reply","requestId":"…","ok":true,"result":…}` | Antwort auf `call` (oder `"ok":false,"error":"…"`) |
| `{"type":"stop"}` | Beenden (sonst nach 2 s gestoppt) |

| An TouchDeck | Bedeutung |
|---|---|
| `{"type":"result","requestId":"r_1","ok":true,"message":"…"}` | Aktion fertig (oder `"ok":false,"error":"…"`); mit `"data":{…}` Rückgabe als Daten (siehe oben) |
| `{"type":"setValue","name":"…","value":"…"}` | Wert setzen |
| `{"type":"getValue","requestId":"…","name":"…"}` | Wert abfragen |
| `{"type":"fire","trigger":"fertig","data":{…}}` | Auslöser melden |
| `{"type":"notify","title":"…","body":"…"}` | Benachrichtigung |
| `{"type":"log","level":"info","message":"…"}` | Protokoll |
| `{"type":"call","requestId":"…","op":"deck.showPage","args":{"page":"OBS"}}` | Erweiterte Plugins brauchen dafür keine Berechtigung, `live.set` nur die ID in `lives`. Dieselben Funktionen wie `td.live` / `td.deck` / `td.status` in JavaScript: `live.set` (`id`, `value`), `deck.pages`, `deck.showPage` (`page`), `deck.setLook` (`button`, `look`), `deck.resetLook` (`button`), `deck.timer` (`name`, `op`, `seconds`), `saver.set` (`card`), `saver.clear`, `status` |

Vollständige Beispiele in Python (nur Standardbibliothek) und Node.js:
[`examples/example-python`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-python) und [`examples/example-node`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-node).

Mods für Spiele: Ein erweitertes Plugin kann den Mod mitbringen, den sein Spiel braucht, und
TouchDeck legt ihn mit einem Klick in den Mod-Ordner des Spiels (Plugin-Karte › „🧩 Mod installieren“):

```json
"mods": [{
  "name": "TouchDeck Bridge",
  "file": "TouchDeckBridge_*.zip",
  "unpack": true,
  "keep": ["TouchDeckBridge/settings.json"],
  "conflicts": ["TouchDeckBridge-dev"],
  "targets": {
    "linux":   ["{steamGame:294100}/Mods"],
    "windows": ["{steamGame:294100}/Mods"],
    "proton":  ["{proton:1062090}/Documents/Timberborn/Mods"]
  },
  "page": "https://steamcommunity.com/sharedfiles/filedetails/?id=…",
  "hint": { "en": "Switch the mod on in the game.", "de": "Mod im Spiel einschalten." }
}]
```

| Feld | Bedeutung |
|---|---|
| `file` | Datei im Plugin. Ein `*` steht für die Version: TouchDeck nimmt die neueste passende Datei und liest die Version aus dem Namen – für ein Mod-Update reicht es, die ZIP zu tauschen. |
| `release` | Statt `file`: `{ "repo": "besitzer/repo", "asset": "Mod_*.zip" }` – TouchDeck nimmt die passende Datei aus dem **neuesten GitHub-Release** des Repos; die Version kommt aus dem `*` oder dem Release-Tag. Die regelmäßige Update-Suche schaut nach neuen Releases, die Karte bietet dann „Mod aktualisieren“ an. |
| `url` | Statt `file`: fester Download-Link auf `https://github.com/…`. Dann `version` angeben. |
| `sha256` | Optionale Prüfsumme; passt sie nicht, installiert TouchDeck nicht. |
| `unpack` | `true`: ZIP in den Ordner entpacken (RimWorld, Timberborn). Sonst wird die Datei so kopiert (Factorio), ältere Dateien nach demselben Muster werden entfernt. |
| `keep` | Dateien, die der Mod selbst schreibt (Einstellungen); bleiben beim Aktualisieren erhalten. |
| `conflicts` | Andere Fassungen, die TouchDeck nicht anfassen darf (z. B. ein Entwicklungsordner). Liegt eine davon im Ordner, zeigt TouchDeck sie an und installiert nicht. |
| `targets` | Mod-Ordner je System. `proton` = Windows-Spiel unter Linux. Platzhalter: `{home}`, `{appdata}`, `{localappdata}`, `{documents}`, `{steamGame:APPID}` (Spielordner in einer beliebigen Steam-Bibliothek), `{proton:APPID}` (Benutzerordner im Proton-Prefix). Ordner, die es auf dem PC nicht gibt, fallen weg. |
| `workshop` | `{ "steamApp": "294100", "item": "3012345678" }` – der Mod liegt im Steam Workshop. Die Karte zeigt „Im Steam Workshop abonnieren“ (öffnet Steam); Steam lädt und aktualisiert ihn. Ist er schon abonniert, erkennt TouchDeck das und bietet keine zweite Kopie an. Geht auch allein, ohne `targets`. |
| `factorioPortal` | Name des Mods im Factorio-Mod-Portal. TouchDeck zeigt die neueste Version und meldet Updates. Herunterladen braucht Factorio-Benutzername und Token des Nutzers (Einstellungen › Factorio-Mod-Portal, freiwillig); ohne verlinkt die Karte aufs Portal und sagt, dass man ihn im Spiel herunterlädt. |
| `page` | Optionaler Link zu einer anderen Mod-Seite. |
| `hint` | Erscheint nach dem Installieren, z. B. dass man den Mod im Spiel einschalten muss. |

TouchDeck fragt vor dem Installieren, zeigt den Zielordner und bietet „Mod aktualisieren“ an, wenn
das Plugin eine neuere Fassung mitbringt; die regelmäßige Update-Suche meldet auch neue Mod-Fassungen
(GitHub-Releases, Factorio-Mod-Portal). Außerhalb des Zielordners wird nichts geschrieben.

## Updates

TouchDeck sucht eine Minute nach dem Start und danach alle 6 Stunden nach neuen Versionen der Plugins,
die über GitHub oder den Marktplatz installiert wurden (Einstellung unter Einstellungen › Allgemein ›
Updates). Damit dein Plugin ein Update bekommt:

- **`version` in `plugin.json` hochzählen** (z. B. `1.2.0` → `1.3.0`) und einen Release anlegen bzw. auf
  den Hauptzweig hochladen. Verglichen wird nur die Version.
- **Fertige Seiten** (`.tdprofile`) dürfen sich ändern: TouchDeck merkt sich, welche Seiten ein Nutzer
  daraus importiert hat, und fragt nach dem Update „Ersetzen / Zusätzlich importieren / Behalten“.
- **Mitgelieferte Icon-Pakete** werden ersetzt, wenn ihre `version` in `pack.json` höher ist; die
  Paket-ID bleibt, Buttons behalten ihre Bilder.
- Plugins mit vollen Rechten werden nie automatisch aktualisiert, immer nur nach Klick.

## Marktplatz

Der Tab **Marktplatz** liest eine Liste aus einem öffentlichen Repo
(`plugins.json`):

```json
{
  "plugins": [
    { "repo": "besitzer/touchdeck-mein-plugin", "name": "Mein Plugin",
      "description": "Kurzbeschreibung", "author": "Dein Name" }
  ]
}
```

Der Marktplatz zeigt deine `README.md` als Beschreibung des Plugins (Knopf „📖 Beschreibung“);
für deutsche Nutzer die `README.de.md`, falls es sie gibt. Bilder mit relativem Pfad
(`![Bildschirmfoto](bildschirmfoto.png)`) werden aus deinem Repo geladen – ein Bild deiner
Seite hilft sehr. Liegt das Plugin in einem Unterordner eines Repos, kommt `"path": "ordner/des/plugins"`
dazu (oder in TouchDeck direkt `besitzer/repo/ordner` eingeben). Wer sein Plugin dort
eintragen möchte, schickt einen Pull Request mit einer neuen Zeile. Vor dem Installieren zeigt TouchDeck immer Quelle, Version und Berechtigungen
an. Installiert wird genau der geprüfte Stand.
