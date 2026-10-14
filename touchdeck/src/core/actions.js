// Alle Aktionstypen: Beschreibung für den Editor und Ausführung auf dem PC.
import * as linux from '../system/linux.js';
import * as steam from '../integrations/steam.js';
import { sendWebhook } from '../integrations/discord.js';
import path from 'node:path';

/** Ersetzt {Name} durch den Wert, {Event.X} durch Daten des auslösenden Events und {Zeit}/{Datum}. */
export function fillVariables(text, store, extra = {}) {
  const now = new Date();
  const builtin = { Zeit: now.toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' }), Datum: now.toLocaleDateString('de-DE'), ...extra };
  return text.replace(/\{([^{}]{1,40})\}/g, (m, name) => (name in store.variables ? String(store.variables[name]) : name in builtin ? String(builtin[name]) : m));
}

const opt = (value, label) => ({ value, label });

export const ACTION_TYPES = [
  {
    type: 'open_url', category: 'Öffnen', label: 'Webseite öffnen', icon: '🌐',
    fields: [{ key: 'url', label: 'Adresse', type: 'text', placeholder: 'https://…' }],
    async run(p) {
      if (!/^[a-z][a-z0-9+.-]*:/i.test(p.url ?? '')) throw new Error('Bitte eine vollständige Adresse mit https:// angeben.');
      await linux.openWithDefault(p.url);
    },
  },
  {
    type: 'open_path', category: 'Öffnen', label: 'Datei oder Ordner öffnen', icon: '📂',
    fields: [{ key: 'path', label: 'Pfad', type: 'text', placeholder: '~/Dokumente' }],
    async run(p) {
      if (!p.path) throw new Error('Kein Pfad angegeben.');
      await linux.openWithDefault(linux.expandHome(p.path));
    },
  },
  {
    type: 'launch_app', category: 'Programme', label: 'Programm starten', icon: '🚀',
    fields: [{ key: 'app', label: 'Programm', type: 'app' }],
    async run(p) {
      if (!p.app) throw new Error('Kein Programm ausgewählt.');
      await linux.launchApp(p.app);
    },
  },
  {
    type: 'run_command', category: 'Programme', label: 'Befehl ausführen', icon: '⌨️',
    fields: [
      { key: 'command', label: 'Befehl', type: 'textarea', placeholder: 'z. B. gnome-screenshot -i' },
      { key: 'wait', label: 'Auf Ende warten und Ausgabe anzeigen', type: 'checkbox' },
    ],
    async run(p) {
      if (!p.command?.trim()) throw new Error('Kein Befehl angegeben.');
      const out = await linux.shell(p.command, { wait: !!p.wait });
      return p.wait && out ? out.slice(0, 500) : undefined;
    },
  },
  {
    type: 'hotkey', category: 'Tastatur & Maus', label: 'Tastenkürzel drücken', icon: '🎹',
    fields: [{ key: 'keys', label: 'Tastenkürzel', type: 'hotkey', placeholder: 'ctrl+shift+s' }],
    async run(p) {
      if (!p.keys) throw new Error('Kein Tastenkürzel angegeben.');
      await linux.sendHotkey(p.keys);
    },
  },
  {
    type: 'type_text', category: 'Tastatur & Maus', label: 'Text tippen', icon: '✍️',
    fields: [{ key: 'text', label: 'Text', type: 'textarea' }],
    async run(p) {
      if (p.text) await linux.typeText(p.text);
    },
  },
  {
    type: 'media', category: 'Medien & Audio', label: 'Mediensteuerung', icon: '⏯️',
    fields: [{
      key: 'op', label: 'Aktion', type: 'select', default: 'play-pause',
      options: [opt('play-pause', 'Abspielen / Pause'), opt('next', 'Nächster Titel'), opt('previous', 'Vorheriger Titel'), opt('stop', 'Stopp')],
    }],
    async run(p) {
      const player = await linux.media(p.op || 'play-pause');
      return `Player: ${player}`;
    },
  },
  {
    type: 'volume', category: 'Medien & Audio', label: 'Lautstärke', icon: '🔊',
    fields: [
      { key: 'op', label: 'Aktion', type: 'select', default: 'up', options: [opt('up', 'Lauter'), opt('down', 'Leiser'), opt('set', 'Auf Wert setzen'), opt('mute', 'Stumm an/aus')] },
      { key: 'step', label: 'Schritt in %', type: 'number', default: 5, min: 1, max: 50, showIf: { op: ['up', 'down'] } },
      { key: 'value', label: 'Lautstärke in %', type: 'number', default: 50, min: 0, max: 100, showIf: { op: ['set'] } },
    ],
    async run(p) {
      const v = await linux.volume(p.op || 'up', p);
      return v.muted ? 'Stumm' : `Lautstärke ${v.volume} %`;
    },
  },
  {
    type: 'mic_mute', category: 'Medien & Audio', label: 'Mikrofon stumm', icon: '🎙️',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('mute', 'Stumm'), opt('unmute', 'Laut')] }],
    async run(p) {
      const v = await linux.mic(p.op || 'toggle');
      return v.muted ? 'Mikrofon stumm' : 'Mikrofon an';
    },
  },
  {
    type: 'go_page', category: 'Navigation', label: 'Seite wechseln', icon: '📄',
    fields: [{ key: 'page', label: 'Zielseite', type: 'page' }],
    async run(p, ctx) {
      if (p.page !== '__back' && !ctx.store.page(p.page)) throw new Error('Die Zielseite gibt es nicht mehr.');
      ctx.navigate = p.page;
    },
  },
  {
    type: 'delay', category: 'Ablauf', label: 'Warten', icon: '⏱️',
    fields: [{ key: 'ms', label: 'Millisekunden', type: 'number', default: 500, min: 0, max: 60000 }],
    async run(p) {
      await new Promise((r) => setTimeout(r, Math.max(0, Math.min(60000, Number(p.ms) || 0))));
    },
  },
  {
    type: 'notify', category: 'Ablauf', label: 'Benachrichtigung zeigen', icon: '💬',
    fields: [{ key: 'title', label: 'Titel', type: 'text' }, { key: 'body', label: 'Text', type: 'textarea' }],
    async run(p) {
      await linux.notify(p.title, p.body);
    },
  },
  {
    type: 'http', category: 'Netzwerk', label: 'HTTP-Anfrage', icon: '📡',
    fields: [
      { key: 'method', label: 'Methode', type: 'select', default: 'GET', options: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((m) => opt(m, m)) },
      { key: 'url', label: 'Adresse', type: 'text', placeholder: 'http://…' },
      { key: 'headers', label: 'Header (eine Zeile pro Header: Name: Wert)', type: 'textarea' },
      { key: 'body', label: 'Inhalt', type: 'textarea', showIf: { method: ['POST', 'PUT', 'PATCH', 'DELETE'] } },
    ],
    async run(p) {
      if (!/^https?:\/\//i.test(p.url ?? '')) throw new Error('Adresse muss mit http:// oder https:// beginnen.');
      const headers = {};
      for (const line of (p.headers ?? '').split('\n')) {
        const i = line.indexOf(':');
        if (i > 0) headers[line.slice(0, i).trim()] = line.slice(i + 1).trim();
      }
      const method = p.method || 'GET';
      const res = await fetch(p.url, { method, headers, body: method === 'GET' ? undefined : p.body || undefined, signal: AbortSignal.timeout(15000) });
      return `Antwort ${res.status} ${res.statusText}`;
    },
  },
  // ---------- System ----------
  {
    type: 'audio_output', category: 'Medien & Audio', label: 'Audioausgang wechseln', icon: '🎧',
    fields: [{ key: 'target', label: 'Ausgang', type: 'dynamic', source: 'audio-sinks', default: 'next' }],
    async run(p) {
      return `Ausgang: ${await linux.setDefaultAudio('sinks', p.target || 'next')}`;
    },
  },
  {
    type: 'audio_input', category: 'Medien & Audio', label: 'Mikrofon wechseln', icon: '🎙️',
    fields: [{ key: 'target', label: 'Eingang', type: 'dynamic', source: 'audio-sources', default: 'next' }],
    async run(p) {
      return `Eingang: ${await linux.setDefaultAudio('sources', p.target || 'next')}`;
    },
  },
  {
    type: 'screenshot', category: 'System', label: 'Bildschirmfoto', icon: '📸',
    fields: [{ key: 'mode', label: 'Bereich', type: 'select', default: 'dialog', options: [opt('dialog', 'Dialog öffnen'), opt('full', 'Ganzer Bildschirm'), opt('window', 'Aktives Fenster'), opt('area', 'Bereich auswählen')] }],
    async run(p) {
      await linux.screenshot(p.mode);
    },
  },
  {
    type: 'lock_screen', category: 'System', label: 'Bildschirm sperren', icon: '🔒',
    fields: [],
    async run() {
      await linux.lockScreen();
    },
  },
  {
    type: 'power', category: 'System', label: 'Energie', icon: '⏻',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'suspend', options: [opt('suspend', 'Bereitschaft'), opt('logout', 'Abmelden'), opt('reboot', 'Neu starten'), opt('poweroff', 'Herunterfahren')] }],
    async run(p) {
      await linux.power(p.op || 'suspend');
    },
  },
  {
    type: 'night_light', category: 'System', label: 'Nachtlicht', icon: '🌙',
    fields: [{ key: 'mode', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('on', 'An'), opt('off', 'Aus')] }],
    async run(p) {
      return (await linux.nightLight(p.mode || 'toggle')) ? 'Nachtlicht an' : 'Nachtlicht aus';
    },
  },
  {
    type: 'dark_mode', category: 'System', label: 'Hell / Dunkel', icon: '🌗',
    fields: [{ key: 'mode', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('on', 'Dunkel'), opt('off', 'Hell')] }],
    async run(p) {
      return (await linux.darkMode(p.mode || 'toggle')) ? 'Dunkles Design' : 'Helles Design';
    },
  },
  {
    type: 'wake_on_lan', category: 'Netzwerk', label: 'Rechner aufwecken (Wake-on-LAN)', icon: '⏰',
    fields: [{ key: 'mac', label: 'MAC-Adresse', type: 'text', placeholder: '00:11:22:33:44:55' }],
    async run(p) {
      await linux.wakeOnLan(p.mac);
      return 'Weckpaket gesendet';
    },
  },

  // ---------- Steam ----------
  {
    type: 'steam_game', category: 'Steam', label: 'Spiel starten', icon: '🎮',
    fields: [{ key: 'game', label: 'Spiel', type: 'dynamic', source: 'steam-games' }],
    async run(p) {
      await steam.runGame(p.game);
    },
  },
  {
    type: 'steam_open', category: 'Steam', label: 'Steam öffnen', icon: '🕹️',
    fields: [{
      key: 'view', label: 'Bereich', type: 'select', default: 'library',
      options: [opt('library', 'Bibliothek'), opt('downloads', 'Downloads & Updates'), opt('bigpicture', 'Big Picture'), opt('friends', 'Freunde'), opt('store', 'Shop'), opt('screenshots', 'Bildschirmfotos')],
    }],
    async run(p) {
      await steam.openSteam(p.view);
    },
  },

  // ---------- OBS ----------
  {
    type: 'obs_scene', category: 'OBS Studio', label: 'Szene wechseln', icon: '🎬',
    fields: [{ key: 'scene', label: 'Szene', type: 'dynamic', source: 'obs-scenes' }],
    async run(p, ctx) {
      if (!p.scene) throw new Error('Keine Szene ausgewählt.');
      await ctx.obs.call('SetCurrentProgramScene', { sceneName: p.scene });
    },
  },
  {
    type: 'obs_stream', category: 'OBS Studio', label: 'Stream', icon: '📡',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Starten / Stoppen'), opt('start', 'Starten'), opt('stop', 'Stoppen')] }],
    async run(p, ctx) {
      await ctx.obs.call({ toggle: 'ToggleStream', start: 'StartStream', stop: 'StopStream' }[p.op || 'toggle']);
    },
  },
  {
    type: 'obs_record', category: 'OBS Studio', label: 'Aufnahme', icon: '⏺️',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Starten / Stoppen'), opt('start', 'Starten'), opt('stop', 'Stoppen'), opt('pause', 'Pause an/aus')] }],
    async run(p, ctx) {
      await ctx.obs.call({ toggle: 'ToggleRecord', start: 'StartRecord', stop: 'StopRecord', pause: 'ToggleRecordPause' }[p.op || 'toggle']);
    },
  },
  {
    type: 'obs_replay', category: 'OBS Studio', label: 'Wiederholungspuffer', icon: '⏪',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'save', options: [opt('save', 'Clip speichern'), opt('toggle', 'Puffer an/aus')] }],
    async run(p, ctx) {
      await ctx.obs.call(p.op === 'toggle' ? 'ToggleReplayBuffer' : 'SaveReplayBuffer');
      return p.op === 'toggle' ? undefined : 'Clip gespeichert';
    },
  },
  {
    type: 'obs_mute', category: 'OBS Studio', label: 'Audioquelle stumm', icon: '🔈',
    fields: [
      { key: 'input', label: 'Audioquelle', type: 'dynamic', source: 'obs-inputs' },
      { key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('mute', 'Stumm'), opt('unmute', 'Laut')] },
    ],
    async run(p, ctx) {
      if (!p.input) throw new Error('Keine Audioquelle ausgewählt.');
      if (p.op === 'mute' || p.op === 'unmute') await ctx.obs.call('SetInputMute', { inputName: p.input, inputMuted: p.op === 'mute' });
      else await ctx.obs.call('ToggleInputMute', { inputName: p.input });
    },
  },
  {
    type: 'obs_source', category: 'OBS Studio', label: 'Quelle ein-/ausblenden', icon: '👁️',
    fields: [
      { key: 'item', label: 'Quelle', type: 'dynamic', source: 'obs-scene-items' },
      { key: 'mode', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('show', 'Einblenden'), opt('hide', 'Ausblenden')] },
    ],
    async run(p, ctx) {
      return (await ctx.obs.setSourceVisible(p.item, p.mode || 'toggle')) ? 'Eingeblendet' : 'Ausgeblendet';
    },
  },
  {
    type: 'obs_misc', category: 'OBS Studio', label: 'Studio-Modus / Virtuelle Kamera', icon: '🎛️',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'virtualcam', options: [opt('virtualcam', 'Virtuelle Kamera an/aus'), opt('studio', 'Studio-Modus an/aus')] }],
    async run(p, ctx) {
      if (p.op === 'studio') {
        const { studioModeEnabled } = await ctx.obs.call('GetStudioModeEnabled');
        await ctx.obs.call('SetStudioModeEnabled', { studioModeEnabled: !studioModeEnabled });
      } else {
        await ctx.obs.call('ToggleVirtualCam');
      }
    },
  },
  {
    type: 'obs_preview', category: 'OBS Studio', label: 'Vorschauszene setzen (Studio-Modus)', icon: '🖼️',
    fields: [{ key: 'scene', label: 'Szene', type: 'dynamic', source: 'obs-scenes' }],
    async run(p, ctx) {
      if (!p.scene) throw new Error('Keine Szene ausgewählt.');
      await ctx.obs.call('SetCurrentPreviewScene', { sceneName: p.scene });
    },
  },
  {
    type: 'obs_transition', category: 'OBS Studio', label: 'Übergang', icon: '🔀',
    fields: [
      { key: 'op', label: 'Aktion', type: 'select', default: 'take', options: [opt('take', 'Vorschau auf Sendung schalten (Studio-Modus)'), opt('set', 'Übergang auswählen')] },
      { key: 'transition', label: 'Übergang', type: 'dynamic', source: 'obs-transitions', showIf: { op: ['set'] } },
      { key: 'duration', label: 'Dauer in ms (0 = unverändert)', type: 'number', default: 0, min: 0, max: 20000, showIf: { op: ['set'] } },
    ],
    async run(p, ctx) {
      if (p.op === 'set') {
        if (!p.transition) throw new Error('Kein Übergang ausgewählt.');
        await ctx.obs.call('SetCurrentSceneTransition', { transitionName: p.transition });
        if (Number(p.duration) > 0) await ctx.obs.call('SetCurrentSceneTransitionDuration', { transitionDuration: Number(p.duration) });
      } else {
        await ctx.obs.call('TriggerStudioModeTransition');
      }
    },
  },
  {
    type: 'obs_volume', category: 'OBS Studio', label: 'Lautstärke einer Audioquelle', icon: '🔉',
    fields: [
      { key: 'input', label: 'Audioquelle', type: 'dynamic', source: 'obs-inputs' },
      { key: 'op', label: 'Aktion', type: 'select', default: 'up', options: [opt('up', 'Lauter'), opt('down', 'Leiser'), opt('set', 'Auf Wert setzen')] },
      { key: 'step', label: 'Schritt in dB', type: 'number', default: 3, min: 1, max: 30, showIf: { op: ['up', 'down'] } },
      { key: 'value', label: 'Lautstärke in %', type: 'number', default: 80, min: 0, max: 100, showIf: { op: ['set'] } },
    ],
    async run(p, ctx) {
      if (!p.input) throw new Error('Keine Audioquelle ausgewählt.');
      if (p.op === 'set') {
        await ctx.obs.call('SetInputVolume', { inputName: p.input, inputVolumeMul: (Math.max(0, Math.min(100, Number(p.value))) / 100) ** 2 });
        return `${p.value} %`;
      }
      const { inputVolumeDb } = await ctx.obs.call('GetInputVolume', { inputName: p.input });
      const step = (Number(p.step) || 3) * (p.op === 'down' ? -1 : 1);
      // OBS erlaubt -100 bis +26 dB; -100 gilt als stumm.
      const db = Math.max(-100, Math.min(26, (Number.isFinite(inputVolumeDb) ? Math.max(inputVolumeDb, -60) : -60) + step));
      await ctx.obs.call('SetInputVolume', { inputName: p.input, inputVolumeDb: db });
      return `${db.toFixed(1)} dB`;
    },
  },
  {
    type: 'obs_media', category: 'OBS Studio', label: 'Medienquelle steuern', icon: '🎞️',
    fields: [
      { key: 'input', label: 'Medienquelle', type: 'dynamic', source: 'obs-media-inputs' },
      { key: 'op', label: 'Aktion', type: 'select', default: 'playpause', options: [opt('playpause', 'Abspielen / Pause'), opt('restart', 'Von vorn'), opt('stop', 'Stopp'), opt('next', 'Nächstes'), opt('previous', 'Vorheriges')] },
    ],
    async run(p, ctx) {
      if (!p.input) throw new Error('Keine Medienquelle ausgewählt.');
      let action = { restart: 'RESTART', stop: 'STOP', next: 'NEXT', previous: 'PREVIOUS' }[p.op];
      if (!action) {
        const { mediaState } = await ctx.obs.call('GetMediaInputStatus', { inputName: p.input });
        action = mediaState === 'OBS_MEDIA_STATE_PLAYING' ? 'PAUSE' : 'PLAY';
      }
      await ctx.obs.call('TriggerMediaInputAction', { inputName: p.input, mediaAction: `OBS_WEBSOCKET_MEDIA_INPUT_ACTION_${action}` });
    },
  },
  {
    type: 'obs_filter', category: 'OBS Studio', label: 'Filter ein-/ausschalten', icon: '✨',
    fields: [
      { key: 'filter', label: 'Filter', type: 'dynamic', source: 'obs-filters' },
      { key: 'mode', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('on', 'Einschalten'), opt('off', 'Ausschalten')] },
    ],
    async run(p, ctx) {
      return (await ctx.obs.setFilter(p.filter, p.mode || 'toggle')) ? 'Filter an' : 'Filter aus';
    },
  },
  {
    type: 'obs_text', category: 'OBS Studio', label: 'Text einer Textquelle setzen', icon: '🔤',
    fields: [
      { key: 'input', label: 'Textquelle', type: 'dynamic', source: 'obs-text-inputs' },
      { key: 'text', label: 'Text ({Variable} wird ersetzt)', type: 'textarea' },
    ],
    async run(p, ctx) {
      if (!p.input) throw new Error('Keine Textquelle ausgewählt.');
      await ctx.obs.call('SetInputSettings', { inputName: p.input, inputSettings: { text: fillVariables(p.text ?? '', ctx.store, ctx.eventData) } });
    },
  },
  {
    type: 'obs_browser', category: 'OBS Studio', label: 'Browserquelle', icon: '🌐',
    fields: [
      { key: 'input', label: 'Browserquelle', type: 'dynamic', source: 'obs-browser-inputs' },
      { key: 'op', label: 'Aktion', type: 'select', default: 'reload', options: [opt('reload', 'Neu laden'), opt('url', 'Adresse ändern')] },
      { key: 'url', label: 'Adresse', type: 'text', placeholder: 'https://…', showIf: { op: ['url'] } },
    ],
    async run(p, ctx) {
      if (!p.input) throw new Error('Keine Browserquelle ausgewählt.');
      if (p.op === 'url') await ctx.obs.call('SetInputSettings', { inputName: p.input, inputSettings: { url: p.url ?? '' } });
      else await ctx.obs.call('PressInputPropertiesButton', { inputName: p.input, propertyName: 'refreshnocache' });
    },
  },
  {
    type: 'obs_image', category: 'OBS Studio', label: 'Bild einer Bildquelle ändern', icon: '🖼️',
    fields: [
      { key: 'input', label: 'Bildquelle', type: 'dynamic', source: 'obs-image-inputs' },
      { key: 'file', label: 'Bilddatei', type: 'text', placeholder: '~/Bilder/…png' },
    ],
    async run(p, ctx) {
      if (!p.input || !p.file) throw new Error('Bildquelle und Datei angeben.');
      await ctx.obs.call('SetInputSettings', { inputName: p.input, inputSettings: { file: linux.expandHome(p.file) } });
    },
  },
  {
    type: 'obs_profile', category: 'OBS Studio', label: 'Profil / Szenensammlung wechseln', icon: '🗂️',
    fields: [
      { key: 'kind', label: 'Was wechseln?', type: 'select', default: 'profile', options: [opt('profile', 'Profil'), opt('collection', 'Szenensammlung')] },
      { key: 'profile', label: 'Profil', type: 'dynamic', source: 'obs-profiles', showIf: { kind: ['profile'] } },
      { key: 'collection', label: 'Szenensammlung', type: 'dynamic', source: 'obs-collections', showIf: { kind: ['collection'] } },
    ],
    async run(p, ctx) {
      if (p.kind === 'collection') await ctx.obs.call('SetCurrentSceneCollection', { sceneCollectionName: p.collection });
      else await ctx.obs.call('SetCurrentProfile', { profileName: p.profile });
    },
  },
  {
    type: 'obs_hotkey', category: 'OBS Studio', label: 'OBS-Tastenkürzel auslösen', icon: '⌨️',
    fields: [{ key: 'hotkey', label: 'Funktion', type: 'dynamic', source: 'obs-hotkeys' }],
    async run(p, ctx) {
      if (!p.hotkey) throw new Error('Keine Funktion ausgewählt.');
      await ctx.obs.call('TriggerHotkeyByName', { hotkeyName: p.hotkey });
    },
  },
  {
    type: 'obs_screenshot', category: 'OBS Studio', label: 'Bildschirmfoto einer Szene/Quelle', icon: '📷',
    fields: [
      { key: 'source', label: 'Szene oder Quelle', type: 'dynamic', source: 'obs-sources' },
      { key: 'folder', label: 'Ordner', type: 'text', default: '~/Bilder', placeholder: '~/Bilder' },
    ],
    async run(p, ctx) {
      if (!p.source) throw new Error('Keine Quelle ausgewählt.');
      const d = new Date();
      const two = (n) => String(n).padStart(2, '0');
      const stamp = `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}-${two(d.getMinutes())}-${two(d.getSeconds())}`;
      const file = path.join(linux.expandHome(p.folder || '~/Bilder'), `OBS ${p.source} ${stamp}.png`);
      await ctx.obs.call('SaveSourceScreenshot', { sourceName: p.source, imageFormat: 'png', imageFilePath: file });
      return `Gespeichert: ${file}`;
    },
  },
  {
    type: 'obs_raw', category: 'OBS Studio', label: 'Eigene OBS-Anfrage (für Fortgeschrittene)', icon: '🧪',
    fields: [
      { key: 'request', label: 'Anfrage (requestType)', type: 'text', placeholder: 'z. B. SetInputMute' },
      { key: 'data', label: 'Daten als JSON', type: 'textarea', placeholder: '{"inputName": "Mikrofon-/AUX-Audio", "inputMuted": true}' },
    ],
    async run(p, ctx) {
      if (!p.request) throw new Error('Keine Anfrage angegeben.');
      let data = {};
      if (p.data?.trim()) {
        try {
          data = JSON.parse(p.data);
        } catch {
          throw new Error('Die Daten sind kein gültiges JSON.');
        }
      }
      const r = await ctx.obs.call(p.request, data);
      return Object.keys(r).length ? JSON.stringify(r).slice(0, 300) : undefined;
    },
  },

  // ---------- Maus & Fenster ----------
  {
    type: 'mouse_click', category: 'Tastatur & Maus', label: 'Mausklick', icon: '🖱️',
    fields: [{ key: 'button', label: 'Taste', type: 'select', default: 'left', options: [opt('left', 'Links'), opt('double', 'Doppelklick links'), opt('right', 'Rechts'), opt('middle', 'Mitte'), opt('scrollup', 'Rad hoch'), opt('scrolldown', 'Rad runter')] }],
    async run(p) {
      await linux.mouseClick(p.button || 'left');
    },
  },
  {
    type: 'mouse_move', category: 'Tastatur & Maus', label: 'Maus bewegen', icon: '↗️',
    fields: [
      { key: 'mode', label: 'Art', type: 'select', default: 'abs', options: [opt('abs', 'An Bildschirmposition'), opt('rel', 'Um Abstand verschieben')] },
      { key: 'x', label: 'X', type: 'number', default: 0 },
      { key: 'y', label: 'Y', type: 'number', default: 0 },
    ],
    async run(p) {
      await linux.mouseMove(Number(p.x) || 0, Number(p.y) || 0, p.mode === 'rel');
    },
  },
  {
    type: 'key_hold', category: 'Tastatur & Maus', label: 'Taste gedrückt halten', icon: '⏳',
    fields: [
      { key: 'keys', label: 'Taste(n)', type: 'hotkey', placeholder: 'w' },
      { key: 'ms', label: 'Dauer in ms', type: 'number', default: 1000, min: 50, max: 60000 },
    ],
    async run(p) {
      if (!p.keys) throw new Error('Keine Taste angegeben.');
      await linux.holdKeys(p.keys, Number(p.ms) || 1000);
    },
  },
  {
    type: 'window_activate', category: 'Programme', label: 'Fenster nach vorn holen', icon: '🪟',
    fields: [{ key: 'window', label: 'Programm (Fensterklasse oder Teil des Titels)', type: 'dynamic', source: 'windows' }],
    async run(p) {
      if (!p.window) throw new Error('Kein Fenster ausgewählt.');
      return `Aktiviert: ${await linux.activateWindow(p.window)}`;
    },
  },
  {
    type: 'kill_app', category: 'Programme', label: 'Programm beenden', icon: '⛔',
    fields: [
      { key: 'name', label: 'Prozessname', type: 'text', placeholder: 'z. B. firefox' },
      { key: 'force', label: 'Sofort erzwingen (ohne Speichern)', type: 'checkbox' },
    ],
    async run(p) {
      await linux.killProcess(p.name, !!p.force);
    },
  },

  // ---------- Soundboard & Dateien ----------
  {
    type: 'sound_play', category: 'Medien & Audio', label: 'Sound abspielen', icon: '🔔',
    fields: [
      { key: 'file', label: 'Audiodatei (WAV, OGG, FLAC, MP3)', type: 'text', placeholder: '~/Musik/sound.mp3' },
      { key: 'volume', label: 'Lautstärke in %', type: 'number', default: 100, min: 0, max: 100 },
      { key: 'exclusive', label: 'Andere Sounds vorher stoppen', type: 'checkbox' },
    ],
    async run(p) {
      linux.playSound(p.file, p.volume ?? 100, { exclusive: !!p.exclusive });
    },
  },
  {
    type: 'sound_stop', category: 'Medien & Audio', label: 'Alle Sounds stoppen', icon: '🔕',
    fields: [],
    async run() {
      linux.stopSounds();
    },
  },
  {
    type: 'file_write', category: 'Dateien', label: 'In Datei schreiben', icon: '📝',
    fields: [
      { key: 'path', label: 'Datei', type: 'text', placeholder: '~/notizen.txt' },
      { key: 'mode', label: 'Art', type: 'select', default: 'append', options: [opt('append', 'Als neue Zeile anhängen'), opt('write', 'Datei überschreiben')] },
      { key: 'text', label: 'Text ({Variable} und {Zeit} werden ersetzt)', type: 'textarea' },
    ],
    async run(p, ctx) {
      linux.writeFile(p.path, fillVariables(p.text ?? '', ctx.store, ctx.eventData), p.mode);
    },
  },

  // ---------- Logik & Variablen ----------
  {
    type: 'var_set', category: 'Logik & Variablen', label: 'Variable setzen', icon: '🏷️',
    fields: [
      { key: 'name', label: 'Variable', type: 'dynamic', source: 'variables', allowNew: true },
      { key: 'value', label: 'Wert ({Variable} und {Zeit} werden ersetzt)', type: 'text' },
    ],
    async run(p, ctx) {
      ctx.store.setVariable(p.name, fillVariables(p.value ?? '', ctx.store, ctx.eventData));
    },
  },
  {
    type: 'var_change', category: 'Logik & Variablen', label: 'Zähler ändern', icon: '🔢',
    fields: [
      { key: 'name', label: 'Variable', type: 'dynamic', source: 'variables', allowNew: true },
      { key: 'op', label: 'Aktion', type: 'select', default: 'add', options: [opt('add', 'Erhöhen'), opt('sub', 'Verringern'), opt('reset', 'Auf Startwert setzen')] },
      { key: 'amount', label: 'Um / Startwert', type: 'number', default: 1 },
    ],
    async run(p, ctx) {
      const cur = Number(ctx.store.variables[p.name]) || 0;
      const n = Number(p.amount) || 0;
      const next = p.op === 'reset' ? n : p.op === 'sub' ? cur - n : cur + n;
      ctx.store.setVariable(p.name, String(next));
      return `${p.name} = ${next}`;
    },
  },
  {
    type: 'var_toggle', category: 'Logik & Variablen', label: 'Variable an/aus umschalten', icon: '🔁',
    fields: [{ key: 'name', label: 'Variable', type: 'dynamic', source: 'variables', allowNew: true }],
    async run(p, ctx) {
      const on = !['an', 'true', '1', 'ja', 'on'].includes(String(ctx.store.variables[p.name] ?? '').toLowerCase());
      ctx.store.setVariable(p.name, on ? 'an' : 'aus');
      return `${p.name}: ${on ? 'an' : 'aus'}`;
    },
  },
  {
    type: 'condition', category: 'Logik & Variablen', label: 'Bedingung: nur weiter, wenn …', icon: '🚦',
    fields: [
      { key: 'name', label: 'Variable', type: 'dynamic', source: 'variables', allowNew: true },
      { key: 'op', label: 'Vergleich', type: 'select', default: 'eq', options: [opt('eq', 'ist gleich'), opt('ne', 'ist nicht gleich'), opt('gt', 'ist größer als'), opt('lt', 'ist kleiner als'), opt('contains', 'enthält'), opt('empty', 'ist leer')] },
      { key: 'value', label: 'Wert', type: 'text', showIf: { op: ['eq', 'ne', 'gt', 'lt', 'contains'] } },
    ],
    async run(p, ctx) {
      const v = String(ctx.store.variables[p.name] ?? '');
      const w = String(p.value ?? '');
      const ok = {
        eq: () => v === w, ne: () => v !== w, gt: () => Number(v) > Number(w), lt: () => Number(v) < Number(w),
        contains: () => v.includes(w), empty: () => v === '',
      }[p.op || 'eq']();
      // Nicht erfüllt: die restlichen Aktionen dieses Buttons überspringen.
      if (!ok) ctx.stop = true;
    },
  },

  // ---------- Twitch ----------
  {
    type: 'twitch_chat', category: 'Twitch', label: 'Chat-Nachricht senden', icon: '💬',
    fields: [{ key: 'message', label: 'Nachricht ({Wert} und {Event.X} möglich)', type: 'textarea' }],
    async run(p, ctx) {
      const t = ctx.twitch;
      const r = await t.api('POST', '/chat/messages', { broadcaster_id: t.me, sender_id: t.me, message: fillVariables(p.message ?? '', ctx.store, ctx.eventData).slice(0, 500) });
      if (r.data?.[0]?.is_sent === false) throw new Error(r.data[0].drop_reason?.message ?? 'Nachricht wurde nicht gesendet.');
    },
  },
  {
    type: 'twitch_announcement', category: 'Twitch', label: 'Ankündigung im Chat', icon: '📣',
    fields: [
      { key: 'message', label: 'Text', type: 'textarea' },
      { key: 'color', label: 'Farbe', type: 'select', default: 'primary', options: [opt('primary', 'Kanalfarbe'), opt('blue', 'Blau'), opt('green', 'Grün'), opt('orange', 'Orange'), opt('purple', 'Lila')] },
    ],
    async run(p, ctx) {
      const t = ctx.twitch;
      await t.api('POST', `/chat/announcements?broadcaster_id=${t.me}&moderator_id=${t.me}`, { message: fillVariables(p.message ?? '', ctx.store, ctx.eventData).slice(0, 500), color: p.color || 'primary' });
    },
  },
  {
    type: 'twitch_clip', category: 'Twitch', label: 'Clip erstellen', icon: '🎞️',
    fields: [],
    async run(p, ctx) {
      const t = ctx.twitch;
      const r = await t.api('POST', `/clips?broadcaster_id=${t.me}`);
      const id = r.data?.[0]?.id;
      if (id) ctx.store.setVariable('Twitch.Clip', `https://clips.twitch.tv/${id}`);
      return id ? 'Clip erstellt – Link im Wert „Twitch.Clip“' : undefined;
    },
  },
  {
    type: 'twitch_marker', category: 'Twitch', label: 'Stream-Marker setzen', icon: '📍',
    fields: [{ key: 'description', label: 'Beschreibung (optional)', type: 'text' }],
    async run(p, ctx) {
      const t = ctx.twitch;
      await t.api('POST', '/streams/markers', { user_id: t.me, description: fillVariables(p.description ?? '', ctx.store, ctx.eventData).slice(0, 140) });
      return 'Marker gesetzt';
    },
  },
  {
    type: 'twitch_channel', category: 'Twitch', label: 'Titel / Kategorie ändern', icon: '🏷️',
    fields: [
      { key: 'title', label: 'Neuer Titel (leer = unverändert)', type: 'text' },
      { key: 'game', label: 'Neue Kategorie/Spiel (leer = unverändert)', type: 'text', placeholder: 'z. B. RimWorld' },
    ],
    async run(p, ctx) {
      const t = ctx.twitch;
      const body = {};
      if (p.title) body.title = fillVariables(p.title, ctx.store, ctx.eventData).slice(0, 140);
      if (p.game) {
        const g = await t.api('GET', `/games?name=${encodeURIComponent(p.game)}`);
        if (!g.data.length) throw new Error(`Kategorie „${p.game}“ nicht gefunden (genauen Namen wie auf Twitch angeben).`);
        body.game_id = g.data[0].id;
      }
      if (!Object.keys(body).length) throw new Error('Weder Titel noch Kategorie angegeben.');
      await t.api('PATCH', `/channels?broadcaster_id=${t.me}`, body);
    },
  },
  {
    type: 'twitch_ad', category: 'Twitch', label: 'Werbung starten', icon: '📺',
    fields: [{ key: 'length', label: 'Länge', type: 'select', default: '60', options: ['30', '60', '90', '120', '150', '180'].map((s) => opt(s, `${s} Sekunden`)) }],
    async run(p, ctx) {
      const t = ctx.twitch;
      const r = await t.api('POST', '/channels/commercial', { broadcaster_id: t.me, length: Number(p.length) || 60 });
      return r.data?.[0]?.message || 'Werbung gestartet';
    },
  },
  {
    type: 'twitch_chat_mode', category: 'Twitch', label: 'Chat-Modus umschalten', icon: '🛡️',
    fields: [
      { key: 'mode', label: 'Modus', type: 'select', default: 'emote_mode', options: [opt('emote_mode', 'Nur Emotes'), opt('follower_mode', 'Nur Follower'), opt('subscriber_mode', 'Nur Abonnenten'), opt('slow_mode', 'Langsamer Modus'), opt('unique_chat_mode', 'Einzigartiger Chat')] },
      { key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('on', 'An'), opt('off', 'Aus')] },
    ],
    async run(p, ctx) {
      const t = ctx.twitch;
      const q = `broadcaster_id=${t.me}&moderator_id=${t.me}`;
      const mode = p.mode || 'emote_mode';
      let on = p.op === 'on';
      if (!p.op || p.op === 'toggle') on = !(await t.api('GET', `/chat/settings?${q}`)).data[0][mode];
      await t.api('PATCH', `/chat/settings?${q}`, { [mode]: on });
      return on ? 'Modus an' : 'Modus aus';
    },
  },
  {
    type: 'twitch_shoutout', category: 'Twitch', label: 'Shoutout', icon: '📢',
    fields: [{ key: 'user', label: 'Kanal (Twitch-Name, {Event.Nutzer} möglich)', type: 'text' }],
    async run(p, ctx) {
      const t = ctx.twitch;
      const to = await t.userId(fillVariables(p.user ?? '', ctx.store, ctx.eventData));
      await t.api('POST', `/chat/shoutouts?from_broadcaster_id=${t.me}&to_broadcaster_id=${to}&moderator_id=${t.me}`);
    },
  },
  {
    type: 'twitch_raid', category: 'Twitch', label: 'Raid starten / abbrechen', icon: '🚀',
    fields: [
      { key: 'op', label: 'Aktion', type: 'select', default: 'start', options: [opt('start', 'Raid starten'), opt('cancel', 'Raid abbrechen')] },
      { key: 'user', label: 'Ziel-Kanal (Twitch-Name)', type: 'text', showIf: { op: ['start'] } },
    ],
    async run(p, ctx) {
      const t = ctx.twitch;
      if (p.op === 'cancel') return void await t.api('DELETE', `/raids?broadcaster_id=${t.me}`);
      const to = await t.userId(fillVariables(p.user ?? '', ctx.store, ctx.eventData));
      await t.api('POST', `/raids?from_broadcaster_id=${t.me}&to_broadcaster_id=${to}`);
      return 'Raid läuft – in 90 s geht es los';
    },
  },
  {
    type: 'twitch_reward', category: 'Twitch', label: 'Kanalpunkte-Belohnung an/aus', icon: '💎',
    fields: [
      { key: 'reward', label: 'Belohnung (nur von TouchDeck angelegte lassen sich ändern)', type: 'dynamic', source: 'twitch-rewards' },
      { key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Aktivieren/Deaktivieren'), opt('pause', 'Pausieren/Fortsetzen')] },
    ],
    async run(p, ctx) {
      const t = ctx.twitch;
      if (!p.reward) throw new Error('Keine Belohnung ausgewählt.');
      const q = `broadcaster_id=${t.me}&id=${p.reward}`;
      const cur = (await t.api('GET', `/channel_points/custom_rewards?${q}&only_manageable_rewards=true`)).data[0];
      if (!cur) throw new Error('Belohnung nicht gefunden.');
      const key = p.op === 'pause' ? 'is_paused' : 'is_enabled';
      await t.api('PATCH', `/channel_points/custom_rewards?${q}`, { [key]: !cur[key] });
    },
  },
  {
    type: 'twitch_info', category: 'Twitch', label: 'Kanal-Infos in Werte schreiben', icon: '📊',
    fields: [],
    async run(p, ctx) {
      const t = ctx.twitch;
      const s = await t.stream();
      const ch = (await t.api('GET', `/channels?broadcaster_id=${t.me}`)).data[0];
      const set = (k, v) => ctx.store.setVariable(`Twitch.${k}`, String(v ?? ''));
      set('Live', s ? 'an' : 'aus');
      set('Zuschauer', s?.viewer_count ?? 0);
      set('Titel', ch?.title);
      set('Kategorie', ch?.game_name);
      set('Follower', await t.followers());
      return 'Werte Twitch.Live, Twitch.Zuschauer, Twitch.Titel, Twitch.Kategorie, Twitch.Follower aktualisiert';
    },
  },


  // ---------- Discord ----------
  {
    type: 'discord_mute', category: 'Discord', label: 'Mikrofon stumm (Discord)', icon: '🎙️',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('on', 'Stumm'), opt('off', 'Laut')] }],
    async run(p, ctx) {
      const v = await ctx.discord.voiceSettings();
      const mute = p.op === 'on' ? true : p.op === 'off' ? false : !v.mute;
      await ctx.discord.setVoice({ mute });
      return mute ? 'Discord: stumm' : 'Discord: Mikrofon an';
    },
  },
  {
    type: 'discord_deafen', category: 'Discord', label: 'Taub schalten (Discord)', icon: '🎧',
    fields: [{ key: 'op', label: 'Aktion', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('on', 'Taub'), opt('off', 'Hören')] }],
    async run(p, ctx) {
      const v = await ctx.discord.voiceSettings();
      const deaf = p.op === 'on' ? true : p.op === 'off' ? false : !v.deaf;
      await ctx.discord.setVoice({ deaf });
      return deaf ? 'Discord: taub' : 'Discord: hört wieder';
    },
  },
  {
    type: 'discord_channel', category: 'Discord', label: 'Sprachkanal betreten / verlassen', icon: '🔊',
    fields: [
      { key: 'op', label: 'Aktion', type: 'select', default: 'join', options: [opt('join', 'Betreten'), opt('toggle', 'Betreten / verlassen'), opt('leave', 'Verlassen')] },
      { key: 'channel', label: 'Sprachkanal', type: 'dynamic', source: 'discord-voice-channels', showIf: { op: ['join', 'toggle'] } },
    ],
    async run(p, ctx) {
      const d = ctx.discord;
      const current = (await d.call('GET_SELECTED_VOICE_CHANNEL'))?.id;
      const leave = p.op === 'leave' || (p.op === 'toggle' && current === p.channel);
      if (leave) {
        await d.call('SELECT_VOICE_CHANNEL', { channel_id: null });
        return 'Sprachkanal verlassen';
      }
      if (!p.channel) throw new Error('Kein Sprachkanal ausgewählt.');
      await d.call('SELECT_VOICE_CHANNEL', { channel_id: p.channel, force: true });
    },
  },
  {
    type: 'discord_volume', category: 'Discord', label: 'Lautstärke (Discord)', icon: '🔉',
    fields: [
      { key: 'target', label: 'Was?', type: 'select', default: 'output', options: [opt('output', 'Ausgabe (andere hören)'), opt('input', 'Eingabe (eigenes Mikrofon)')] },
      { key: 'op', label: 'Aktion', type: 'select', default: 'up', options: [opt('up', 'Lauter'), opt('down', 'Leiser'), opt('set', 'Auf Wert setzen')] },
      { key: 'step', label: 'Schritt', type: 'number', default: 10, min: 1, max: 100, showIf: { op: ['up', 'down'] } },
      { key: 'value', label: 'Wert (Ausgabe 0–200, Eingabe 0–100)', type: 'number', default: 100, min: 0, max: 200, showIf: { op: ['set'] } },
    ],
    async run(p, ctx) {
      const target = p.target === 'input' ? 'input' : 'output';
      const max = target === 'input' ? 100 : 200;
      const v = await ctx.discord.voiceSettings();
      const cur = v[target]?.volume ?? 100;
      const step = Number(p.step) || 10;
      const next = Math.round(Math.max(0, Math.min(max, p.op === 'set' ? Number(p.value) : cur + (p.op === 'down' ? -step : step))));
      await ctx.discord.setVoice({ [target]: { volume: next } });
      return `Discord ${target === 'input' ? 'Eingabe' : 'Ausgabe'}: ${next}`;
    },
  },
  {
    type: 'discord_mode', category: 'Discord', label: 'Sprachmodus (Push-to-Talk)', icon: '📻',
    fields: [{ key: 'op', label: 'Modus', type: 'select', default: 'toggle', options: [opt('toggle', 'Umschalten'), opt('PUSH_TO_TALK', 'Push-to-Talk'), opt('VOICE_ACTIVITY', 'Sprachaktivierung')] }],
    async run(p, ctx) {
      const v = await ctx.discord.voiceSettings();
      const type = p.op === 'toggle' || !p.op ? (v.mode?.type === 'PUSH_TO_TALK' ? 'VOICE_ACTIVITY' : 'PUSH_TO_TALK') : p.op;
      await ctx.discord.setVoice({ mode: { type } });
      return type === 'PUSH_TO_TALK' ? 'Push-to-Talk' : 'Sprachaktivierung';
    },
  },
  {
    type: 'discord_webhook', category: 'Discord', label: 'Nachricht in Kanal senden (Webhook)', icon: '💬',
    fields: [
      { key: 'url', label: 'Webhook-URL (Kanal bearbeiten → Integrationen → Webhooks)', type: 'text', placeholder: 'https://discord.com/api/webhooks/…' },
      { key: 'message', label: 'Nachricht ({Wert} und {Event.X} möglich)', type: 'textarea' },
      { key: 'username', label: 'Absendername (optional)', type: 'text' },
    ],
    async run(p, ctx) {
      await sendWebhook(p.url, { content: fillVariables(p.message ?? '', ctx.store, ctx.eventData), username: p.username });
      return 'Nachricht gesendet';
    },
  },

  // ---------- Logik ----------
  {
    type: 'run_flow', category: 'Logik', label: 'Vorlage ausführen', icon: '🧩',
    fields: [
      { key: 'flow', label: 'Vorlage', type: 'dynamic', source: 'flows' },
      { key: 'times', label: 'Wie oft? (Wiederholen)', type: 'number', default: 1, min: 1, max: 100 },
    ],
    async run(p, ctx) {
      const flow = ctx.store.flows.find((f) => f.id === p.flow);
      if (!flow) throw new Error('Die Vorlage gibt es nicht mehr.');
      if ((ctx.flowDepth ?? 0) >= 8) throw new Error('Vorlagen rufen sich gegenseitig zu oft auf.');
      const messages = [];
      for (let i = 0; i < Math.max(1, Math.min(100, Number(p.times) || 1)); i++) {
        const r = await runActions(flow.actions, { ...ctx, flowDepth: (ctx.flowDepth ?? 0) + 1, navigate: null, stop: false });
        if (r.navigate) ctx.navigate = r.navigate;
        messages.push(...r.messages);
        if (!r.ok) throw new Error(`Vorlage „${flow.name}“: ${r.error}`);
      }
      return messages.join(' · ') || undefined;
    },
  },
  {
    type: 'stop', category: 'Logik', label: 'Stopp (restliche Aktionen abbrechen)', icon: '🛑',
    fields: [],
    async run(p, ctx) {
      ctx.stop = true;
    },
  },
  {
    type: 'comment', category: 'Logik', label: 'Kommentar (tut nichts)', icon: '💭',
    fields: [{ key: 'text', label: 'Notiz', type: 'textarea' }],
    async run() {},
  },

  // ---------- Visuelles ----------
  {
    type: 'change_visuals', category: 'Visuelles', label: 'Aussehen eines Buttons ändern', icon: '🎨',
    fields: [
      { key: 'target', label: 'Button', type: 'dynamic', source: 'buttons', default: 'self' },
      { key: 'title', label: 'Neue Beschriftung (leer = unverändert, {Wert} möglich)', type: 'text' },
      { key: 'color', label: 'Neue Hintergrundfarbe (leer = unverändert)', type: 'color' },
      { key: 'textColor', label: 'Neue Textfarbe (leer = unverändert)', type: 'color' },
      { key: 'icon', label: 'Neues Emoji (leer = unverändert)', type: 'text' },
    ],
    async run(p, ctx) {
      const id = resolveButton(p.target, ctx);
      const patch = {};
      if (p.title) patch.title = fillVariables(p.title, ctx.store, ctx.eventData);
      if (/^#[0-9a-f]{6}$/i.test(p.color ?? '')) patch.color = p.color;
      if (/^#[0-9a-f]{6}$/i.test(p.textColor ?? '')) patch.textColor = p.textColor;
      if (p.icon) patch.icon = { type: 'emoji', value: p.icon };
      ctx.store.setOverride('button', id, patch);
    },
  },
  {
    type: 'restore_visuals', category: 'Visuelles', label: 'Ursprüngliches Aussehen wiederherstellen', icon: '↩️',
    fields: [{ key: 'target', label: 'Button', type: 'dynamic', source: 'buttons', default: 'self' }],
    async run(p, ctx) {
      ctx.store.setOverride('button', resolveButton(p.target, ctx), null);
    },
  },
  {
    type: 'page_background', category: 'Visuelles', label: 'Seitenhintergrund ändern', icon: '🖌️',
    fields: [
      { key: 'page', label: 'Seite', type: 'page' },
      { key: 'color', label: 'Farbe (leer = Original)', type: 'color' },
    ],
    async run(p, ctx) {
      if (!ctx.store.page(p.page)) throw new Error('Keine Seite ausgewählt.');
      ctx.store.setOverride('page', p.page, /^#[0-9a-f]{6}$/i.test(p.color ?? '') ? { background: p.color } : null);
    },
  },
  {
    type: 'unsupported', category: 'Sonstiges', label: 'Nicht unterstützt', icon: '❔', hidden: true,
    fields: [],
    async run(p) {
      throw new Error(`Die Aktion „${p.original}“ wird noch nicht unterstützt.`);
    },
  },
];

function resolveButton(target, ctx) {
  const id = !target || target === 'self' ? ctx.buttonId : target;
  if (!id) throw new Error('„Dieser Button“ geht nur in Aktionen eines Buttons.');
  if (!ctx.store.findButton(id)) throw new Error('Den Button gibt es nicht mehr.');
  return id;
}

// Einteilung der Aktionen: Oberbegriff › Kategorie.
const CATEGORY = {
  open_url: 'Ausführen', open_path: 'Ausführen', launch_app: 'Ausführen', run_command: 'Ausführen', window_activate: 'Ausführen', kill_app: 'Ausführen',
  go_page: 'Navigation',
  hotkey: 'Eingabe', type_text: 'Eingabe', key_hold: 'Eingabe', mouse_click: 'Eingabe', mouse_move: 'Eingabe',
  notify: 'Dienste', wake_on_lan: 'Dienste', screenshot: 'Dienste', lock_screen: 'Dienste', power: 'Dienste', night_light: 'Dienste', dark_mode: 'Dienste',
  delay: 'Logik', condition: 'Logik',
  var_set: 'Werte', var_change: 'Werte', var_toggle: 'Werte',
  file_write: 'Dateien', http: 'HTTP',
  media: 'Medien', volume: 'Audio', mic_mute: 'Audio', audio_output: 'Audio', audio_input: 'Audio', sound_play: 'Soundboard', sound_stop: 'Soundboard',
  steam_game: 'Steam', steam_open: 'Steam',
};
const GROUP = {
  Ausführen: 'Allgemein', Navigation: 'Allgemein', Eingabe: 'Allgemein', Dienste: 'Allgemein', Logik: 'Allgemein', Werte: 'Allgemein',
  Dateien: 'Allgemein', HTTP: 'Allgemein', Visuelles: 'Allgemein',
  Audio: 'Audio, Musik & Medien', Medien: 'Audio, Musik & Medien', Soundboard: 'Audio, Musik & Medien',
  'OBS Studio': 'Streaming', Twitch: 'Streaming',
  Discord: 'Kommunikation',
  Steam: 'Spiele',
};
for (const t of ACTION_TYPES) {
  t.category = CATEGORY[t.type] ?? t.category;
  t.group = GROUP[t.category] ?? 'Sonstiges';
}

const BY_TYPE = new Map(ACTION_TYPES.map((t) => [t.type, t]));

/** Beschreibung der Aktionen für den Editor (ohne Funktionen). */
export function describeActionTypes() {
  return ACTION_TYPES.map(({ run, ...rest }) => rest);
}

export async function runAction(action, ctx) {
  const def = BY_TYPE.get(action.type);
  if (!def) throw new Error(`Unbekannte Aktion: ${action.type}`);
  return def.run(action.params ?? {}, ctx);
}

/** Auswahllisten, die erst zur Laufzeit feststehen (Spiele, Audiogeräte, OBS-Szenen …). */
export const OPTION_SOURCES = {
  'steam-games': () => steam.listGames(),
  'audio-sinks': async () => [{ value: 'next', label: '↻ Immer zum nächsten Ausgang' }, ...await linux.audioDevices('sinks')],
  'audio-sources': async () => [{ value: 'next', label: '↻ Immer zum nächsten Eingang' }, ...await linux.audioDevices('sources')],
  'obs-scenes': async ({ obs }) => (await obs.scenes()).map((v) => ({ value: v, label: v })),
  'obs-inputs': async ({ obs }) => (await obs.inputs()).map((v) => ({ value: v, label: v })),
  'obs-scene-items': ({ obs }) => obs.sceneItems(),
  'obs-sources': async ({ obs }) => [...await obs.scenes(), ...await obs.inputs()].map((v) => ({ value: v, label: v })),
  'obs-media-inputs': ({ obs }) => obs.inputsOfKind(/ffmpeg_source|vlc_source/),
  'obs-text-inputs': ({ obs }) => obs.inputsOfKind(/^text_/),
  'obs-browser-inputs': ({ obs }) => obs.inputsOfKind(/^browser_source/),
  'obs-image-inputs': ({ obs }) => obs.inputsOfKind(/^image_source|slideshow/),
  'obs-filters': ({ obs }) => obs.filters(),
  'obs-transitions': ({ obs }) => obs.listOf('GetSceneTransitionList', 'transitions', 'transitionName'),
  'obs-profiles': ({ obs }) => obs.listOf('GetProfileList', 'profiles'),
  'obs-collections': ({ obs }) => obs.listOf('GetSceneCollectionList', 'sceneCollections'),
  'obs-hotkeys': ({ obs }) => obs.listOf('GetHotkeyList', 'hotkeys'),
  windows: () => linux.openWindows(),
  flows: ({ store }) => store.flows.map((f) => ({ value: f.id, label: f.name })),
  buttons: ({ store }) => [
    { value: 'self', label: '⟲ Dieser Button' },
    ...store.pages.flatMap((pg) => pg.buttons.map((b) => ({ value: b.id, label: `${pg.name} › ${b.title || b.icon.value || `Feld ${b.row + 1}/${b.col + 1}`}` }))),
  ],
  'toggle-buttons': ({ store }) => store.pages.flatMap((pg) => pg.buttons.filter((b) => b.mode === 'toggle').map((b) => ({ value: b.id, label: `${pg.name} › ${b.title || b.id}` }))),
  'audio-sinks-only': () => linux.audioDevices('sinks'),
  'discord-voice-channels': ({ discord }) => discord.voiceChannels(),
  'twitch-rewards': async ({ twitch }) => (await twitch.api('GET', `/channel_points/custom_rewards?broadcaster_id=${twitch.me}&only_manageable_rewards=true`)).data
    .map((r) => ({ value: r.id, label: `${r.title} (${r.cost})` })),
  variables: ({ store }) => {
    // Gesetzte Variablen plus Namen, die in Aktionen oder Anzeigen schon vorkommen.
    const names = new Set(Object.keys(store.variables));
    for (const name of Object.keys(store.values)) names.add(name);
    const lists = [
      ...store.pages.flatMap((pg) => pg.buttons.flatMap((b) => [b.actions, b.actionsOff, b.holdActions, b.releaseActions, ...b.events.map((e) => e.actions)])),
      ...store.flows.map((f) => f.actions),
      ...store.events.map((e) => e.actions),
    ];
    for (const b of store.pages.flatMap((pg) => pg.buttons)) if (b.live.variable) names.add(b.live.variable);
    for (const a of lists.flat()) if (/^(var_|condition)/.test(a.type) && a.params.name) names.add(a.params.name);
    return [...names].sort((a, b) => a.localeCompare(b, 'de')).map((v) => ({ value: v, label: v }));
  },
};

/** Führt die Aktionen nacheinander aus und bricht beim ersten Fehler ab.
 * services: { store, obs, buttonId?, eventData?, flowDepth? }. */
export async function runActions(actions, services) {
  const ctx = { ...services, navigate: null, stop: false };
  const messages = [];
  for (const action of actions) {
    if (!action.enabled) continue;
    const def = BY_TYPE.get(action.type);
    try {
      const msg = await runAction(action, ctx);
      if (msg) messages.push(msg);
    } catch (err) {
      return { ok: false, error: `${def?.label ?? action.type}: ${err.message}`, messages, navigate: ctx.navigate };
    }
    if (ctx.stop) break;
  }
  return { ok: true, messages, navigate: ctx.navigate };
}
