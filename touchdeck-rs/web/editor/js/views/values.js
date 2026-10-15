// Bereich „Werte“: eigene Variablen mit aktuellem Inhalt, Startwert, Ordner und Verwendungen.
import { t as tr } from '/shared/i18n.js';
import { h, api, toast, prompt, empty } from '../dom.js';
import { S, mutate, render } from '../state.js';

const NAME = /^[\p{L}\p{N} _.-]{1,40}$/u;
let filter = '';
let folder = '';

/** Alle Stellen, an denen Werte vorkommen: Aktionen, Auslöser, Live-Anzeigen, Beschriftungen. */
function* references() {
  const lists = [];
  for (const pg of S.pages) {
    for (const b of pg.buttons) {
      const where = tr('values.page', [pg.name, b.title || 'Button']);
      lists.push([where, b.actions], [where, b.actionsOff], [where, b.holdActions ?? []], [where, b.releaseActions ?? []]);
      for (const ev of b.events ?? []) {
        lists.push([tr('values.event', [where]), ev.actions]);
        yield { where: tr('values.event', [where]), obj: ev.trigger.params, key: 'name', kind: 'trigger', trigger: ev.trigger };
      }
      if (b.live.variable) yield { where, obj: b.live, key: 'variable' };
      if (b.title.includes('{')) yield { where, obj: b, key: 'title', kind: 'text' };
    }
  }
  for (const f of S.flows) lists.push([tr('common.flow', [f.name]), f.actions]);
  for (const e of S.events) {
    lists.push([tr('common.event2', [e.name]), e.actions]);
    yield { where: tr('common.event2', [e.name]), obj: e.trigger.params, key: 'name', kind: 'trigger', trigger: e.trigger };
  }
  for (const [where, list] of lists) {
    for (const a of list) {
      if (/^(var_|condition)/.test(a.type)) yield { where, obj: a.params, key: 'name' };
      for (const k of ['text', 'value', 'title']) if (typeof a.params[k] === 'string' && a.params[k].includes('{')) yield { where, obj: a.params, key: k, kind: 'text' };
    }
  }
}

function usages(name) {
  const found = [];
  for (const r of references()) {
    if (r.kind === 'trigger' && r.trigger.type !== 'value_change') continue;
    if (r.kind === 'text' ? r.obj[r.key].includes(`{${name}}`) : r.obj[r.key] === name) found.push(r.where);
  }
  return found;
}

function allNames() {
  const names = new Set([...Object.keys(S.values), ...Object.keys(S.variables)]);
  for (const r of references()) if (r.kind !== 'text' && (r.kind !== 'trigger' || r.trigger.type === 'value_change') && r.obj[r.key]) names.add(r.obj[r.key]);
  return [...names].sort((a, b) => a.localeCompare(b, 'de'));
}

const meta = (name) => S.values[name] ?? { default: '', description: '', folder: '', resetOnStart: false };

function editMeta(name, patch, opts) {
  mutate(() => (S.values[name] = { ...meta(name), ...patch }), { coalesce: `val:${name}:${Object.keys(patch)[0]}`, ...opts });
}

async function setCurrent(name, value) {
  try {
    const r = await api('PUT', '/api/variables', { name, value });
    S.variables = r.variables;
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function addValue() {
  const name = await prompt(tr('values.newValue'), tr('values.nameOfValueGoals'));
  if (!name) return;
  if (!NAME.test(name)) return toast(tr('values.name140Characters'), 'error');
  if (allNames().includes(name)) return toast(tr('values.thisValueAlreadyExists'), 'error');
  mutate(() => (S.values[name] = { default: '', description: '', folder, resetOnStart: false }));
  await setCurrent(name, '');
}

async function renameValue(name) {
  const next = await prompt(tr('values.renameValue'), tr('values.newNameForAll', [name]), name);
  if (!next || next === name) return;
  if (!NAME.test(next)) return toast(tr('values.name140Characters'), 'error');
  if (allNames().includes(next)) return toast(tr('values.thisNameAlreadyExists'), 'error');
  const current = S.variables[name];
  mutate(() => {
    for (const r of references()) {
      if (r.kind === 'trigger' && r.trigger.type !== 'value_change') continue;
      if (r.kind === 'text') r.obj[r.key] = r.obj[r.key].replaceAll(`{${name}}`, `{${next}}`);
      else if (r.obj[r.key] === name) r.obj[r.key] = next;
    }
    if (S.values[name]) {
      S.values[next] = S.values[name];
      delete S.values[name];
    }
  });
  if (current !== undefined) {
    await setCurrent(next, current);
    await api('POST', '/api/variables/delete', { name }).catch(() => {});
  }
  toast(tr('values.isNowCalled', [name, next]), 'ok');
}

async function deleteValue(name) {
  const used = usages(name);
  if (used.length && !confirm(tr('values.isStillUsedIn', [name, used.length, used.slice(0, 8).join('\n')]))) return;
  mutate(() => delete S.values[name]);
  const r = await api('POST', '/api/variables/delete', { name }).catch(() => null);
  if (r) S.variables = r.variables;
  render();
}

export function mountValues(root) {
  root.className = 'view view-list';
  root.replaceChildren(
    h('div', { class: 'list-page' },
      h('div', { class: 'list-page-head' },
        h('div', {}, h('h1', {}, tr('common.values')), h('p', { class: 'hint-text' }, tr('values.yourOwnVariablesFor'))),
        h('div', { class: 'spacer' }),
        h('input', { class: 'search', placeholder: tr('common.search'), value: filter, oninput: (e) => { filter = e.target.value; render(); } }),
        h('select', { id: 'value-folder', onchange: (e) => { folder = e.target.value; render(); } }),
        h('button', { class: 'btn primary', onclick: addValue }, tr('values.value'))),
      h('div', { id: 'values-body' })),
  );
}

export function renderValues({ detail = true } = {}) {
  const names = allNames();
  const folders = [...new Set(names.map((n) => meta(n).folder).filter(Boolean))].sort();
  const sel = document.getElementById('value-folder');
  sel.replaceChildren(h('option', { value: '' }, tr('values.allFolders')), ...folders.map((f) => h('option', { value: f }, f)));
  sel.value = folders.includes(folder) ? folder : '';
  if (!detail) return;
  const q = filter.trim().toLowerCase();
  const shown = names.filter((n) => (!q || n.toLowerCase().includes(q) || meta(n).description.toLowerCase().includes(q)) && (!folder || meta(n).folder === folder));
  const body = document.getElementById('values-body');
  if (!names.length) {
    body.replaceChildren(empty('🏷️', tr('values.noValuesYet'), tr('values.createOneOrUse')));
    return;
  }
  body.replaceChildren(h('table', { class: 'values-table' },
    h('thead', {}, h('tr', {}, [tr('common.name'), tr('values.currentContent'), tr('values.startValue'), tr('values.folder'), tr('values.description'), tr('values.resetAtStart'), tr('values.usedIn'), ''].map((t) => h('th', {}, t)))),
    h('tbody', {}, shown.map((name) => {
      const m = meta(name);
      const used = usages(name);
      return h('tr', {},
        h('td', {}, h('button', { class: 'linklike', title: tr('values.rename'), onclick: () => renameValue(name) }, name)),
        h('td', {}, h('input', { value: S.variables[name] ?? '', placeholder: '(leer)', onchange: (e) => setCurrent(name, e.target.value) })),
        h('td', {}, h('input', { value: m.default, placeholder: '(leer)', oninput: (e) => editMeta(name, { default: e.target.value }, { detail: false }) })),
        h('td', {}, h('input', { value: m.folder, list: 'value-folders', placeholder: '–', oninput: (e) => editMeta(name, { folder: e.target.value }, { detail: false }) })),
        h('td', {}, h('input', { value: m.description, placeholder: '–', oninput: (e) => editMeta(name, { description: e.target.value }, { detail: false }) })),
        h('td', { class: 'center' }, h('input', { type: 'checkbox', checked: m.resetOnStart, onchange: (e) => editMeta(name, { resetOnStart: e.target.checked }) })),
        h('td', { class: 'center', title: used.join('\n') || tr('values.nowhere') }, used.length ? `${used.length}×` : '–'),
        h('td', {}, h('button', { class: 'btn icon ghost danger', title: tr('common.delete'), onclick: () => deleteValue(name) }, '✕')));
    })),
  ), h('datalist', { id: 'value-folders' }, folders.map((f) => h('option', { value: f }))));
}
