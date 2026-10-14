import { test } from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { WebSocketServer } from 'ws';
import { ObsClient } from '../src/integrations/obs.js';

const sha = (s) => crypto.createHash('sha256').update(s).digest('base64');

/** Minimaler obs-websocket-5-Server mit Passwort, wie OBS ihn spricht. */
function fakeObs(password) {
  const salt = 'salz';
  const challenge = 'aufgabe';
  const state = { scene: 'Start', streaming: false, calls: [] };
  const wss = new WebSocketServer({ port: 0 });
  wss.on('connection', (ws) => {
    ws.send(JSON.stringify({ op: 0, d: { rpcVersion: 1, authentication: { salt, challenge } } }));
    ws.on('message', (raw) => {
      const msg = JSON.parse(raw);
      if (msg.op === 1) {
        if (msg.d.authentication !== sha(sha(password + salt) + challenge)) return ws.close(4009);
        return ws.send(JSON.stringify({ op: 2, d: { negotiatedRpcVersion: 1 } }));
      }
      const { requestType, requestId, requestData } = msg.d;
      state.calls.push(requestType);
      const ok = (responseData) => ws.send(JSON.stringify({ op: 7, d: { requestType, requestId, requestStatus: { result: true, code: 100 }, responseData } }));
      const fail = (comment) => ws.send(JSON.stringify({ op: 7, d: { requestType, requestId, requestStatus: { result: false, code: 600, comment } } }));
      switch (requestType) {
        case 'GetSceneList': return ok({ scenes: [{ sceneName: 'Pause' }, { sceneName: 'Start' }] });
        case 'SetCurrentProgramScene': state.scene = requestData.sceneName; return ok();
        case 'ToggleStream': state.streaming = !state.streaming; return ok({ outputActive: state.streaming });
        case 'GetStreamStatus': return ok({ outputActive: state.streaming });
        case 'GetRecordStatus': return ok({ outputActive: false, outputPaused: false });
        default: return fail('Unbekannt');
      }
    });
  });
  return new Promise((resolve) => wss.on('listening', () => resolve({ wss, state, port: wss.address().port })));
}

test('OBS: Anmeldung mit Passwort, Szenen, Szenenwechsel, Status', async () => {
  const { wss, state, port } = await fakeObs('geheim');
  const obs = new ObsClient(() => ({ host: '127.0.0.1', port, password: 'geheim' }));
  try {
    assert.deepEqual(await obs.scenes(), ['Start', 'Pause']);
    await obs.call('SetCurrentProgramScene', { sceneName: 'Pause' });
    assert.equal(state.scene, 'Pause');
    await obs.call('ToggleStream');
    assert.deepEqual(await obs.status(), { streaming: true, recording: false, paused: false });
    await assert.rejects(obs.call('Quatsch'), /OBS: Unbekannt/);
  } finally {
    obs.reset();
    wss.close();
  }
});

test('OBS: falsches Passwort und nicht laufendes OBS geben verständliche Fehler', async () => {
  const { wss, port } = await fakeObs('geheim');
  try {
    const wrong = new ObsClient(() => ({ host: '127.0.0.1', port, password: 'falsch' }));
    await assert.rejects(wrong.scenes(), /Passwort/);
    const none = new ObsClient(() => ({ host: '127.0.0.1', port: 1, password: '' }));
    await assert.rejects(none.scenes(), /nicht erreichbar/);
  } finally {
    wss.close();
  }
});
