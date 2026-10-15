// Aktionslisten, Eingabefelder, Aktionsauswahl und Auslöser-Editor – genutzt von Buttons, Vorlagen und Events.
import { t as tr, getLang } from '/shared/i18n.js';
import { $, h, api, toast, uid } from './dom.js';
import { S, mutate, render, save, onRender } from './state.js';

const refresh = () => render();

// ---------- Zusammenfassungen ----------

export function fieldVisible(f, params, def) {
  if (!f.showIf) return true;
  return Object.entries(f.showIf).every(([k, allowed]) => allowed.includes(params[k] ?? def.fields.find((x) => x.key === k)?.default));
}

function describeParams(def, params) {
  const parts = [];
  for (const f of def.fields) {
    if (!fieldVisible(f, params, def) || f.type === 'checkbox') continue;
    const v = params[f.key] ?? f.default;
    if (v === undefined || v === '') continue;
    if (f.type === 'select') parts.push(f.options.find((o) => o.value === v)?.label ?? v);
    else if (f.type === 'page') parts.push(v === '__back' ? tr('Vorherige Seite') : S.pages.find((p) => p.id === v)?.name ?? tr('⚠ Seite fehlt'));
    else if (f.type === 'app') parts.push(params.appName || v);
    else if (f.source === 'flows') parts.push(S.flows.find((fl) => fl.id === v)?.name ?? tr('⚠ Vorlage fehlt'));
    else if (f.source === 'buttons') parts.push(v === 'self' ? tr('dieser Button') : tr('anderer Button'));
    else parts.push(String(v).split('\n')[0].replace('␟', ' › '));
  }
  return parts.join(' · ');
}

export function summary(a) {
  const def = S.typeMap.get(a.type);
  if (!def) return a.type;
  if (a.type === 'unsupported') return tr('Aus Touch Portal: {0}', [a.params.original]);
  return describeParams(def, a.params) || tr('Noch nicht eingerichtet');
}

export function triggerSummary(t) {
  const def = S.triggerMap.get(t.type);
  if (!def) return t.type;
  const detail = describeParams(def, t.params);
  return detail ? `${def.label}: ${detail}` : def.label;
}

// ---------- Testen ----------

export async function testActions(actions, label, { buttonId } = {}) {
  if (!actions.length) return toast(tr('Keine Aktionen zum Testen.'), 'error');
  await save();
  try {
    const r = await api('POST', '/api/test-actions', { actions, buttonId });
    if (!r.ok) return toast(r.error, 'error');
    const extra = [...r.messages];
    if (r.navigate) extra.push(tr('Würde wechseln zu: {0}', [r.navigate === '__back' ? tr('vorherige Seite') : S.pages.find((p) => p.id === r.navigate)?.name]));
    toast(`${tr('{0} ausgeführt ✓', [label])}${extra.length ? `\n${extra.join('\n')}` : ''}`, 'ok');
  } catch (err) {
    toast(err.message, 'error');
  }
}

// ---------- Aktionslisten ----------

/**
 * Eine Aktionsliste (owner[key] ist das Array). compact = schmale Übersicht für die Eigenschaften-Spalte;
 * bearbeitet wird dann im großen Aktions-Editor. context benennt den Besitzer im Editor-Titel.
 */
export function actionList(owner, key, title, { hint, buttonId, compact = false, context = '' } = {}) {
  const list = owner[key];
  const open = (focusId) => openActionEditor(owner, key, context ? `${context} · ${title}` : title, { buttonId, focusId });
  return h('div', { class: `action-section${compact ? ' compact' : ''}` },
    h('div', { class: 'list-title' },
      h('h3', {}, title),
      list.length > 0 && h('button', { class: 'btn small ghost', title: tr('Diese Aktionen jetzt ausführen'), onclick: () => testActions(list, title, { buttonId }) }, '▶'),
      h('button', { class: 'btn small', onclick: () => open() }, compact && list.length ? tr('✎ Bearbeiten') : tr('＋ Aktion'))),
    hint && h('p', { class: 'hint-text' }, hint),
    !list.length
      ? h('button', { type: 'button', class: 'empty-drop', onclick: () => open() }, tr('＋ Erste Aktion hinzufügen'))
      : compact
        ? h('div', { class: 'action-mini-list' }, list.map((a) => miniRow(a, () => open(a.id))))
        : flowList(list, buttonId));
}

/** Kurze Zeile für die Übersicht: Klick öffnet den Editor mit dieser Aktion. */
function miniRow(a, onOpen) {
  const def = S.typeMap.get(a.type) ?? { label: a.type, icon: '❔' };
  return h('button', { type: 'button', class: `mini-row${a.enabled ? '' : ' disabled'}`, title: tr('Im Aktions-Editor bearbeiten'), onclick: onOpen },
    h('span', { class: 'ai' }, def.icon),
    h('span', { class: 'al' }, h('b', {}, def.label), h('span', {}, summary(a))));
}

// Laufendes Ziehen: neue Aktion aus dem Katalog oder Verschieben innerhalb einer Liste.
let drag = null;

/** Bearbeitbare Liste mit Ziehen zum Sortieren und Ablegen neuer Aktionen. */
function flowList(list, buttonId) {
  const box = h('div', { class: 'action-list flow-list' }, list.map((a, i) => actionCard(list, a, i, buttonId)));
  const marker = h('div', { class: 'drop-marker' });
  const indexAt = (y) => {
    const cards = [...box.querySelectorAll(':scope > .action-card')];
    const i = cards.findIndex((c) => y < c.getBoundingClientRect().top + c.offsetHeight / 2);
    return i < 0 ? cards.length : i;
  };
  box.addEventListener('dragover', (e) => {
    if (!drag || (drag.kind === 'move' && drag.list !== list)) return;
    e.preventDefault();
    const i = indexAt(e.clientY);
    const cards = box.querySelectorAll(':scope > .action-card');
    if (i < cards.length) box.insertBefore(marker, cards[i]);
    else box.append(marker);
  });
  box.addEventListener('dragleave', (e) => {
    if (!box.contains(e.relatedTarget)) marker.remove();
  });
  box.addEventListener('drop', (e) => {
    if (!drag) return;
    e.preventDefault();
    const i = indexAt(e.clientY);
    marker.remove();
    if (drag.kind === 'new') insertAction(list, drag.def, i);
    else if (drag.list === list && drag.index !== i && drag.index + 1 !== i) {
      mutate(() => {
        const [x] = list.splice(drag.index, 1);
        list.splice(i > drag.index ? i - 1 : i, 0, x);
      });
    }
    drag = null;
  });
  return box;
}

function actionCard(list, a, i, buttonId) {
  const def = S.typeMap.get(a.type) ?? { label: a.type, icon: '❔', fields: [] };
  const open = S.expanded.has(a.id);
  const sum = h('span', {}, summary(a));
  const stop = (fn) => (e) => {
    e.stopPropagation();
    fn();
  };
  const move = (d) => mutate(() => {
    const [x] = list.splice(i, 1);
    list.splice(i + d, 0, x);
  });
  const setParam = (k, v, { structural = false } = {}) => {
    mutate(() => (a.params[k] = v), { coalesce: `${a.id}:${k}`, detail: structural });
    sum.textContent = summary(a);
  };

  const body = open && h('div', { class: 'action-body' },
    def.fields.filter((f) => fieldVisible(f, a.params, def)).map((f) => fieldInput(f, a, setParam)),
    a.type === 'unsupported' && h('p', { class: 'hint-text' }, tr('Diese Aktion wurde importiert und hat in TouchDeck noch kein Gegenstück. Deaktiviere oder ersetze sie.')),
    h('button', { class: 'btn small', onclick: () => testActions([{ ...a, enabled: true }], def.label, { buttonId }) }, tr('▶ Nur diese Aktion testen')));

  const card = h('div', { class: `action-card${a.enabled ? '' : ' disabled'}${a.type === 'unsupported' ? ' unsupported' : ''}${a.type === 'comment' ? ' comment' : ''}${open ? ' open' : ''}`, 'data-id': a.id },
    h('div', { class: 'action-head', onclick: () => { open ? S.expanded.delete(a.id) : S.expanded.add(a.id); refresh(); } },
      h('span', {
        class: 'drag-handle', title: tr('Ziehen zum Verschieben'), draggable: 'true',
        onclick: (e) => e.stopPropagation(),
        ondragstart: (e) => {
          drag = { kind: 'move', list, index: i };
          e.dataTransfer.effectAllowed = 'move';
          e.dataTransfer.setData('text/plain', a.id);
          e.dataTransfer.setDragImage(card, 24, 20);
          requestAnimationFrame(() => card.classList.add('dragging'));
        },
        ondragend: () => {
          drag = null;
          card.classList.remove('dragging');
        },
      }, '⠿'),
      h('span', { class: 'ai' }, def.icon),
      h('div', { class: 'al' }, h('b', {}, def.label), sum),
      h('input', { type: 'checkbox', checked: a.enabled, title: tr('Aktiv'), onclick: (e) => e.stopPropagation(), onchange: (e) => mutate(() => (a.enabled = e.target.checked)) }),
      h('button', { class: 'btn icon ghost', title: tr('Nach oben'), disabled: i === 0, onclick: stop(() => move(-1)) }, '↑'),
      h('button', { class: 'btn icon ghost', title: tr('Nach unten'), disabled: i === list.length - 1, onclick: stop(() => move(1)) }, '↓'),
      h('button', { class: 'btn icon ghost', title: tr('Duplizieren'), onclick: stop(() => mutate(() => list.splice(i + 1, 0, { ...structuredClone(a), id: uid('a_') }))) }, '⧉'),
      h('button', { class: 'btn icon ghost danger', title: tr('Entfernen'), onclick: stop(() => mutate(() => list.splice(i, 1))) }, '✕')),
    body);
  return card;
}

// ---------- Eingabefelder ----------

export function fieldInput(f, obj, setParam) {
  const v = obj.params[f.key] ?? f.default ?? '';
  const label = (control) => h('label', { class: 'field' }, f.label, control);
  switch (f.type) {
    case 'textarea':
      return label(h('textarea', { value: v, placeholder: f.placeholder, oninput: (e) => setParam(f.key, e.target.value) }));
    case 'number':
      return label(h('input', { type: 'number', value: v, min: f.min, max: f.max, onchange: (e) => setParam(f.key, Number(e.target.value)) }));
    case 'checkbox':
      return h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked: !!v, onchange: (e) => setParam(f.key, e.target.checked) }), f.label);
    case 'select':
      return label(h('select', { value: v, onchange: (e) => setParam(f.key, e.target.value, { structural: true }) }, f.options.map((o) => h('option', { value: o.value }, o.label))));
    case 'color':
      return h('div', { class: 'field' }, f.label,
        h('div', { class: 'hotkey' },
          h('input', { type: 'color', value: v || '#2b2f3a', oninput: (e) => setParam(f.key, e.target.value) }),
          h('span', { class: 'color-value' }, v || tr('unverändert')),
          v && h('button', { type: 'button', class: 'btn small ghost', onclick: () => setParam(f.key, '', { structural: true }) }, 'leeren')));
    case 'page': {
      const options = [h('option', { value: '' }, tr('– Seite wählen –')), h('option', { value: '__back' }, tr('↩ Vorherige Seite')), ...S.pages.map((p) => h('option', { value: p.id }, p.name))];
      return label(h('select', { value: v, onchange: (e) => setParam(f.key, e.target.value) }, options));
    }
    case 'app': {
      if (!S.apps) {
        api('GET', '/api/apps').then((apps) => {
          S.apps = apps;
          refresh();
        }).catch((err) => toast(err.message, 'error'));
        return label(h('select', { disabled: true }, h('option', {}, tr('Programme werden geladen …'))));
      }
      const options = [h('option', { value: '' }, tr('– Programm wählen –')), ...S.apps.map((app) => h('option', { value: app.id, title: app.comment }, app.name))];
      return label(h('select', { value: v, onchange: (e) => {
        const app = S.apps.find((x) => x.id === e.target.value);
        obj.params.appName = app?.name ?? '';
        setParam(f.key, e.target.value);
      } }, options));
    }
    case 'dynamic':
      return dynamicSelect(f, v, (value) => setParam(f.key, value, { structural: !f.allowNew }));
    case 'hotkey': {
      const input = h('input', { value: v, placeholder: f.placeholder, oninput: (e) => setParam(f.key, e.target.value) });
      const rec = h('button', { type: 'button', class: 'btn small', onclick: () => recordHotkey(input, rec, (combo) => setParam(f.key, combo)) }, tr('⏺ Aufnehmen'));
      return h('div', { class: 'field' }, f.label, h('div', { class: 'hotkey' }, input, rec));
    }
    default:
      return label(h('input', { value: v, placeholder: f.placeholder, oninput: (e) => setParam(f.key, e.target.value) }));
  }
}

/** Auswahl, deren Einträge der Server erst zur Laufzeit liefert (Spiele, Audiogeräte, OBS, Werte …). */
export function dynamicSelect(f, value, onChange) {
  const entry = S.options[f.source];
  const reload = () => {
    delete S.options[f.source];
    refresh();
  };
  if (!entry) {
    S.options[f.source] = { loading: true };
    api('GET', `/api/options/${f.source}`)
      .then((items) => (S.options[f.source] = { items }))
      .catch((err) => (S.options[f.source] = { error: err.message }))
      .finally(refresh);
  }
  if (!entry || entry.loading) return h('label', { class: 'field' }, f.label, h('select', { disabled: true }, h('option', {}, tr('Wird geladen …'))));
  const reloadBtn = h('button', { type: 'button', class: 'btn small icon ghost', title: tr('Liste neu laden'), onclick: reload }, '⟳');
  if (f.allowNew && !entry.error) {
    // Vorhandenen Namen wählen oder einen neuen eintippen (z. B. für Werte).
    const listId = `dl-${f.source}-${Math.random().toString(36).slice(2, 8)}`;
    return h('div', { class: 'field' }, f.label,
      h('div', { class: 'hotkey' },
        h('input', { value: value ?? '', list: listId, placeholder: tr('Name wählen oder neu eingeben'), oninput: (e) => onChange(e.target.value.trim()), onchange: () => delete S.options[f.source] }),
        h('datalist', { id: listId }, entry.items.map((o) => h('option', { value: o.value }))),
        reloadBtn));
  }
  if (entry.error) {
    return h('div', { class: 'field' }, f.label,
      h('div', { class: 'hotkey' }, h('input', { value, placeholder: tr('Von Hand eingeben'), oninput: (e) => onChange(e.target.value) }), reloadBtn),
      h('p', { class: 'hint-text error' }, entry.error));
  }
  const items = [...entry.items];
  if (value && !items.some((o) => o.value === value)) items.push({ value, label: tr('{0} (nicht gefunden)', [value]) });
  const emptyLabel = /\((leer|empty) = /.test(f.label) ? tr('– jede/jedes –') : items.length ? tr('– auswählen –') : tr('– nichts vorhanden –');
  return h('div', { class: 'field' }, f.label,
    h('div', { class: 'hotkey' },
      h('select', { value: value ?? '', onchange: (e) => onChange(e.target.value) },
        h('option', { value: '' }, emptyLabel),
        items.map((o) => h('option', { value: o.value }, o.label))),
      reloadBtn),
    !entry.items.length && h('p', { class: 'hint-text' }, f.source.startsWith('obs-')
      ? tr('In OBS gibt es dazu noch nichts. Lege es in OBS an und klicke dann auf ⟳.')
      : tr('Nichts gefunden. Mit ⟳ neu laden.')));
}

// ---------- Tastenkürzel aufnehmen ----------

const CODE_NAMES = {
  ArrowUp: 'up', ArrowDown: 'down', ArrowLeft: 'left', ArrowRight: 'right', Enter: 'enter', NumpadEnter: 'enter', Escape: 'esc',
  Space: 'space', Tab: 'tab', Backspace: 'backspace', Delete: 'delete', Insert: 'insert', Home: 'home', End: 'end',
  PageUp: 'pageup', PageDown: 'pagedown', PrintScreen: 'print', Pause: 'pause', ContextMenu: 'menu', CapsLock: 'capslock',
  MediaPlayPause: 'playpause', MediaTrackNext: 'nexttrack', MediaTrackPrevious: 'prevtrack', MediaStop: 'stop',
  AudioVolumeUp: 'volumeup', AudioVolumeDown: 'volumedown', AudioVolumeMute: 'volumemute',
};
const MODIFIER_KEYS = new Set(['Control', 'Shift', 'Alt', 'Meta', 'AltGraph', 'OS']);

/** Nimmt ein Tastenkürzel auf. Zeichen kommen aus e.key, damit das deutsche Layout (z/y, ü …) stimmt. */
function recordHotkey(input, button, done) {
  input.classList.add('recording');
  input.value = '';
  input.placeholder = tr('Tastenkombination drücken …');
  button.textContent = tr('… drücken');
  const onKey = (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (MODIFIER_KEYS.has(e.key)) return;
    const parts = [];
    if (e.ctrlKey) parts.push('ctrl');
    if (e.altKey) parts.push('alt');
    if (e.shiftKey) parts.push('shift');
    if (e.metaKey) parts.push('super');
    const key = CODE_NAMES[e.code] ?? (/^F\d+$/.test(e.code) ? e.code.toLowerCase() : e.key === '+' ? 'plus' : e.key.toLowerCase());
    parts.push(key);
    const combo = parts.join('+');
    window.removeEventListener('keydown', onKey, true);
    input.classList.remove('recording');
    input.value = combo;
    button.textContent = tr('⏺ Aufnehmen');
    done(combo);
  };
  window.addEventListener('keydown', onKey, true);
}

// ---------- Aktions-Editor ----------

// Großes Fenster: links Kategorien, in der Mitte die Aktionen als Kacheln, rechts der Ablauf.
let ed = null; // { owner, key, title, buttonId }
let edCat = 'all';
let edQuery = '';
const RECENT_KEY = 'touchdeck.recentActions';

function recent() {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]').filter((t) => S.typeMap.has(t));
  } catch {
    return [];
  }
}

function remember(type) {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify([type, ...recent().filter((t) => t !== type)].slice(0, 12)));
  } catch {
    // Ohne Speicher gibt es eben keine „Zuletzt benutzt“-Liste.
  }
}

function newAction(def) {
  const params = {};
  for (const f of def.fields) if (f.default !== undefined) params[f.key] = f.default;
  if (def.type === 'go_page') params.page = S.pages.find((p) => p.id !== S.pageId)?.id ?? '__back';
  return { id: uid('a_'), type: def.type, enabled: true, params };
}

/** Neue Aktion an Position index einfügen und zum Bearbeiten aufklappen. */
function insertAction(list, def, index = list.length) {
  const a = newAction(def);
  remember(def.type);
  S.expanded.clear();
  S.expanded.add(a.id);
  mutate(() => list.splice(index, 0, a));
  requestAnimationFrame(() => document.querySelector(`#ae-flow [data-id="${a.id}"]`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));
}

function editorDialog() {
  let dlg = document.getElementById('dlg-actions');
  if (dlg) return dlg;
  dlg = h('dialog', { id: 'dlg-actions', class: 'action-editor' },
    h('div', { class: 'ae-head' },
      h('div', {}, h('h2', { id: 'ae-title' }), h('p', { class: 'hint-text' }, tr('Aktion links anklicken oder in den Ablauf ziehen · im Ablauf am ⠿ ziehen zum Sortieren'))),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn', id: 'ae-test', onclick: () => ed && testActions(ed.owner[ed.key], ed.title, { buttonId: ed.buttonId }) }, tr('▶ Ablauf testen')),
      h('button', { class: 'btn primary', onclick: () => dlg.close() }, tr('Fertig'))),
    h('div', { class: 'ae-body' },
      h('aside', { class: 'ae-nav' },
        h('input', {
          id: 'ae-search', class: 'search', placeholder: tr('🔍 Aktion suchen …'), autocomplete: 'off',
          oninput: (e) => { edQuery = e.target.value; renderCatalog(); },
          onkeydown: (e) => {
            if (e.key !== 'Enter') return;
            e.preventDefault();
            document.querySelector('#ae-tiles .ae-tile')?.click();
          },
        }),
        h('div', { id: 'ae-cats', class: 'ae-cats' })),
      h('section', { class: 'ae-catalog' }, h('h3', { id: 'ae-cat-title' }), h('div', { id: 'ae-tiles', class: 'ae-tiles' })),
      h('section', { class: 'ae-flow' },
        h('div', { class: 'ae-flow-head' }, h('h3', {}, tr('Ablauf')), h('span', { id: 'ae-count', class: 'muted-note' })),
        h('div', { id: 'ae-flow' }))));
  dlg.addEventListener('close', () => {
    ed = null;
    render();
  });
  document.body.append(dlg);
  return dlg;
}

/** Öffnet den Aktions-Editor für owner[key]; focusId klappt eine Aktion direkt auf. */
export function openActionEditor(owner, key, title, { buttonId, focusId } = {}) {
  const dlg = editorDialog();
  ed = { owner, key, title, buttonId };
  S.expanded.clear();
  if (focusId) S.expanded.add(focusId);
  edQuery = '';
  document.getElementById('ae-search').value = '';
  if (edCat !== 'all' && edCat !== 'recent' && !S.actionTypes.some((t) => `${t.group}|${t.category}` === edCat)) edCat = 'all';
  if (edCat === 'recent' && !recent().length) edCat = 'all';
  document.getElementById('ae-title').textContent = title;
  renderCatalog();
  renderFlow();
  dlg.showModal();
  if (!focusId) document.getElementById('ae-search').focus();
  else requestAnimationFrame(() => document.querySelector(`#ae-flow [data-id="${focusId}"]`)?.scrollIntoView({ block: 'center' }));
}

function renderCatalog() {
  const visible = S.actionTypes.filter((t) => !t.hidden);
  const rec = recent();
  const groups = [...new Set(visible.map((t) => t.group))];
  const navItem = (id, icon, label, count) => h('button', {
    type: 'button', class: `ae-cat${!edQuery && edCat === id ? ' active' : ''}`,
    onclick: () => { edCat = id; edQuery = ''; document.getElementById('ae-search').value = ''; renderCatalog(); },
  }, h('span', { class: 'ae-cat-icon' }, icon), h('span', { class: 'ae-cat-label' }, label), count !== undefined && h('span', { class: 'ae-cat-count' }, String(count)));

  document.getElementById('ae-cats').replaceChildren(...[
    rec.length > 0 && navItem('recent', '⭐', tr('Zuletzt benutzt'), rec.length),
    navItem('all', '▦', tr('Alle Aktionen'), visible.length),
    ...groups.flatMap((g) => [
      h('div', { class: 'ae-group' }, g),
      ...[...new Set(visible.filter((t) => t.group === g).map((t) => t.category))].map((c) => {
        const items = visible.filter((t) => t.group === g && t.category === c);
        return navItem(`${g}|${c}`, items[0].icon, c, items.length);
      }),
    ]),
  ].filter(Boolean));

  const q = edQuery.trim().toLowerCase();
  let shown;
  let heading;
  if (q) {
    shown = visible.filter((t) => `${t.label} ${t.category} ${t.group}`.toLowerCase().includes(q));
    heading = tr('Suche: „{0}“', [edQuery.trim()]);
  } else if (edCat === 'recent') {
    shown = rec.map((type) => S.typeMap.get(type)).filter(Boolean);
    heading = tr('Zuletzt benutzt');
  } else if (edCat === 'all') {
    shown = visible;
    heading = tr('Alle Aktionen');
  } else {
    const [g, c] = edCat.split('|');
    shown = visible.filter((t) => t.group === g && t.category === c);
    heading = `${g} › ${c}`;
  }
  document.getElementById('ae-cat-title').textContent = heading;
  const showCategory = q || edCat === 'all' || edCat === 'recent';
  document.getElementById('ae-tiles').replaceChildren(...(shown.length ? shown.map((def) => h('button', {
    type: 'button', class: 'ae-tile', draggable: 'true', title: tr('{0} – klicken zum Hinzufügen oder in den Ablauf ziehen', [def.label]),
    onclick: () => ed && insertAction(ed.owner[ed.key], def),
    ondragstart: (e) => {
      drag = { kind: 'new', def };
      e.dataTransfer.effectAllowed = 'copy';
      e.dataTransfer.setData('text/plain', def.type);
    },
    ondragend: () => (drag = null),
  },
  h('span', { class: 'ae-tile-icon' }, def.icon),
  h('span', { class: 'ae-tile-label' }, def.label),
  showCategory && h('span', { class: 'ae-tile-cat' }, def.category))) : [h('p', { class: 'muted-note' }, tr('Keine passende Aktion gefunden.'))]));
}

function renderFlow() {
  if (!ed) return;
  const list = ed.owner[ed.key];
  document.getElementById('ae-count').textContent = list.length === 1 ? tr('1 Aktion') : tr('{0} Aktionen', [list.length]);
  document.getElementById('ae-test').disabled = !list.length;
  const flow = flowList(list, ed.buttonId);
  if (!list.length) flow.append(h('div', { class: 'ae-empty' }, h('div', { class: 'empty-icon' }, '⬅️'), h('p', {}, tr('Klicke links auf eine Aktion oder ziehe sie hierher.'))));
  document.getElementById('ae-flow').replaceChildren(flow);
}

// Nach jeder Änderung den Ablauf neu zeichnen – außer beim Tippen (detail=false), damit der Fokus bleibt.
onRender(({ detail }) => {
  if (!ed || !document.getElementById('dlg-actions')?.open) return;
  if (detail) renderFlow();
  else document.getElementById('ae-count').textContent = tr('{0} Aktionen', [ed.owner[ed.key].length]);
});

// ---------- Auslöser (Events) ----------

/** Auswahl des Auslösers plus seine Felder. trigger = { type, params } wird direkt bearbeitet. */
export function triggerEditor(trigger) {
  const def = S.triggerMap.get(trigger.type);
  const setParam = (k, v, { structural = false } = {}) => mutate(() => (trigger.params[k] = v), { coalesce: `trg:${k}`, detail: structural });
  const cats = [...new Set(S.triggerTypes.map((t) => t.category))];
  return h('div', { class: 'trigger-editor' },
    h('label', { class: 'field' }, tr('Wenn …'),
      h('select', {
        value: trigger.type,
        onchange: (e) => mutate(() => {
          trigger.type = e.target.value;
          trigger.params = {};
          for (const f of S.triggerMap.get(trigger.type).fields) if (f.default !== undefined) trigger.params[f.key] = f.default;
        }),
      }, cats.map((c) => h('optgroup', { label: c }, S.triggerTypes.filter((t) => t.category === c).map((t) => h('option', { value: t.type }, `${t.icon} ${t.label}`)))))),
    def && def.fields.filter((f) => fieldVisible(f, trigger.params, def)).map((f) => fieldInput(f, trigger, setParam)),
    def && h('p', { class: 'hint-text' }, eventDataHint(trigger.type)));
}

const EVENT_DATA = {
  value_change: '{Event.Name}, {Event.Wert}, {Event.Alt}',
  obs_scene: '{Event.Szene}', obs_preview: '{Event.Szene}', obs_stream: '{Event.Status}', obs_record: '{Event.Status}',
  obs_mute: '{Event.Quelle}', obs_source: '{Event.Quelle}, {Event.Szene}', obs_filter: '{Event.Filter}, {Event.Quelle}',
  obs_profile: '{Event.Name}', obs_replay_saved: '{Event.Datei}', steam_game: '{Event.Spiel}', audio_output: '{Event.Gerät}',
  audio_mute: '{Event.Status}', device: '{Event.Gerät}', file_change: '{Event.Datei}', time_of_day: '{Event.Zeit}',
  twitch_follow: '{Event.Nutzer}', twitch_sub: '{Event.Nutzer}, {Event.Stufe}, {Event.Anzahl}, {Event.Monate}, {Event.Nachricht}',
  twitch_cheer: '{Event.Nutzer}, {Event.Bits}, {Event.Nachricht}', twitch_raid: '{Event.Nutzer}, {Event.Zuschauer}',
  twitch_redeem: '{Event.Nutzer}, {Event.Belohnung}, {Event.Kosten}, {Event.Eingabe}', twitch_chat: '{Event.Nutzer}, {Event.Nachricht}',
  twitch_stream: '{Event.Status}', twitch_ad: '{Event.Dauer}', discord_voice: '{Event.Status}', discord_channel: '{Event.Kanal}',
};
// Englische Namen der Event-Daten (der Server kennt beide).
const EVENT_ALIASES = {
  Nutzer: 'User', Wert: 'Value', Alt: 'Old', Szene: 'Scene', Quelle: 'Source', Gerät: 'Device', Spiel: 'Game', Datei: 'File',
  Kanal: 'Channel', Nachricht: 'Message', Belohnung: 'Reward', Kosten: 'Cost', Eingabe: 'Input', Zuschauer: 'Viewers',
  Monate: 'Months', Stufe: 'Tier', Anzahl: 'Count', Dauer: 'Duration', Zeit: 'Time',
};
function eventDataHint(type) {
  let names = EVENT_DATA[type];
  if (!names) return '';
  if (getLang() !== 'de') names = names.replace(/\{Event\.([^}]+)\}/g, (m, k) => `{Event.${EVENT_ALIASES[k] ?? k}}`);
  return tr('In den Aktionen verfügbar: {0}', [names]);
}

export function newTrigger(type = 'value_change') {
  const params = {};
  for (const f of S.triggerMap.get(type)?.fields ?? []) if (f.default !== undefined) params[f.key] = f.default;
  return { type, params };
}
