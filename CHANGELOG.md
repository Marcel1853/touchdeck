# TouchDeck Changelog

All notable changes to TouchDeck, the macro remote control for Linux, and its Android app.

**English** · [Deutsch](CHANGELOG.de.md)

```text
---------------------------------------------------------------------------------------------------
Version: 0.9.0
Date: 2026-10-16
  Major Features:
    - Editor, deck, action catalog, server messages and desktop app are fully available in German
      and English. All texts live in translation files per language; the language can be switched
      under Settings › General and open decks reload by themselves.
    - Free pages: besides the grid, a page can be "Free". Buttons are placed and sized freely on an
      area with a selectable format (16:9, 16:10, 4:3 …): drag to move, drag the corner to resize,
      arrow keys nudge (with Shift further). Switching between grid and free keeps all buttons.
    - Grid pages can have custom column widths and row heights.
    - Page logic: every page has its own events, with the new trigger "Page opened / left" (also
      fires when a device disconnects) and the placeholder {Event.Page}.
    - Double press: buttons have their own action list "When double-pressed" besides press and
      hold. Only buttons with double-press actions wait briefly (0.3 s) for a second tap, all
      others react immediately.
    - Profiles can be exported and imported as .tdprofile files (one page or all pages, including
      used templates, values, events and images). Importing adds pages and never overwrites
      anything.
    - Status bar at the bottom of the editor: connected devices with name, address and the page
      they show, state of OBS, Twitch and Discord, plus address and version.
  Features:
    - Optional footer in the deck (Settings › Deck › "Show footer"): connection state with the
      PC's name and, in the Android app, a "Switch PC" button.
    - Icon editor (Icon packs › "Create icon"): builds a PNG from background (colour, gradient
      or transparent; square, rounded or circle), emoji and text with live preview and saves it
      to the own images or straight into an icon pack.
    - Button textures: a background image of its own (own image or from an icon pack) below icon
      and label, plus overlays on top: gloss, frame, stripes, dots, vignette, glow. "Copy look"
      includes them; profiles export the images.
    - Live displays with a number (CPU, RAM, temperature, volume, timer, numeric values) can
      additionally be shown as a progress bar at the bottom of the button.
    - Font options for labels and live text: font (normal, serif, monospace, condensed,
      handwriting – fonts every device has, so the look can differ slightly), bold, italic and an
      outline (0–6 px) for text on images. "Copy look" includes them.
    - Live display "Page indicator (●○○)" shows the position of the page in the page list, and
      "Go to page" can now jump to the next / previous page in the list.
    - Timer: action "Timer (countdown / stopwatch)" with start, pause and reset, live display
      "Timer" showing the time left every second, and trigger "Timer finished".
    - New trigger "Program in the foreground" and action "Show page on all devices": e.g. phone
      and tablet switch to the OBS page by themselves as soon as OBS is in the foreground.
    - New action "Random: one of the next actions": runs exactly one randomly chosen action of the
      following ones (count adjustable, 0 = all following) and skips the others.
    - Right-click on an empty field or the free area › "Create folder here": creates a sub-page
      with a back button and a button at this spot that opens it.
    - Right-click › "Copy logic" / "Paste logic" transfers all actions (press, off, hold, release,
      double press) and events of a button to another one.
    - Buttons can have their own icon and label for the "on" state (e.g. "Sound on" / "Muted").
      "On" is a switched-on toggle or a live display reporting "on"; the "on" colour now also
      works for live displays.
    - New event trigger "Sound or microphone muted/unmuted"; the mute button on the start page
      uses it as an example.
    - Ready-made pages (System, OBS, Steam) rebuilt with visible logic: buttons show their state
      via events with "Change look" and "Restore original look" instead of hidden live logic.
      OBS scene buttons compare against the new value "OBS scene".
    - Steam pages update themselves: they are checked at startup and when a Steam page is opened,
      keep the order and your own changes of existing game buttons, drop uninstalled games and
      add new ones at the end.
    - Missing game covers are downloaded once from Steam's image server.
    - New deck setting "Shape of the cells": "Fill the screen" lets cells become somewhat wider or
      taller than square so pages use the screen better; "Square" is still available. Gaps and
      margins adapt to the size.
    - Icon packs have their own section in the left sidebar; Ctrl+1 … 8 switches between sections.
  Minor Features:
    - Placeholders also understand English names, e.g. {Time}, {Date} and {Event.User}; the editor
      shows hints for all triggers.
    - Trigger "Toggle switched on/off" shows a hint when there is no button with the "Toggle"
      behaviour yet.
    - Triggers without a "previous page" offer a page selection.
    - Badge "2×" in the editor for buttons with double-press actions.
    - The deck reports its name after connecting; the Android app sends the device name from its
      settings, browsers send their ID.
    - /api/info lists the connected devices.
  Changes:
    - Deselecting a button is easier: click next to the page or the new "✕" in the button
      properties (Esc still works) – then the page settings appear.
    - Page settings (shown when no button is selected) are split into the tabs "Logic" and
      "Grid" / "Area", with the logic first.
    - A single click on an empty grid field or the free area no longer creates a button, it only
      clears the selection. New buttons are created by double-click, right-click or "＋ Button" in
      the toolbar.
    - Live displays are only used for numbers (time, CPU, RAM, temperature, music title, Steam
      updates).
    - Free pages are shown on phone and tablet exactly in their format, like in the editor preview,
      instead of being stretched.
    - Compact toolbar in the editor.
    - READMEs and the plugin guide are now in English, with German versions and a language switch
      at the top; their feature lists are up to date.
  Bugfixes:
    - Fixed the autostart entry pointing to a file that no longer existed after an update; it now
      follows the running version on every start.
    - Fixed the missing TouchDeck icon in taskbar and start menu; a desktop launcher entry with
      icon is now created in the user folder.
  Translation:
    - Tray menu, notices and startup messages of the desktop app follow the selected language.
    - Server errors, live texts, logs, startup message and dates follow the selected language.
    - The confirmation phrase for advanced mode is also available in English.
    - Tests check that both languages have the same keys and placeholders.
  Android app:
    - App 0.6.0: the app is available in German and English.
    - App 0.6.1: the app language follows the TouchDeck setting, including the connect screen
      (before the first connection: language of the found PC, otherwise the phone's language).
    - App 0.6.2: the app sends the device name from its settings for the editor's status bar.

---------------------------------------------------------------------------------------------------
Version: 0.8.0
Date: 2026-10-14
  Features:
    - Settable orientation for phone and tablet: automatic, portrait or landscape. The Android app
      turns immediately (automatic follows the device's rotation lock); in the browser it applies
      in full screen.
  Changes:
    - Editor and deck moved into the Rust version; the old Electron version was removed.
    - The README of the Rust version is now the main guide (connecting a phone, actions, Twitch,
      Discord, OBS).
    - New setting "language" (German/English); the default is the system language.
  Modding:
    - Advanced plugins with full rights: own programs (Python, Node, …) that TouchDeck starts and
      talks to via JSON lines on stdin/stdout, without an open port.
    - Advanced plugins only run in advanced mode, which needs a double confirmation (warning and
      typing "AT MY OWN RISK"); every plugin must also be confirmed on installation. With the
      mode off they are locked and hidden in the editor.
    - Advanced plugins are also only installed from public GitHub repos or in test mode. The
      checksum of the preview now covers all files.
    - Crashed advanced plugins are restarted at most three times per minute.
    - plugin.json and marketplace entries can state a license; the editor shows it in preview,
      plugin card and marketplace.
    - Texts in plugin.json can be translated ({ "en": …, "de": … }); TouchDeck shows the selected
      language, otherwise English.
    - Example plugins moved to the public touchdeck-plugins repo; the plugin wiki is linked in
      the editor.
    - Installing from a GitHub link without a marketplace entry shows an "at your own risk" notice.
  Android app:
    - App 0.5.0: follows the orientation setting.

---------------------------------------------------------------------------------------------------
Version: 0.7.0
Date: 2026-10-14
  Major Features:
    - Icon packs: create your own image collections, add and remove images, export and import them
      as .tdicons; images are picked with search on the button.
  Modding:
    - Plugin system: extensions in JavaScript, sandboxed in QuickJS with memory and time limits.
      Access only via td.* and only with the permissions declared in plugin.json (network only to
      listed addresses without redirects, values, notifications).
    - Plugins are installed only from public GitHub repos, with a preview of source, version and
      permissions; exactly the checked state is installed.
    - Marketplace tab, test mode for local folders, example plugin and plugin guide (PLUGINS.md).

---------------------------------------------------------------------------------------------------
Version: 0.6.0
Date: 2026-10-14
  Major Features:
    - Initial release: macro remote control for Linux with editor, deck for the phone, values,
      events, ready-made pages, logs and backups.
    - Integrations for Steam, OBS and Twitch.
    - Discord integration via the local Discord app: mute, deafen, join/leave voice channel,
      volume, push-to-talk; events and live displays for mute, deafen and voice channel;
      messages to a channel via webhook.
    - Import pages from another macro app.
    - Android app with automatic search in the local network, pairing by confirmation or QR code
      and several saved PCs.
    - TouchDeck rewritten in Rust with Tauri: server, all actions, events, live values, sliders,
      press-and-hold, backups, pairing and network search. Packages as .deb and AppImage; the
      data stays the same.
  Features:
    - New action editor as a large window: categories and search on the left, actions as tiles in
      the middle, flow on the right; add by click or drag, reorder by drag, "Recently used".
    - The properties column shows actions compactly; a click opens the action editor.
    - Right-click menu on buttons: test, duplicate, copy, cut, paste, transfer look, move or copy
      to other pages; Ctrl+C/X/V.
  Minor Features:
    - Column widths in the editor can be resized by dragging and are remembered (double-click
      resets).
    - "Ready-made pages" in the sidebar can be collapsed.
    - The info page shows the uptime.
  Optimizations:
    - Keyboard, mouse and windows are controlled directly via X11/XTest instead of Python.
```
