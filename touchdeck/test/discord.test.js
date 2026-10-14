import { test, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import net from 'node:net';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { DiscordClient, sendWebhook } from '../src/integrations/discord.js';
import { runActions } from '../src/core/actions.js';
import { TRIGGER_TYPES } from '../src/core/events.js';

const realFetch = globalThis.fetch;
afterEach(() => (globalThis.fetch = realFetch));
const silentLog = { info() {}, warn() {}, error() {}, debug() {} };

const frame = (op, data) => {
  const json = Buffer.from(JSON.stringify(data));
  const head = Buffer.alloc(8);
  head.writeInt32LE(op, 0);
  head.writeInt32LE(json.length, 4);
  return Buffer.concat([head, json]);
};

/** Minimaler Discord-RPC-Server über einen Unix-Socket, wie ihn das Discord-Programm anbietet. */
function fakeDiscord() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'td-discord-'));
  const sockPath = path.join(dir, 'discord-ipc-0');
  const state = { voice: { mute: false, deaf: false, input: { volume: 100 }, output: { volume: 100 }, mode: { type: 'VOICE_ACTIVITY' } }, calls: [], client: null };
  const server = net.createServer((c) => {
    state.client = c;
    let buf = Buffer.alloc(0);
    c.on('data', (chunk) => {
      buf = Buffer.concat([buf, chunk]);
      while (buf.length >= 8) {
        const op = buf.readInt32LE(0);
        const len = buf.readInt32LE(4);
        if (buf.length < 8 + len) return;
        const msg = JSON.parse(buf.subarray(8, 8 + len));
        buf = buf.subarray(8 + len);
        if (op === 0) {
          if (msg.client_id !== '111111111111111111') return c.write(frame(2, { code: 4000, message: 'Invalid Client ID' }));
          c.write(frame(1, { cmd: 'DISPATCH', evt: 'READY', data: { v: 1 } }));
          continue;
        }
        state.calls.push(msg.cmd);
        const reply = (data) => c.write(frame(1, { cmd: msg.cmd, nonce: msg.nonce, data, evt: null }));
        switch (msg.cmd) {
          case 'AUTHORIZE': reply({ code: 'CODE123' }); break;
          case 'AUTHENTICATE': reply(msg.args.access_token === 'AT' ? { user: { username: 'marcel', global_name: 'Marcel' } } : null); break;
          case 'SUBSCRIBE': reply({ evt: msg.evt }); break;
          case 'GET_VOICE_SETTINGS': reply(state.voice); break;
          case 'GET_SELECTED_VOICE_CHANNEL': reply(null); break;
          case 'SET_VOICE_SETTINGS':
            Object.assign(state.voice, msg.args);
            reply(state.voice);
            c.write(frame(1, { cmd: 'DISPATCH', evt: 'VOICE_SETTINGS_UPDATE', data: state.voice }));
            break;
          default: reply({});
        }
      }
    });
  });
  return new Promise((resolve) => server.listen(sockPath, () => resolve({ server, state, sockPath })));
}

test('Discord: Freigabe, Anmeldung, Stummschalten per Aktion, Event bei Stummschaltung', async () => {
  const { server, state, sockPath } = await fakeDiscord();
  let tokenForm = null;
  globalThis.fetch = async (url, opts) => {
    tokenForm = Object.fromEntries(new URLSearchParams(opts.body));
    return new Response(JSON.stringify({ access_token: 'AT', refresh_token: 'RT', expires_in: 604800 }), { status: 200 });
  };
  const auth = { clientId: '111111111111111111', clientSecret: 'geheim' };
  const discord = new DiscordClient({ auth: () => auth, saveAuth: (p) => Object.assign(auth, p), log: silentLog, paths: () => [sockPath] });
  const events = [];
  discord.onEvent((type, data) => events.push([type, data.state]));
  try {
    await discord.login();
    assert.equal(tokenForm.code, 'CODE123');
    assert.equal(tokenForm.client_secret, 'geheim');
    assert.equal(tokenForm.redirect_uri, 'http://localhost');
    assert.equal(auth.user, 'Marcel');
    assert.equal(discord.status().state, 'connected');

    const r = await runActions([{ type: 'discord_mute', enabled: true, params: { op: 'toggle' } }], { store: { variables: {} }, discord });
    assert.equal(r.ok, true, r.error);
    assert.equal(state.voice.mute, true);
    await new Promise((res) => setTimeout(res, 50));
    assert.deepEqual(events, [['discord_voice', 'muted']]);
    const trig = TRIGGER_TYPES.find((t) => t.type === 'discord_voice');
    assert.equal(trig.match({ state: 'muted' }, { state: 'muted' }), true);
    assert.equal(trig.match({ state: 'deafened' }, { state: 'muted' }), false);
  } finally {
    discord.stop();
    server.close();
  }
});

test('Discord: falsche Client-ID und nicht laufendes Discord geben verständliche Fehler', async () => {
  const { server, sockPath } = await fakeDiscord();
  try {
    const wrong = new DiscordClient({ auth: () => ({ clientId: '999', accessToken: 'AT' }), saveAuth() {}, log: silentLog, paths: () => [sockPath] });
    await assert.rejects(wrong.connect(), /Invalid Client ID/);
    const none = new DiscordClient({ auth: () => ({ clientId: '111111111111111111', accessToken: 'AT' }), saveAuth() {}, log: silentLog, paths: () => ['/tmp/gibt-es-nicht-ipc-0'] });
    await assert.rejects(none.connect(), /Discord läuft nicht/);
  } finally {
    server.close();
  }
});

test('Discord-Webhook prüft die Adresse und sendet die Nachricht', async () => {
  let sent = null;
  globalThis.fetch = async (url, opts) => {
    sent = { url, body: JSON.parse(opts.body) };
    return new Response(null, { status: 204 });
  };
  await assert.rejects(sendWebhook('https://example.org/x', { content: 'hi' }), /Ungültige Webhook-Adresse/);
  await sendWebhook('https://discord.com/api/webhooks/123/abc-DEF_1', { content: 'Stream ist live!', username: 'TouchDeck' });
  assert.deepEqual(sent.body, { content: 'Stream ist live!', username: 'TouchDeck' });
});
