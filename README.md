# TouchDeck

**English** · [Deutsch](README.de.md)

Macro remote for Linux: design button pages on your PC and trigger them from your
phone or tablet – launch programs, keyboard shortcuts, media and volume, Steam
games, OBS Studio, Twitch, Discord and much more.

| Folder | Contents |
|---|---|
| [`touchdeck-rs/`](touchdeck-rs/) | Desktop program in Rust (Tauri), version 0.9.0: server, actions and integrations, editor and deck, window with system tray icon |
| [`android/`](android/) | Android app (Kotlin), version 0.6.2, in German and English: finds the PC on the Wi-Fi, pairs via confirmation or QR code, shows the deck in full screen |

Changes per version: [`CHANGELOG.md`](CHANGELOG.md)

## Features

- **Editor** with pages, buttons, live displays, values (variables), events
  (“When …, then …”), flows (reusable action lists), logs and backups; right-click
  a button to duplicate, copy, cut, paste, transfer its look, copy or paste its
  logic, or move it to another page
- **Status bar** at the bottom of the editor: connected devices with name, address
  and the page they show, state of OBS, Twitch and Discord, plus address and version
- **Pages** as a **grid** (with custom column widths and row heights) or **free**
  (buttons placed anywhere, position and size in percent, on an area with a chosen
  aspect ratio – phone and tablet show it exactly in that format); every page can
  have its own events, e.g. with the trigger “Page opened / left”. New buttons are
  created by double-click or “＋ Button” (a single click only clears the selection);
  right-click › “Create folder here” adds a sub-page with a back button
- **Profiles**: export one page or all pages as a `.tdprofile` file, including the
  images, flows, values and events they use, and import them elsewhere – importing
  only adds and never overwrites anything
- **Buttons**: press, toggle and slider, plus actions for hold and double press,
  a separate look “When on” and events per button
- **Ready-made pages** for System, OBS and Steam, built only from normal, visible
  actions and events; Steam pages update automatically when games are installed or
  removed and download missing covers from Steam
- **Around 80 actions**: programs, keyboard and mouse, windows, audio and media
  (PipeWire, MPRIS), soundboard, system, files, HTTP, timers (countdown / stopwatch),
  random choice, Steam, OBS Studio (obs-websocket 5), Twitch (own client ID, device
  code login), Discord (voice control via the Discord app, webhooks); triggers such
  as “Program in the foreground” (e.g. phone and tablet switch to the OBS page as
  soon as OBS is in front) or “Timer finished”
- **Phone and tablet**: Android app or deck in the browser; several PCs, automatic
  discovery on the Wi-Fi, pairing with confirmation on the PC, optional PIN; deck
  setting “Shape of the cells”: “Fill the screen” or “Square”
- **German and English**: the whole interface (editor, deck, server messages, tray
  menu) follows the system language and can be switched in Settings › General.
  The **Android app** is available in German and English as well and follows the
  language set in TouchDeck on the PC – including the connect screen and dialogs
  (before the first connection: the language of the PC it found, otherwise the
  phone's language). Plugins can ship their own translations
- **Icon packs**: their own section in the left sidebar of the editor (🖼️ Icon
  packs, next to Plugins): create your own image collections, export them and share
  them
- **Plugins**: extensions in JavaScript, sandboxed and only from public GitHub
  repos, with a marketplace; plus advanced plugins with full rights, only after a
  double confirmation ([guide](touchdeck-rs/PLUGINS.md))
- **Desktop program** with system tray icon (menu in the selected language), its
  own launcher entry with icon, autostart (always pointing at the running version)
  and firewall rule for the home network

Detailed description: [`touchdeck-rs/README.md`](touchdeck-rs/README.md)

## Building

Requirements: Rust, Java 17+ and an Android SDK (for the app). `env.sh` points all
paths to folders inside the project (`.rustup/`, `.cargo/`, `.android-sdk/`,
`.cache/`), so nothing has to be installed system-wide.

```sh
source env.sh
cd touchdeck-rs
cargo test                     # automated tests
scripts/build-android.sh       # build the Android app (signed with android/keystore/, see below)
cd app && XDG_CACHE_HOME="$PWD/../../.cache" cargo tauri build   # .deb and AppImage
```

The Android app is signed with its own key. It is **not** part of the repository:
create `android/keystore/keystore.properties` with `storeFile`, `storePassword`,
`keyAlias` and `keyPassword` (generate the key with `keytool -genkeypair`). Without
it, `assembleDebug` still builds.

## Platform

Linux with X11 (keyboard shortcuts and mouse actions use XTest), PipeWire,
D-Bus/MPRIS. Tested on Linux Mint 22 (Cinnamon) and Android 16.
