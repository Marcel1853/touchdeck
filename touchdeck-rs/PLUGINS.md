# Writing TouchDeck plugins

**English** · [Deutsch](PLUGINS.de.md)

> The detailed public guide is in the
> [plugin wiki](https://github.com/Marcel1853/touchdeck-plugins/wiki), examples to
> copy are in [touchdeck-plugins](https://github.com/Marcel1853/touchdeck-plugins).
> This file is the short version for developers of TouchDeck itself.

There are two kinds: **normal plugins** (JavaScript, sandboxed – the default) and
**advanced plugins** (separate programs with full rights, only at your own risk, see
below).

A plugin adds new **actions** (for buttons, events and templates) and new
**triggers** (for events) to TouchDeck. Plugins are JavaScript and run sandboxed in
their own small JS engine. They can only do what TouchDeck offers them through the
`td` object, and only what they declare as a permission in `plugin.json`.

## Rules

- **Public on GitHub:** plugins are installed only from public GitHub repos, so
  anyone can check the source code. TouchDeck loads the latest release or, if there
  is none, the main branch.
- **Everything in the top folder:** `plugin.json`, the script (`plugin.js`) and
  optionally `README.md` and images. Subfolders and other file types are ignored.
- **Limits:** 32 MB memory, 5 seconds of computing time per call, 10 MB plugin size,
  network responses up to 1 MB. Redirects are not followed.
- **No access** to files, programs, keyboard or mouse. `require`, `fetch`, `process`
  and the like do not exist.

## Developing

1. Create a folder with `plugin.json` and `plugin.js` (template: [`examples/example`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example)).
2. In TouchDeck: enter the folder under **Plugins › Develop** and click **Load**.
   After changes, click **Reload** on the plugin. Output from `td.log` appears under
   **Logs**.
3. Done? Publish it as a public GitHub repo, ideally with a release (e.g. `v1.0.0`).
   Then anyone can install it via **Plugins › Marketplace** with `owner/repo`.

## plugin.json

```json
{
  "id": "my-plugin",
  "name": "My plugin",
  "version": "1.0.0",
  "author": "Your name",
  "description": "What the plugin does.",
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
    { "key": "token", "label": "API key", "type": "password" }
  ],
  "actions": [
    { "id": "send", "label": "Send message", "icon": "📨",
      "fields": [{ "key": "text", "label": "Text", "type": "text" }] }
  ],
  "triggers": [
    { "id": "new", "label": "New message", "icon": "🔔",
      "fields": [{ "key": "channel", "label": "Channel (empty = any)", "type": "text" }] }
  ],
  "lives": [
    { "id": "unread", "label": "Unread messages" }
  ]
}
```

| Field | Meaning |
|---|---|
| `id` | 1–32 characters: lowercase letters, digits, hyphen. Must be unique. |
| `license` | The plugin's license, e.g. `MIT` or `All rights reserved`. Public does not mean free: without a license, the code belongs to the author alone. |
| Texts | `name`, `description`, `label` etc. may be a translation instead of a text: `{ "en": "Light", "de": "Licht" }`. TouchDeck uses the selected language, otherwise English, otherwise German. This way a plugin ships its own translations. |
| `permissions.http` | Allowed addresses: exact name, `*.domain` for all subdomains, `lan` for devices on the home network. Without an entry, no network. |
| `permissions.values` | Read and set TouchDeck values. |
| `permissions.notify` | Show notifications on the PC. |
| `permissions.events` | React to TouchDeck events with `td.on` (page opened, muted, program in the foreground, timer finished …). |
| `permissions.deck` | Control the deck: list pages, show a page on all devices, change and restore the look of buttons, control timers. |
| `permissions.status` | Read connected devices and the state of OBS, Twitch and Discord. |
| `lives` | Live displays of the plugin. They appear in the editor under a button's live display as “Plugin › Label” and can show text, a progress bar and the “on” look. No permission needed. |
| `settings` | Fields the user fills in for the plugin (`text`, `password`, `number`, `checkbox`). |
| `actions[].fields` / `triggers[].fields` | Input fields like those of the built-in actions: `text`, `textarea`, `number`, `checkbox`, `select` (with `options`), `dynamic` (e.g. `"source": "variables"`). |

TouchDeck replaces placeholders such as `{Value}` or `{Event.User}` in action fields
before the plugin receives them.

A trigger matches when every filled-in field equals the entry of the same name in
the data passed to `td.fire`. Empty fields always match. All data is available in
events as `{Event.Name}`.

## Ready-made pages

Put one or more `.tdprofile` files (exported from TouchDeck via the page menu “⋯”) into
your plugin – in the main folder or in sub-folders (up to 3 levels, e.g.
`pages/power/Overview.tdprofile`, shown as “power › Overview”). One file may contain
several pages; their folders in the page list are kept. TouchDeck shows them on the plugin card and under
**Ready-made pages** in the page list; one click imports the page with its flows,
values and images. Importing only adds, it never overwrites anything.

## Designs and icon packs

A plugin can bring designs for buttons and sliders (`looks` in plugin.json). They appear
in the editor under **Look › Designs**; a click takes them over. Images are paths
inside the plugin (`icons/…`, png/svg/jpg/webp/gif).

```json
"looks": [
  { "id": "rust", "name": { "en": "Rust", "de": "Rost" },
    "style": { "color": "#7a3b1e", "textColor": "#ffe2c4", "bgImage": "icons/rust.jpg",
               "texFit": "cover", "texVeil": 20, "overlay": "frame", "iconSize": 120 },
    "slider": { "shape": "mixer", "color": "#ff8a3d", "thickness": 12 } }
],
"iconPacks": ["packs/factorio.tdicons"]
```

`style` knows color, colorOn, textColor, bgOpacity, fontSize, fontFamily, bold, italic,
textOutline, titlePos, iconSize, iconFit, overlay, texFit, texVeil, bgImage and icon;
`slider` knows shape (fill, rail, fader, knob, dial, mixer, combo), color and thickness.

`iconPacks` lists icon packs (`.tdicons`, exported from TouchDeck) the plugin needs. TouchDeck
imports them when the plugin is installed – only if there is no pack with that name yet.
Icon packs can carry designs themselves: `looks` in their `pack.json`, images by file name.

## The `td` object

```js
td.action('send', (params, info) => {
  // params: fields of the action; info: { buttonId, event }
  // Return value (optional): text shown as a message. Errors with throw.
  return 'Sent';
});

td.every(30000, () => { /* every 30 s (at least 1 s) */ });

td.fire('new', { channel: 'general', User: 'anna' });       // report your own trigger

const res = td.http({ method: 'POST', url: 'https://api.example.com/x',
  headers: { Authorization: `Bearer ${td.settings.token}` }, json: { a: 1 } });
res.status; res.ok; res.body; res.json();

td.values.get('Name'); td.values.set('Name', 'value');     // needs "values"
td.notify('Title', 'Text');                                 // needs "notify"
td.log('Info'); td.warn('Warning'); console.log('works too');
td.settings.token;                                          // the user's settings
td.plugin.id; td.plugin.version;

// needs "events": the same events the event triggers use; '*' = all
td.on('page_view', (data) => {          // data: { pageId, state: 'open'|'close', Seite, Page }
  if (data.state === 'open') td.log(`Page ${data.Page} opened`);
});

// own live display from "lives": text, number 0–100 (progress bar), "on" look
td.live.set('unread', { text: '3 new', value: 30, on: true });
td.live.set('unread', 'just a text');
// real number for graphs (not limited to 0–100), e.g. power in MW:
td.live.set('power', { text: '35.2 MW', number: 35.2 });
// several data series for one graph (up to 12); new names become new lines automatically:
td.live.set('power', { text: '66 MW', number: 66, series: { Steam: 30.5, Solar: 12, Nuclear: 23.5 } });

// needs "deck"
td.deck.pages();                        // [{ id, name, buttons: [{ id, title }] }]
td.deck.showPage('OBS');                // id or name, shown on all devices
td.deck.setLook(buttonId, { title: 'Live', color: '#c0392b', textColor: '#fff', icon: '🔴' });
td.deck.resetLook(buttonId);            // back to the original look
td.deck.timer('Break', 'start', 300);   // start | stop | reset | toggle, seconds
// card on the screensaver (below the clock), also needs "deck"; clear() removes it
td.saver.set({ title: 'Factorio', color: '#e6b45a', items: [{ label: 'Research', text: 'Logistics 2 · 42 %' }, { label: 'Power', text: '35.2 MW' }] });
td.saver.clear();

// needs "status"
td.status();  // { version, devices: [{ name, ip, page }], obs, twitch, discord }
```

Event names are the trigger types of the editor, e.g. `page_view`, `audio_mute`,
`app_focus`, `timer_done`, `value_change`, `obs_scene`. Event data carries the
German and English field names (`Seite` and `Page`). Plugin triggers reported with
`td.fire` are not passed on to plugins.

Plugin actions and triggers work everywhere in TouchDeck: on press, hold and double
press, in page logic (“Page opened / left”), in flows and in events.

All calls are synchronous (no `await` needed). Each plugin runs in its own thread;
long network requests do not hold up TouchDeck.

## Actions that return data

Instead of a short message, an action can return **data**. TouchDeck builds the display itself – the
plugin needs no user interface of its own for it.

```js
td.action('status', () => ({ players: 7, memory: 4210, uptime: 50432 }));
```

In `plugin.json`, `returns` says what the values are called and in which unit they come:

```json
{ "id": "status", "label": "Server status", "fields": [],
  "returns": [
    { "key": "players", "label": { "en": "Players", "de": "Spieler" }, "unit": "number" },
    { "key": "memory",  "label": "RAM", "unit": "mb" },
    { "key": "uptime",  "label": { "en": "Uptime", "de": "Laufzeit" }, "unit": "seconds" }
  ] }
```

Units: `number`, `bytes`, `kb`, `mb`, `gb`, `seconds`, `ms`, `percent`, `text`.
TouchDeck turns them into readable values (4210 MB → “4.1 GB”, 50432 s → “14 h”).

- **Display:** a button with the live display “Plugin › Action (result)” shows a list of label and value
  after every call (up to 8 rows). Without `returns`, the keys are shown as delivered.
- **Reuse:** following actions in the same list get the values as `{Result.players}` (German
  `{Ergebnis.players}`), e.g. for “Set value” or a condition.
- Advanced plugins add `"data": { … }` to their `result` reply.

## Advanced plugins (full rights)

If you need more than the sandbox allows, you can write a **separate program** as a
plugin, as known from other macro and streaming programs: in Python, Node.js, Rust
or another language. Such plugins do **not** run sandboxed and can do everything
the user can do on the PC.

- They only run if the user turns on advanced mode under **Plugins › Advanced**.
  This takes two confirmations: a warning and then typing “AT MY OWN RISK” (in the
  German interface “AUF EIGENE GEFAHR”).
- Every advanced plugin has to be confirmed once more when it is installed.
- They too are only available from public GitHub repos (or, in test mode, from a
  local folder). Here subfolders and any files are allowed (up to 80 MB).
- When the mode is off, advanced plugins are blocked and their actions do not appear
  in the editor.

In `plugin.json`, give a start command instead of `main`. It is run without a shell
in the plugin folder; `./name` refers to a file in the plugin folder.

```json
{
  "id": "my-tool",
  "name": "My tool",
  "native": { "command": ["python3", "plugin.py"] },
  "actions": [{ "id": "go", "label": "Go", "fields": [] }],
  "triggers": [{ "id": "done", "label": "Done" }],
  "settings": [{ "key": "target", "label": "Target", "type": "text" }]
}
```

TouchDeck starts the program and talks to it via **JSON lines**: one message per
line on stdin (from TouchDeck) and stdout (to TouchDeck). Everything on stderr
appears in the log. There is no open network port. If the program crashes,
TouchDeck restarts it, at most three times per minute.

| From TouchDeck | Meaning |
|---|---|
| `{"type":"start","plugin":{…},"settings":{…},"touchdeck":"0.10.0"}` | The program has started |
| `{"type":"action","requestId":"r_1","id":"go","params":{…},"info":{…}}` | Run an action, answer with `result` |
| `{"type":"value","requestId":"…","value":"…"}` | Answer to `getValue` |
| `{"type":"event","kind":"page_view","data":{…}}` | A TouchDeck event (always sent, no permission needed) |
| `{"type":"reply","requestId":"…","ok":true,"result":…}` | Answer to `call` (or `"ok":false,"error":"…"`) |
| `{"type":"stop"}` | Quit (otherwise stopped after 2 s) |

| To TouchDeck | Meaning |
|---|---|
| `{"type":"result","requestId":"r_1","ok":true,"message":"…"}` | Action finished (or `"ok":false,"error":"…"`); with `"data":{…}` it returns data (see above) |
| `{"type":"setValue","name":"…","value":"…"}` | Set a value |
| `{"type":"getValue","requestId":"…","name":"…"}` | Query a value |
| `{"type":"fire","trigger":"done","data":{…}}` | Report a trigger |
| `{"type":"notify","title":"…","body":"…"}` | Notification |
| `{"type":"log","level":"info","message":"…"}` | Log |
| `{"type":"call","requestId":"…","op":"deck.showPage","args":{"page":"OBS"}}` | Advanced plugins need no permissions for it, `live.set` only needs the id in `lives`. Same functions as `td.live` / `td.deck` / `td.status` in JavaScript: `live.set` (`id`, `value`), `deck.pages`, `deck.showPage` (`page`), `deck.setLook` (`button`, `look`), `deck.resetLook` (`button`), `deck.timer` (`name`, `op`, `seconds`), `saver.set` (`card`), `saver.clear`, `status` |

Complete examples in Python (standard library only) and Node.js:
[`examples/example-python`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-python) and [`examples/example-node`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-node).

## Updates

TouchDeck checks one minute after start and then every 6 hours for new versions of plugins installed
from GitHub or the marketplace (setting under Settings › General › Updates). For your plugin to get an
update:

- **Raise `version` in `plugin.json`** (e.g. `1.2.0` → `1.3.0`) and create a release or push to the main
  branch. Only the version is compared.
- **Ready-made pages** (`.tdprofile`) may change: TouchDeck remembers which pages a user imported from
  them and asks after the update “Replace / Import additionally / Keep”.
- **Bundled icon packs** are replaced when their `version` in `pack.json` is higher; the pack id stays,
  buttons keep their images.
- Plugins with full rights are never updated automatically, only after a click.

## Marketplace

The **Marketplace** tab reads a list from a public repo (`plugins.json`):

```json
{
  "plugins": [
    { "repo": "owner/touchdeck-my-plugin", "name": "My plugin",
      "description": "Short description", "author": "Your name" }
  ]
}
```

The marketplace shows your `README.md` as the plugin's description (button “📖 Description”);
for German users `README.de.md` is shown if it exists. Images with relative paths
(`![Screenshot](screenshot.png)`) are loaded from your repo – a screenshot of your page helps.
If the plugin lives in a sub-folder of a repo, add `"path": "folder/of/the/plugin"`
(or enter `owner/repo/folder` directly in TouchDeck). To list your plugin there, send a
pull request with a new line. Before installing,
TouchDeck always shows the source, version and permissions. Exactly the reviewed
version is installed.
