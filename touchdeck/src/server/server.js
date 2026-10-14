// HTTP- und WebSocket-Server: Editor (nur lokal) und Bedienfeld fürs Handy (mit Kopplungsschlüssel und optional PIN).
import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { WebSocketServer } from 'ws';
import QRCode from 'qrcode';
import { describeActionTypes, runActions, OPTION_SOURCES } from '../core/actions.js';
import { EventEngine, describeTriggerTypes } from '../core/events.js';
import { collectLive, applySlider, LIVE_SOURCES, SLIDER_TARGETS } from '../core/live.js';
import { Log } from '../core/log.js';
import { Backups } from '../core/backup.js';
import { newId } from '../core/store.js';
import { ObsClient, readLocalObsConfig } from '../integrations/obs.js';
import { buildSteamPages } from '../integrations/steam-pages.js';
import { importTouchPortal, DEFAULT_TP_DIR } from '../integrations/touchportal.js';
import { TwitchClient } from '../integrations/twitch.js';
import { DiscordClient } from '../integrations/discord.js';
import { lanAddresses, listApps, firewallActive, openFirewall } from '../system/linux.js';
import { send, readJson, serveFile, isLoopback, safeEqual, httpError } from './http.js';
import { startDiscovery } from './discovery.js';

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const PUBLIC = path.join(ROOT, 'public');
const VERSION = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8')).version;
const APK = path.join(ROOT, 'downloads', 'TouchDeck.apk');
const ICON_TYPES = { 'image/png': '.png', 'image/jpeg': '.jpg', 'image/gif': '.gif', 'image/webp': '.webp', 'image/svg+xml': '.svg' };

export function createServer(store, { port, host = '0.0.0.0', onPairRequest = () => {} }) {
  const log = new Log(path.join(store.dataDir, 'logs'), () => store.settings.logs.level);
  const obs = new ObsClient(() => store.settings.obs);
  const backups = new Backups(store, log);
  const twitch = new TwitchClient({ auth: () => store.settings.twitch, saveAuth: (p) => store.setTwitchAuth(p), log });
  const discord = new DiscordClient({ auth: () => store.settings.discord, saveAuth: (p) => store.setDiscordAuth(p), log });
  const startedAt = Date.now();
  let liveValues = {};

  /** Aktionen ausführen und Fehler protokollieren. extra: { buttonId, eventData, source }. */
  const run = async (actions, { source = '', ...extra } = {}) => {
    const result = await runActions(actions, { store, obs, twitch, discord, ...extra });
    if (!result.ok) log.error('Aktion', `${source ? `${source}: ` : ''}${result.error}`);
    return result;
  };
  const events = new EventEngine({ store, obs, twitch, discord, log, run });

  // ---------- Zugriff ----------

  /** 'local' (dieser PC), 'device' (gekoppeltes Handy), 'pin' (Schlüssel stimmt, PIN fehlt) oder null. */
  const auth = (req, url) => {
    if (isLoopback(req)) return 'local';
    if (!store.settings.lanEnabled) return null;
    const token = url.searchParams.get('token') ?? req.headers['x-token'];
    if (!safeEqual(token, store.settings.token)) return null;
    const pin = store.settings.pin;
    if (pin && !safeEqual(url.searchParams.get('pin') ?? req.headers['x-pin'], pin)) return 'pin';
    return 'device';
  };

  const deckState = () => ({
    version: VERSION,
    revision: store.revision,
    pages: store.pages,
    toggles: store.toggles,
    variables: store.variables,
    overrides: store.overrides,
    live: liveValues,
    startPage: store.settings.startPage,
    deck: store.settings.deck,
  });

  const editorState = () => ({
    ...deckState(),
    flows: store.flows,
    events: store.events,
    values: store.values,
    settings: store.publicSettings(),
    actionTypes: describeActionTypes(),
    triggerTypes: describeTriggerTypes(),
    liveSources: LIVE_SOURCES,
    sliderTargets: SLIDER_TARGETS,
    pairingUrls: pairingUrls(),
    version: VERSION,
    port,
  });

  const pairingUrls = () => lanAddresses().map((ip) => `http://${ip}:${port}/deck/?token=${store.settings.token}`);

  // ---------- Buttons ----------

  async function press(pageId, buttonId) {
    const page = store.page(pageId);
    const button = page?.buttons.find((b) => b.id === buttonId);
    if (!button) return { ok: false, error: 'Button nicht gefunden.' };
    let actions = button.actions;
    if (button.mode === 'toggle') {
      const on = !store.toggles[button.id];
      actions = on ? button.actions : button.actionsOff;
      store.setToggle(button.id, on);
      events.fire('button_toggle', { buttonId: button.id, state: on ? 'on' : 'off' });
    }
    const label = `${page.name} › ${button.title || button.icon.value || button.id}`;
    const result = await run(actions, { buttonId, source: label });
    if (result.ok) log.info('Button', `${label} gedrückt`);
    return result;
  }

  // Gedrückthalten: Aktionen beim langen Drücken, auf Wunsch wiederholt, und Aktionen beim Loslassen.
  const holds = new Map();
  function holdStart(ws, pageId, buttonId) {
    const button = store.page(pageId)?.buttons.find((b) => b.id === buttonId);
    if (!button?.holdActions.length) return;
    const key = `${ws.id}:${buttonId}`;
    holdEnd(ws, buttonId, false);
    const once = () => run(button.holdActions, { buttonId, source: `Halten: ${button.title || buttonId}` }).then((r) => ws.send(JSON.stringify({ type: 'result', buttonId, ...r })));
    once();
    const timer = button.holdRepeat > 0 ? setInterval(once, Math.max(50, button.holdRepeat)) : null;
    holds.set(key, { timer, button });
  }
  function holdEnd(ws, buttonId, release = true) {
    const key = `${ws.id}:${buttonId}`;
    const h = holds.get(key);
    if (!h) return;
    clearInterval(h.timer);
    holds.delete(key);
    if (release && h.button.releaseActions.length) run(h.button.releaseActions, { buttonId, source: `Loslassen: ${h.button.title || buttonId}` });
  }

  // ---------- Koppeln per Bestätigung am PC ----------

  // Die App fragt an, am PC wird bestätigt, erst dann bekommt die App den Schlüssel.
  const pairings = new Map();
  const PAIR_TIMEOUT = 2 * 60 * 1000;
  function pairRequest(device, ip) {
    for (const [id, p] of pairings) if (Date.now() - p.created > PAIR_TIMEOUT || p.ip === ip) pairings.delete(id);
    if (pairings.size >= 5) throw httpError(429, 'Zu viele offene Anfragen. Bitte kurz warten.');
    const id = newId('pr_');
    const name = String(device ?? 'Handy').replace(/[^\p{L}\p{N} ._()-]/gu, '').slice(0, 60) || 'Handy';
    pairings.set(id, { device: name, ip, created: Date.now(), state: 'pending' });
    log.info('Gerät', `${name} (${ip}) möchte sich verbinden`);
    const payload = JSON.stringify({ type: 'pair-request', id, device: name, ip });
    for (const ws of wss.clients) if (ws.role === 'editor') ws.send(payload);
    onPairRequest({ id, device: name, ip });
    return { id, expiresIn: PAIR_TIMEOUT / 1000 };
  }
  function pairStatus(id) {
    const p = pairings.get(id);
    if (!p || Date.now() - p.created > PAIR_TIMEOUT) return { state: 'expired' };
    if (p.state === 'approved') {
      pairings.delete(id);
      return { state: 'approved', token: store.settings.token, pin: !!store.settings.pin };
    }
    return { state: p.state };
  }

  // ---------- HTTP-Routen ----------

  const routes = {
    'GET /api/state': ({ who }) => (who === 'local' ? editorState() : deckState()),
    'PUT /api/pages': ({ body }) => {
      if (body.revision !== undefined && body.revision !== store.revision) {
        throw httpError(409, 'Die Seiten wurden inzwischen in einem anderen Fenster geändert.');
      }
      store.setProject(body);
      return { ok: true, revision: store.revision };
    },
    'PUT /api/settings': ({ body }) => {
      store.setSettings(body);
      if (body.obs) {
        obs.reset();
        obs.lastFailure = null;
      }
      return { ok: true, settings: store.publicSettings(), pairingUrls: pairingUrls() };
    },
    'POST /api/press': ({ body }) => press(body.pageId, body.buttonId),
    'POST /api/test-actions': ({ body }) => run(Array.isArray(body.actions) ? body.actions : [], { buttonId: body.buttonId, source: 'Test' }),

    // Werte
    'PUT /api/variables': ({ body }) => {
      store.setVariable(body.name, body.value ?? '');
      return { ok: true, variables: store.variables };
    },
    'POST /api/variables/delete': ({ body }) => {
      store.deleteVariable(body.name);
      return { ok: true, variables: store.variables };
    },
    'POST /api/overrides/reset': () => {
      store.resetOverrides();
      return { ok: true };
    },

    // Logs
    'GET /api/logs': () => log.entries,
    'POST /api/logs/clear': () => {
      log.clear();
      return { ok: true };
    },

    // Backups
    'GET /api/backups': () => backups.list(),
    'POST /api/backups': ({ body }) => ({ ok: true, name: backups.create(body.note || 'manuell'), list: backups.list() }),
    'POST /api/backups/restore': ({ body }) => {
      backups.restore(body.config ?? body.name);
      return { ok: true, list: backups.list() };
    },
    'POST /api/backups/delete': ({ body }) => {
      backups.remove(body.name);
      return { ok: true, list: backups.list() };
    },

    // Information
    'GET /api/info': () => ({
      version: VERSION,
      node: process.versions.node,
      electron: process.versions.electron ?? null,
      platform: `${os.type()} ${os.release()}`,
      host: os.hostname(),
      dataDir: store.dataDir,
      uptime: Math.round((Date.now() - startedAt) / 1000),
      lan: lanAddresses(),
      port,
      devices: [...wss.clients].filter((ws) => ws.role === 'device').length,
      obsConnected: obs.connected,
      twitch: twitch.status(),
      discord: discord.status(),
      counts: {
        pages: store.pages.length,
        buttons: store.pages.reduce((n, p) => n + p.buttons.length, 0),
        flows: store.flows.length,
        events: store.events.length,
        values: Object.keys(store.variables).length,
      },
    }),

    // Integrationen
    'POST /api/steam/pages': () => {
      const existing = store.pages.filter((p) => p.template === 'steam');
      const pages = buildSteamPages(store.iconDir, existing.map((p) => p.id));
      store.replaceTemplatePages('steam', pages);
      return { ok: true, pages: store.pages, revision: store.revision, created: pages.map((p) => p.id), updated: existing.length > 0 };
    },
    'POST /api/obs/autoconfig': () => {
      const local = readLocalObsConfig();
      store.setSettings({ obs: { host: '127.0.0.1', port: local.port, password: local.password } });
      obs.reset();
      obs.lastFailure = null;
      return { ok: true, settings: store.publicSettings(), serverEnabled: local.serverEnabled, flatpak: local.flatpak };
    },
    'POST /api/obs/test': async ({ body }) => {
      // Mit den eingegebenen Daten testen, ohne sie vorher zu speichern.
      const probe = new ObsClient(() => ({ ...store.settings.obs, ...body }));
      try {
        const scenes = await probe.scenes();
        const { obsVersion } = await probe.call('GetVersion');
        return { ok: true, version: obsVersion, scenes: scenes.length };
      } finally {
        probe.reset();
      }
    },
    'GET /api/hello': () => ({ app: 'touchdeck', name: os.hostname(), version: VERSION, port }),
    'POST /api/firewall/open': async () => {
      await openFirewall(port);
      store.setSettings({ firewallOpened: true });
      log.info('Server', 'Firewall für das Heimnetz freigegeben');
      return { ok: true };
    },
    'POST /api/pair/request': ({ body, req }) => pairRequest(body.device, req.socket.remoteAddress.replace('::ffff:', '')),
    'GET /api/pair/status': ({ url }) => pairStatus(url.searchParams.get('id')),
    'POST /api/pair/answer': ({ body }) => {
      const p = pairings.get(body.id);
      if (!p) throw httpError(404, 'Die Anfrage ist abgelaufen.');
      p.state = body.allow ? 'approved' : 'denied';
      log.info('Gerät', `${p.device}: ${body.allow ? 'erlaubt' : 'abgelehnt'}`);
      return { ok: true };
    },
    'GET /api/discord/status': () => discord.status(),
    'POST /api/discord/connect': async ({ body }) => {
      store.setSettings({ discord: { clientId: body.clientId ?? '', clientSecret: body.clientSecret ?? '' } });
      await discord.login();
      events.reload();
      return discord.status();
    },
    'POST /api/discord/disconnect': () => {
      discord.disconnect();
      return discord.status();
    },
    'GET /api/twitch/status': () => twitch.status(),
    'POST /api/twitch/connect': async ({ body }) => {
      if (typeof body.clientId === 'string') store.setSettings({ twitch: { clientId: body.clientId } });
      return twitch.startDeviceLogin();
    },
    'POST /api/twitch/cancel': () => {
      twitch.cancelLogin();
      return twitch.status();
    },
    'POST /api/twitch/disconnect': () => {
      twitch.disconnect();
      return twitch.status();
    },
    'GET /api/apps': () => listApps(),
    'GET /api/pairing': async () => {
      const urls = pairingUrls();
      const qr = urls.length ? await QRCode.toString(urls[0], { type: 'svg', margin: 1, color: { dark: '#0d0f14', light: '#ffffff' } }) : null;
      const apkUrl = fs.existsSync(APK) && lanAddresses().length ? `http://${lanAddresses()[0]}:${port}/app/TouchDeck.apk` : null;
      const apkQr = apkUrl ? await QRCode.toString(apkUrl, { type: 'svg', margin: 1, color: { dark: '#0d0f14', light: '#ffffff' } }) : null;
      const ip = lanAddresses()[0];
      const subnet = ip ? `${ip.split('.').slice(0, 3).join('.')}.0/24` : null;
      const firewall = !store.settings.firewallOpened && (await firewallActive()) && subnet
        ? `sudo ufw allow from ${subnet} to any port ${port} proto tcp && sudo ufw allow from ${subnet} to any port 8769 proto udp`
        : null;
      return { urls, qr, lanEnabled: store.settings.lanEnabled, pin: !!store.settings.pin, apkUrl, apkQr, firewall };
    },

    // Bilder
    'GET /api/icons': () => fs.readdirSync(store.iconDir).filter((f) => !f.startsWith('.')).sort(),
    'POST /api/icons': ({ body }) => {
      const m = /^data:([\w/+.-]+);base64,(.+)$/s.exec(body.data ?? '');
      const ext = m && ICON_TYPES[m[1]];
      if (!ext) throw httpError(400, 'Nur PNG, JPG, GIF, WebP oder SVG.');
      const name = `${newId('i_')}${ext}`;
      fs.writeFileSync(path.join(store.iconDir, name), Buffer.from(m[2], 'base64'));
      return { name };
    },
    'POST /api/import/touchportal': ({ body }) => {
      const existing = store.pages.filter((p) => p.template === 'touchportal');
      const { pages, stats } = importTouchPortal(body.dir || DEFAULT_TP_DIR, store.iconDir, new Map(existing.map((p) => [p.name, p.id])));
      if (pages.length === 0) throw httpError(404, 'Keine Seiten gefunden.');
      store.replaceTemplatePages('touchportal', pages);
      return { ok: true, stats, pages: store.pages, revision: store.revision, updated: existing.length > 0 };
    },
  };
  // Diese Routen darf auch ein gekoppeltes Handy aufrufen, alles andere nur der lokale Editor.
  const DEVICE_ROUTES = new Set(['GET /api/state', 'POST /api/press']);
  // Zum Koppeln braucht ein Handy noch keinen Schlüssel (nur WLAN-Zugriff muss erlaubt sein).
  const PAIR_ROUTES = new Set(['GET /api/hello', 'POST /api/pair/request', 'GET /api/pair/status']);

  const server = http.createServer(async (req, res) => {
    const url = new URL(req.url, 'http://localhost');
    const who = auth(req, url);

    if (req.method === 'GET' && url.pathname.startsWith('/api/options/')) {
      if (who !== 'local') return send(res, 401, { error: 'Nicht gekoppelt' });
      const name = url.pathname.slice('/api/options/'.length);
      const source = Object.hasOwn(OPTION_SOURCES, name) ? OPTION_SOURCES[name] : null;
      if (!source) return send(res, 404, { error: 'Unbekannte Liste' });
      try {
        return send(res, 200, await source({ store, obs, twitch, discord }));
      } catch (err) {
        return send(res, 502, { error: err.message });
      }
    }

    if (url.pathname.startsWith('/api/')) {
      const key = `${req.method} ${url.pathname}`;
      const handler = routes[key];
      if (!handler) return send(res, 404, { error: 'Unbekannter Aufruf' });
      const pairing = PAIR_ROUTES.has(key) && (isLoopback(req) || store.settings.lanEnabled);
      if (!pairing && who === 'pin') return send(res, 401, { error: 'PIN erforderlich', pin: true });
      if (!pairing && (!who || (who === 'device' && !DEVICE_ROUTES.has(key)))) return send(res, 401, { error: 'Nicht gekoppelt' });
      // Schutz gegen fremde Webseiten: Schreibzugriffe brauchen einen eigenen Header (erzwingt CORS-Prüfung).
      if (req.method !== 'GET' && req.headers['x-touchdeck'] !== '1') return send(res, 403, { error: 'Fehlender Header' });
      try {
        const body = req.method === 'GET' ? {} : await readJson(req);
        return send(res, 200, await handler({ body, who, req, url }));
      } catch (err) {
        if (!err.status) log.error('Server', `${key}: ${err.message}`);
        return send(res, err.status ?? 500, { error: err.message });
      }
    }

    if (url.pathname === '/') {
      res.writeHead(302, { Location: who === 'local' ? '/editor/' : `/deck/${url.search}` });
      return res.end();
    }
    if (url.pathname.startsWith('/editor/')) {
      if (who !== 'local') return send(res, 403, 'Der Editor ist nur auf diesem PC erreichbar.');
      return serveFile(res, PUBLIC, url.pathname.endsWith('/') ? `${url.pathname}index.html` : url.pathname);
    }
    if (url.pathname.startsWith('/deck/') || url.pathname.startsWith('/shared/')) {
      return serveFile(res, PUBLIC, url.pathname.endsWith('/') ? `${url.pathname}index.html` : url.pathname);
    }
    if (url.pathname.startsWith('/icons/')) {
      // SVGs dürfen hier keine Skripte ausführen.
      return serveFile(res, store.iconDir, decodeURIComponent(url.pathname.slice('/icons/'.length)), { 'Content-Security-Policy': "default-src 'none'; style-src 'unsafe-inline'; sandbox" });
    }
    if (url.pathname === '/app/TouchDeck.apk' && (who || store.settings.lanEnabled)) {
      return serveFile(res, path.dirname(APK), path.basename(APK), { 'Content-Type': 'application/vnd.android.package-archive', 'Content-Disposition': 'attachment; filename="TouchDeck.apk"' });
    }
    if (url.pathname === '/backups/download' && who === 'local') {
      return send(res, 200, JSON.stringify(backups.read(url.searchParams.get('name')), null, 2), {
        'Content-Type': 'application/json; charset=utf-8',
        'Content-Disposition': `attachment; filename="${url.searchParams.get('name')}"`,
      });
    }
    return send(res, 404, 'Nicht gefunden');
  });

  // ---------- WebSocket: Bedienfelder und Editor ----------

  const wss = new WebSocketServer({ noServer: true, maxPayload: 64 * 1024 });
  let wsSeq = 0;
  server.on('upgrade', (req, socket, head) => {
    const url = new URL(req.url, 'http://localhost');
    const who = url.pathname === '/ws' ? auth(req, url) : null;
    if (!who || who === 'pin') {
      socket.write('HTTP/1.1 401 Unauthorized\r\n\r\n');
      return socket.destroy();
    }
    wss.handleUpgrade(req, socket, head, (ws) => {
      ws.id = ++wsSeq;
      // Der Editor meldet sich mit role=editor; alles andere ist ein Bedienfeld.
      ws.role = who === 'local' && url.searchParams.get('role') === 'editor' ? 'editor' : 'device';
      wss.emit('connection', ws, req);
    });
  });

  wss.on('connection', (ws, req) => {
    const name = req.socket.remoteAddress.replace('::ffff:', '');
    if (ws.role === 'device') {
      log.info('Gerät', `verbunden: ${name}`);
      events.fire('device', { state: 'connected', Gerät: name });
    }
    ws.isAlive = true;
    ws.on('pong', () => (ws.isAlive = true));
    ws.send(JSON.stringify({ type: 'state', ...deckState() }));
    ws.on('message', async (data) => {
      let msg;
      try {
        msg = JSON.parse(data.toString());
      } catch {
        return;
      }
      if (msg.type === 'press') {
        const result = await press(msg.pageId, msg.buttonId);
        ws.send(JSON.stringify({ type: 'result', reqId: msg.reqId, buttonId: msg.buttonId, ...result }));
      } else if (msg.type === 'hold') {
        if (msg.phase === 'start') holdStart(ws, msg.pageId, msg.buttonId);
        else holdEnd(ws, msg.buttonId);
      } else if (msg.type === 'slide') {
        slide(msg.pageId, msg.buttonId, msg.value, ws);
      } else if (msg.type === 'view' && typeof msg.pageId === 'string') {
        ws.viewing = msg.pageId;
        refreshLive();
      }
    });
    ws.on('close', () => {
      for (const key of [...holds.keys()]) if (key.startsWith(`${ws.id}:`)) holdEnd(ws, key.split(':')[1], false);
      if (ws.role === 'device') {
        log.info('Gerät', `getrennt: ${name}`);
        events.fire('device', { state: 'disconnected', Gerät: name });
      }
    });
  });

  // Neue Log-Einträge sofort an offene Editoren schicken.
  log.onEntry((entry) => {
    const payload = JSON.stringify({ type: 'log', entry });
    for (const ws of wss.clients) if (ws.role === 'editor') ws.send(payload);
  });

  // Regler: immer nur den neuesten Wert anwenden, während ein Befehl noch läuft.
  const sliderQueue = new Map();
  function slide(pageId, buttonId, value, ws) {
    const button = store.page(pageId)?.buttons.find((b) => b.id === buttonId && b.mode === 'slider');
    if (!button || !Number.isFinite(Number(value))) return;
    const q = sliderQueue.get(buttonId) ?? { busy: false, next: null };
    sliderQueue.set(buttonId, q);
    q.next = Number(value);
    if (q.busy) return;
    const pump = async () => {
      q.busy = true;
      while (q.next !== null) {
        const v = q.next;
        q.next = null;
        try {
          await applySlider(button, v, obs);
          liveValues = { ...liveValues, [buttonId]: { ...liveValues[buttonId], value: Math.round(v) } };
        } catch (err) {
          ws.send(JSON.stringify({ type: 'result', buttonId, ok: false, error: err.message }));
        }
      }
      q.busy = false;
    };
    pump();
  }

  // Live-Werte alle 2 Sekunden aktualisieren – nur für Seiten, die ein Bedienfeld gerade anzeigt.
  // Wechselt ein Handy die Seite, während noch eine Abfrage läuft, folgt direkt danach eine weitere.
  let liveBusy = false;
  let liveAgain = false;
  async function refreshLive() {
    const viewed = new Set([...wss.clients].map((ws) => ws.viewing).filter(Boolean));
    if (viewed.size === 0) return;
    if (liveBusy) {
      liveAgain = true;
      return;
    }
    liveBusy = true;
    try {
      const next = { ...liveValues, ...await collectLive(store.pages.filter((p) => viewed.has(p.id)), obs, store.variables, twitch, discord) };
      if (JSON.stringify(next) !== JSON.stringify(liveValues)) {
        liveValues = next;
        const payload = JSON.stringify({ type: 'live', values: next });
        for (const ws of wss.clients) if (ws.viewing) ws.send(payload);
      }
    } finally {
      liveBusy = false;
      if (liveAgain) {
        liveAgain = false;
        refreshLive();
      }
    }
  }
  const livePoll = setInterval(refreshLive, 2000);

  const heartbeat = setInterval(() => {
    for (const ws of wss.clients) {
      if (!ws.isAlive) {
        ws.terminate();
        continue;
      }
      ws.isAlive = false;
      ws.ping();
    }
  }, 15000);

  // Einmal am Tag automatisch sichern (beim Start und danach stündlich prüfen).
  const backupTimer = setInterval(() => backups.autoBackup(), 60 * 60 * 1000);
  const stopDiscovery = port ? startDiscovery({ port, version: VERSION, enabled: () => store.settings.lanEnabled, log }) : () => {};

  store.onChange((what) => {
    if (what === 'security') {
      // Neuer Schlüssel, PIN geändert oder WLAN aus: gekoppelte Geräte trennen, damit sie sich neu anmelden.
      for (const ws of wss.clients) if (ws.role === 'device') ws.close(4001, 'Einstellungen geändert');
      return;
    }
    const payload = JSON.stringify({ type: 'state', ...deckState() });
    for (const ws of wss.clients) ws.send(payload);
  });

  server.on('close', () => {
    clearInterval(heartbeat);
    clearInterval(livePoll);
    clearInterval(backupTimer);
    for (const h of holds.values()) clearInterval(h.timer);
    events.stop();
    stopDiscovery();
    twitch.stop();
    discord.stop();
    obs.reset();
  });
  server.touchdeck = { log, events, backups };

  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, host, () => {
      log.info('Server', `TouchDeck ${VERSION} läuft auf Port ${server.address().port}`);
      try {
        backups.autoBackup();
      } catch (err) {
        log.warn('Backup', err.message);
      }
      events.start();
      if (twitch.connected) twitch.validate().then(() => twitch.syncEvents()).catch((err) => log.warn('Twitch', err.message));
      resolve(server);
    });
  });
}
