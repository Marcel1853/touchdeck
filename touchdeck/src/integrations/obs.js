// Anbindung an OBS Studio über obs-websocket 5 (ab OBS 28 eingebaut, Werkzeuge → WebSocket-Servereinstellungen).
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import WebSocket from 'ws';

const sha256b64 = (s) => crypto.createHash('sha256').update(s).digest('base64');

export class ObsClient {
  constructor(getSettings) {
    this.getSettings = getSettings;
    this.ws = null;
    this.ready = null;
    this.pending = new Map();
    this.nextId = 1;
    this.lastFailure = null;
    this.eventListeners = new Set();
  }

  /** Meldet OBS-Ereignisse (Szenenwechsel, Stream, Aufnahme …) an fn(eventType, eventData). */
  onEvent(fn) {
    this.eventListeners.add(fn);
    return () => this.eventListeners.delete(fn);
  }

  get connected() {
    return !!this.ws && this.ws.readyState === WebSocket.OPEN;
  }

  /** Verbindung herstellen (oder bestehende wiederverwenden). */
  connect() {
    if (this.ready) return this.ready;
    // Nach einem Fehlschlag 10 s warten, statt OBS bei jeder Live-Abfrage erneut anzufragen.
    if (this.lastFailure && Date.now() - this.lastFailure.at < 10000) return Promise.reject(this.lastFailure.error);
    const { host = '127.0.0.1', port = 4455, password = '' } = this.getSettings() ?? {};
    this.ready = new Promise((resolve, reject) => {
      const ws = new WebSocket(`ws://${host}:${port}`, { handshakeTimeout: 3000 });
      const fail = (msg) => {
        const error = new Error(msg);
        this.reset();
        this.lastFailure = { at: Date.now(), error };
        reject(error);
      };
      const timer = setTimeout(() => fail('OBS antwortet nicht.'), 4000);
      ws.on('error', (err) => {
        clearTimeout(timer);
        fail(err.code === 'ECONNREFUSED'
          ? 'OBS ist nicht erreichbar. Läuft OBS und ist der WebSocket-Server eingeschaltet (Werkzeuge → WebSocket-Servereinstellungen)?'
          : `OBS-Verbindung: ${err.message}`);
      });
      ws.on('close', (code) => {
        clearTimeout(timer);
        if (code === 4009) fail('OBS lehnt das Passwort ab. Bitte in den TouchDeck-Einstellungen prüfen.');
        else this.reset();
      });
      ws.on('message', (data) => {
        const msg = JSON.parse(data.toString());
        if (msg.op === 0) {
          // Alle Ereignisse außer den sehr häufigen (Pegel, aktive Quellen).
          const identify = { rpcVersion: 1, eventSubscriptions: 0x7ff };
          const auth = msg.d.authentication;
          if (auth) {
            if (!password) return fail('OBS verlangt ein Passwort. Bitte in den TouchDeck-Einstellungen eintragen.');
            identify.authentication = sha256b64(sha256b64(password + auth.salt) + auth.challenge);
          }
          ws.send(JSON.stringify({ op: 1, d: identify }));
        } else if (msg.op === 2) {
          clearTimeout(timer);
          this.lastFailure = null;
          resolve(ws);
        } else if (msg.op === 5) {
          for (const fn of this.eventListeners) fn(msg.d.eventType, msg.d.eventData ?? {});
        } else if (msg.op === 7) {
          const p = this.pending.get(msg.d.requestId);
          if (!p) return;
          this.pending.delete(msg.d.requestId);
          const st = msg.d.requestStatus;
          if (st.result) p.resolve(msg.d.responseData ?? {});
          else p.reject(new Error(`OBS: ${st.comment || `Fehler ${st.code}`}`));
        }
      });
      this.ws = ws;
    });
    return this.ready;
  }

  reset() {
    for (const p of this.pending.values()) p.reject(new Error('OBS-Verbindung getrennt.'));
    this.pending.clear();
    this.ws?.terminate();
    this.ws = null;
    this.ready = null;
  }

  async call(requestType, requestData = {}) {
    const ws = await this.connect();
    const requestId = String(this.nextId++);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        reject(new Error(`OBS antwortet nicht auf ${requestType}.`));
      }, 5000);
      this.pending.set(requestId, {
        resolve: (v) => (clearTimeout(timer), resolve(v)),
        reject: (e) => (clearTimeout(timer), reject(e)),
      });
      ws.send(JSON.stringify({ op: 6, d: { requestType, requestId, requestData } }));
    });
  }

  async scenes() {
    const r = await this.call('GetSceneList');
    return r.scenes.map((s) => s.sceneName).reverse();
  }

  async inputs() {
    const r = await this.call('GetInputList');
    return r.inputs.map((i) => i.inputName).sort((a, b) => a.localeCompare(b, 'de'));
  }

  /** Alle Quellen aller Szenen als „Szene › Quelle“. */
  async sceneItems() {
    const result = [];
    for (const scene of await this.scenes()) {
      const r = await this.call('GetSceneItemList', { sceneName: scene });
      for (const item of r.sceneItems) result.push({ value: `${scene}␟${item.sourceName}`, label: `${scene} › ${item.sourceName}` });
    }
    return result;
  }

  async setSourceVisible(target, mode) {
    const [sceneName, sourceName] = String(target).split('␟');
    if (!sourceName) throw new Error('Keine Quelle ausgewählt.');
    const { sceneItemId } = await this.call('GetSceneItemId', { sceneName, sourceName });
    let enabled = mode === 'show';
    if (mode === 'toggle') enabled = !(await this.call('GetSceneItemEnabled', { sceneName, sceneItemId })).sceneItemEnabled;
    await this.call('SetSceneItemEnabled', { sceneName, sceneItemId, sceneItemEnabled: enabled });
    return enabled;
  }

  /** Eingänge einer Art, z. B. /text/ für Textquellen (inputKind wie text_ft2_source_v2). */
  async inputsOfKind(pattern) {
    const r = await this.call('GetInputList');
    return r.inputs.filter((i) => pattern.test(i.inputKind)).map((i) => ({ value: i.inputName, label: i.inputName }));
  }

  /** Alle Filter aller Szenen und Quellen als „Quelle › Filter“. */
  async filters() {
    const sources = [...await this.scenes(), ...await this.inputs()];
    const result = [];
    for (const sourceName of sources) {
      const r = await this.call('GetSourceFilterList', { sourceName });
      for (const f of r.filters) result.push({ value: `${sourceName}\u241f${f.filterName}`, label: `${sourceName} › ${f.filterName}` });
    }
    return result;
  }

  async setFilter(target, mode) {
    const [sourceName, filterName] = String(target).split('\u241f');
    if (!filterName) throw new Error('Kein Filter ausgewählt.');
    let enabled = mode === 'on';
    if (mode === 'toggle') enabled = !(await this.call('GetSourceFilter', { sourceName, filterName })).filterEnabled;
    await this.call('SetSourceFilterEnabled', { sourceName, filterName, filterEnabled: enabled });
    return enabled;
  }

  async listOf(request, key, field) {
    const r = await this.call(request);
    return r[key].map((x) => (typeof x === 'string' ? x : x[field])).map((v) => ({ value: v, label: v }));
  }

  async status() {
    const [stream, record] = await Promise.all([this.call('GetStreamStatus'), this.call('GetRecordStatus')]);
    return { streaming: stream.outputActive, recording: record.outputActive, paused: record.outputPaused };
  }
}

/** Liest Port und Passwort aus der lokalen OBS-Konfiguration (normale Installation oder Flatpak). */
export function readLocalObsConfig() {
  const rel = 'obs-studio/plugin_config/obs-websocket/config.json';
  const candidates = [
    path.join(os.homedir(), '.var/app/com.obsproject.Studio/config', rel),
    path.join(process.env.XDG_CONFIG_HOME || path.join(os.homedir(), '.config'), rel),
  ];
  const file = candidates.find((f) => fs.existsSync(f));
  if (!file) throw new Error('Keine OBS-Konfiguration gefunden. Ist OBS installiert und wurde es schon einmal gestartet?');
  const cfg = JSON.parse(fs.readFileSync(file, 'utf8'));
  return {
    port: cfg.server_port ?? 4455,
    password: cfg.auth_required ? cfg.server_password ?? '' : '',
    serverEnabled: !!cfg.server_enabled,
    flatpak: file.includes('/.var/app/'),
  };
}
