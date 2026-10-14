import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { TwitchClient, translate } from '../src/integrations/twitch.js';
import { TRIGGER_TYPES } from '../src/core/events.js';
import { runActions } from '../src/core/actions.js';

const realFetch = globalThis.fetch;
afterEach(() => (globalThis.fetch = realFetch));

const json = (status, body) => new Response(body === null ? null : JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
const silentLog = { info() {}, warn() {}, error() {}, debug() {} };

function client(auth) {
  const state = { clientId: 'abc', ...auth };
  const t = new TwitchClient({ auth: () => state, saveAuth: (p) => Object.assign(state, p), log: silentLog });
  return { t, state };
}

test('Anmeldung per Geräte-Code speichert Tokens und Nutzer', async () => {
  const calls = [];
  let polls = 0;
  globalThis.fetch = async (url, opts = {}) => {
    calls.push(String(url));
    if (url.endsWith('/oauth2/device')) return json(200, { device_code: 'dc', user_code: 'ABCD-EFGH', verification_uri: 'https://www.twitch.tv/activate', expires_in: 60, interval: 0.01 });
    if (url.endsWith('/oauth2/token')) {
      polls++;
      return polls < 2 ? json(400, { message: 'authorization_pending' }) : json(200, { access_token: 'AT', refresh_token: 'RT', expires_in: 3600 });
    }
    if (url.endsWith('/oauth2/validate')) return json(200, { user_id: '42', login: 'marcel' });
    if (url.includes('/helix/users?id=42')) return json(200, { data: [{ id: '42', display_name: 'Marcel' }] });
    return json(404, {});
  };
  const { t, state } = client();
  const st = await t.startDeviceLogin();
  assert.equal(st.state, 'waiting');
  assert.equal(st.userCode, 'ABCD-EFGH');
  for (let i = 0; i < 100 && t.status().state !== 'connected'; i++) await new Promise((r) => setTimeout(r, 20));
  assert.equal(t.status().state, 'connected');
  assert.equal(state.accessToken, 'AT');
  assert.equal(state.displayName, 'Marcel');
});

test('Abgelaufener Zugang wird automatisch erneuert; Chat-Aktion sendet mit Platzhaltern', async () => {
  let sent = null;
  let tokenUsed = [];
  globalThis.fetch = async (url, opts = {}) => {
    if (url.endsWith('/oauth2/token')) return json(200, { access_token: 'NEU', refresh_token: 'RT2', expires_in: 3600 });
    if (url.includes('/helix/chat/messages')) {
      tokenUsed.push(opts.headers.Authorization);
      if (opts.headers.Authorization === 'Bearer ALT') return json(401, { message: 'Invalid OAuth token' });
      sent = JSON.parse(opts.body);
      return json(200, { data: [{ is_sent: true }] });
    }
    return json(404, {});
  };
  const { t, state } = client({ accessToken: 'ALT', refreshToken: 'RT', expiresAt: Date.now() + 3600e3, userId: '42' });
  const store = { variables: { Tore: '3' } };
  const r = await runActions([{ type: 'twitch_chat', enabled: true, params: { message: 'Tor Nr. {Tore} von {Event.Nutzer}!' } }], { store, twitch: t, eventData: { 'Event.Nutzer': 'Anna' } });
  assert.equal(r.ok, true, r.error);
  assert.deepEqual(tokenUsed, ['Bearer ALT', 'Bearer NEU']);
  assert.equal(state.accessToken, 'NEU');
  assert.deepEqual(sent, { broadcaster_id: '42', sender_id: '42', message: 'Tor Nr. 3 von Anna!' });
});

test('Ohne Verbindung gibt es eine verständliche Meldung', async () => {
  const { t } = client();
  const r = await runActions([{ type: 'twitch_marker', enabled: true, params: {} }], { store: { variables: {} }, twitch: t });
  assert.match(r.error, /Nicht mit Twitch verbunden/);
});

test('Twitch-Ereignisse werden zu Auslösern mit deutschen Platzhaltern', () => {
  const chat = translate('channel.chat.message', { chatter_user_name: 'anna', message: { text: '!szene spiel' }, badges: [{ set_id: 'moderator' }] });
  const trig = TRIGGER_TYPES.find((x) => x.type === 'twitch_chat');
  assert.equal(trig.match({ mode: 'command', text: '!szene', who: 'mod' }, chat.data), true);
  assert.equal(trig.match({ mode: 'command', text: '!szene', who: 'sub' }, chat.data), false);
  assert.equal(trig.match({ mode: 'command', text: '!sz', who: 'all' }, chat.data), false, 'Befehl muss ganz passen');
  const cheer = translate('channel.cheer', { bits: 100, user_name: 'bob', is_anonymous: false, message: 'hi' });
  assert.equal(TRIGGER_TYPES.find((x) => x.type === 'twitch_cheer').match({ min: 50 }, cheer.data), true);
  assert.equal(cheer.data.Nutzer, 'bob');
  const gift = translate('channel.subscription.gift', { is_anonymous: true, tier: '1000', total: 5 });
  assert.deepEqual([gift.data.Nutzer, gift.data.Anzahl, gift.data.Stufe], ['Anonym', 5, '1']);
});

test('Beim Start mit abgelaufenem Zugang wird erneuert statt „ungültig“ gemeldet', async () => {
  const seen = [];
  globalThis.fetch = async (url, opts = {}) => {
    if (url.endsWith('/oauth2/token')) {
      seen.push('refresh');
      return json(200, { access_token: 'FRISCH', refresh_token: 'RT2', expires_in: 14400 });
    }
    if (url.endsWith('/oauth2/validate')) {
      seen.push(`validate ${opts.headers.Authorization}`);
      return opts.headers.Authorization === 'OAuth FRISCH' ? json(200, { user_id: '42', login: 'marcel' }) : json(401, { message: 'invalid access token' });
    }
    if (url.includes('/helix/users')) return json(200, { data: [{ id: '42', display_name: 'Marcel' }] });
    return json(404, {});
  };
  // Wie heute Nacht: Zugang seit Stunden abgelaufen, Erneuerungsschlüssel noch gültig.
  const { t, state } = client({ accessToken: 'ALT', refreshToken: 'RT', expiresAt: Date.now() - 3 * 3600e3 });
  await t.validate();
  assert.deepEqual(seen, ['refresh', 'validate OAuth FRISCH']);
  assert.equal(state.accessToken, 'FRISCH');
  assert.equal(t.status().state, 'connected');
});

test('Gleichzeitige Anfragen erneuern den Zugang nur einmal', async () => {
  let refreshes = 0;
  globalThis.fetch = async (url, opts = {}) => {
    if (url.endsWith('/oauth2/token')) {
      refreshes++;
      await new Promise((r) => setTimeout(r, 30));
      // Wie bei Twitch: derselbe Erneuerungsschlüssel geht nur einmal.
      return refreshes === 1 ? json(200, { access_token: 'NEU', refresh_token: 'RT2', expires_in: 14400 }) : json(400, { message: 'Invalid refresh token' });
    }
    if (url.includes('/helix/')) return json(200, { data: [] });
    return json(404, {});
  };
  const { t, state } = client({ accessToken: 'ALT', refreshToken: 'RT', expiresAt: Date.now() - 1000, userId: '42' });
  await Promise.all([t.api('GET', '/streams?user_id=42'), t.api('GET', '/channels?broadcaster_id=42'), t.api('GET', '/users?id=42')]);
  assert.equal(refreshes, 1);
  assert.equal(state.accessToken, 'NEU');
  assert.equal(state.refreshToken, 'RT2');
});
