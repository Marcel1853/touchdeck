// Bereich „Logs“: was TouchDeck getan hat – Buttons, Events, Geräte, Fehler. Live.
import { t as tr } from '/shared/i18n.js';
import { h, api, toast } from '../dom.js';
import { S, onLog } from '../state.js';

const LEVELS = { debug: tr('logs.details'), info: tr('logs.info'), warn: tr('logs.warning'), error: tr('common.error') };
let level = 'all';
let query = '';
let follow = true;
let mounted = false;

function matches(e) {
  if (level === 'problems' && e.level !== 'warn' && e.level !== 'error') return false;
  if (level !== 'all' && level !== 'problems' && e.level !== level) return false;
  const q = query.trim().toLowerCase();
  return !q || `${e.source} ${e.message}`.toLowerCase().includes(q);
}

const row = (e) => h('tr', { class: `log-${e.level}` },
  h('td', { class: 'log-time' }, new Date(e.time).toLocaleTimeString('de-DE')),
  h('td', {}, h('span', { class: `log-badge ${e.level}` }, LEVELS[e.level] ?? e.level)),
  h('td', { class: 'log-source' }, e.source),
  h('td', {}, e.message));

function scrollDown() {
  const box = document.getElementById('log-box');
  if (follow && box) box.scrollTop = box.scrollHeight;
}

onLog((e) => {
  if (!mounted || !matches(e)) return;
  document.getElementById('log-rows')?.append(row(e));
  scrollDown();
});

export function mountLogs(root) {
  mounted = true;
  root.className = 'view view-list';
  root.replaceChildren(
    h('div', { class: 'list-page logs-page' },
      h('div', { class: 'list-page-head' },
        h('div', {}, h('h1', {}, tr('common.logs')), h('p', { class: 'hint-text' }, tr('logs.everythingTouchdeckDoesLive'))),
        h('div', { class: 'spacer' }),
        h('input', { class: 'search', placeholder: tr('common.search'), value: query, oninput: (e) => { query = e.target.value; renderLogs(); } }),
        h('select', { value: level, onchange: (e) => { level = e.target.value; renderLogs(); } },
          h('option', { value: 'all' }, tr('logs.all')), h('option', { value: 'problems' }, tr('logs.problemsOnly')),
          Object.entries(LEVELS).map(([v, l]) => h('option', { value: v }, l))),
        h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: follow, onchange: (e) => { follow = e.target.checked; scrollDown(); } }), 'mitlaufen'),
        h('button', { class: 'btn', onclick: async () => {
          await api('POST', '/api/logs/clear', {}).catch((err) => toast(err.message, 'error'));
          S.logs = [];
          renderLogs();
        } }, tr('logs.clear'))),
      h('div', { id: 'log-box', class: 'log-box' }, h('table', { class: 'log-table' }, h('tbody', { id: 'log-rows' })))),
  );
  api('GET', '/api/logs').then((entries) => {
    S.logs = entries;
    renderLogs();
  }).catch((err) => toast(err.message, 'error'));
}

export function unmountLogs() {
  mounted = false;
}

export function renderLogs() {
  const rows = document.getElementById('log-rows');
  if (!rows) return;
  const shown = S.logs.filter(matches);
  rows.replaceChildren(...(shown.length ? shown.map(row) : [h('tr', {}, h('td', { colspan: 4, class: 'muted-note' }, tr('logs.noEntries')))]));
  scrollDown();
}
