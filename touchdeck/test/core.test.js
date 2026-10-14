import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Store, sanitizePage } from '../src/core/store.js';
import { importTouchPortal } from '../src/integrations/touchportal.js';
import { createServer } from '../src/server/server.js';

const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'touchdeck-test-'));

test('sanitizePage verwirft überlappende und herausragende Buttons', () => {
  const page = sanitizePage({
    name: 'x', rows: 2, cols: 2,
    buttons: [
      { id: 'a', row: 0, col: 0, colSpan: 2 },
      { id: 'b', row: 0, col: 1 },
      { id: 'c', row: 5, col: 0 },
      { id: 'd', row: 1, col: 1, colSpan: 9 },
    ],
  });
  assert.deepEqual(page.buttons.map((b) => b.id), ['a', 'd']);
  assert.equal(page.buttons[1].colSpan, 1);
});

test('Store legt Beispieldaten an und speichert Änderungen', () => {
  const dir = tmp();
  const store = new Store(dir);
  assert.equal(store.pages.length, 1);
  store.setPages([...store.pages, { name: 'Zwei', rows: 1, cols: 1, buttons: [] }]);
  const again = new Store(dir);
  assert.deepEqual(again.pages.map((p) => p.name), ['Start', 'Zwei']);
  assert.throws(() => again.setPages([]), /Mindestens eine Seite/);
});

test('Touch-Portal-Import übernimmt Raster, Farben, Bilder und Aktionen', () => {
  const tp = tmp();
  fs.mkdirSync(path.join(tp, 'pages'));
  fs.mkdirSync(path.join(tp, 'icons'));
  fs.writeFileSync(path.join(tp, 'icons', 'x.png'), 'bild');
  const btn = (extra) => ({ id: 'u1', T: 'Hi', BG: -13684944, TC: -1, I: 'x.png', A: [], ...extra });
  fs.writeFileSync(path.join(tp, 'pages', '(main).tml'), JSON.stringify({
    KEY_COLUMNS: 3, KEY_ROWS: 2, BG: -14145496,
    BUTTONS: [
      [btn({ A: [{ KEY_TYPE: 'OPEN_URL_ACTION', KEY_URL: 'https://example.org' }] }), {}],
      [{}, btn({ A: [{ KEY_TYPE: 'KEY_PRESS_ACTION', KEY_KEYCODES: [17, 18, 84] }, { KEY_TYPE: 'GOTO_PAGE_ACTION', KEY_PAGE_NAME: 'Zwei' }] })],
      [btn({ A: [{ KEY_TYPE: 'OBS_SOMETHING_ACTION' }] }), {}],
    ],
  }));
  fs.writeFileSync(path.join(tp, 'pages', 'Zwei.tml'), JSON.stringify({ KEY_COLUMNS: 1, KEY_ROWS: 1, BUTTONS: [[{}]] }));

  const iconDir = tmp();
  const { pages, stats } = importTouchPortal(tp, iconDir);
  const main = pages.find((p) => p.name.startsWith('Hauptseite'));
  const zwei = pages.find((p) => p.name === 'Zwei');
  assert.equal(main.cols, 3);
  assert.equal(main.background, '#282828');
  const at = (r, c) => main.buttons.find((b) => b.row === r && b.col === c);
  assert.equal(at(0, 0).actions[0].params.url, 'https://example.org');
  assert.equal(at(0, 0).color, '#2f2f30');
  assert.equal(at(0, 0).icon.value, 'tp_x.png');
  assert.equal(at(1, 1).actions[0].params.keys, 'ctrl+alt+t');
  assert.equal(at(1, 1).actions[1].params.page, zwei.id);
  assert.equal(at(0, 2).actions[0].type, 'unsupported');
  assert.deepEqual(stats.unsupported, { OBS_SOMETHING_ACTION: 1 });
  assert.ok(fs.existsSync(path.join(iconDir, 'tp_x.png')));
});

test('Server: Editor-API nur lokal mit Header, Handy nur mit Schlüssel', async () => {
  const store = new Store(tmp());
  const server = await createServer(store, { port: 0, host: '127.0.0.1', log: () => {} });
  const base = `http://127.0.0.1:${server.address().port}`;
  try {
    assert.equal((await fetch(`${base}/api/state`)).status, 200);
    const noHeader = await fetch(`${base}/api/pages`, { method: 'PUT', body: '{}' });
    assert.equal(noHeader.status, 403);
    const press = await fetch(`${base}/api/press`, { method: 'POST', headers: { 'X-TouchDeck': '1' }, body: JSON.stringify({ pageId: 'nope', buttonId: 'nope' }) });
    assert.equal((await press.json()).ok, false);
    const traversal = await fetch(`${base}/icons/..%2Fconfig.json`);
    assert.notEqual(traversal.status, 200);
  } finally {
    server.closeAllConnections();
    server.close();
  }
});

test('Vorlagen ersetzen ihre Seiten an Ort und Stelle statt sie zu verdoppeln', () => {
  const store = new Store(tmp());
  const startId = store.pages[0].id;
  store.addPages([{ id: 'p_s1', name: 'Steam-Spiele', template: 'steam', rows: 1, cols: 1 }, { name: 'Danach', rows: 1, cols: 1 }]);
  store.replaceTemplatePages('steam', [{ id: 'p_s1', name: 'Steam-Spiele', template: 'steam', rows: 2, cols: 2 }]);
  assert.deepEqual(store.pages.map((p) => p.name), ['Start', 'Steam-Spiele', 'Danach']);
  assert.equal(store.pages[1].rows, 2);
  store.replaceTemplatePages('obs', [{ name: 'OBS', template: 'obs', rows: 1, cols: 1 }]);
  assert.deepEqual(store.pages.map((p) => p.template), ['', 'steam', '', 'obs']);
  assert.equal(store.pages[0].id, startId);
});

test('Erneuter Touch-Portal-Import behält die Seiten-IDs', () => {
  const tp = tmp();
  fs.mkdirSync(path.join(tp, 'pages'));
  fs.writeFileSync(path.join(tp, 'pages', '(main).tml'), JSON.stringify({ KEY_COLUMNS: 1, KEY_ROWS: 1, BUTTONS: [[{}]] }));
  const first = importTouchPortal(tp, tmp()).pages[0];
  const again = importTouchPortal(tp, tmp(), new Map([[first.name, first.id]])).pages[0];
  assert.equal(again.id, first.id);
  assert.equal(again.template, 'touchportal');
});

test('Server weist Speichern aus einem veralteten Editor-Tab ab', async () => {
  const store = new Store(tmp());
  const server = await createServer(store, { port: 0, host: '127.0.0.1', log: () => {} });
  const base = `http://127.0.0.1:${server.address().port}`;
  const put = (revision) => fetch(`${base}/api/pages`, { method: 'PUT', headers: { 'X-TouchDeck': '1' }, body: JSON.stringify({ pages: store.pages, revision }) });
  try {
    const rev = (await (await fetch(`${base}/api/state`)).json()).revision;
    const ok = await put(rev);
    assert.equal(ok.status, 200);
    assert.equal((await ok.json()).revision, rev + 1);
    assert.equal((await put(rev)).status, 409);
  } finally {
    server.closeAllConnections();
    server.close();
  }
});

test('Variablen: setzen, zählen, Bedingung stoppt die restlichen Aktionen, Platzhalter', async () => {
  const { runActions, fillVariables } = await import('../src/core/actions.js');
  const store = new Store(tmp());
  const act = (type, params) => ({ type, enabled: true, params });
  let r = await runActions([act('var_set', { name: 'Szene', value: 'Spiel' }), act('var_change', { name: 'Tore', op: 'add', amount: 2 })], { store });
  assert.equal(r.ok, true);
  assert.deepEqual(store.variables, { Szene: 'Spiel', Tore: '2' });
  r = await runActions([act('condition', { name: 'Tore', op: 'gt', value: '5' }), act('var_set', { name: 'Szene', value: 'Pause' })], { store });
  assert.equal(r.ok, true);
  assert.equal(store.variables.Szene, 'Spiel', 'Bedingung nicht erfüllt – zweite Aktion darf nicht laufen');
  await runActions([act('var_toggle', { name: 'Licht' })], { store });
  assert.equal(store.variables.Licht, 'an');
  assert.equal(fillVariables('{Szene}: {Tore} Tore {Unbekannt}', store), 'Spiel: 2 Tore {Unbekannt}');
  assert.throws(() => store.setVariable('', 'x'), /Variablenname/);
});

test('Koppeln per Bestätigung: Anfrage, Erlauben, Schlüssel genau einmal', async () => {
  const store = new Store(tmp());
  const requests = [];
  const server = await createServer(store, { port: 0, host: '127.0.0.1', onPairRequest: (r) => requests.push(r) });
  const base = `http://127.0.0.1:${server.address().port}`;
  const post = (url, body) => fetch(`${base}${url}`, { method: 'POST', headers: { 'X-TouchDeck': '1', 'Content-Type': 'application/json' }, body: JSON.stringify(body) }).then((r) => r.json());
  try {
    const { id } = await post('/api/pair/request', { device: 'Tablet <script>' });
    assert.equal(requests[0].device, 'Tablet script', 'Gerätename wird bereinigt');
    assert.equal((await (await fetch(`${base}/api/pair/status?id=${id}`)).json()).state, 'pending');
    await post('/api/pair/answer', { id, allow: true });
    const ok = await (await fetch(`${base}/api/pair/status?id=${id}`)).json();
    assert.equal(ok.state, 'approved');
    assert.equal(ok.token, store.settings.token);
    assert.equal((await (await fetch(`${base}/api/pair/status?id=${id}`)).json()).state, 'expired');
  } finally {
    server.closeAllConnections();
    server.close();
  }
});
