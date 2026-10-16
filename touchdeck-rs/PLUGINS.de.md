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
    "notify": true
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
  ]
}
```

| Feld | Bedeutung |
|---|---|
| `id` | 1–32 Zeichen: Kleinbuchstaben, Ziffern, Bindestrich. Muss eindeutig sein. |
| `license` | Lizenz des Plugins, z. B. `MIT` oder `Alle Rechte vorbehalten`. Öffentlich heißt nicht frei: Ohne Lizenz gehört der Code allein dem Autor. |
| Texte | `name`, `description`, `label` usw. dürfen statt eines Texts eine Übersetzung sein: `{ "en": "Light", "de": "Licht" }`. TouchDeck nimmt die eingestellte Sprache, sonst Englisch, sonst Deutsch. So kann ein Plugin seine Übersetzungen gleich mitbringen. |
| `permissions.http` | Erlaubte Adressen: genauer Name, `*.domain` für alle Unteradressen, `lan` für Geräte im Heimnetz. Ohne Eintrag kein Netz. |
| `permissions.values` | TouchDeck-Werte lesen und setzen. |
| `permissions.notify` | Benachrichtigungen am PC zeigen. |
| `settings` | Felder, die der Nutzer beim Plugin ausfüllt (`text`, `password`, `number`, `checkbox`). |
| `actions[].fields` / `triggers[].fields` | Eingabefelder wie bei den eingebauten Aktionen: `text`, `textarea`, `number`, `checkbox`, `select` (mit `options`), `dynamic` (z. B. `"source": "variables"`). |

Platzhalter wie `{Wert}` oder `{Event.Nutzer}` in Aktionsfeldern ersetzt TouchDeck,
bevor das Plugin sie bekommt.

Ein Auslöser passt, wenn jedes ausgefüllte Feld gleich dem gleichnamigen Eintrag in
den Daten von `td.fire` ist. Leere Felder passen immer. Alle Daten stehen in Events
als `{Event.Name}` zur Verfügung.

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
```

Alle Aufrufe sind synchron (kein `await` nötig). Ein Plugin läuft in einem eigenen
Thread; lange Netzanfragen halten TouchDeck nicht auf.

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
| `{"type":"start","plugin":{…},"settings":{…},"touchdeck":"0.7.0"}` | Programm ist gestartet |
| `{"type":"action","requestId":"r_1","id":"los","params":{…},"info":{…}}` | Aktion ausführen, mit `result` antworten |
| `{"type":"value","requestId":"…","value":"…"}` | Antwort auf `getValue` |
| `{"type":"stop"}` | Beenden (sonst nach 2 s gestoppt) |

| An TouchDeck | Bedeutung |
|---|---|
| `{"type":"result","requestId":"r_1","ok":true,"message":"…"}` | Aktion fertig (oder `"ok":false,"error":"…"`) |
| `{"type":"setValue","name":"…","value":"…"}` | Wert setzen |
| `{"type":"getValue","requestId":"…","name":"…"}` | Wert abfragen |
| `{"type":"fire","trigger":"fertig","data":{…}}` | Auslöser melden |
| `{"type":"notify","title":"…","body":"…"}` | Benachrichtigung |
| `{"type":"log","level":"info","message":"…"}` | Protokoll |

Vollständige Beispiele in Python (nur Standardbibliothek) und Node.js:
[`examples/example-python`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-python) und [`examples/example-node`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-node).

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

Wer sein Plugin dort eintragen möchte, schickt einen Pull Request mit einer neuen
Zeile. Vor dem Installieren zeigt TouchDeck immer Quelle, Version und Berechtigungen
an. Installiert wird genau der geprüfte Stand.
