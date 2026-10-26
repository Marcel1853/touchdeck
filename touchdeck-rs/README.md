# TouchDeck

**English** · [Deutsch](README.de.md)

Macro remote for Linux: design button pages on your PC and trigger them from your
phone or tablet – with an editor, an Android app and a deck in the browser. Written
in Rust; the program window uses Tauri with the system's web view.

Current version: **0.12.0** (desktop program), Android app **0.8.3**. Changes per
version: [`CHANGELOG.md`](../CHANGELOG.md)

## Installing

```sh
sudo apt install ./TouchDeck_<version>_amd64.deb   # with menu entry
# or without installing: make the AppImage executable and double-click it
./TouchDeck_<version>_amd64.AppImage
```

- During installation the `.deb` opens the firewall (ufw) for the home network and
  closes it again on removal. Python or Node.js are not needed.
- Closing the window = TouchDeck keeps running in the background (phones stay
  connected). Via the system tray icon (menu in the selected language): open the editor, connect a phone, preview,
  **Start at login**, quit.
- Starting it a second time brings the existing window to the front.
- Without the `.deb` (e.g. the AppImage), TouchDeck creates its own launcher entry
  with icon in your user folder so the start menu and taskbar show it properly. The
  autostart entry always points at the version that is currently running.
- Data is stored in `~/.config/TouchDeck/data/`.
- Uninstall: `sudo apt remove touchdeck`

## Language

The whole interface – editor, deck, Android app and server messages – is available
in German and English. By default it follows the system language; you can change it
in **Settings › General**. The translations live in `web/shared/lang/de.json` and
`en.json` (editor and deck), `server/lang/*.json` (server messages) and
`server/catalog/lang/*.json` (names of actions and triggers). Plugins can ship their
own translations.

The **Android app** is available in German and English too. Its texts – connect
screen, dialogs and the deck – follow the language set in TouchDeck on the PC and
switch along when you change it there. Before the first connection the app uses the
language of the PC it found, otherwise the phone's language.

## Connecting a phone

**With the Android app (recommended):**

1. In the editor, open **📱 Connect phone** and scan the QR code under “Android app”
   – the app is downloaded from the PC. The first time, allow “Install unknown apps”
   for the browser.
2. Open the app: it finds the PC on the Wi-Fi by itself. Tap it → on the PC,
   “… wants to connect” appears → **Allow**.
3. Done: deck in full screen, screen stays on, vibration, the back button goes back
   through pages. The app remembers several PCs.

Alternatively use “Scan QR code from PC” in the app or enter the address by hand.
**Without the app:** scan the QR code in the “Connect phone” dialog with the phone's
camera – the deck opens in the browser.

The editor can only be reached on the PC itself; phones need the key (via
confirmation or QR code) and – if set – the PIN. **Create a new key** signs out all
devices.

## Pages

- **Grid** pages arrange buttons in rows and columns. Column widths and row heights
  can be adjusted individually. Double-click an empty field (or use “＋ Button” in
  the toolbar) to create a button; a single click only clears the selection.
- **Free** pages place buttons anywhere on an area with a chosen aspect ratio (e.g.
  16:10, ideally matching the device). Position and size are stored in percent of
  the page, so phone and tablet show the page exactly in its format, like the
  preview. New buttons are added by double-clicking the area, with “＋ Button” or by
  right-clicking.
- **Folders:** right-click an empty field or the free area › **Create folder here**
  creates a sub-page with a back button and a button at this spot that opens it.
- **Page logic:** every page can have its own events, e.g. with the trigger
  “Page opened / left” to switch an OBS scene or set a value.
- **Ready-made pages** for System, OBS and Steam are built only from normal, visible
  actions and events, so you can change everything afterwards. Steam pages update
  automatically when games are installed or removed and download missing covers
  from Steam.
- **Profiles:** via “⋯” next to the page list or by right-clicking a page, one page
  or all pages can be exported as a `.tdprofile` file – including the images, flows,
  values and events they use. Importing a profile only adds pages and never
  overwrites anything.
- In the deck settings, **Shape of the cells** chooses between “Fill the screen”
  (cells may become a bit wider or taller than square) and “Square”.

## Buttons

Buttons come as **Press**, **Toggle** (on/off with its own look “When on” and its
own actions), **Switch** (flip switch, horizontal or vertical), **States** (each press
moves to the next state with its own look and actions, or the state follows a value)
and **Slider** (volume, microphone, OBS audio source or a custom command with
`{value}`). Every button can have a **live display** (time, CPU, RAM,
temperature, volume, music title, Steam, OBS, Twitch, Discord, values, command
output), react to **events**, and run its own actions on **hold** and on **double
press** (only buttons with double-press actions wait briefly, 0.3 s, for a second
tap). Right-click a button: duplicate, copy, cut, paste, transfer look, **Copy
logic** / **Paste logic** (all actions and events of the button), move to another
page.

## Actions

About 80 actions in groups:

| Group | Actions |
|---|---|
| General | Websites, files, programs and commands, switch pages, keyboard shortcuts, type text, hold keys, mouse, bring windows to the front, quit programs, notifications, screenshot, lock, power, night light, light/dark mode, Wake-on-LAN, HTTP, write files, values, conditions, random (one of the next actions), timer (countdown / stopwatch), show page on all devices, flows, change the look of buttons |
| Audio, music & media | Media control (MPRIS), volume and microphone (PipeWire), switch audio output/input, soundboard |
| Streaming | OBS Studio (scenes, stream, recording, clips, audio sources, sources, filters, texts, transitions, profiles …), Twitch |
| Communication | Discord (mute, deafen, voice channel, volume, push-to-talk, webhooks) |
| Games | Steam (launch games, library, downloads, Big Picture …) |
| Plugins | Actions of installed plugins |

Values appear in labels via `{Name}` (e.g. “Goals: {Goals}”), as a live display and
in OBS texts. `{Time}` and `{Date}` are always available. Values can be filled by
**Calculate value** (e.g. `({Goals} + 1) * 2`), by the response of an **HTTP request**
(also a single JSON field such as `data.0.name`) or by the output of **Run command**.
**Flows** can have inputs that “Run flow” fills per call; inside they are `{Input.Name}`.

Examples of the newer actions and triggers:

- **If / Else if / Else**, **Repeat (n times)**, **While** and **Exit loop** build
  blocks up to **End**; the editor indents them. The current round is `{Event.Loop}`.
- **Random: one of the next actions** runs exactly one randomly chosen action of the
  following ones (count adjustable, 0 = all following).
- **Safety and conditions:** “Only trigger on the second press” asks “Sure?”; a
  button can depend on another button being on or a value matching (otherwise greyed
  out with a lock); buttons can be hidden at start and appear by action. After every
  successful action the button briefly shows a check mark.
- **Sleep and PC lock:** the deck dims or shows a clock after a few minutes; if you
  like, it is locked while the PC is locked.
- **Widget, quick settings and NFC (Android app):** trigger buttons on the PC without
  opening the deck.
- **Knobs and slider styles:** a slider can be an area, a rail with handle, a knob
  (ring) or a knob with pointer, with its own colour and thickness.
- **Timer (countdown / stopwatch)** with start, pause and reset; the live display
  **Timer** shows the time left, the trigger **Timer finished** reacts when it runs
  out.
- **Shortcut pressed on the PC** (trigger) reacts to a key combination anywhere on
  the PC (X11); **Key sequence (recorded)** plays back keys recorded in the editor
  with their real pauses.
- **Program in the foreground** + **Show page on all devices**: e.g. phone and tablet
  switch to the OBS page by themselves as soon as OBS is in front.
- **Page indicator (●○○)** as a live display shows the position of the page in the page
  list; **Go to page** can also jump to the next / previous page in the list.

## Sections in the editor

In the sidebar on the left (Ctrl+1 … Ctrl+8):

- **Pages** – design grid or free pages, page logic; ready-made pages for Steam,
  System and OBS.
- **Values** – your own variables with start value, folder and description.
- **Events** – “When …, then …”: value changes, start, interval, time of day, file,
  phone, toggle, page opened / left, program in the foreground, timer finished,
  audio (incl. sound or microphone muted), Steam, OBS, Twitch, Discord, plugins.
  With cooldown; endless loops are caught.
- **Flows** – reusable action lists.
- **Logs** – everything that happens, live.
- **Plugins** – installed plugins, marketplace, develop, advanced mode.
- **🖼️ Icon packs** – your own image collections (see below).
- **Settings** – General, Phone & security, Deck, Look, Integrations, Backups,
  Log, Information.

The **status bar** at the bottom shows the connected devices with name, address and
the page they currently show, the state of OBS, Twitch and Discord (a click opens
the settings), plus the PC's address and the version.

## Setting up Twitch

1. Register an application at <https://dev.twitch.tv/console>: redirect URL
   `http://localhost`, category “Application Integration”, client type **Public**.
2. In TouchDeck: enter the client ID under **Settings → Integrations → Twitch** and
   click **Connect to Twitch**.
3. Enter the code shown at <https://www.twitch.tv/activate>.

The login stays on the PC only and is renewed automatically.

## Setting up Discord

1. Create a “New Application” at <https://discord.com/developers/applications>.
2. Under **OAuth2 → Redirects**, add `http://localhost`.
3. Copy the **Client ID** and **Client Secret** (via “Reset Secret”).
4. In TouchDeck: enter them under **Settings → Integrations → Discord**, click
   **Connect to Discord** and **Authorize** in Discord.

Messages to a channel are sent without login via webhook (in Discord: edit channel →
Integrations → Webhooks → copy webhook URL).

## Setting up OBS

1. In OBS: **Tools → WebSocket Server Settings → Enable WebSocket server**.
2. In TouchDeck: **Settings → Integrations → Take over from OBS** – port and password
   are read from the OBS configuration (Flatpak too).
3. **Test connection**.

## Icon packs

Your own image collections, in their own section **🖼️ Icon packs** in the left
sidebar of the editor (next to Plugins): create them, add images, export
them as a `.tdicons` file and share them, import packs from others. Stored in the
data folder under `icons/packs/`.

- Every pack has a name, author, link, short license, **description, version, cover
  image, license text** and an optional **matching button colour** – picking an image
  from the pack gives a button with the default colour this colour.
- Every image can get a **name and tags** (click it in the pack). The image picker on
  a button searches file names, names and tags.
- **Create icon** builds a PNG from background, emoji and text.
- The import recognises the content, not the file extension: `.tdicons`, ZIP files
  with images (also in sub-folders) and common description files such as a key/value
  `info.txt` or a `manifest.json` with `icons.json` (names and tags) and a license
  file. Only import images you are allowed to use.

## AI assistants (MCP)

Under **Settings › AI assistants** you can switch on an MCP server. AI assistants like Claude can
then operate TouchDeck: look at pages, buttons and live values, press buttons, read and set values
and switch the page on the devices. With the second option they may also create, change and delete
pages and buttons and run any action.

- Only reachable from the PC itself at `http://localhost:8770/mcp`, with its own key.
- The settings show the ready-made command for Claude Code
  (`claude mcp add --transport http touchdeck …`) and the configuration for other programs.
- Every use is logged under “MCP”.

## Plugins

Normal plugins are JavaScript and run sandboxed (QuickJS) with fixed limits and
permissions. Advanced plugins are separate programs with full rights and only run
after a double confirmation, at your own risk. Plugins are installed only from
public GitHub repos; there is a marketplace and a test mode. Plugins can ship their
own translations.
Guide: [plugin wiki](https://github.com/Marcel1853/touchdeck-plugins/wiki),
examples: [touchdeck-plugins](https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples),
short version: [PLUGINS.md](PLUGINS.md).

## Development

Everything lives in the project folder (Rust, Cargo cache, Android SDK, tools):

```bash
source ../env.sh
cargo test                     # tests
cargo run -p touchdeck         # program with window
cargo run -p touchdeck-server  # server only, editor in the browser
scripts/build-android.sh       # build the Android app and put it in downloads/
```

Layout:

```
touchdeck-rs/
├── server/                 library + command-line server (touchdeck-server)
│   ├── catalog/            description of actions and triggers for the editor (with lang/)
│   ├── lang/               translations of server messages
│   └── src/
│       ├── core/           data (store), actions, events, live values, timers, profiles, icon packs, plugins, log, backups
│       ├── server/         HTTP routes, WebSocket, discovery on the Wi-Fi
│       ├── system/         Linux: programs, audio, media (linux.rs), keyboard/mouse/windows (x11.rs)
│       └── integrations/   OBS, Steam, Twitch, Discord, page import
├── app/                    desktop program (Tauri): window, system tray icon, autostart, launcher entry
├── web/                    editor (editor/), deck for the phone (deck/), shared parts and translations (shared/)
├── downloads/              Android app that TouchDeck offers to phones (built, not in the repo)
└── scripts/                helper scripts
```

Useful environment variables:

| Variable | Meaning |
|---|---|
| `TOUCHDECK_PORT` | different port (default 8770), e.g. for testing next to a running version |
| `TOUCHDECK_DATA` | different data folder |
| `TOUCHDECK_PUBLIC` | different folder for editor and deck |

To build the window, the system needs once:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

## Building installation packages

```bash
source ../env.sh
scripts/build-android.sh       # once: the Android app that goes into the package
cd app
XDG_CACHE_HOME="$PWD/../../.cache" cargo tauri build
```

Result in `target/release/bundle/`: a `.deb` (about 7 MB) and an AppImage
(about 85 MB, includes WebKitGTK).
