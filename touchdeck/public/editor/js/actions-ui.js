// Aktionslisten, Eingabefelder, Aktionsauswahl und Auslöser-Editor – genutzt von Buttons, Vorlagen und Events.
import { $, h, api, toast, uid } from './dom.js';
import { S, mutate, render, save } from './state.js';

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
    else if (f.type === 'page') parts.push(v === '__back' ? 'Vorherige Seite' : S.pages.find((p) => p.id === v)?.name ?? '⚠ Seite fehlt');
    else if (f.type === 'app') parts.push(params.appName || v);
    else if (f.source === 'flows') parts.push(S.flows.find((fl) => fl.id === v)?.name ?? '⚠ Vorlage fehlt');
    else if (f.source === 'buttons') parts.push(v === 'self' ? 'dieser Button' : 'anderer Button');
    else parts.push(String(v).split('\n')[0].replace('␟', ' › '));
  }
  return parts.join(' · ');
}

export function summary(a) {
  const def = S.typeMap.get(a.type);
  if (!def) return a.type;
  if (a.type === 'unsupported') return `Aus Touch Portal: ${a.params.original}`;
  return describeParams(def, a.params) || 'Noch nicht eingerichtet';
}

export function triggerSummary(t) {
  const def = S.triggerMap.get(t.type);
  if (!def) return t.type;
  const detail = describeParams(def, t.params);
  return detail ? `${def.label}: ${detail}` : def.label;
}

// ---------- Testen ----------

export async function testActions(actions, label, { buttonId } = {}) {
  if (!actions.length) return toast('Keine Aktionen zum Testen.', 'error');
  await save();
  try {
    const r = await api('POST', '/api/test-actions', { actions, buttonId });
    if (!r.ok) return toast(r.error, 'error');
    const extra = [...r.messages];
    if (r.navigate) extra.push(`Würde wechseln zu: ${r.navigate === '__back' ? 'vorherige Seite' : S.pages.find((p) => p.id === r.navigate)?.name}`);
    toast(`${label} ausgeführt ✓${extra.length ? `\n${extra.join('\n')}` : ''}`, 'ok');
  } catch (err) {
    toast(err.message, 'error');
  }
}

// ---------- Aktionslisten ----------

/** Eine bearbeitbare Liste von Aktionen: owner[key] ist das Array. */
export function actionList(owner, key, title, { hint, buttonId } = {}) {
  const list = owner[key];
  return h('div', { class: 'action-section' },
    h('div', { class: 'list-title' },
      h('h3', {}, title),
      list.length > 0 && h('button', { class: 'btn small ghost', title: 'Diese Aktionen jetzt ausführen', onclick: () => testActions(list, title, { buttonId }) }, '▶'),
      h('button', { class: 'btn small', onclick: () => openPicker(owner, key) }, '＋ Aktion')),
    hint && h('p', { class: 'hint-text' }, hint),
    list.length
      ? h('div', { class: 'action-list' }, list.map((a, i) => actionCard(list, a, i, buttonId)))
      : h('p', { class: 'muted-note' }, 'Noch keine Aktion. Klicke auf „＋ Aktion“.'));
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
    a.type === 'unsupported' && h('p', { class: 'hint-text' }, 'Diese Aktion stammt aus Touch Portal und hat in TouchDeck noch kein Gegenstück. Deaktiviere oder ersetze sie.'),
    h('button', { class: 'btn small', onclick: () => testActions([{ ...a, enabled: true }], def.label, { buttonId }) }, '▶ Nur diese Aktion testen'));

  return h('div', { class: `action-card${a.enabled ? '' : ' disabled'}${a.type === 'unsupported' ? ' unsupported' : ''}${a.type === 'comment' ? ' comment' : ''}` },
    h('div', { class: 'action-head', onclick: () => { open ? S.expanded.delete(a.id) : S.expanded.add(a.id); refresh(); } },
      h('span', { class: 'ai' }, def.icon),
      h('div', { class: 'al' }, h('b', {}, def.label), sum),
      h('input', { type: 'checkbox', checked: a.enabled, title: 'Aktiv', onclick: (e) => e.stopPropagation(), onchange: (e) => mutate(() => (a.enabled = e.target.checked)) }),
      h('button', { class: 'btn icon ghost', title: 'Nach oben', disabled: i === 0, onclick: stop(() => move(-1)) }, '↑'),
      h('button', { class: 'btn icon ghost', title: 'Nach unten', disabled: i === list.length - 1, onclick: stop(() => move(1)) }, '↓'),
      h('button', { class: 'btn icon ghost', title: 'Duplizieren', onclick: stop(() => mutate(() => list.splice(i + 1, 0, { ...structuredClone(a), id: uid('a_') }))) }, '⧉'),
      h('button', { class: 'btn icon ghost danger', title: 'Entfernen', onclick: stop(() => mutate(() => list.splice(i, 1))) }, '✕')),
    body);
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
          h('span', { class: 'color-value' }, v || 'unverändert'),
          v && h('button', { type: 'button', class: 'btn small ghost', onclick: () => setParam(f.key, '', { structural: true }) }, 'leeren')));
    case 'page': {
      const options = [h('option', { value: '' }, '– Seite wählen –'), h('option', { value: '__back' }, '↩ Vorherige Seite'), ...S.pages.map((p) => h('option', { value: p.id }, p.name))];
      return label(h('select', { value: v, onchange: (e) => setParam(f.key, e.target.value) }, options));
    }
    case 'app': {
      if (!S.apps) {
        api('GET', '/api/apps').then((apps) => {
          S.apps = apps;
          refresh();
        }).catch((err) => toast(err.message, 'error'));
        return label(h('select', { disabled: true }, h('option', {}, 'Programme werden geladen …')));
      }
      const options = [h('option', { value: '' }, '– Programm wählen –'), ...S.apps.map((app) => h('option', { value: app.id, title: app.comment }, app.name))];
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
      const rec = h('button', { type: 'button', class: 'btn small', onclick: () => recordHotkey(input, rec, (combo) => setParam(f.key, combo)) }, '⏺ Aufnehmen');
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
  if (!entry || entry.loading) return h('label', { class: 'field' }, f.label, h('select', { disabled: true }, h('option', {}, 'Wird geladen …')));
  const reloadBtn = h('button', { type: 'button', class: 'btn small icon ghost', title: 'Liste neu laden', onclick: reload }, '⟳');
  if (f.allowNew && !entry.error) {
    // Vorhandenen Namen wählen oder einen neuen eintippen (z. B. für Werte).
    const listId = `dl-${f.source}-${Math.random().toString(36).slice(2, 8)}`;
    return h('div', { class: 'field' }, f.label,
      h('div', { class: 'hotkey' },
        h('input', { value: value ?? '', list: listId, placeholder: 'Name wählen oder neu eingeben', oninput: (e) => onChange(e.target.value.trim()), onchange: () => delete S.options[f.source] }),
        h('datalist', { id: listId }, entry.items.map((o) => h('option', { value: o.value }))),
        reloadBtn));
  }
  if (entry.error) {
    return h('div', { class: 'field' }, f.label,
      h('div', { class: 'hotkey' }, h('input', { value, placeholder: 'Von Hand eingeben', oninput: (e) => onChange(e.target.value) }), reloadBtn),
      h('p', { class: 'hint-text error' }, entry.error));
  }
  const items = [...entry.items];
  if (value && !items.some((o) => o.value === value)) items.push({ value, label: `${value} (nicht gefunden)` });
  const emptyLabel = /\(leer = /.test(f.label) ? '– jede/jedes –' : items.length ? '– auswählen –' : '– nichts vorhanden –';
  return h('div', { class: 'field' }, f.label,
    h('div', { class: 'hotkey' },
      h('select', { value: value ?? '', onchange: (e) => onChange(e.target.value) },
        h('option', { value: '' }, emptyLabel),
        items.map((o) => h('option', { value: o.value }, o.label))),
      reloadBtn),
    !entry.items.length && h('p', { class: 'hint-text' }, f.source.startsWith('obs-')
      ? 'In OBS gibt es dazu noch nichts. Lege es in OBS an und klicke dann auf ⟳.'
      : 'Nichts gefunden. Mit ⟳ neu laden.'));
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
  input.placeholder = 'Tastenkombination drücken …';
  button.textContent = '… drücken';
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
    button.textContent = '⏺ Aufnehmen';
    done(combo);
  };
  window.addEventListener('keydown', onKey, true);
}

// ---------- Aktionsauswahl ----------

let pickerTarget = null;

export function openPicker(owner, key) {
  pickerTarget = { owner, key };
  $('#picker-search').value = '';
  renderPicker('');
  $('#dlg-picker').showModal();
  $('#picker-search').focus();
}

export function renderPicker(query) {
  const q = query.trim().toLowerCase();
  const types = S.actionTypes.filter((t) => !t.hidden && (!q || `${t.label} ${t.category} ${t.group}`.toLowerCase().includes(q)));
  const groups = [...new Set(types.map((t) => t.group))];
  $('#picker-list').replaceChildren(...groups.flatMap((g) => [
    h('h2', { class: 'picker-group' }, g),
    ...[...new Set(types.filter((t) => t.group === g).map((t) => t.category))].flatMap((c) => [
      h('h3', {}, c),
      ...types.filter((t) => t.group === g && t.category === c).map((t) => h('button', { type: 'button', onclick: () => addAction(t) }, h('span', { class: 'pi' }, t.icon), t.label)),
    ]),
  ]));
}

function addAction(def) {
  const { owner, key } = pickerTarget;
  const params = {};
  for (const f of def.fields) if (f.default !== undefined) params[f.key] = f.default;
  if (def.type === 'go_page') params.page = S.pages.find((p) => p.id !== S.pageId)?.id ?? '__back';
  const a = { id: uid('a_'), type: def.type, enabled: true, params };
  $('#dlg-picker').close();
  S.expanded.add(a.id);
  mutate(() => owner[key].push(a));
}

// ---------- Auslöser (Events) ----------

/** Auswahl des Auslösers plus seine Felder. trigger = { type, params } wird direkt bearbeitet. */
export function triggerEditor(trigger) {
  const def = S.triggerMap.get(trigger.type);
  const setParam = (k, v, { structural = false } = {}) => mutate(() => (trigger.params[k] = v), { coalesce: `trg:${k}`, detail: structural });
  const cats = [...new Set(S.triggerTypes.map((t) => t.category))];
  return h('div', { class: 'trigger-editor' },
    h('label', { class: 'field' }, 'Wenn …',
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
  device: '{Event.Gerät}', file_change: '{Event.Datei}', time_of_day: '{Event.Zeit}',
};
function eventDataHint(type) {
  return EVENT_DATA[type] ? `In den Aktionen verfügbar: ${EVENT_DATA[type]}` : '';
}

export function newTrigger(type = 'value_change') {
  const params = {};
  for (const f of S.triggerMap.get(type)?.fields ?? []) if (f.default !== undefined) params[f.key] = f.default;
  return { type, params };
}
