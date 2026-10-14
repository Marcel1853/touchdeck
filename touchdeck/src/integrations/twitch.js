// Twitch: Anmeldung per Geräte-Code (jeder Nutzer mit eigener Client-ID), Helix-API und EventSub-Ereignisse.
import WebSocket from 'ws';

const ID = 'https://id.twitch.tv/oauth2';
const HELIX = 'https://api.twitch.tv/helix';
const EVENTSUB = 'wss://eventsub.wss.twitch.tv/ws';

export const SCOPES = [
  'user:read:chat', 'user:write:chat', 'moderator:manage:announcements', 'moderator:manage:chat_settings',
  'moderator:manage:shoutouts', 'moderator:read:followers', 'clips:edit', 'channel:manage:broadcast',
  'channel:edit:commercial', 'channel:manage:raids', 'channel:manage:redemptions', 'channel:read:redemptions',
  'channel:read:subscriptions', 'bits:read', 'channel:read:ads',
];

/** EventSub-Typen je Auslöser (nur abonniert, wenn ein Event sie braucht). */
export const EVENTSUB_TYPES = {
  twitch_follow: [['channel.follow', '2', (u) => ({ broadcaster_user_id: u, moderator_user_id: u })]],
  twitch_sub: [
    ['channel.subscribe', '1', (u) => ({ broadcaster_user_id: u })],
    ['channel.subscription.gift', '1', (u) => ({ broadcaster_user_id: u })],
    ['channel.subscription.message', '1', (u) => ({ broadcaster_user_id: u })],
  ],
  twitch_cheer: [['channel.cheer', '1', (u) => ({ broadcaster_user_id: u })]],
  twitch_raid: [['channel.raid', '1', (u) => ({ to_broadcaster_user_id: u })]],
  twitch_redeem: [['channel.channel_points_custom_reward_redemption.add', '1', (u) => ({ broadcaster_user_id: u })]],
  twitch_chat: [['channel.chat.message', '1', (u) => ({ broadcaster_user_id: u, user_id: u })]],
  twitch_stream: [
    ['stream.online', '1', (u) => ({ broadcaster_user_id: u })],
    ['stream.offline', '1', (u) => ({ broadcaster_user_id: u })],
  ],
  twitch_ad: [['channel.ad_break.begin', '1', (u) => ({ broadcaster_user_id: u })]],
};

const form = (data) => new URLSearchParams(data).toString();

export class TwitchClient {
  /** auth(): gespeicherte Anmeldung; saveAuth(patch): Anmeldung speichern; log: Protokoll. */
  constructor({ auth, saveAuth, log }) {
    this.auth = auth;
    this.saveAuth = saveAuth;
    this.log = log;
    this.device = null; // laufende Anmeldung per Code
    this.ws = null;
    this.sessionId = null;
    this.wanted = new Set(); // benötigte Auslöser
    this.listeners = new Set();
    this.cache = new Map();
    this.reconnectTimer = null;
    this.refreshing = null; // laufende Erneuerung (es darf nur eine geben)
  }

  get connected() {
    return !!this.auth().accessToken;
  }

  status() {
    const a = this.auth();
    if (this.device) return { state: 'waiting', userCode: this.device.userCode, verificationUri: this.device.verificationUri, expiresAt: this.device.expiresAt };
    if (a.accessToken) return { state: 'connected', login: a.login, displayName: a.displayName, events: !!this.sessionId };
    return { state: a.lastError ? 'error' : 'idle', error: a.lastError ?? null };
  }

  // ---------- Anmeldung ----------

  /** Startet die Anmeldung: liefert den Code, den der Nutzer auf twitch.tv/activate eingibt. */
  async startDeviceLogin() {
    const clientId = this.auth().clientId;
    if (!clientId) throw new Error('Bitte zuerst die Client-ID deiner Twitch-Anwendung eintragen.');
    const res = await fetch(`${ID}/device`, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: form({ client_id: clientId, scopes: SCOPES.join(' ') }) });
    const d = await res.json();
    if (!res.ok) throw new Error(`Twitch lehnt ab: ${d.message ?? res.status}. Ist die Client-ID richtig und die Anwendung „Öffentlich“?`);
    this.device = { code: d.device_code, userCode: d.user_code, verificationUri: d.verification_uri, expiresAt: Date.now() + d.expires_in * 1000, interval: d.interval };
    this.#pollDevice(clientId);
    return this.status();
  }

  async #pollDevice(clientId) {
    const dev = this.device;
    while (this.device === dev && Date.now() < dev.expiresAt) {
      await new Promise((r) => setTimeout(r, Math.max(1, dev.interval) * 1000));
      if (this.device !== dev) return;
      try {
        const res = await fetch(`${ID}/token`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
          body: form({ client_id: clientId, scopes: SCOPES.join(' '), device_code: dev.code, grant_type: 'urn:ietf:params:oauth:grant-type:device_code' }),
        });
        const d = await res.json();
        if (res.ok) {
          this.device = null;
          await this.#storeTokens(d);
          const me = await this.validate();
          this.log.info('Twitch', `Verbunden als ${me.login}`);
          this.syncEvents();
          return;
        }
        if (d.message !== 'authorization_pending') {
          this.device = null;
          this.saveAuth({ lastError: `Anmeldung abgebrochen: ${d.message}` });
          return;
        }
      } catch (err) {
        this.log.warn('Twitch', `Anmeldung: ${err.message}`);
      }
    }
    if (this.device === dev) {
      this.device = null;
      this.saveAuth({ lastError: 'Der Code ist abgelaufen. Bitte noch einmal verbinden.' });
    }
  }

  cancelLogin() {
    this.device = null;
  }

  async #storeTokens(d) {
    this.saveAuth({ accessToken: d.access_token, refreshToken: d.refresh_token, expiresAt: Date.now() + (d.expires_in ?? 3600) * 1000, lastError: null });
  }

  /** Prüft die Anmeldung; ist der Zugang abgelaufen (gilt nur ca. 4 h), wird er zuerst erneuert. */
  async validate() {
    const check = () => fetch(`${ID}/validate`, { headers: { Authorization: `OAuth ${this.auth().accessToken}` } });
    if (this.auth().expiresAt && this.auth().expiresAt - Date.now() < 60000) await this.#refresh();
    let res = await check();
    if (res.status === 401) {
      await this.#refresh();
      res = await check();
    }
    if (!res.ok) throw new Error('Twitch-Anmeldung ungültig. Bitte in den Einstellungen neu verbinden.');
    const d = await res.json();
    const users = await this.api('GET', `/users?id=${d.user_id}`);
    const u = users.data[0];
    this.saveAuth({ userId: d.user_id, login: d.login, displayName: u?.display_name ?? d.login });
    return d;
  }

  /**
   * Erneuert den Zugang. Twitch lässt jeden Erneuerungsschlüssel öffentlicher Anwendungen nur einmal zu –
   * deshalb läuft immer nur eine Erneuerung, alle gleichzeitigen Aufrufe warten auf dasselbe Ergebnis.
   */
  #refresh() {
    this.refreshing ??= this.#doRefresh().finally(() => (this.refreshing = null));
    return this.refreshing;
  }

  async #doRefresh() {
    const a = this.auth();
    if (!a.refreshToken) throw new Error('Nicht mit Twitch verbunden.');
    let res;
    try {
      res = await fetch(`${ID}/token`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: form({ client_id: a.clientId, grant_type: 'refresh_token', refresh_token: a.refreshToken }),
        signal: AbortSignal.timeout(10000),
      });
    } catch (err) {
      // Netz weg: Anmeldung behalten und später erneut versuchen.
      throw new Error(`Twitch nicht erreichbar: ${err.message}`);
    }
    const d = await res.json().catch(() => ({}));
    if (!res.ok) {
      this.log.warn('Twitch', `Erneuern abgelehnt (${res.status}): ${d.message ?? 'ohne Begründung'}`);
      // Nur bei echter Ablehnung abmelden, nicht bei Serverproblemen von Twitch.
      if (res.status === 400 || res.status === 401) {
        this.saveAuth({ accessToken: '', refreshToken: '', lastError: 'Twitch-Anmeldung abgelaufen. Bitte neu verbinden.' });
        this.#closeEvents();
      }
      throw new Error('Twitch-Anmeldung abgelaufen. Bitte in den Einstellungen neu verbinden.');
    }
    await this.#storeTokens(d);
    this.log.debug('Twitch', 'Zugang erneuert');
  }

  disconnect() {
    const a = this.auth();
    if (a.accessToken) fetch(`${ID}/revoke`, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: form({ client_id: a.clientId, token: a.accessToken }) }).catch(() => {});
    this.#closeEvents();
    this.saveAuth({ accessToken: '', refreshToken: '', expiresAt: 0, userId: '', login: '', displayName: '', lastError: null });
    this.log.info('Twitch', 'Verbindung getrennt');
  }

  // ---------- Helix-API ----------

  async api(method, path, body, retry = true) {
    const a = this.auth();
    if (!a.accessToken) throw new Error('Nicht mit Twitch verbunden (Einstellungen → Integrationen).');
    if (a.expiresAt && a.expiresAt - Date.now() < 60000) await this.#refresh();
    const res = await fetch(`${HELIX}${path}`, {
      method,
      headers: { 'Client-Id': a.clientId, Authorization: `Bearer ${this.auth().accessToken}`, ...(body && { 'Content-Type': 'application/json' }) },
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(10000),
    });
    if (res.status === 401 && retry) {
      await this.#refresh();
      return this.api(method, path, body, false);
    }
    if (res.status === 204) return {};
    const d = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(`Twitch: ${d.message ?? res.statusText}`);
    return d;
  }

  get me() {
    const id = this.auth().userId;
    if (!id) throw new Error('Nicht mit Twitch verbunden (Einstellungen → Integrationen).');
    return id;
  }

  async userId(login) {
    const name = String(login ?? '').trim().replace(/^@/, '').toLowerCase();
    if (!name) throw new Error('Kein Twitch-Name angegeben.');
    const d = await this.api('GET', `/users?login=${encodeURIComponent(name)}`);
    if (!d.data.length) throw new Error(`Twitch-Nutzer „${name}“ nicht gefunden.`);
    return d.data[0].id;
  }

  /** Kurz zwischengespeicherte Abfragen (Live-Anzeigen fragen sonst alle 2 s). */
  async cached(key, ms, fn) {
    const hit = this.cache.get(key);
    if (hit && Date.now() - hit.at < ms) return hit.value;
    const value = await fn();
    this.cache.set(key, { at: Date.now(), value });
    return value;
  }

  stream() {
    return this.cached('stream', 30000, async () => (await this.api('GET', `/streams?user_id=${this.me}`)).data[0] ?? null);
  }

  followers() {
    return this.cached('followers', 60000, async () => (await this.api('GET', `/channels/followers?broadcaster_id=${this.me}&first=1`)).total);
  }

  // ---------- EventSub ----------

  onEvent(fn) {
    this.listeners.add(fn);
  }

  /** Abonniert genau die Ereignisse, die aktive Events brauchen. */
  setWanted(triggerTypes) {
    const next = new Set(triggerTypes.filter((t) => EVENTSUB_TYPES[t]));
    const same = next.size === this.wanted.size && [...next].every((t) => this.wanted.has(t));
    this.wanted = next;
    if (!same) this.syncEvents();
  }

  syncEvents() {
    this.#closeEvents();
    if (!this.wanted.size || !this.connected) return;
    this.#openEvents(EVENTSUB);
  }

  #openEvents(url) {
    const ws = new WebSocket(url);
    this.ws = ws;
    ws.on('message', (raw) => this.#onEventSub(ws, JSON.parse(raw.toString())).catch((err) => this.log.warn('Twitch', `Ereignisse: ${err.message}`)));
    ws.on('close', () => {
      if (this.ws !== ws) return;
      this.sessionId = null;
      this.ws = null;
      // Verbindung verloren: nach 10 s neu verbinden.
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = setTimeout(() => this.syncEvents(), 10000);
    });
    ws.on('error', () => {});
  }

  #closeEvents() {
    clearTimeout(this.reconnectTimer);
    const ws = this.ws;
    this.ws = null;
    this.sessionId = null;
    ws?.close();
  }

  async #onEventSub(ws, msg) {
    const type = msg.metadata?.message_type;
    if (type === 'session_welcome') {
      this.sessionId = msg.payload.session.id;
      const user = this.me;
      let ok = 0;
      for (const trigger of this.wanted) {
        for (const [subType, version, condition] of EVENTSUB_TYPES[trigger]) {
          try {
            await this.api('POST', '/eventsub/subscriptions', { type: subType, version, condition: condition(user), transport: { method: 'websocket', session_id: this.sessionId } });
            ok++;
          } catch (err) {
            this.log.warn('Twitch', `Ereignis ${subType} nicht verfügbar: ${err.message}`);
          }
        }
      }
      this.log.info('Twitch', `Ereignisse aktiv (${ok} Abos)`);
    } else if (type === 'session_reconnect') {
      // Twitch zieht um: neue Adresse übernehmen, alte Verbindung danach schließen.
      const old = ws;
      this.#openEvents(msg.payload.session.reconnect_url);
      setTimeout(() => old.close(), 5000);
    } else if (type === 'notification') {
      const ev = translate(msg.payload.subscription.type, msg.payload.event);
      if (ev) for (const fn of this.listeners) fn(ev.trigger, ev.data);
    }
  }

  stop() {
    this.device = null;
    this.#closeEvents();
  }
}

/** Twitch-Ereignis → Auslöser von TouchDeck mit deutschen Platzhaltern. */
export function translate(type, e) {
  switch (type) {
    case 'channel.follow':
      return { trigger: 'twitch_follow', data: { Nutzer: e.user_name } };
    case 'channel.subscribe':
      return { trigger: 'twitch_sub', data: { kind: e.is_gift ? 'gift_received' : 'new', Nutzer: e.user_name, Stufe: tier(e.tier), Anzahl: 1 } };
    case 'channel.subscription.gift':
      return { trigger: 'twitch_sub', data: { kind: 'gift', Nutzer: e.is_anonymous ? 'Anonym' : e.user_name, Stufe: tier(e.tier), Anzahl: e.total } };
    case 'channel.subscription.message':
      return { trigger: 'twitch_sub', data: { kind: 'resub', Nutzer: e.user_name, Stufe: tier(e.tier), Monate: e.cumulative_months, Nachricht: e.message?.text ?? '' } };
    case 'channel.cheer':
      return { trigger: 'twitch_cheer', data: { bits: e.bits, Nutzer: e.is_anonymous ? 'Anonym' : e.user_name, Bits: e.bits, Nachricht: e.message } };
    case 'channel.raid':
      return { trigger: 'twitch_raid', data: { viewers: e.viewers, Nutzer: e.from_broadcaster_user_name, Zuschauer: e.viewers } };
    case 'channel.channel_points_custom_reward_redemption.add':
      return { trigger: 'twitch_redeem', data: { reward: e.reward.title, Nutzer: e.user_name, Belohnung: e.reward.title, Kosten: e.reward.cost, Eingabe: e.user_input ?? '' } };
    case 'channel.chat.message':
      return { trigger: 'twitch_chat', data: { text: e.message.text, badges: (e.badges ?? []).map((b) => b.set_id), Nutzer: e.chatter_user_name, Nachricht: e.message.text } };
    case 'stream.online':
      return { trigger: 'twitch_stream', data: { state: 'online', Status: 'online' } };
    case 'stream.offline':
      return { trigger: 'twitch_stream', data: { state: 'offline', Status: 'offline' } };
    case 'channel.ad_break.begin':
      return { trigger: 'twitch_ad', data: { Dauer: e.duration_seconds } };
    default:
      return null;
  }
}

const tier = (t) => ({ 1000: '1', 2000: '2', 3000: '3' })[t] ?? t;
