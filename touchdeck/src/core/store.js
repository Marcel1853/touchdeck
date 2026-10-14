// Speichert Seiten, Buttons, Werte, Vorlagen, Events und Einstellungen als JSON im Datenordner.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { LIVE_SOURCES, SLIDER_TARGETS } from './live.js';

export function newId(prefix = '') {
  return prefix + crypto.randomBytes(6).toString('hex');
}

export const VARIABLE_NAME = /^[\p{L}\p{N} _.-]{1,40}$/u;

export const TEMPLATES = ['steam', 'system', 'obs', 'touchportal'];

const LIMITS = { rows: [1, 12], cols: [1, 16], fontSize: [8, 64], actions: 100, text: 4000 };

const clamp = (v, [min, max], fallback) => {
  const n = Number.parseInt(v, 10);
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback;
};
const str = (v, fallback = '', max = LIMITS.text) => (typeof v === 'string' ? v.slice(0, max) : fallback);
const color = (v, fallback) => (typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v) ? v.toLowerCase() : fallback);

function sanitizeParams(raw) {
  const params = {};
  for (const [k, v] of Object.entries(raw ?? {})) {
    if (['string', 'number', 'boolean'].includes(typeof v)) params[k.slice(0, 64)] = typeof v === 'string' ? str(v) : v;
  }
  return params;
}

function sanitizeAction(a) {
  if (!a || typeof a !== 'object' || typeof a.type !== 'string') return null;
  return { id: str(a.id) || newId('a_'), type: a.type.slice(0, 64), enabled: a.enabled !== false, params: sanitizeParams(a.params) };
}

const actionList = (list) => (Array.isArray(list) ? list.slice(0, LIMITS.actions).map(sanitizeAction).filter(Boolean) : []);

/** Auslöser eines Events: { type, params }. */
function sanitizeTrigger(t) {
  return { type: typeof t?.type === 'string' ? t.type.slice(0, 64) : 'value_change', params: sanitizeParams(t?.params) };
}

function sanitizeButtonEvent(e) {
  if (!e || typeof e !== 'object') return null;
  return { id: str(e.id) || newId('e_'), enabled: e.enabled !== false, trigger: sanitizeTrigger(e.trigger), actions: actionList(e.actions) };
}

function sanitizeButton(b, page) {
  if (!b || typeof b !== 'object') return null;
  const row = Number.parseInt(b.row, 10);
  const col = Number.parseInt(b.col, 10);
  if (!(row >= 0 && row < page.rows && col >= 0 && col < page.cols)) return null;
  const icon = b.icon && typeof b.icon === 'object' && ['emoji', 'image'].includes(b.icon.type)
    ? { type: b.icon.type, value: str(b.icon.value, '', 256) }
    : { type: 'emoji', value: '' };
  return {
    id: str(b.id) || newId('b_'),
    row,
    col,
    rowSpan: clamp(b.rowSpan, [1, page.rows - row], 1),
    colSpan: clamp(b.colSpan, [1, page.cols - col], 1),
    title: str(b.title, '', 200),
    icon,
    color: color(b.color, '#2b2f3a'),
    colorOn: color(b.colorOn, '#2f9e6b'),
    textColor: color(b.textColor, '#ffffff'),
    fontSize: clamp(b.fontSize, LIMITS.fontSize, 14),
    titlePos: ['top', 'center', 'bottom', 'hidden'].includes(b.titlePos) ? b.titlePos : 'bottom',
    iconFit: b.iconFit === 'cover' ? 'cover' : 'contain',
    mode: ['toggle', 'slider'].includes(b.mode) ? b.mode : 'press',
    live: {
      source: LIVE_SOURCES.some((o) => o.value === b.live?.source) ? b.live.source : 'none',
      command: str(b.live?.command, '', 1000),
      variable: str(b.live?.variable, '', 40),
    },
    slider: {
      target: SLIDER_TARGETS.some((o) => o.value === b.slider?.target) ? b.slider.target : 'volume',
      input: str(b.slider?.input, '', 200),
      command: str(b.slider?.command, '', 1000),
    },
    actions: actionList(b.actions),
    actionsOff: actionList(b.actionsOff),
    // Gedrückthalten: Aktionen beim langen Drücken (optional wiederholt) und beim Loslassen danach.
    holdActions: actionList(b.holdActions),
    releaseActions: actionList(b.releaseActions),
    holdRepeat: clamp(b.holdRepeat, [0, 10000], 0),
    // Bei Event: der Button reagiert selbst auf Ereignisse (z. B. Farbe ändern, wenn ein Wert sich ändert).
    events: (Array.isArray(b.events) ? b.events.slice(0, 20) : []).map(sanitizeButtonEvent).filter(Boolean),
  };
}

export function sanitizePage(p) {
  if (!p || typeof p !== 'object') return null;
  const page = {
    id: str(p.id) || newId('p_'),
    name: str(p.name, 'Seite', 100) || 'Seite',
    rows: clamp(p.rows, LIMITS.rows, 3),
    cols: clamp(p.cols, LIMITS.cols, 5),
    background: color(p.background, '#14161b'),
    // Woher die Seite stammt, damit Vorlagen sie wiederfinden statt doppelt anzulegen.
    template: TEMPLATES.includes(p.template) ? p.template : '',
    buttons: [],
  };
  // Überlappende Buttons verwerfen, der erste gewinnt.
  const taken = new Set();
  for (const raw of Array.isArray(p.buttons) ? p.buttons : []) {
    const b = sanitizeButton(raw, page);
    if (!b) continue;
    const cells = [];
    for (let r = b.row; r < b.row + b.rowSpan; r++) for (let c = b.col; c < b.col + b.colSpan; c++) cells.push(`${r}:${c}`);
    if (cells.some((k) => taken.has(k))) continue;
    cells.forEach((k) => taken.add(k));
    page.buttons.push(b);
  }
  return page;
}

/** Vorlage: eine wiederverwendbare Aktionsliste. */
function sanitizeFlow(f) {
  if (!f || typeof f !== 'object') return null;
  return { id: str(f.id) || newId('f_'), name: str(f.name, 'Vorlage', 100) || 'Vorlage', actions: actionList(f.actions) };
}

/** Globales Event: Wenn <Auslöser>, dann <Aktionen>; optional mit Abklingzeit. */
function sanitizeEvent(e) {
  if (!e || typeof e !== 'object') return null;
  return {
    id: str(e.id) || newId('ev_'),
    name: str(e.name, 'Event', 100) || 'Event',
    enabled: e.enabled !== false,
    trigger: sanitizeTrigger(e.trigger),
    cooldown: clamp(e.cooldown, [0, 86400], 0),
    actions: actionList(e.actions),
  };
}

/** Beschreibung eines Werts (der aktuelle Inhalt steht in variables). */
function sanitizeValueMeta(v) {
  return {
    default: str(v?.default, '', 1000),
    description: str(v?.description, '', 300),
    folder: str(v?.folder, '', 60),
    resetOnStart: v?.resetOnStart === true,
  };
}

function sanitizeValues(values) {
  const out = {};
  for (const [name, meta] of Object.entries(values && typeof values === 'object' ? values : {})) {
    if (VARIABLE_NAME.test(name)) out[name] = sanitizeValueMeta(meta);
  }
  return out;
}

function sampleConfig() {
  const pageId = newId('p_');
  const btn = (row, col, title, emoji, color, actions) => ({ id: newId('b_'), row, col, title, icon: { type: 'emoji', value: emoji }, color, actions });
  const act = (type, params) => ({ id: newId('a_'), type, enabled: true, params });
  return {
    version: 1,
    settings: defaultSettings(pageId),
    pages: [sanitizePage({
      id: pageId,
      name: 'Start',
      rows: 3,
      cols: 5,
      buttons: [
        btn(0, 0, 'Abspielen', '⏯️', '#3b3f8f', [act('media', { op: 'play-pause' })]),
        btn(0, 1, 'Zurück', '⏮️', '#2b2f3a', [act('media', { op: 'previous' })]),
        btn(0, 2, 'Weiter', '⏭️', '#2b2f3a', [act('media', { op: 'next' })]),
        btn(1, 0, 'Leiser', '🔉', '#2b2f3a', [act('volume', { op: 'down', step: 5 })]),
        btn(1, 1, 'Lauter', '🔊', '#2b2f3a', [act('volume', { op: 'up', step: 5 })]),
        btn(1, 2, 'Stumm', '🔇', '#8f3b3b', [act('volume', { op: 'mute' })]),
        btn(2, 0, 'Terminal', '🖥️', '#2b2f3a', [act('hotkey', { keys: 'ctrl+alt+t' })]),
        btn(2, 1, 'Dateien', '📁', '#2b2f3a', [act('open_path', { path: '~' })]),
      ],
    })],
    flows: [],
    events: [],
    values: {},
    toggles: {},
    variables: {},
    overrides: { buttons: {}, pages: {} },
  };
}

function defaultSettings(startPage) {
  return {
    token: crypto.randomBytes(16).toString('hex'),
    startPage,
    lanEnabled: true,
    pin: '',
    deck: { pressAnimation: 'shrink', vibrate: true, sound: false, showLabels: true },
    obs: { host: '127.0.0.1', port: 4455, password: '' },
    twitch: { clientId: '', accessToken: '', refreshToken: '', expiresAt: 0, userId: '', login: '', displayName: '', lastError: null },
    discord: { clientId: '', clientSecret: '', accessToken: '', refreshToken: '', expiresAt: 0, user: '', lastError: null },
    theme: { mode: 'dark', accent: '#7c8cff' },
    backups: { auto: true, keep: 10 },
    logs: { level: 'info' },
  };
}

export class Store {
  constructor(dataDir) {
    this.dataDir = dataDir;
    this.iconDir = path.join(dataDir, 'icons');
    this.file = path.join(dataDir, 'config.json');
    fs.mkdirSync(this.iconDir, { recursive: true });
    this.config = this.#load();
    this.listeners = new Set();
    // Zählt Änderungen am Projekt, damit ein veralteter Editor-Tab nichts überschreibt.
    this.revision = 1;
  }

  #load() {
    if (!fs.existsSync(this.file)) {
      const cfg = sampleConfig();
      this.#write(cfg);
      return cfg;
    }
    const cfg = JSON.parse(fs.readFileSync(this.file, 'utf8'));
    cfg.pages = (cfg.pages ?? []).map(sanitizePage).filter(Boolean);
    if (cfg.pages.length === 0) cfg.pages = sampleConfig().pages;
    cfg.flows = (cfg.flows ?? []).map(sanitizeFlow).filter(Boolean);
    cfg.events = (cfg.events ?? []).map(sanitizeEvent).filter(Boolean);
    cfg.values = sanitizeValues(cfg.values);
    cfg.toggles ??= {};
    cfg.variables ??= {};
    cfg.overrides = { buttons: cfg.overrides?.buttons ?? {}, pages: cfg.overrides?.pages ?? {} };
    const defaults = defaultSettings(cfg.pages[0].id);
    cfg.settings = { ...defaults, ...cfg.settings };
    for (const k of ['deck', 'obs', 'twitch', 'discord', 'theme', 'backups', 'logs']) cfg.settings[k] = { ...defaults[k], ...cfg.settings[k] };
    cfg.settings.token ||= defaults.token;
    if (!cfg.pages.some((p) => p.id === cfg.settings.startPage)) cfg.settings.startPage = cfg.pages[0].id;
    // Werte mit „beim Start zurücksetzen“ und noch nie gesetzte Werte bekommen ihren Startwert.
    for (const [name, meta] of Object.entries(cfg.values)) {
      if (meta.resetOnStart || !(name in cfg.variables)) cfg.variables[name] = meta.default;
    }
    return cfg;
  }

  #write(cfg) {
    const tmp = `${this.file}.tmp`;
    fs.writeFileSync(tmp, JSON.stringify(cfg, null, 2), { mode: 0o600 });
    fs.renameSync(tmp, this.file);
  }

  #changed(what, detail) {
    this.#write(this.config);
    for (const fn of this.listeners) fn(what, detail);
  }

  onChange(fn) {
    this.listeners.add(fn);
  }

  get pages() {
    return this.config.pages;
  }

  get flows() {
    return this.config.flows;
  }

  get events() {
    return this.config.events;
  }

  get values() {
    return this.config.values;
  }

  get settings() {
    return this.config.settings;
  }

  get toggles() {
    return this.config.toggles;
  }

  get overrides() {
    return this.config.overrides;
  }

  page(id) {
    return this.config.pages.find((p) => p.id === id);
  }

  findButton(id) {
    for (const page of this.config.pages) {
      const button = page.buttons.find((b) => b.id === id);
      if (button) return { page, button };
    }
    return null;
  }

  /** Speichert alles, was der Editor bearbeitet: Seiten, Vorlagen, Events, Werte. */
  setProject({ pages, flows, events, values }) {
    const clean = (Array.isArray(pages) ? pages : this.config.pages).map(sanitizePage).filter(Boolean);
    if (clean.length === 0) throw new Error('Mindestens eine Seite wird benötigt.');
    this.config.pages = clean;
    if (Array.isArray(flows)) this.config.flows = flows.map(sanitizeFlow).filter(Boolean);
    if (Array.isArray(events)) this.config.events = events.map(sanitizeEvent).filter(Boolean);
    if (values && typeof values === 'object') {
      this.config.values = sanitizeValues(values);
      for (const [name, meta] of Object.entries(this.config.values)) if (!(name in this.config.variables)) this.config.variables[name] = meta.default;
    }
    this.revision++;
    if (!this.page(this.settings.startPage)) this.settings.startPage = clean[0].id;
    const ids = new Set(clean.flatMap((p) => p.buttons.map((b) => b.id)));
    for (const id of Object.keys(this.toggles)) if (!ids.has(id)) delete this.toggles[id];
    for (const id of Object.keys(this.overrides.buttons)) if (!ids.has(id)) delete this.overrides.buttons[id];
    this.#changed('pages');
  }

  setPages(pages) {
    this.setProject({ pages });
  }

  addPages(pages) {
    this.setPages([...this.config.pages, ...pages]);
  }

  /** Ersetzt alle Seiten einer Vorlage an der Stelle der ersten; gibt es noch keine, kommen sie ans Ende. */
  replaceTemplatePages(template, pages) {
    const result = [];
    let inserted = false;
    for (const p of this.config.pages) {
      if (p.template !== template) result.push(p);
      else if (!inserted) {
        result.push(...pages);
        inserted = true;
      }
    }
    if (!inserted) result.push(...pages);
    this.setPages(result);
  }

  setSettings(patch) {
    const s = this.settings;
    if (typeof patch.startPage === 'string' && this.page(patch.startPage)) s.startPage = patch.startPage;
    if (typeof patch.lanEnabled === 'boolean') s.lanEnabled = patch.lanEnabled;
    if (typeof patch.pin === 'string' && /^\d{0,8}$/.test(patch.pin)) s.pin = patch.pin;
    if (typeof patch.firewallOpened === 'boolean') s.firewallOpened = patch.firewallOpened;
    if (patch.deck && typeof patch.deck === 'object') {
      const d = patch.deck;
      if (['none', 'shrink', 'squeeze', 'wobble', 'glow'].includes(d.pressAnimation)) s.deck.pressAnimation = d.pressAnimation;
      for (const k of ['vibrate', 'sound', 'showLabels']) if (typeof d[k] === 'boolean') s.deck[k] = d[k];
    }
    if (patch.obs && typeof patch.obs === 'object') {
      const o = patch.obs;
      if (typeof o.host === 'string' && /^[\w.:-]{1,253}$/.test(o.host)) s.obs.host = o.host;
      if (Number.isInteger(o.port) && o.port > 0 && o.port < 65536) s.obs.port = o.port;
      if (typeof o.password === 'string') s.obs.password = o.password.slice(0, 200);
    }
    if (patch.twitch && typeof patch.twitch === 'object' && typeof patch.twitch.clientId === 'string') {
      const id = patch.twitch.clientId.trim().slice(0, 64);
      // Neue Client-ID: alte Anmeldung gilt nicht mehr.
      if (id !== s.twitch.clientId) Object.assign(s.twitch, { clientId: id, accessToken: '', refreshToken: '', userId: '', login: '', displayName: '', lastError: null });
    }
    if (patch.discord && typeof patch.discord === 'object') {
      const d = s.discord;
      const id = typeof patch.discord.clientId === 'string' ? patch.discord.clientId.trim().slice(0, 32) : d.clientId;
      const secret = typeof patch.discord.clientSecret === 'string' && patch.discord.clientSecret ? patch.discord.clientSecret.trim().slice(0, 64) : d.clientSecret;
      // Andere Anwendung: alte Anmeldung gilt nicht mehr.
      if (id !== d.clientId) Object.assign(d, { accessToken: '', refreshToken: '', user: '', lastError: null });
      Object.assign(d, { clientId: id, clientSecret: secret });
    }
    if (patch.theme && typeof patch.theme === 'object') {
      if (['dark', 'light', 'system'].includes(patch.theme.mode)) s.theme.mode = patch.theme.mode;
      s.theme.accent = color(patch.theme.accent, s.theme.accent);
    }
    if (patch.backups && typeof patch.backups === 'object') {
      if (typeof patch.backups.auto === 'boolean') s.backups.auto = patch.backups.auto;
      s.backups.keep = clamp(patch.backups.keep, [1, 100], s.backups.keep);
    }
    if (patch.logs && ['debug', 'info', 'warn', 'error'].includes(patch.logs.level)) s.logs.level = patch.logs.level;
    if (patch.regenerateToken === true) s.token = crypto.randomBytes(16).toString('hex');
    const security = patch.regenerateToken === true || patch.lanEnabled === false || typeof patch.pin === 'string';
    this.#changed(security ? 'security' : 'settings');
  }

  /** Twitch-Anmeldung speichern (nur der Server ruft das auf; Tokens gehen nie an Editor oder Handy). */
  setTwitchAuth(patch) {
    Object.assign(this.settings.twitch, patch);
    this.#changed('settings');
  }

  setDiscordAuth(patch) {
    Object.assign(this.settings.discord, patch);
    this.#changed('settings');
  }

  /** Einstellungen ohne Geheimnisse für den Editor. */
  publicSettings() {
    const { accessToken, refreshToken, ...twitch } = this.settings.twitch;
    const { accessToken: dAt, refreshToken: dRt, clientSecret, ...discord } = this.settings.discord;
    return { ...this.settings, twitch: { ...twitch, connected: !!accessToken }, discord: { ...discord, hasSecret: !!clientSecret, connected: !!dAt } };
  }

  get variables() {
    return this.config.variables;
  }

  setVariable(name, value) {
    if (!VARIABLE_NAME.test(name ?? '')) throw new Error('Variablenname: 1–40 Zeichen, Buchstaben, Ziffern, Leerzeichen, _ . -');
    const v = typeof value === 'string' ? value.slice(0, 1000) : String(value ?? '');
    const old = this.config.variables[name];
    if (old === v) return;
    this.config.variables[name] = v;
    this.#changed('variables', { name, old, value: v });
  }

  deleteVariable(name) {
    if (!(name in this.config.variables)) return;
    delete this.config.variables[name];
    this.#changed('variables', { name, old: undefined, value: undefined });
  }

  setToggle(buttonId, on) {
    this.config.toggles[buttonId] = on;
    this.#changed('toggles');
  }

  /** Optische Änderung zur Laufzeit (Aktion „Visuelles“); null stellt das Original wieder her. */
  setOverride(kind, id, patch) {
    const map = this.config.overrides[kind === 'page' ? 'pages' : 'buttons'];
    if (patch === null) delete map[id];
    else map[id] = { ...map[id], ...patch };
    this.#changed('overrides');
  }

  resetOverrides() {
    this.config.overrides = { buttons: {}, pages: {} };
    this.#changed('overrides');
  }

  /** Ersetzt die komplette Konfiguration (Backup wiederherstellen). */
  restore(cfg) {
    const before = this.config;
    try {
      this.#write(cfg);
      this.config = this.#load();
    } catch (err) {
      this.config = before;
      this.#write(before);
      throw err;
    }
    this.revision++;
    this.#changed('pages');
  }
}
