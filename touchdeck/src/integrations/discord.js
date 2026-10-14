// Discord: Steuerung des Discord-Programms am PC über dessen lokale Schnittstelle (RPC über Unix-Socket)
// und Nachrichten per Webhook. Für die Sprachsteuerung braucht jeder Nutzer eine eigene Discord-Anwendung.
import net from 'node:net';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';

const API = 'https://discord.com/api/v10';
export const SCOPES = ['rpc', 'rpc.voice.read', 'rpc.voice.write', 'identify'];
export const REDIRECT_URI = 'http://localhost';

const OP = { HANDSHAKE: 0, FRAME: 1, CLOSE: 2, PING: 3, PONG: 4 };

/** Mögliche Orte des Sockets: normales Paket, Flatpak, Snap; Discord nimmt den ersten freien von 0 bis 9. */
export function socketPaths() {
  const bases = [process.env.XDG_RUNTIME_DIR, process.env.TMPDIR, process.env.TMP, process.env.TEMP, '/tmp'].filter(Boolean);
  const subdirs = ['', 'app/com.discordapp.Discord', 'app/com.discordapp.DiscordCanary', 'snap.discord', 'snap.discord-canary', '.flatpak/com.discordapp.Discord/xdg-run'];
  const result = [];
  for (const base of bases) for (const sub of subdirs) for (let i = 0; i < 10; i++) result.push(path.join(base, sub, `discord-ipc-${i}`));
  return [...new Set(result)];
}

function encode(op, data) {
  const json = Buffer.from(JSON.stringify(data));
  const head = Buffer.alloc(8);
  head.writeInt32LE(op, 0);
  head.writeInt32LE(json.length, 4);
  return Buffer.concat([head, json]);
}

export class DiscordClient {
  /** auth(): gespeicherte Daten; saveAuth(patch): speichern; log: Protokoll; paths: Socket-Orte (für Tests). */
  constructor({ auth, saveAuth, log, paths = socketPaths }) {
    this.auth = auth;
    this.saveAuth = saveAuth;
    this.log = log;
    this.paths = paths;
    this.sock = null;
    this.ready = null;
    this.pending = new Map();
    this.buffer = Buffer.alloc(0);
    this.listeners = new Set();
    this.lastFailure = 0;
    this.authorizing = false;
    this.voice = null; // zuletzt bekannte Spracheinstellungen
    this.channel = null; // aktueller Sprachkanal
  }

  get configured() {
    const a = this.auth();
    return !!(a.clientId && a.accessToken);
  }

  get connected() {
    return !!this.sock && this.authenticated;
  }

  status() {
    const a = this.auth();
    if (this.authorizing) return { state: 'waiting' };
    if (a.accessToken) return { state: 'connected', user: a.user, running: this.connected };
    return { state: a.lastError ? 'error' : 'idle', error: a.lastError ?? null };
  }

  onEvent(fn) {
    this.listeners.add(fn);
  }

  // ---------- Verbindung zum Discord-Programm ----------

  /** Verbindet mit dem laufenden Discord und meldet sich an (nutzt die gespeicherte Anmeldung). */
  connect() {
    if (this.ready) return this.ready;
    if (Date.now() - this.lastFailure < 10000) return Promise.reject(new Error(this.lastError ?? 'Discord läuft nicht.'));
    this.ready = this.#open().then(async () => {
      if (this.auth().accessToken) await this.#authenticate();
      return this;
    }).catch((err) => {
      this.lastFailure = Date.now();
      this.lastError = err.message;
      this.#close();
      throw err;
    });
    return this.ready;
  }

  #open() {
    const clientId = this.auth().clientId;
    if (!clientId) return Promise.reject(new Error('Bitte zuerst die Discord-Anwendung in den Einstellungen eintragen.'));
    const candidates = this.paths().filter((p) => fs.existsSync(p));
    if (!candidates.length) return Promise.reject(new Error('Discord läuft nicht (oder ist nicht installiert).'));
    return new Promise((resolve, reject) => {
      const sock = net.createConnection(candidates[0]);
      const timer = setTimeout(() => {
        sock.destroy();
        reject(new Error('Discord antwortet nicht.'));
      }, 5000);
      sock.on('connect', () => sock.write(encode(OP.HANDSHAKE, { v: 1, client_id: clientId })));
      sock.on('data', (chunk) => this.#onData(chunk));
      sock.on('error', (err) => {
        clearTimeout(timer);
        reject(new Error(`Discord: ${err.message}`));
      });
      sock.on('close', () => {
        clearTimeout(timer);
        if (this.sock === sock) this.#close();
      });
      this.sock = sock;
      this.onReady = () => {
        clearTimeout(timer);
        resolve();
      };
      this.onHandshakeError = (msg) => {
        clearTimeout(timer);
        reject(new Error(msg));
      };
    });
  }

  #close() {
    for (const p of this.pending.values()) p.reject(new Error('Verbindung zu Discord getrennt.'));
    this.pending.clear();
    this.sock?.destroy();
    this.sock = null;
    this.ready = null;
    this.authenticated = false;
    this.buffer = Buffer.alloc(0);
  }

  #onData(chunk) {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    while (this.buffer.length >= 8) {
      const op = this.buffer.readInt32LE(0);
      const len = this.buffer.readInt32LE(4);
      if (this.buffer.length < 8 + len) return;
      const body = JSON.parse(this.buffer.subarray(8, 8 + len).toString('utf8'));
      this.buffer = this.buffer.subarray(8 + len);
      if (op === OP.PING) this.sock?.write(encode(OP.PONG, body));
      else if (op === OP.CLOSE) this.onHandshakeError?.(`Discord lehnt ab: ${body.message ?? body.code} (stimmt die Client-ID?)`);
      else if (op === OP.FRAME) this.#onFrame(body);
    }
  }

  #onFrame(msg) {
    if (msg.cmd === 'DISPATCH' && msg.evt === 'READY') return this.onReady?.();
    if (msg.cmd === 'DISPATCH') return this.#onDispatch(msg.evt, msg.data ?? {});
    const p = this.pending.get(msg.nonce);
    if (!p) return;
    this.pending.delete(msg.nonce);
    if (msg.evt === 'ERROR') p.reject(new Error(`Discord: ${msg.data?.message ?? 'Fehler'}`));
    else p.resolve(msg.data ?? {});
  }

  /** Einen RPC-Befehl an Discord schicken. */
  send(cmd, args = {}, evt) {
    if (!this.sock) return Promise.reject(new Error('Nicht mit Discord verbunden.'));
    const nonce = crypto.randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(nonce);
        reject(new Error(`Discord antwortet nicht auf ${cmd}.`));
      }, cmd === 'AUTHORIZE' ? 120000 : 8000);
      this.pending.set(nonce, {
        resolve: (v) => (clearTimeout(timer), resolve(v)),
        reject: (e) => (clearTimeout(timer), reject(e)),
      });
      this.sock.write(encode(OP.FRAME, { cmd, args, nonce, ...(evt && { evt }) }));
    });
  }

  /** Verbinden (falls nötig) und Befehl ausführen. */
  async call(cmd, args) {
    if (!this.auth().accessToken) throw new Error('Nicht mit Discord verbunden (Einstellungen → Integrationen → Discord).');
    await this.connect();
    return this.send(cmd, args);
  }

  // ---------- Anmeldung ----------

  /** Erstes Verbinden: Discord zeigt ein Fenster „TouchDeck möchte … – Autorisieren“. */
  async login() {
    const { clientId, clientSecret } = this.auth();
    if (!clientId || !clientSecret) throw new Error('Bitte Client-ID und Client-Secret deiner Discord-Anwendung eintragen.');
    this.#close();
    this.lastFailure = 0;
    this.authorizing = true;
    try {
      await this.#open();
      // Discord zeigt jetzt im Programm die Freigabe-Abfrage.
      const { code } = await this.send('AUTHORIZE', { client_id: clientId, scopes: SCOPES });
      const tokens = await this.#token({ grant_type: 'authorization_code', code, redirect_uri: REDIRECT_URI });
      this.saveAuth({ ...tokens, lastError: null });
      this.ready = this.#authenticate().then(() => this);
      await this.ready;
      this.log.info('Discord', `Verbunden als ${this.auth().user}`);
      this.#subscribe();
    } catch (err) {
      this.saveAuth({ lastError: err.message });
      this.#close();
      throw err;
    } finally {
      this.authorizing = false;
    }
  }

  async #token(form) {
    const { clientId, clientSecret } = this.auth();
    const res = await fetch(`${API}/oauth2/token`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({ client_id: clientId, client_secret: clientSecret, ...form }),
      signal: AbortSignal.timeout(10000),
    });
    const d = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(`Discord-Anmeldung: ${d.error_description ?? d.error ?? res.status}`);
    return { accessToken: d.access_token, refreshToken: d.refresh_token, expiresAt: Date.now() + (d.expires_in ?? 604800) * 1000 };
  }

  async #authenticate() {
    const a = this.auth();
    if (a.expiresAt && a.expiresAt - Date.now() < 3600000 && a.refreshToken) {
      this.saveAuth(await this.#token({ grant_type: 'refresh_token', refresh_token: a.refreshToken }));
    }
    try {
      const d = await this.send('AUTHENTICATE', { access_token: this.auth().accessToken });
      this.authenticated = true;
      this.saveAuth({ user: d.user?.global_name || d.user?.username || '', lastError: null });
    } catch (err) {
      this.saveAuth({ accessToken: '', refreshToken: '', lastError: 'Discord-Anmeldung abgelaufen. Bitte neu verbinden.' });
      throw err;
    }
    this.#subscribe();
  }

  /** Spracheinstellungen und Kanalwechsel abonnieren (für Live-Anzeigen und Events). */
  async #subscribe() {
    try {
      await this.send('SUBSCRIBE', {}, 'VOICE_SETTINGS_UPDATE');
      await this.send('SUBSCRIBE', {}, 'VOICE_CHANNEL_SELECT');
      this.voice = await this.send('GET_VOICE_SETTINGS');
      const ch = await this.send('GET_SELECTED_VOICE_CHANNEL');
      this.channel = ch?.id ? { id: ch.id, name: ch.name } : null;
    } catch (err) {
      this.log.warn('Discord', `Ereignisse nicht verfügbar: ${err.message}`);
    }
  }

  #onDispatch(evt, data) {
    if (evt === 'VOICE_SETTINGS_UPDATE') {
      const before = this.voice;
      this.voice = data;
      if (before && before.mute !== data.mute) this.#emit('discord_voice', { state: data.mute ? 'muted' : 'unmuted', Status: data.mute ? 'stumm' : 'an' });
      if (before && before.deaf !== data.deaf) this.#emit('discord_voice', { state: data.deaf ? 'deafened' : 'undeafened', Status: data.deaf ? 'taub' : 'hört' });
    } else if (evt === 'VOICE_CHANNEL_SELECT') {
      const before = this.channel;
      this.channel = data.channel_id ? { id: data.channel_id, name: data.channel_name ?? '' } : null;
      if (this.channel) this.send('GET_CHANNEL', { channel_id: data.channel_id }).then((c) => (this.channel = { id: c.id, name: c.name })).catch(() => {});
      if (!before && this.channel) this.#emit('discord_channel', { state: 'joined', channel: data.channel_id, Kanal: data.channel_name ?? '' });
      else if (before && !this.channel) this.#emit('discord_channel', { state: 'left', channel: before.id, Kanal: before.name });
      else if (before && this.channel && before.id !== this.channel.id) this.#emit('discord_channel', { state: 'joined', channel: data.channel_id, Kanal: data.channel_name ?? '' });
    }
  }

  #emit(type, data) {
    for (const fn of this.listeners) fn(type, data);
  }

  disconnect() {
    this.#close();
    this.saveAuth({ accessToken: '', refreshToken: '', expiresAt: 0, user: '', lastError: null });
    this.log.info('Discord', 'Verbindung getrennt');
  }

  // ---------- Hilfen für Aktionen ----------

  async voiceSettings() {
    this.voice = await this.call('GET_VOICE_SETTINGS');
    return this.voice;
  }

  async setVoice(patch) {
    this.voice = await this.call('SET_VOICE_SETTINGS', patch);
    return this.voice;
  }

  /** Alle Sprachkanäle aller Server als „Server › Kanal“. */
  async voiceChannels() {
    const { guilds } = await this.call('GET_GUILDS');
    const result = [];
    for (const g of guilds) {
      const { channels } = await this.call('GET_CHANNELS', { guild_id: g.id });
      for (const c of channels) if (c.type === 2 || c.type === 13) result.push({ value: c.id, label: `${g.name} › ${c.name}` });
    }
    return result;
  }

  stop() {
    this.#close();
  }
}

/** Nachricht über einen Discord-Webhook senden (ohne Anmeldung, nur mit der Webhook-Adresse). */
export async function sendWebhook(url, { content, username }) {
  if (!/^https:\/\/(?:\w+\.)?discord(?:app)?\.com\/api\/webhooks\/\d+\/[\w-]+$/.test(url ?? '')) {
    throw new Error('Ungültige Webhook-Adresse (Discord: Kanal bearbeiten → Integrationen → Webhooks → Webhook-URL kopieren).');
  }
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content: String(content ?? '').slice(0, 2000), ...(username && { username: username.slice(0, 80) }) }),
    signal: AbortSignal.timeout(10000),
  });
  if (!res.ok) throw new Error(`Discord-Webhook: Fehler ${res.status}`);
}
