// Events: „Wenn <Auslöser>, dann <Aktionen>“ – global (Bereich Events) und je Button („Bei Event“).
import fs from 'node:fs';
import { AsyncLocalStorage } from 'node:async_hooks';
import * as linux from '../system/linux.js';
import * as steam from '../integrations/steam.js';

const opt = (value, label) => ({ value, label });
const STATE_ANY = opt('any', 'egal');
const stateField = (options) => ({ key: 'state', label: 'Zustand', type: 'select', default: options[0].value, options });

const compare = (op, actual, expected) => {
  const a = String(actual ?? '');
  const e = String(expected ?? '');
  switch (op) {
    case 'eq': return a === e;
    case 'ne': return a !== e;
    case 'gt': return Number(a) > Number(e);
    case 'lt': return Number(a) < Number(e);
    case 'contains': return a.includes(e);
    default: return true; // „ändert sich“
  }
};
const stateMatches = (want, actual) => !want || want === 'any' || want === actual;

/** Alle Auslöser: Beschreibung für den Editor plus Prüfung, ob ein Ereignis passt. */
export const TRIGGER_TYPES = [
  {
    type: 'value_change', category: 'Werte', label: 'Wert ändert sich', icon: '🏷️',
    fields: [
      { key: 'name', label: 'Wert', type: 'dynamic', source: 'variables', allowNew: true },
      { key: 'op', label: 'Wann?', type: 'select', default: 'any', options: [opt('any', 'bei jeder Änderung'), opt('eq', 'wird gleich'), opt('ne', 'wird ungleich'), opt('gt', 'wird größer als'), opt('lt', 'wird kleiner als'), opt('contains', 'enthält')] },
      { key: 'value', label: 'Vergleichswert', type: 'text', showIf: { op: ['eq', 'ne', 'gt', 'lt', 'contains'] } },
    ],
    match: (p, d) => p.name === d.name && compare(p.op, d.value, p.value),
  },
  { type: 'app_start', category: 'Allgemein', label: 'TouchDeck startet', icon: '🚀', fields: [], match: () => true },
  {
    type: 'timer', category: 'Allgemein', label: 'Zeitgeber (regelmäßig)', icon: '⏱️',
    fields: [{ key: 'seconds', label: 'Alle … Sekunden', type: 'number', default: 60, min: 1, max: 86400 }],
    match: () => true,
  },
  {
    type: 'time_of_day', category: 'Allgemein', label: 'Uhrzeit erreicht', icon: '⏰',
    fields: [
      { key: 'time', label: 'Uhrzeit (HH:MM)', type: 'text', placeholder: '18:30' },
      { key: 'days', label: 'Tage', type: 'select', default: 'all', options: [opt('all', 'täglich'), opt('weekdays', 'Mo–Fr'), opt('weekend', 'Sa und So')] },
    ],
    match: () => true,
  },
  {
    type: 'file_change', category: 'Dateien', label: 'Datei ändert sich', icon: '📄',
    fields: [{ key: 'path', label: 'Datei', type: 'text', placeholder: '~/status.txt' }],
    match: () => true,
  },
  {
    type: 'device', category: 'Allgemein', label: 'Handy verbindet/trennt sich', icon: '📱',
    fields: [stateField([opt('connected', 'verbunden'), opt('disconnected', 'getrennt'), STATE_ANY])],
    match: (p, d) => stateMatches(p.state, d.state),
  },
  {
    type: 'button_toggle', category: 'Allgemein', label: 'Umschalter wird an/aus geschaltet', icon: '🔘',
    fields: [
      { key: 'button', label: 'Button', type: 'dynamic', source: 'toggle-buttons' },
      stateField([opt('on', 'an'), opt('off', 'aus'), STATE_ANY]),
    ],
    match: (p, d) => (!p.button || p.button === d.buttonId) && stateMatches(p.state, d.state),
  },
  {
    type: 'audio_output', category: 'Audio', label: 'Audioausgang wechselt', icon: '🎧',
    fields: [{ key: 'device', label: 'Zu Gerät (leer = jedes)', type: 'dynamic', source: 'audio-sinks-only' }],
    match: (p, d) => !p.device || p.device === d.device,
  },
  {
    type: 'steam_game', category: 'Steam', label: 'Spiel startet/endet', icon: '🎮',
    fields: [
      { key: 'game', label: 'Spiel (leer = jedes)', type: 'dynamic', source: 'steam-games' },
      stateField([opt('started', 'startet'), opt('stopped', 'endet'), STATE_ANY]),
    ],
    match: (p, d) => (!p.game || p.game === d.appid) && stateMatches(p.state, d.state),
  },
  {
    type: 'obs_scene', category: 'OBS Studio', label: 'Szene wechselt', icon: '🎬',
    fields: [{ key: 'scene', label: 'Zu Szene (leer = jede)', type: 'dynamic', source: 'obs-scenes' }],
    match: (p, d) => !p.scene || p.scene === d.scene,
  },
  {
    type: 'obs_preview', category: 'OBS Studio', label: 'Vorschauszene wechselt', icon: '🖼️',
    fields: [{ key: 'scene', label: 'Zu Szene (leer = jede)', type: 'dynamic', source: 'obs-scenes' }],
    match: (p, d) => !p.scene || p.scene === d.scene,
  },
  {
    type: 'obs_stream', category: 'OBS Studio', label: 'Stream startet/stoppt', icon: '📡',
    fields: [stateField([opt('started', 'startet'), opt('stopped', 'stoppt'), STATE_ANY])],
    match: (p, d) => stateMatches(p.state, d.state),
  },
  {
    type: 'obs_record', category: 'OBS Studio', label: 'Aufnahme startet/stoppt/pausiert', icon: '⏺️',
    fields: [stateField([opt('started', 'startet'), opt('stopped', 'stoppt'), opt('paused', 'pausiert'), opt('resumed', 'läuft weiter'), STATE_ANY])],
    match: (p, d) => stateMatches(p.state, d.state),
  },
  { type: 'obs_replay_saved', category: 'OBS Studio', label: 'Clip gespeichert', icon: '⏪', fields: [], match: () => true },
  {
    type: 'obs_mute', category: 'OBS Studio', label: 'Audioquelle stumm/laut', icon: '🔈',
    fields: [{ key: 'input', label: 'Audioquelle (leer = jede)', type: 'dynamic', source: 'obs-inputs' }, stateField([opt('muted', 'stumm'), opt('unmuted', 'laut'), STATE_ANY])],
    match: (p, d) => (!p.input || p.input === d.input) && stateMatches(p.state, d.state),
  },
  {
    type: 'obs_source', category: 'OBS Studio', label: 'Quelle ein-/ausgeblendet', icon: '👁️',
    fields: [{ key: 'item', label: 'Quelle', type: 'dynamic', source: 'obs-scene-items' }, stateField([opt('shown', 'eingeblendet'), opt('hidden', 'ausgeblendet'), STATE_ANY])],
    match: (p, d) => (!p.item || p.item === d.item) && stateMatches(p.state, d.state),
  },
  {
    type: 'obs_filter', category: 'OBS Studio', label: 'Filter ein/aus', icon: '✨',
    fields: [{ key: 'filter', label: 'Filter', type: 'dynamic', source: 'obs-filters' }, stateField([opt('on', 'an'), opt('off', 'aus'), STATE_ANY])],
    match: (p, d) => (!p.filter || p.filter === d.filter) && stateMatches(p.state, d.state),
  },
  {
    type: 'obs_studio', category: 'OBS Studio', label: 'Studio-Modus an/aus', icon: '🎛️',
    fields: [stateField([opt('on', 'an'), opt('off', 'aus'), STATE_ANY])],
    match: (p, d) => stateMatches(p.state, d.state),
  },
  {
    type: 'obs_profile', category: 'OBS Studio', label: 'Profil/Szenensammlung wechselt', icon: '🗂️',
    fields: [{ key: 'kind', label: 'Was?', type: 'select', default: 'any', options: [STATE_ANY, opt('profile', 'Profil'), opt('collection', 'Szenensammlung')] }],
    match: (p, d) => stateMatches(p.kind, d.kind),
  },
  {
    type: 'obs_connection', category: 'OBS Studio', label: 'OBS verbunden/getrennt', icon: '🔌',
    fields: [stateField([opt('connected', 'verbunden'), opt('disconnected', 'getrennt'), STATE_ANY])],
    match: (p, d) => stateMatches(p.state, d.state),
  },
  {
    type: 'twitch_follow', category: 'Twitch', label: 'Neuer Follower', icon: '💜', fields: [],
    match: () => true,
  },
  {
    type: 'twitch_sub', category: 'Twitch', label: 'Abo (neu, verlängert, verschenkt)', icon: '⭐',
    fields: [{ key: 'kind', label: 'Art', type: 'select', default: 'any', options: [STATE_ANY, opt('new', 'neues Abo'), opt('resub', 'verlängert'), opt('gift', 'verschenkt')] }],
    match: (p, d) => stateMatches(p.kind, d.kind === 'gift_received' ? 'gift' : d.kind),
  },
  {
    type: 'twitch_cheer', category: 'Twitch', label: 'Bits', icon: '💎',
    fields: [{ key: 'min', label: 'Ab wie vielen Bits?', type: 'number', default: 1, min: 1 }],
    match: (p, d) => d.bits >= (Number(p.min) || 1),
  },
  {
    type: 'twitch_raid', category: 'Twitch', label: 'Raid', icon: '🚀',
    fields: [{ key: 'min', label: 'Ab wie vielen Zuschauern?', type: 'number', default: 1, min: 1 }],
    match: (p, d) => d.viewers >= (Number(p.min) || 1),
  },
  {
    type: 'twitch_redeem', category: 'Twitch', label: 'Kanalpunkte eingelöst', icon: '🎁',
    fields: [{ key: 'reward', label: 'Belohnung (Titel, leer = jede)', type: 'text' }],
    match: (p, d) => !p.reward || p.reward.trim().toLowerCase() === d.reward.toLowerCase(),
  },
  {
    type: 'twitch_chat', category: 'Twitch', label: 'Chat-Nachricht / Befehl', icon: '💬',
    fields: [
      { key: 'mode', label: 'Wann?', type: 'select', default: 'command', options: [opt('command', 'Nachricht beginnt mit'), opt('contains', 'Nachricht enthält'), opt('any', 'jede Nachricht')] },
      { key: 'text', label: 'Text (z. B. !szene)', type: 'text', showIf: { mode: ['command', 'contains'] } },
      { key: 'who', label: 'Von wem?', type: 'select', default: 'all', options: [opt('all', 'allen'), opt('mod', 'nur Moderatoren und dir'), opt('sub', 'nur Abonnenten')] },
    ],
    match: (p, d) => {
      const msg = d.text.trim().toLowerCase();
      const t = String(p.text ?? '').trim().toLowerCase();
      const textOk = p.mode === 'any' || !t || (p.mode === 'contains' ? msg.includes(t) : msg === t || msg.startsWith(`${t} `));
      const role = p.who === 'mod' ? ['broadcaster', 'moderator'] : p.who === 'sub' ? ['broadcaster', 'subscriber', 'founder'] : null;
      return textOk && (!role || d.badges.some((b) => role.includes(b)));
    },
  },
  {
    type: 'twitch_stream', category: 'Twitch', label: 'Stream geht online/offline', icon: '🔴',
    fields: [stateField([opt('online', 'online'), opt('offline', 'offline'), STATE_ANY])],
    match: (p, d) => stateMatches(p.state, d.state),
  },
  { type: 'twitch_ad', category: 'Twitch', label: 'Werbepause beginnt', icon: '📺', fields: [], match: () => true },
];

const BY_TYPE = new Map(TRIGGER_TYPES.map((t) => [t.type, t]));
export const describeTriggerTypes = () => TRIGGER_TYPES.map(({ match, ...rest }) => rest);

const OUTPUT_STATES = {
  OBS_WEBSOCKET_OUTPUT_STARTED: 'started',
  OBS_WEBSOCKET_OUTPUT_STOPPED: 'stopped',
  OBS_WEBSOCKET_OUTPUT_PAUSED: 'paused',
  OBS_WEBSOCKET_OUTPUT_RESUMED: 'resumed',
};

// Verkettung erkennen: ein Event, das ein Event auslöst, das ein Event auslöst …
const chain = new AsyncLocalStorage();
const MAX_DEPTH = 8;

export class EventEngine {
  /** run(actions, { eventData, buttonId, source }) führt Aktionen aus (runActions mit Diensten). */
  constructor({ store, obs, twitch, log, run }) {
    this.store = store;
    this.obs = obs;
    this.twitch = twitch;
    twitch?.onEvent((type, data) => this.fire(type, data));
    this.log = log;
    this.run = run;
    this.subs = [];
    this.timers = [];
    this.lastRun = new Map();
    this.state = { obsConnected: false, running: null, sink: null };
    obs.onEvent((type, data) => this.#onObs(type, data).catch((err) => this.log.warn('Events', `OBS-Ereignis ${type}: ${err.message}`)));
    store.onChange((what, detail) => {
      if (what === 'variables' && detail?.name) {
        this.fire('value_change', { name: detail.name, value: detail.value, Name: detail.name, Wert: detail.value ?? '', Alt: detail.old ?? '' });
      }
      if (what === 'pages') this.reload();
    });
  }

  start() {
    this.reload();
    this.fire('app_start', {});
  }

  /** Alle aktiven Events neu einlesen und Zeitgeber/Beobachter neu aufsetzen. */
  reload() {
    for (const stop of this.timers) stop();
    this.timers = [];
    this.subs = [];
    for (const ev of this.store.events) {
      if (ev.enabled) this.subs.push({ key: ev.id, name: ev.name, trigger: ev.trigger, actions: ev.actions, cooldown: ev.cooldown });
    }
    for (const page of this.store.pages) {
      for (const b of page.buttons) {
        for (const ev of b.events) {
          if (ev.enabled) this.subs.push({ key: `${b.id}:${ev.id}`, name: `Button „${b.title || b.id}“`, trigger: ev.trigger, actions: ev.actions, cooldown: 0, buttonId: b.id });
        }
      }
    }
    const has = (type) => this.subs.some((s) => s.trigger.type === type);
    const every = (ms, fn) => {
      const t = setInterval(fn, ms);
      this.timers.push(() => clearInterval(t));
    };

    for (const sub of this.subs.filter((s) => s.trigger.type === 'timer')) {
      const ms = Math.max(1, Number(sub.trigger.params.seconds) || 60) * 1000;
      every(ms, () => this.#runSub(sub, {}));
    }
    if (has('time_of_day')) every(15000, () => this.#checkTimeOfDay());
    for (const sub of this.subs.filter((s) => s.trigger.type === 'file_change' && s.trigger.params.path)) {
      const file = linux.expandHome(sub.trigger.params.path);
      const onChange = (cur, prev) => cur.mtimeMs !== prev.mtimeMs && this.#runSub(sub, { Datei: file });
      fs.watchFile(file, { interval: 1500 }, onChange);
      this.timers.push(() => fs.unwatchFile(file, onChange));
    }
    if (this.subs.some((s) => s.trigger.type.startsWith('obs_'))) {
      // OBS-Verbindung halten, damit Ereignisse ankommen; ohne OBS alle 10 s erneut versuchen.
      const keep = () => {
        const was = this.state.obsConnected;
        this.obs.connect().then(() => {
          this.state.obsConnected = true;
          if (!was) this.fire('obs_connection', { state: 'connected' });
        }).catch(() => {
          this.state.obsConnected = false;
          if (was) this.fire('obs_connection', { state: 'disconnected' });
        });
      };
      keep();
      every(10000, keep);
    }
    if (has('steam_game')) every(4000, () => this.#checkSteam());
    if (has('audio_output')) every(4000, () => this.#checkAudio().catch(() => {}));
    this.twitch?.setWanted(this.subs.map((s) => s.trigger.type));
    this.log.debug('Events', `${this.subs.length} Event(s) aktiv`);
  }

  /** Ein Ereignis ist eingetreten: alle passenden Events ausführen. */
  fire(type, data) {
    const def = BY_TYPE.get(type);
    if (!def) return;
    for (const sub of this.subs) {
      if (sub.trigger.type === type && def.match(sub.trigger.params, data)) this.#runSub(sub, data);
    }
  }

  #runSub(sub, data) {
    const depth = chain.getStore() ?? 0;
    if (depth >= MAX_DEPTH) {
      this.log.warn('Events', `„${sub.name}“ übersprungen: Events lösen sich gegenseitig endlos aus.`);
      return;
    }
    const now = Date.now();
    if (sub.cooldown && now - (this.lastRun.get(sub.key) ?? 0) < sub.cooldown * 1000) {
      this.log.debug('Events', `„${sub.name}“ wartet noch (Abklingzeit).`);
      return;
    }
    this.lastRun.set(sub.key, now);
    const eventData = Object.fromEntries(Object.entries(data).map(([k, v]) => [`Event.${k}`, v]));
    chain.run(depth + 1, () => {
      this.run(sub.actions, { eventData, buttonId: sub.buttonId, source: sub.name }).then((r) => {
        if (r.ok) this.log.info('Events', `„${sub.name}“ ausgeführt (${sub.trigger.type})`);
        else this.log.error('Events', `„${sub.name}“: ${r.error}`);
      });
    });
  }

  #checkTimeOfDay() {
    const now = new Date();
    const hhmm = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
    const today = now.toDateString();
    const day = now.getDay();
    for (const sub of this.subs.filter((s) => s.trigger.type === 'time_of_day')) {
      const { time, days } = sub.trigger.params;
      if (time !== hhmm || this.lastRun.get(`${sub.key}:day`) === today) continue;
      if (days === 'weekdays' && (day === 0 || day === 6)) continue;
      if (days === 'weekend' && day > 0 && day < 6) continue;
      this.lastRun.set(`${sub.key}:day`, today);
      this.#runSub(sub, { Zeit: hhmm });
    }
  }

  #checkSteam() {
    const now = steam.runningGames();
    const before = this.state.running;
    this.state.running = now;
    if (!before) return;
    const name = (id) => steam.listGames().find((g) => g.value === id)?.label ?? id;
    for (const id of now) if (!before.has(id)) this.fire('steam_game', { appid: id, state: 'started', Spiel: name(id) });
    for (const id of before) if (!now.has(id)) this.fire('steam_game', { appid: id, state: 'stopped', Spiel: name(id) });
  }

  async #checkAudio() {
    const sink = (await linux.audioDevices('sinks')).find((d) => d.isDefault)?.value ?? null;
    const before = this.state.sink;
    this.state.sink = sink;
    if (before && sink && before !== sink) this.fire('audio_output', { device: sink, Gerät: sink });
  }

  async #onObs(type, d) {
    switch (type) {
      case 'CurrentProgramSceneChanged':
        return this.fire('obs_scene', { scene: d.sceneName, Szene: d.sceneName });
      case 'CurrentPreviewSceneChanged':
        return this.fire('obs_preview', { scene: d.sceneName, Szene: d.sceneName });
      case 'StreamStateChanged':
      case 'RecordStateChanged': {
        const state = OUTPUT_STATES[d.outputState];
        if (state) this.fire(type === 'StreamStateChanged' ? 'obs_stream' : 'obs_record', { state, Status: state });
        return undefined;
      }
      case 'ReplayBufferSaved':
        return this.fire('obs_replay_saved', { Datei: d.savedReplayPath });
      case 'InputMuteStateChanged':
        return this.fire('obs_mute', { input: d.inputName, state: d.inputMuted ? 'muted' : 'unmuted', Quelle: d.inputName });
      case 'SceneItemEnableStateChanged': {
        const { sourceName } = await this.obs.call('GetSceneItemSource', { sceneName: d.sceneName, sceneItemId: d.sceneItemId });
        return this.fire('obs_source', { item: `${d.sceneName}␟${sourceName}`, state: d.sceneItemEnabled ? 'shown' : 'hidden', Quelle: sourceName, Szene: d.sceneName });
      }
      case 'SourceFilterEnableStateChanged':
        return this.fire('obs_filter', { filter: `${d.sourceName}␟${d.filterName}`, state: d.filterEnabled ? 'on' : 'off', Filter: d.filterName, Quelle: d.sourceName });
      case 'StudioModeStateChanged':
        return this.fire('obs_studio', { state: d.studioModeEnabled ? 'on' : 'off' });
      case 'CurrentProfileChanged':
        return this.fire('obs_profile', { kind: 'profile', Name: d.profileName });
      case 'CurrentSceneCollectionChanged':
        return this.fire('obs_profile', { kind: 'collection', Name: d.sceneCollectionName });
      case 'ExitStarted':
        this.state.obsConnected = false;
        return this.fire('obs_connection', { state: 'disconnected' });
      default:
        return undefined;
    }
  }

  stop() {
    for (const stop of this.timers) stop();
    this.timers = [];
  }
}
