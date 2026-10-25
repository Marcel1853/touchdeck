# TouchDeck Changelog

All notable changes to TouchDeck, the macro remote control for Linux, and its Android app.

**English** · [Deutsch](CHANGELOG.de.md)

```text
---------------------------------------------------------------------------------------------------
Version: 0.12.0
Date: 2026-10-25
  Features:
    - AI assistants (MCP): Settings › AI assistants switches on an MCP server. Assistants like Claude
      see pages, buttons and live values, press buttons, read and set values and switch the page on the
      devices. With a second option they may also create, change and delete pages and buttons and run
      any action. Only from the PC itself, with its own key, every use logged; ready-made commands for
      Claude Code and other programs to copy.
    - Plugins can fill the screensaver: td.saver.set({ title, items, color }) shows a card below the
      clock (e.g. research and power from Factorio); can be switched off in the settings.
    - Preview with device choice: phone and tablet sizes, rotate, scale; your own devices appear with
      their real screen size (remembered when they connect).
    - Deck: page selection at the top left as a clear dropdown button; in the app “Change connection”
      (⇄) and “Settings of this device” (⚙) at the top right.

---------------------------------------------------------------------------------------------------
Version: 0.11.0
Date: 2026-10-20
  Major Features:
    - Buttons with several states: button type “States” – each press moves to the next state with
      its own title, emoji, colours and actions. Optionally “state from value”: the state follows a
      value (e.g. “10..50”, “>80”, “on”).
    - Switch as a new button type: flip switch, horizontal or vertical, on/off with own actions like
      a toggle.
    - Actions that return data (plugins): a plugin action can return data (e.g. { players: 7,
      memory: 4210, uptime: 50432 }). TouchDeck shows it automatically as a list with readable units
      (“RAM 4.1 GB”, “Uptime 14 h”) and passes it on to following actions as {Result.players}. New in
      the manifest: “returns”.
    - Android app 0.7.0/0.8.0: home screen widget with up to four buttons, quick settings tile and NFC
      stickers trigger buttons on the PC without opening the deck.
  Features:
    - Confirmation: buttons with “Only trigger on the second press” show “Sure?” on the first press.
    - Dependencies: a button only triggers when another button is on or a value matches (e.g. first
      “Arm”, then “Send”); otherwise it is greyed out with a lock.
    - Hidden buttons: “Hidden at start” and the action “Show / hide button” (also for a few seconds).
    - Trigger “Program in foreground”: option “Only the first time” – fires again only after the
      program was closed and restarted (e.g. show the game page only on start).
    - Action “Set state of a button” (next, previous, number).
    - “Show notification”: on the PC, on the deck or both. On the deck it appears on the device that
      pressed (for events on all of them).
    - Feedback on the button: a short check mark when the actions worked (can be switched off).
    - Deck sleep: after some minutes dim, show a clock or go black; the first tap only wakes it. The
      app also lowers the brightness.
    - Screensaver: large clock on black (digital thin, bold or analogue) with date, own colour or the
      Android system colour; it moves a little every minute against burn-in. Up to four shortcuts
      appear on the first tap (in the app also when the device is moved).
    - PC locked: lock the deck while the PC is locked, and trigger “PC locked / unlocked” (Cinnamon,
      GNOME, KDE, MATE).
    - USB connection: Settings › Phone & security › "Connect Android devices via USB cable" forwards
      the port to devices plugged in with USB debugging ("adb reverse", needs adb on the PC); in the
      app choose "Connect via USB cable". No Wi-Fi needed.
    - Marketplace: button "Reload marketplace" fetches the list fresh from GitHub, so new plugins
      show up right away (no restart).
    - Tray menu: "Restart TouchDeck".
  Bugfixes:
    - “Press shortcut” to a specific program: only looks for the program, no longer the window title,
      and only presses when exactly that window is in front. If the program is not running, you get a
      message instead of keys in the wrong window (e.g. an editor with “Factorio” in its title).
  Performance:
    - Values, toggles and looks are saved in batches instead of writing the whole configuration on
      every change; config.json is compact.
    - Devices only get what changed: small messages instead of the whole state, live values only for
      their own page and only changed ones, graphs only new points. The tablet only redraws changed
      buttons.
    - Plugin, timer and value displays update instantly without querying all other values.
    - “Program in foreground” uses a persistent X11 connection; hotkeys and clipboard wait for events
      instead of checking up to 50 times per second.
    - Audio triggers use “pactl subscribe” instead of asking every 2 seconds; Steam detection only
      reads new processes.
    - A deck in the background (app left, screen off) no longer receives live values; the app pauses
      the page.
  Android app:
    - App 0.6.4: new button "Connect via USB cable".
    - App 0.8.0: “Settings of this device” (connect screen and page menu on the deck): sleep and its
      kind per device, brightness while sleeping and in use, keep screen on, orientation, vibration and
      click sound; moving the device shows the screensaver shortcuts.
    - App 0.8.1: settings in full screen like the deck; gear at the top right of the deck.
    - App 0.7.0: widget, quick settings tile, write NFC stickers (connect screen › Shortcuts), links
      “touchdeck://press?…”; sleep lowers the brightness.

---------------------------------------------------------------------------------------------------
Version: 0.10.0
Date: 2026-10-18
  Features:
    - Designs from plugins and icon packs: Look › Designs shows them as previews, a click takes over
      colours, font, texture, overlay and slider shape. Plugins can require icon packs, which are
      imported on installation.
    - Port and address (IP or name for links and QR codes) can be set under Settings › Phone &
      security.
    - Events get folders, multi-selection and a right-click menu; plugin cards can be collapsed;
      plugins and icon packs have a right-click menu.
    - New slider styles "Knob (like a mixing desk)" with scale, grip ring and pointer, and "Fader +
      knob": one button with a fader for one target and a knob for a second one (e.g. volume and
      microphone).
    - Ready-made pages of plugins may also lie in sub-folders of the plugin (e.g. pages/power/…);
      page folders are kept when exporting and importing profiles.
    - Folders for pages and flows: create your own folders (also empty ones), collapse them, move
      pages and flows into them by dragging or right-click › "Move to folder"; right-click on a
      folder renames it, dissolves it (contents stay) or deletes it with its contents.
    - Several pages or flows at once: Ctrl+click or Shift+click selects several, right-click deletes,
      moves or (pages) exports them together.
    - Values: right-click on a value (rename, move to folder, delete) and on a folder (rename,
      dissolve, delete with values).
    - Right-click on a page in the page list: open, rename, duplicate, make start page, move up/down
      and delete, besides export and import.
    - Graphs with several data series: add more sources (each with name and colour) or let a plugin
      deliver them with "series" – new names become new lines by themselves. New style "stacked
      area"; bars with several series are stacked; a legend shows the names.
    - New button type "Display": does not react to taps (no actions, no press animation) – for live
      values and graphs. With "Transparent" only the content stays; on free pages any size.
    - Graphs: every live display with a number can show its history as a line, area or bars
      (number of values, interval, fixed or automatic range, colour). The history is recorded in
      the background even when the page is not open; bigger buttons make bigger charts. Plugins can
      report real numbers (e.g. 35.2 MW) with "number". Value and label sit below the graph, aligned
      left, centre or right.
    - Marketplace: "📖 Description" shows a plugin's README (README.de.md for German) with headings,
      lists, tables and images, with an install button right there.
    - Plugins can live in a sub-folder of a repo (several plugins in one repo): marketplace entries
      take a "path", and "owner/repo/folder" or a copied GitHub folder link can be entered directly.
    - Plugins can bring ready-made pages: .tdprofile files in the plugin folder appear on the plugin
      card and under "Ready-made pages" and are imported with one click.
    - New button type "Website": shows a web page right in the button (address, zoom, usable on the
      device, reload every n seconds) – e.g. a dashboard or a status page in the home network.
    - New slider style "Fader (like a mixing desk)": dark rail, coloured fill and a square handle.
      Sliders with a rail now take the value along the rail.
    - Values are grouped by folder in collapsible sections, with "New folder" and a "＋" per folder.
    - Icon size per button (30–200 %), texture fit (fill, show completely, stretch, tile) and an
      adjustable colour veil over the texture (0 % = texture unchanged); "Copy look" includes them.
    - "Change look" can also set an image as icon, a texture and an overlay, e.g. from an event; the
      images are picked with previews from your own images and icon packs.
    - New live displays: graphics card temperature and load, disk usage, network speed (↓/↑) and
      how long the PC has been running.
    - New action "Screen (off / brightness)": turns the screen off right away (the mouse wakes it)
      or sets the brightness – real brightness via ddcutil if installed, otherwise dimmed.
    - "Create shortcut page …" (page menu "⋯"): a list like "Undo = ctrl+z | ↩️" becomes a page
      whose buttons send the shortcuts to a chosen program; lists can be loaded from text files.
    - "Press keyboard shortcut" can send the shortcut to a chosen program: it is brought to the
      front first and TouchDeck optionally returns to the previous window afterwards.
    - New actions "Copy to clipboard" (with placeholders, optionally paste right away – also for
      text with characters the keyboard layout does not have) and "Save clipboard in value".
    - "Time reached" knows several times (08:00, 18:30), single weekdays and a day of the month.
    - Timer action "Alarm: count down to a time" (e.g. 07:30, today or tomorrow); "Timer
      finished" fires when it is reached.
    - New trigger "Shortcut pressed on the PC": a key combination anywhere on the PC (X11) starts
      events, also with TouchDeck in the background; shortcuts taken by other programs are logged.
    - New action "Key sequence (recorded)": record keys with their real pauses in the editor, edit
      them as a list (e.g. ctrl+c, 250ms, ctrl+v) and play them back at an adjustable speed.
    - New action "Calculate value": + − × ÷ %, powers, brackets and round/floor/ceil/abs/sqrt/
      min/max with placeholders, e.g. ({Goals} + 1) * 2; decimal commas from values are understood.
    - "HTTP request" can save the response – or one field of a JSON response such as data.0.name –
      in a value; URL, headers and body understand placeholders.
    - "Run command" can save the output in a value.
    - Flows with inputs: a flow can declare inputs with default values (e.g. "Scene"), "Run flow"
      fills them per call (placeholders allowed) and the flow uses them as {Input.Name}.
    - Logic blocks for action lists: "If …" / "Else if …" / "Else", "Repeat (n times)", "While …"
      and "Exit loop", each closed by "End". The editor indents the blocks and adds the "End" by
      itself. The current round is available as {Event.Loop}; endless loops stop after 20000 steps.
    - Conditions can compare placeholders (e.g. {Event.User}) and know "greater or equal", "less or
      equal" and "is not empty".
  Bugfixes:
    - Horizontal faders sit in the middle of the button.
    - Open decks (phone, tablet, browser) reload by themselves when TouchDeck on the PC was restarted
      or updated, instead of running old code until the app is restarted.
    - Security: pages in website buttons can no longer open pop-ups or new windows, navigate the deck
      away or make the program window open the web browser; the program opens the browser at most
      once every 3 seconds.
    - Website buttons: the program window no longer opens the page in the web browser on every
      change, website buttons without a label are no longer hidden on the deck, and the editor warns
      when a site forbids being embedded. Addresses without "https://" (e.g. www.example.com) are
      completed instead of being dropped.
    - Running a flow from the flow editor (or testing single actions in it) fills its inputs with
      their default values, so {Input.Name} is no longer empty.
    - Test mode loads plugins from ZIP files too (path or "Choose ZIP file …"), with or without a
      main folder inside the ZIP.
    - Exporting profiles, icon packs and backups works in the program window: the file is saved to
      the Downloads folder, shown in the file manager and its path is displayed.
    - Notifications, "Type text", "Open website" and other text actions now replace placeholders
      such as {Event.Loop} or {Input.Name} (shell commands deliberately not, for safety).
    - Programs, commands and advanced plugins (e.g. Python) started from the AppImage no longer
      inherit its library paths (PYTHONHOME, LD_LIBRARY_PATH …) and work as when started normally.
  Android app:
    - App 0.6.3: embedded pages of website buttons stay in the button instead of opening the browser;
      links without a matching app no longer crash the app.

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
    - Knobs and slider styles: sliders can now be a slider (area), a slider with rail and handle,
      a knob (ring) or a knob with pointer, with their own colour and thickness. Knobs are turned
      in a circle (bottom left 0 %, bottom right 100 %) and control everything sliders can.
    - Icon pack import: a file with several packs in sub-folders (each with its own description)
      becomes several packs; links without "https://" (e.g. "www.example.com") are completed
      instead of rejecting the pack.
    - Button background opacity (0–100 %) with a "Transparent" shortcut: at 0 % only icon and
      label remain, shadow and gloss disappear; textures keep their image.
    - Icon packs improved a lot: description, version, cover image, license text and a matching
      button colour per pack; name and tags per image (click it in the pack); the image picker
      searches names and tags; .tdicons format version 2 keeps all of it.
    - The icon pack import recognises the content instead of the file extension: ZIP files with
      images in sub-folders and common description files (key/value info.txt, manifest.json with
      icons.json, license files) are taken over with names, tags and license.
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
    - Removed the page import from another program; it was only used for testing.
    - The hint to turn the device sideways now says "device" instead of "phone" and fades out
      after a few seconds; it shows again when opening another wide page.
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
    - Typing in the image search no longer loses focus after each letter (a following Backspace
      could delete the selected button).
    - Fixed the autostart entry pointing to a file that no longer existed after an update; it now
      follows the running version on every start.
    - Fixed the missing TouchDeck icon in taskbar and start menu; a desktop launcher entry with
      icon is now created in the user folder.
  Translation:
    - Tray menu, notices and startup messages of the desktop app follow the selected language.
    - Server errors, live texts, logs, startup message and dates follow the selected language.
    - The confirmation phrase for advanced mode is also available in English.
    - Tests check that both languages have the same keys and placeholders.
  Modding:
    - Plugin interface extended: plugins can react to TouchDeck events (td.on, e.g. page opened,
      muted, program in the foreground, timer finished), show pages on all devices, change and
      restore the look of buttons, control timers and read connected devices and the state of
      OBS, Twitch and Discord. New permissions "events", "deck" and "status".
    - Plugins can bring their own live displays ("lives" in plugin.json) with text, progress bar
      and "on" look; they appear in the editor's live display list.
    - Advanced plugins receive all events and use the same functions via "call" / "reply".
    - Plugin actions and triggers also work for double press and in page logic.
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
