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
    "notify": true
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
| `settings` | Fields the user fills in for the plugin (`text`, `password`, `number`, `checkbox`). |
| `actions[].fields` / `triggers[].fields` | Input fields like those of the built-in actions: `text`, `textarea`, `number`, `checkbox`, `select` (with `options`), `dynamic` (e.g. `"source": "variables"`). |

TouchDeck replaces placeholders such as `{Value}` or `{Event.User}` in action fields
before the plugin receives them.

A trigger matches when every filled-in field equals the entry of the same name in
the data passed to `td.fire`. Empty fields always match. All data is available in
events as `{Event.Name}`.

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
```

All calls are synchronous (no `await` needed). Each plugin runs in its own thread;
long network requests do not hold up TouchDeck.

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
| `{"type":"start","plugin":{…},"settings":{…},"touchdeck":"0.7.0"}` | The program has started |
| `{"type":"action","requestId":"r_1","id":"go","params":{…},"info":{…}}` | Run an action, answer with `result` |
| `{"type":"value","requestId":"…","value":"…"}` | Answer to `getValue` |
| `{"type":"stop"}` | Quit (otherwise stopped after 2 s) |

| To TouchDeck | Meaning |
|---|---|
| `{"type":"result","requestId":"r_1","ok":true,"message":"…"}` | Action finished (or `"ok":false,"error":"…"`) |
| `{"type":"setValue","name":"…","value":"…"}` | Set a value |
| `{"type":"getValue","requestId":"…","name":"…"}` | Query a value |
| `{"type":"fire","trigger":"done","data":{…}}` | Report a trigger |
| `{"type":"notify","title":"…","body":"…"}` | Notification |
| `{"type":"log","level":"info","message":"…"}` | Log |

Complete examples in Python (standard library only) and Node.js:
[`examples/example-python`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-python) and [`examples/example-node`](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples/example-node).

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

To list your plugin there, send a pull request with a new line. Before installing,
TouchDeck always shows the source, version and permissions. Exactly the reviewed
version is installed.
