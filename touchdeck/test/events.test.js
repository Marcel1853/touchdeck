import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Store } from '../src/core/store.js';
import { runActions } from '../src/core/actions.js';
import { EventEngine } from '../src/core/events.js';
import { Log } from '../src/core/log.js';
import { Backups } from '../src/core/backup.js';

const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'touchdeck-ev-'));
const act = (type, params = {}) => ({ type, enabled: true, params });
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const fakeObs = { onEvent() {}, connect: () => Promise.reject(new Error('aus')), connected: false };

function setup(events = [], pages) {
  const store = new Store(tmp());
  store.setProject({ pages: pages ?? store.pages, events });
  const log = new Log(path.join(store.dataDir, 'logs'));
  const engine = new EventEngine({ store, obs: fakeObs, log, run: (actions, extra) => runActions(actions, { store, obs: fakeObs, ...extra }) });
  engine.start();
  return { store, engine, log };
}

test('Event „Wert ändert sich zu“ führt Aktionen aus und kennt {Event.Wert}', async () => {
  const { store, engine } = setup([{
    name: 'Tor', trigger: { type: 'value_change', params: { name: 'Tore', op: 'gt', value: '2' } },
    actions: [act('var_set', { name: 'Meldung', value: 'Schon {Event.Wert} Tore!' })],
  }]);
  store.setVariable('Tore', '1');
  await wait(20);
  assert.equal(store.variables.Meldung, undefined);
  store.setVariable('Tore', '3');
  await wait(20);
  assert.equal(store.variables.Meldung, 'Schon 3 Tore!');
  engine.stop();
});

test('Abklingzeit verhindert schnelles Wiederholen', async () => {
  const { store, engine } = setup([{
    name: 'Zähler', cooldown: 60, trigger: { type: 'value_change', params: { name: 'X', op: 'any' } },
    actions: [act('var_change', { name: 'Läufe', op: 'add', amount: 1 })],
  }]);
  store.setVariable('X', 'a');
  store.setVariable('X', 'b');
  await wait(20);
  assert.equal(store.variables['Läufe'], '1');
  engine.stop();
});

test('Events, die sich gegenseitig auslösen, laufen nicht endlos', async () => {
  const { store, engine, log } = setup([
    { name: 'A', trigger: { type: 'value_change', params: { name: 'Ping', op: 'any' } }, actions: [act('var_change', { name: 'Pong', op: 'add', amount: 1 })] },
    { name: 'B', trigger: { type: 'value_change', params: { name: 'Pong', op: 'any' } }, actions: [act('var_change', { name: 'Ping', op: 'add', amount: 1 })] },
  ]);
  store.setVariable('Ping', '0');
  await wait(100);
  assert.ok(Number(store.variables.Ping) < 10, `Ping ist ${store.variables.Ping}`);
  assert.ok(log.entries.some((e) => e.message.includes('endlos')));
  engine.stop();
});

test('Vorlage ausführen mit Wiederholen; Stopp bricht ab', async () => {
  const store = new Store(tmp());
  store.setProject({ flows: [{ id: 'f_1', name: 'Plus', actions: [act('var_change', { name: 'N', op: 'add', amount: 1 })] }] });
  let r = await runActions([act('run_flow', { flow: 'f_1', times: 3 })], { store });
  assert.equal(r.ok, true);
  assert.equal(store.variables.N, '3');
  r = await runActions([act('stop'), act('var_set', { name: 'N', value: 'nie' })], { store });
  assert.equal(store.variables.N, '3');
});

test('„Aussehen ändern“ wirkt auf diesen Button und lässt sich zurücksetzen', async () => {
  const store = new Store(tmp());
  const button = store.pages[0].buttons[0];
  await runActions([act('change_visuals', { target: 'self', title: 'Neu {Zeit}', color: '#ff0000' })], { store, buttonId: button.id });
  assert.match(store.overrides.buttons[button.id].title, /^Neu \d\d:\d\d$/);
  assert.equal(store.overrides.buttons[button.id].color, '#ff0000');
  assert.equal(button.color !== '#ff0000', true, 'Das gespeicherte Design bleibt unverändert');
  await runActions([act('restore_visuals', { target: 'self' })], { store, buttonId: button.id });
  assert.equal(store.overrides.buttons[button.id], undefined);
});

test('Backups: anlegen, auflisten, wiederherstellen', () => {
  const store = new Store(tmp());
  const log = new Log(path.join(store.dataDir, 'logs'));
  const backups = new Backups(store, log);
  const name = backups.create('Test');
  store.setProject({ pages: [{ name: 'Nur noch eine', rows: 1, cols: 1 }] });
  backups.restore(name);
  assert.equal(store.pages[0].name, 'Start');
  assert.ok(backups.list().some((b) => b.note === 'vor wiederherstellung'));
});
