// Bereich „Seiten“: Seitenliste, Raster mit Buttons und die Eigenschaften des gewählten Buttons.
import { buttonElement, setupGrid, iconUrl } from '/shared/render.js';
import { $, h, api, toast, ask, uid, segmented, setIfChanged, empty, prefs } from '../dom.js';
import { S, page, sel, mutate, render, save, pushUndo } from '../state.js';
import { actionList, testActions, dynamicSelect, triggerEditor, newTrigger, triggerSummary } from '../actions-ui.js';

const EMOJIS = '⏯️ ⏮️ ⏭️ ⏹️ 🔊 🔉 🔇 🎙️ 🎧 🎵 🎬 📷 🎥 🖥️ 💻 ⌨️ 🖱️ 📁 📂 📄 📝 🌐 🔍 ⚙️ 🔧 🔒 🔓 💡 🔔 💬 📧 📅 ⏰ ⏱️ 🚀 🎮 🕹️ 🏠 ⬅️ ➡️ ⬆️ ⬇️ ↩️ 🔄 ✅ ❌ ⭐ ❤️ 🔥 ⚡ 🌙 ☀️ 🎨 ✂️ 📋 🗑️ 💾 📤 📥 🔴 🟢 🔵 🟡'.split(' ');
const SWATCHES = ['#2b2f3a', '#3b3f8f', '#5b3fa0', '#8f3b6b', '#8f3b3b', '#9a5b1f', '#7a6a1f', '#2f7a4a', '#1f6f7a', '#1f4f9a', '#111318', '#e8eaf0'];

// ---------- Raster-Geometrie ----------

const overlaps = (a, b) => a.row < b.row + b.rowSpan && b.row < a.row + a.rowSpan && a.col < b.col + b.colSpan && b.col < a.col + a.colSpan;

function fits(p, rect, ignore = []) {
  if (rect.row < 0 || rect.col < 0 || rect.row + rect.rowSpan > p.rows || rect.col + rect.colSpan > p.cols) return false;
  return !p.buttons.some((b) => !ignore.includes(b.id) && overlaps(b, rect));
}

function freeCell(p) {
  for (let r = 0; r < p.rows; r++) for (let c = 0; c < p.cols; c++) if (fits(p, { row: r, col: c, rowSpan: 1, colSpan: 1 })) return { row: r, col: c };
  return null;
}

function cloneButton(b) {
  const copy = structuredClone(b);
  copy.id = uid('b_');
  for (const list of [copy.actions, copy.actionsOff, copy.holdActions ?? [], copy.releaseActions ?? [], ...(copy.events ?? []).map((e) => e.actions)]) {
    for (const a of list) a.id = uid('a_');
  }
  for (const e of copy.events ?? []) e.id = uid('e_');
  return copy;
}

export function newButton(row, col, extra = {}) {
  return {
    id: uid('b_'), row, col, rowSpan: 1, colSpan: 1, title: '', icon: { type: 'emoji', value: '' },
    color: '#2b2f3a', colorOn: '#2f9e6b', textColor: '#ffffff', fontSize: 14, titlePos: 'bottom', mode: 'press', iconFit: 'contain',
    actions: [], actionsOff: [], holdActions: [], releaseActions: [], holdRepeat: 0, events: [],
    live: { source: 'none', command: '', variable: '' }, slider: { target: 'volume', input: '', command: '' },
    ...extra,
  };
}

// ---------- Aufbau ----------

export function mountPages(root) {
  root.className = 'view view-pages';
  root.replaceChildren(
    h('aside', { class: 'sidebar' },
      h('div', { class: 'sidebar-head' }, h('h2', {}, 'Seiten'), h('button', { class: 'btn small', title: 'Neue Seite', onclick: addPage }, '＋')),
      h('ul', { id: 'page-list', class: 'page-list' }),
      h('div', { class: `templates${prefs.get('touchdeck.templatesOpen') === '1' ? '' : ' collapsed'}` },
        h('button', {
          type: 'button', class: 'templates-toggle', title: 'Ein-/ausklappen',
          onclick: (e) => {
            const box = e.currentTarget.parentElement;
            box.classList.toggle('collapsed');
            prefs.set('touchdeck.templatesOpen', box.classList.contains('collapsed') ? '0' : '1');
          },
        }, h('span', { class: 'chevron' }, '▸'), 'Fertige Seiten'),
        tplButton('steam', '🎮', 'Steam-Spiele', 'Alle Spiele mit Cover', steamTemplate),
        tplButton('system', '📊', 'System', 'Uhr, CPU, Audio, Sperren …', systemTemplate),
        tplButton('obs', '🎬', 'OBS Studio', 'Szenen, Stream, Aufnahme', obsTemplate),
        tplButton('touchportal', '⤵️', 'Touch Portal', 'Seiten übernehmen', importTouchPortal))),
    h('section', { class: 'workspace' },
      h('div', { class: 'page-toolbar' },
        h('input', { id: 'page-name', class: 'page-name', maxlength: 100, 'aria-label': 'Seitenname',
          oninput: (e) => mutate(() => (page().name = e.target.value || 'Seite'), { coalesce: `${S.pageId}:name`, detail: false }) }),
        h('label', { class: 'field-inline' }, 'Spalten', h('input', { id: 'page-cols', type: 'number', min: 1, max: 16, onchange: (e) => resizePage(page().rows, Number(e.target.value)) })),
        h('label', { class: 'field-inline' }, 'Zeilen', h('input', { id: 'page-rows', type: 'number', min: 1, max: 12, onchange: (e) => resizePage(Number(e.target.value), page().cols) })),
        h('label', { class: 'field-inline' }, 'Hintergrund', h('input', { id: 'page-bg', type: 'color',
          oninput: (e) => mutate(() => (page().background = e.target.value), { coalesce: `${S.pageId}:bg`, detail: false }) })),
        h('button', { class: 'btn small ghost', onclick: duplicatePage }, 'Duplizieren'),
        h('button', { id: 'page-delete', class: 'btn small ghost danger', onclick: deletePage }, 'Löschen')),
      h('div', { class: 'canvas-wrap' }, h('div', { id: 'canvas', class: 'td-grid canvas' })),
      h('p', { class: 'hint' }, 'Leeres Feld anklicken = neuer Button · Buttons ziehen zum Verschieben oder Tauschen · Entf löscht · Strg+D dupliziert')),
    h('aside', { id: 'inspector', class: 'inspector' }),
  );
}

function tplButton(template, icon, title, sub, onclick) {
  return h('button', { class: 'tpl', 'data-template': template, onclick }, h('span', {}, icon), title, h('small', {}, sub));
}

export function renderPages({ detail = true } = {}) {
  renderPageList();
  renderTemplateState();
  renderToolbar();
  renderCanvas();
  if (detail) renderInspector();
}

function renderPageList() {
  let dragFrom = null;
  $('#page-list').replaceChildren(...S.pages.map((p, i) => h('li', {
    class: p.id === S.pageId ? 'active' : '',
    draggable: 'true',
    title: 'Ziehen zum Sortieren',
    onclick: () => selectPage(p.id),
    ondragstart: (e) => {
      dragFrom = i;
      e.dataTransfer.setData('text/x-page', String(i));
    },
    ondragover: (e) => {
      if (!e.dataTransfer.types.includes('text/x-page')) return;
      e.preventDefault();
      e.currentTarget.classList.add('drag-over');
    },
    ondragleave: (e) => e.currentTarget.classList.remove('drag-over'),
    ondrop: (e) => {
      e.preventDefault();
      const from = Number(e.dataTransfer.getData('text/x-page') || dragFrom);
      if (from === i) return render({ detail: false });
      mutate(() => {
        const [moved] = S.pages.splice(from, 1);
        S.pages.splice(i, 0, moved);
      }, { detail: false });
    },
  },
  h('span', { class: 'name' }, p.name),
  p.id === S.settings.startPage && h('span', { class: 'badge' }, 'Start'),
  h('span', { class: 'count' }, String(p.buttons.length)))));
}

function renderTemplateState() {
  for (const btn of document.querySelectorAll('.tpl[data-template]')) {
    const exists = S.pages.some((p) => p.template === btn.dataset.template);
    btn.classList.toggle('exists', exists);
    btn.title = exists ? 'Schon vorhanden – klicken zum Aktualisieren oder Öffnen' : 'Seite anlegen';
  }
}

function renderToolbar() {
  const p = page();
  setIfChanged($('#page-name'), p.name);
  setIfChanged($('#page-cols'), p.cols);
  setIfChanged($('#page-rows'), p.rows);
  setIfChanged($('#page-bg'), p.background);
  $('#page-delete').disabled = S.pages.length < 2;
}

let dragId = null;

function renderCanvas() {
  const p = page();
  const canvas = $('#canvas');
  setupGrid(canvas, p);
  const covered = new Set();
  for (const b of p.buttons) for (let r = b.row; r < b.row + b.rowSpan; r++) for (let c = b.col; c < b.col + b.colSpan; c++) covered.add(`${r}:${c}`);

  const dropHandlers = (el, onDrop) => {
    el.addEventListener('dragover', (e) => {
      if (!dragId) return;
      e.preventDefault();
      el.classList.add('drop-target');
    });
    el.addEventListener('dragleave', () => el.classList.remove('drop-target'));
    el.addEventListener('drop', (e) => {
      e.preventDefault();
      el.classList.remove('drop-target');
      if (dragId) onDrop(dragId);
    });
  };

  const cells = [];
  for (let r = 0; r < p.rows; r++) {
    for (let c = 0; c < p.cols; c++) {
      if (covered.has(`${r}:${c}`)) continue;
      const cell = h('div', { class: 'cell', style: `grid-row:${r + 1};grid-column:${c + 1}`, title: 'Neuer Button', onclick: () => addButton(r, c) }, '＋');
      dropHandlers(cell, (id) => moveButton(id, r, c));
      cells.push(cell);
    }
  }

  const buttons = p.buttons.map((b) => {
    const el = buttonElement(b, { on: false, live: b.mode === 'slider' ? { value: 50 } : null });
    if (b.id === S.selId) el.classList.add('selected');
    const badges = [];
    if (b.mode !== 'slider' && b.live.source !== 'none') badges.push('LIVE');
    if (b.events?.length) badges.push('EVENT');
    if (b.holdActions?.length) badges.push('HALTEN');
    if (badges.length) {
      el.dataset.badges = badges.join(' · ');
      el.classList.add('has-badges');
    }
    el.draggable = true;
    el.addEventListener('click', () => select(b.id));
    el.addEventListener('dragstart', (e) => {
      dragId = b.id;
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/x-button', b.id);
      requestAnimationFrame(() => el.classList.add('dragging'));
    });
    el.addEventListener('dragend', () => {
      dragId = null;
      el.classList.remove('dragging');
    });
    dropHandlers(el, (id) => swapButtons(id, b.id));
    return el;
  });
  canvas.replaceChildren(...cells, ...buttons);
}

// ---------- Seiten ----------

export function selectPage(id) {
  S.pageId = id;
  S.selId = null;
  render();
}

function addPage() {
  const cur = page();
  const p = { id: uid('p_'), name: 'Neue Seite', rows: cur?.rows ?? 3, cols: cur?.cols ?? 5, background: cur?.background ?? '#14161b', buttons: [] };
  mutate(() => {
    S.pages.push(p);
    S.pageId = p.id;
    S.selId = null;
  });
  $('#page-name').select();
}

function duplicatePage() {
  const copy = structuredClone(page());
  copy.id = uid('p_');
  copy.name = `${copy.name} (Kopie)`;
  copy.template = '';
  copy.buttons = copy.buttons.map(cloneButton);
  mutate(() => {
    S.pages.splice(S.pages.indexOf(page()) + 1, 0, copy);
    S.pageId = copy.id;
    S.selId = null;
  });
}

function deletePage() {
  const p = page();
  if (S.pages.length < 2) return;
  if (p.buttons.length && !confirm(`Seite „${p.name}“ mit ${p.buttons.length} Buttons löschen?`)) return;
  mutate(() => {
    const i = S.pages.indexOf(p);
    S.pages.splice(i, 1);
    S.pageId = S.pages[Math.max(0, i - 1)].id;
    S.selId = null;
  });
}

function resizePage(rows, cols) {
  const p = page();
  rows = Math.min(12, Math.max(1, rows || p.rows));
  cols = Math.min(16, Math.max(1, cols || p.cols));
  const lost = p.buttons.filter((b) => b.row >= rows || b.col >= cols);
  if (lost.length && !confirm(`${lost.length} Button(s) liegen außerhalb des neuen Rasters und werden entfernt. Fortfahren?`)) return render({ detail: false });
  mutate(() => {
    p.rows = rows;
    p.cols = cols;
    p.buttons = p.buttons.filter((b) => !lost.includes(b));
    for (const b of p.buttons) {
      b.rowSpan = Math.min(b.rowSpan, rows - b.row);
      b.colSpan = Math.min(b.colSpan, cols - b.col);
    }
    if (!sel()) S.selId = null;
  });
}

// ---------- Buttons ----------

export function select(id) {
  if (S.selId !== id) S.expanded.clear();
  S.selId = id;
  render();
}

function addButton(row, col) {
  const b = newButton(row, col);
  S.tab = 'look';
  mutate(() => {
    page().buttons.push(b);
    S.selId = b.id;
  });
  $('#inspector input')?.focus();
}

function moveButton(id, row, col) {
  const p = page();
  const b = p.buttons.find((x) => x.id === id);
  if (!b) return;
  if (!fits(p, { row, col, rowSpan: b.rowSpan, colSpan: b.colSpan }, [id])) return toast('Der Button passt dort nicht hin.', 'error');
  mutate(() => Object.assign(b, { row, col }));
}

function swapButtons(idA, idB) {
  if (idA === idB) return;
  const p = page();
  const a = p.buttons.find((x) => x.id === idA);
  const b = p.buttons.find((x) => x.id === idB);
  const ra = { row: b.row, col: b.col, rowSpan: a.rowSpan, colSpan: a.colSpan };
  const rb = { row: a.row, col: a.col, rowSpan: b.rowSpan, colSpan: b.colSpan };
  if (!fits(p, ra, [idA, idB]) || !fits(p, rb, [idA, idB]) || overlaps(ra, rb)) return toast('Die beiden Buttons lassen sich wegen ihrer Größe nicht tauschen.', 'error');
  mutate(() => {
    Object.assign(a, { row: ra.row, col: ra.col });
    Object.assign(b, { row: rb.row, col: rb.col });
  });
}

export function duplicateButton() {
  const b = sel();
  if (!b) return;
  const spot = freeCell(page());
  if (!spot) return toast('Auf dieser Seite ist kein Feld mehr frei.', 'error');
  const copy = { ...cloneButton(b), ...spot, rowSpan: 1, colSpan: 1 };
  mutate(() => {
    page().buttons.push(copy);
    S.selId = copy.id;
  });
}

export function deleteButton() {
  const b = sel();
  if (!b) return;
  mutate(() => {
    page().buttons = page().buttons.filter((x) => x !== b);
    S.selId = null;
  });
}

function resizeButton(b, rowSpan, colSpan) {
  const p = page();
  const rect = { row: b.row, col: b.col, rowSpan: Math.max(1, rowSpan || 1), colSpan: Math.max(1, colSpan || 1) };
  if (!fits(p, rect, [b.id])) {
    toast('So groß passt der Button nicht – andere Buttons oder der Rand sind im Weg.', 'error');
    return render();
  }
  mutate(() => Object.assign(b, { rowSpan: rect.rowSpan, colSpan: rect.colSpan }));
}

// ---------- Eigenschaften ----------

function renderInspector() {
  const root = $('#inspector');
  const b = sel();
  if (!b) {
    root.replaceChildren(empty('▦',
      'Wähle einen Button aus oder klicke auf ein leeres Feld, um einen neuen anzulegen.',
      'Mit „Vorschau“ siehst du das Bedienfeld so, wie es auf dem Handy erscheint.'));
    return;
  }
  const counts = {
    actions: b.mode === 'slider' ? null : b.actions.length + (b.mode === 'toggle' ? b.actionsOff.length : 0),
    events: b.events.length,
    hold: b.holdActions.length + b.releaseActions.length,
  };
  const tab = (id, label) => h('button', { class: S.tab === id ? 'active' : '', onclick: () => { S.tab = id; render(); } }, label);
  if (b.mode === 'slider' && S.tab === 'hold') S.tab = 'look';
  const content = {
    look: lookTab,
    actions: b.mode === 'slider' ? sliderTab : pressTab,
    events: eventsTab,
    hold: holdTab,
  }[S.tab] ?? lookTab;
  root.replaceChildren(
    h('div', { class: 'insp-head' },
      h('h2', {}, b.title || 'Button'),
      b.mode !== 'slider' && h('button', { class: 'btn small primary', title: 'Aktionen jetzt auf dem PC ausführen', onclick: () => testActions(b.actions, 'Button', { buttonId: b.id }) }, '▶ Testen')),
    h('div', { class: 'tabs' },
      tab('look', 'Aussehen'),
      tab('actions', b.mode === 'slider' ? 'Regler' : `Drücken${counts.actions ? ` (${counts.actions})` : ''}`),
      tab('events', `Event${counts.events ? ` (${counts.events})` : ''}`),
      b.mode !== 'slider' && tab('hold', `Halten${counts.hold ? ` (${counts.hold})` : ''}`)),
    ...content(b).filter(Boolean),
    h('div', { class: 'insp-footer' },
      h('button', { class: 'btn small', onclick: duplicateButton }, 'Duplizieren'),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn small ghost danger', onclick: deleteButton }, 'Button löschen')),
  );
}

function lookTab(b) {
  const edit = (key, fn, opts) => mutate(fn, { coalesce: `${b.id}:${key}`, ...opts });
  const p = page();

  const iconBox = h('div', { class: 'icon-box' });
  if (b.icon.type === 'image' && b.icon.value) iconBox.append(h('img', { src: iconUrl(b.icon.value), alt: '' }));
  else iconBox.textContent = b.icon.value;

  const upload = h('input', {
    type: 'file', accept: 'image/png,image/jpeg,image/gif,image/webp,image/svg+xml', hidden: true,
    onchange: async (e) => {
      const file = e.target.files[0];
      if (!file) return;
      if (file.size > 5 * 1024 * 1024) return toast('Das Bild ist größer als 5 MB.', 'error');
      const data = await new Promise((res) => {
        const r = new FileReader();
        r.onload = () => res(r.result);
        r.readAsDataURL(file);
      });
      try {
        const { name } = await api('POST', '/api/icons', { data });
        S.icons.push(name);
        mutate(() => (b.icon = { type: 'image', value: name }));
      } catch (err) {
        toast(err.message, 'error');
      }
    },
  });

  return [
    h('label', { class: 'field' }, 'Beschriftung ({Wert} zeigt einen Wert an)',
      h('input', { value: b.title, maxlength: 200, placeholder: 'z. B. Lauter', oninput: (e) => edit('title', () => (b.title = e.target.value), { detail: false }) })),
    h('div', { class: 'field' }, 'Position der Beschriftung',
      segmented([['top', 'Oben'], ['center', 'Mitte'], ['bottom', 'Unten'], ['hidden', 'Aus']], b.titlePos, (v) => mutate(() => (b.titlePos = v)))),
    h('label', { class: 'field' }, `Schriftgröße: ${b.fontSize}`,
      h('input', { type: 'range', min: 8, max: 64, value: b.fontSize, oninput: (e) => {
        e.target.parentElement.firstChild.textContent = `Schriftgröße: ${e.target.value}`;
        edit('fs', () => (b.fontSize = Number(e.target.value)), { detail: false });
      } })),

    h('h3', {}, 'Symbol'),
    h('div', { class: 'icon-preview' },
      iconBox,
      h('input', { value: b.icon.type === 'emoji' ? b.icon.value : '', maxlength: 16, placeholder: 'Emoji eingeben', style: 'flex:1',
        oninput: (e) => edit('emoji', () => (b.icon = { type: 'emoji', value: e.target.value.trim() }), { detail: false }) }),
      h('label', { class: 'btn small', title: 'Eigenes Bild hochladen' }, '🖼️ Bild', upload),
      b.icon.value && h('button', { class: 'btn small ghost', title: 'Symbol entfernen', onclick: () => mutate(() => (b.icon = { type: 'emoji', value: '' })) }, '✕')),
    b.icon.type === 'image' && b.icon.value && h('label', { class: 'switch-row' },
      h('input', { type: 'checkbox', checked: b.iconFit === 'cover', onchange: (e) => mutate(() => (b.iconFit = e.target.checked ? 'cover' : 'contain')) }),
      'Bild füllt den ganzen Button'),
    h('div', { class: 'emoji-grid' }, EMOJIS.map((em) => h('button', { type: 'button', onclick: () => mutate(() => (b.icon = { type: 'emoji', value: em })) }, em))),
    S.icons.length > 0 && h('div', {},
      h('button', { class: 'btn small ghost', style: 'margin-top:8px', onclick: () => { S.showIcons = !S.showIcons; render(); } },
        `${S.showIcons ? '▾' : '▸'} Eigene Bilder (${S.icons.length})`),
      S.showIcons && h('div', { class: 'icon-grid' }, S.icons.map((name) => h('button', { type: 'button', title: name, onclick: () => mutate(() => (b.icon = { type: 'image', value: name })) }, h('img', { src: iconUrl(name), alt: '', loading: 'lazy' }))))),

    h('h3', {}, 'Farben'),
    h('div', { class: 'colors' },
      h('label', {}, h('input', { type: 'color', value: b.color, oninput: (e) => edit('color', () => (b.color = e.target.value), { detail: false }) }), 'Hintergrund'),
      h('label', {}, h('input', { type: 'color', value: b.textColor, oninput: (e) => edit('tc', () => (b.textColor = e.target.value), { detail: false }) }), 'Text'),
      b.mode === 'toggle' && h('label', {}, h('input', { type: 'color', value: b.colorOn, oninput: (e) => edit('con', () => (b.colorOn = e.target.value), { detail: false }) }), 'Wenn an')),
    h('div', { class: 'swatches' }, SWATCHES.map((c) => h('button', { type: 'button', title: c, style: `background:${c}`, onclick: () => mutate(() => (b.color = c)) }))),

    h('h3', {}, 'Größe & Verhalten'),
    h('div', { class: 'row' },
      h('label', { class: 'field' }, 'Breite (Felder)',
        h('input', { type: 'number', min: 1, max: p.cols - b.col, value: b.colSpan, onchange: (e) => resizeButton(b, b.rowSpan, Number(e.target.value)) })),
      h('label', { class: 'field' }, 'Höhe (Felder)',
        h('input', { type: 'number', min: 1, max: p.rows - b.row, value: b.rowSpan, onchange: (e) => resizeButton(b, Number(e.target.value), b.colSpan) }))),
    h('div', { class: 'field' }, 'Art',
      segmented([['press', 'Drücken'], ['toggle', 'Umschalter'], ['slider', 'Regler']], b.mode, (v) => mutate(() => (b.mode = v)))),
    b.mode !== 'slider' && liveSection(b),
  ];
}

function liveSection(b) {
  const hints = {
    steam_running: 'Leuchtet, solange das Spiel aus der Aktion „Spiel starten“ läuft.',
    obs_scene: 'Leuchtet, wenn die Szene aus der Aktion „Szene wechseln“ gerade live ist.',
    temp: 'Leuchtet ab 80 °C.',
    mic: 'Leuchtet, wenn das Mikrofon stumm ist.',
    command: 'Die erste Zeile der Ausgabe wird angezeigt (alle 5 Sekunden neu).',
    variable: 'Leuchtet, wenn der Wert „an“, „ja“, „true“ oder 1 ist. Tipp: In der Beschriftung zeigt {Name} den Wert ebenfalls an.',
  };
  return h('div', {},
    h('h3', {}, 'Live-Anzeige'),
    h('label', { class: 'field' }, 'Wert auf dem Button anzeigen',
      h('select', { value: b.live.source, onchange: (e) => mutate(() => (b.live.source = e.target.value)) },
        S.liveSources.map((o) => h('option', { value: o.value }, o.label)))),
    b.live.source === 'command' && h('label', { class: 'field' }, 'Befehl',
      h('input', { value: b.live.command, placeholder: 'z. B. uptime -p', oninput: (e) => mutate(() => (b.live.command = e.target.value), { coalesce: `${b.id}:livecmd`, detail: false }) })),
    b.live.source === 'variable' && dynamicSelect({ label: 'Wert', source: 'variables', allowNew: true }, b.live.variable,
      (v) => mutate(() => (b.live.variable = v), { coalesce: `${b.id}:livevar`, detail: false })),
    hints[b.live.source] && h('p', { class: 'hint-text' }, hints[b.live.source]));
}

function pressTab(b) {
  if (b.mode === 'toggle') {
    return [
      actionList(b, 'actions', 'Beim Einschalten', { buttonId: b.id, compact: true, context: b.title || 'Button' }),
      actionList(b, 'actionsOff', 'Beim Ausschalten', { buttonId: b.id, compact: true, context: b.title || 'Button' }),
    ];
  }
  return [actionList(b, 'actions', 'Beim Drücken', { buttonId: b.id, compact: true, context: b.title || 'Button', hint: b.holdActions.length ? 'Kurz tippen führt diese Aktionen aus, langes Drücken die unter „Halten“.' : '' })];
}

function sliderTab(b) {
  const sl = b.slider;
  const edit = (fn, key) => mutate(fn, { coalesce: key && `${b.id}:${key}`, detail: !key });
  return [
    h('p', { class: 'hint-text' }, 'Auf dem Handy über den Button wischen, um den Wert einzustellen. Ist der Button höher als breit, ist der Regler senkrecht.'),
    h('label', { class: 'field' }, 'Was soll der Regler steuern?',
      h('select', { value: sl.target, onchange: (e) => edit(() => (sl.target = e.target.value)) }, S.sliderTargets.map((o) => h('option', { value: o.value }, o.label)))),
    sl.target === 'obs_volume' && dynamicSelect({ key: 'input', label: 'OBS-Audioquelle', source: 'obs-inputs' }, sl.input, (v) => edit(() => (sl.input = v))),
    sl.target === 'command' && h('label', { class: 'field' }, 'Befehl ({value} wird durch 0–100 ersetzt)',
      h('textarea', { value: sl.command, placeholder: 'z. B. ddcutil setvcp 10 {value}', oninput: (e) => edit(() => (sl.command = e.target.value), 'slcmd') })),
  ];
}

function eventsTab(b) {
  return [
    h('p', { class: 'hint-text' }, 'Der Button reagiert selbst auf Ereignisse – z. B. Farbe oder Beschriftung ändern („Visuelles“ → „dieser Button“), wenn ein Wert sich ändert oder OBS live geht.'),
    ...b.events.map((ev, i) => h('div', { class: `event-card${ev.enabled ? '' : ' disabled'}` },
      h('div', { class: 'list-title' },
        h('h3', {}, `Event ${i + 1}`),
        h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: ev.enabled, onchange: (e) => mutate(() => (ev.enabled = e.target.checked)) }), 'aktiv'),
        h('button', { class: 'btn icon ghost danger', title: 'Event entfernen', onclick: () => mutate(() => b.events.splice(i, 1)) }, '✕')),
      h('p', { class: 'muted-note' }, triggerSummary(ev.trigger)),
      triggerEditor(ev.trigger),
      actionList(ev, 'actions', 'Dann …', { buttonId: b.id, compact: true, context: `${b.title || 'Button'} · Event ${i + 1}` }))),
    h('button', { class: 'btn', onclick: () => mutate(() => b.events.push({ id: uid('e_'), enabled: true, trigger: newTrigger(), actions: [] })) }, '＋ Event hinzufügen'),
  ];
}

function holdTab(b) {
  return [
    h('p', { class: 'hint-text' }, 'Lange drücken (ab ca. 0,5 s) führt diese Aktionen statt der normalen aus. Beim Loslassen danach laufen die unteren.'),
    actionList(b, 'holdActions', 'Beim Gedrückthalten', { buttonId: b.id, compact: true, context: b.title || 'Button' }),
    h('label', { class: 'field' }, 'Wiederholen, solange gedrückt (ms, 0 = nur einmal)',
      h('input', { type: 'number', min: 0, max: 10000, step: 50, value: b.holdRepeat, onchange: (e) => mutate(() => (b.holdRepeat = Math.max(0, Number(e.target.value) || 0))) })),
    actionList(b, 'releaseActions', 'Beim Loslassen (nach dem Halten)', { buttonId: b.id, compact: true, context: b.title || 'Button' }),
  ];
}

// ---------- Fertige Seiten ----------

function tplBtn(row, col, title, emoji, actions = [], extra = {}) {
  return newButton(row, col, { title, icon: { type: 'emoji', value: emoji }, actions: actions.map(([type, params = {}]) => ({ id: uid('a_'), type, enabled: true, params })), ...extra });
}

const TEMPLATE_NAMES = { steam: 'Steam-Spiele', system: 'System', obs: 'OBS', touchportal: 'Touch Portal' };

/** Fragt vor dem Anlegen nach; gibt es die Seite schon: aktualisieren, öffnen oder abbrechen. */
async function askTemplate(template, createText) {
  const existing = S.pages.filter((p) => p.template === template);
  const name = TEMPLATE_NAMES[template];
  if (!existing.length) return ask(`Seite „${name}“ anlegen?`, createText, [{ value: 'create', label: 'Anlegen', kind: 'primary' }]);
  const r = await ask(`„${name}“ gibt es schon`,
    `${existing.length > 1 ? `Es gibt ${existing.length} Seiten aus dieser Vorlage.` : 'Diese Seite ist schon angelegt.'} Beim Aktualisieren wird sie neu erzeugt – eigene Änderungen daran gehen verloren, Verweise von anderen Seiten bleiben erhalten.`,
    [{ value: 'open', label: 'Vorhandene öffnen' }, { value: 'update', label: 'Aktualisieren', kind: 'primary' }]);
  if (r === 'open') selectPage(existing[0].id);
  return r === 'update' ? 'update' : null;
}

function addTemplatePage(pg, template) {
  pg.template = template;
  const existing = S.pages.find((p) => p.template === template);
  mutate(() => {
    if (existing) {
      pg.id = existing.id;
      S.pages = S.pages.filter((p) => p.template !== template || p === existing).map((p) => (p === existing ? pg : p));
    } else {
      S.pages.push(pg);
    }
    S.pageId = pg.id;
    S.selId = null;
  });
  toast(`Seite „${pg.name}“ ${existing ? 'aktualisiert' : 'angelegt'} ✓`, 'ok');
}

async function systemTemplate() {
  if (!(await askTemplate('system', 'Eine Seite mit Uhrzeit, CPU, RAM, Temperatur, Musik, Ton, Mikrofon, Audioausgang, Bildschirmfoto, Nachtlicht, Sperren und einem Lautstärkeregler.'))) return;
  const live = (source) => ({ live: { source, command: '', variable: '' } });
  addTemplatePage({
    id: uid('p_'), name: 'System', rows: 3, cols: 5, background: '#14161b',
    buttons: [
      tplBtn(0, 0, 'Uhrzeit', '🕐', [], live('clock')),
      tplBtn(0, 1, 'CPU', '🧠', [], live('cpu')),
      tplBtn(0, 2, 'RAM', '💾', [], live('ram')),
      tplBtn(0, 3, 'Temperatur', '🌡️', [], { ...live('temp'), colorOn: '#a33a2a' }),
      tplBtn(0, 4, 'Lautstärke', '', [], { rowSpan: 3, mode: 'slider', color: '#3b3f8f' }),
      tplBtn(1, 0, 'Musik', '⏯️', [['media', { op: 'play-pause' }]], { colSpan: 2, ...live('track'), colorOn: '#2f6e4f' }),
      tplBtn(1, 2, 'Ton aus', '🔇', [['volume', { op: 'mute' }]], { ...live('volume'), colorOn: '#8f3b3b' }),
      tplBtn(1, 3, 'Mikrofon', '🎙️', [['mic_mute', { op: 'toggle' }]], { ...live('mic'), colorOn: '#8f3b3b' }),
      tplBtn(2, 0, 'Ausgang', '🎧', [['audio_output', { target: 'next' }]]),
      tplBtn(2, 1, 'Foto', '📸', [['screenshot', { mode: 'dialog' }]]),
      tplBtn(2, 2, 'Nachtlicht', '🌙', [['night_light', { mode: 'toggle' }]]),
      tplBtn(2, 3, 'Sperren', '🔒', [['lock_screen']], { color: '#5b3fa0' }),
    ],
  }, 'system');
}

async function obsTemplate() {
  if (!(await askTemplate('obs', 'Ein Button pro OBS-Szene (leuchtet, wenn live), dazu Stream, Aufnahme, Pause und Clip.'))) return;
  let scenes = [];
  try {
    scenes = (await api('GET', '/api/options/obs-scenes')).map((o) => o.value);
  } catch (err) {
    toast(`OBS ist gerade nicht erreichbar – die Szenen-Buttons kannst du später auswählen.\n${err.message}`, 'error');
  }
  if (!S.apps) S.apps = await api('GET', '/api/apps').catch(() => []);
  const obsApp = S.apps.find((a) => /obs/i.test(a.name));
  const cols = 5;
  const sceneList = scenes.length ? scenes.slice(0, cols * 2) : [''];
  const sceneRows = Math.ceil(sceneList.length / cols);
  const live = (source) => ({ source, command: '', variable: '' });
  const buttons = sceneList.map((scene, i) => tplBtn(Math.floor(i / cols), i % cols, scene || 'Szene', '🎬',
    [['obs_scene', { scene }]], { live: live('obs_scene'), colorOn: '#2f7a4a' }));
  const r = sceneRows;
  buttons.push(
    tplBtn(r, 0, 'Stream', '📡', [['obs_stream', { op: 'toggle' }]], { live: live('obs_stream'), colorOn: '#b3262d' }),
    tplBtn(r, 1, 'Aufnahme', '⏺️', [['obs_record', { op: 'toggle' }]], { live: live('obs_record'), colorOn: '#b3262d' }),
    tplBtn(r, 2, 'Pause', '⏸️', [['obs_record', { op: 'pause' }]]),
    tplBtn(r, 3, 'Clip', '⏪', [['obs_replay', { op: 'save' }]]),
    obsApp
      ? tplBtn(r, 4, 'OBS starten', '🚀', [['launch_app', { app: obsApp.id, appName: obsApp.name }]])
      : tplBtn(r, 4, 'Virtuelle Kamera', '📷', [['obs_misc', { op: 'virtualcam' }]]),
  );
  addTemplatePage({ id: uid('p_'), name: 'OBS', rows: sceneRows + 1, cols, background: '#14161b', buttons }, 'obs');
}

/** Für Vorlagen, die der Server erzeugt (Steam, Touch Portal): Ergebnis übernehmen. */
async function serverTemplate(url, apply) {
  await save();
  try {
    const r = await api('POST', url, {});
    pushUndo();
    S.pages = r.pages;
    S.revision = r.revision;
    S.selId = null;
    S.icons = await api('GET', '/api/icons');
    apply(r);
    render();
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function steamTemplate() {
  if (!(await askTemplate('steam', 'Alle installierten Steam-Spiele mit Cover. Ein Spiel leuchtet, solange es läuft; unten Bibliothek, Downloads, Big Picture und Freunde.'))) return;
  await serverTemplate('/api/steam/pages', (r) => {
    S.pageId = r.created[0];
    toast(`${r.created.length} Steam-Seite(n) ${r.updated ? 'aktualisiert' : 'angelegt'} ✓\nDie Buttons leuchten grün, solange ein Spiel läuft.`, 'ok');
  });
}

async function importTouchPortal() {
  if (!(await askTemplate('touchportal', 'Übernimmt alle Seiten aus Touch Portal (~/.config/TouchPortal) mit Farben, Bildern und Aktionen.'))) return;
  await serverTemplate('/api/import/touchportal', (r) => {
    S.pageId = S.pages.find((p) => p.template === 'touchportal')?.id ?? S.pageId;
    const st = r.stats;
    const missing = Object.entries(st.unsupported);
    toast(`${r.updated ? 'Aktualisiert' : 'Übernommen'}: ${st.pages} Seite(n), ${st.buttons} Buttons, ${st.actions} Aktionen, ${st.icons} Bilder.${missing.length ? `\nNoch nicht unterstützt: ${missing.map(([k, n]) => `${k} (${n}×)`).join(', ')}` : ''}`, 'ok');
  });
}
