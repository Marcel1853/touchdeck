// Bereich „Seiten“: Seitenliste, Raster mit Buttons und die Eigenschaften des gewählten Buttons.
import { t as tr, getLang } from '/shared/i18n.js';
import { buttonElement, setupGrid, iconUrl, isFree } from '/shared/render.js';
import { $, h, api, toast, ask, uid, segmented, setIfChanged, empty, prefs, contextMenu, saveFile } from '../dom.js';
import { S, page, sel, mutate, render, save, pushUndo } from '../state.js';
import { iconPicker, readDataUrl } from './iconpacks.js';
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

// ---------- Freie Seiten ----------

const RATIOS = ['16:9', '16:10', '3:2', '4:3', '1:1', '3:4', '10:16', '9:16'];
const SNAP = 0.5;
const snap = (n) => Math.round(n / SNAP) * SNAP;
const clampNum = (n, min, max) => Math.min(max, Math.max(min, n));

/** Lage eines Buttons auf der freien Fläche, begrenzt auf die Seite. */
function fitFree(f) {
  const w = clampNum(snap(f.w), 2, 100);
  const h = clampNum(snap(f.h), 2, 100);
  return { x: clampNum(snap(f.x), 0, 100 - w), y: clampNum(snap(f.y), 0, 100 - h), w, h };
}

/** Anteil (0–1) vom Anfang bis zum Anfang von Spalte/Zeile i, mit eigenen Größen. */
function offsets(sizes, count) {
  const list = sizes?.length === count ? sizes : Array(count).fill(1);
  const total = list.reduce((a, n) => a + n, 0);
  const starts = [0];
  for (const n of list) starts.push(starts.at(-1) + n / total);
  return starts;
}

/** Rasterfeld eines Buttons als freie Lage (mit etwas Abstand), z. B. beim Wechsel auf „Frei“. */
function freeFromCell(p, b) {
  const xs = offsets(p.colSizes, p.cols);
  const ys = offsets(p.rowSizes, p.rows);
  const x0 = xs[b.col], x1 = xs[Math.min(p.cols, b.col + b.colSpan)];
  const y0 = ys[b.row], y1 = ys[Math.min(p.rows, b.row + b.rowSpan)];
  const padX = (x1 - x0) * 0.05 / b.colSpan, padY = (y1 - y0) * 0.05 / b.rowSpan;
  return fitFree({ x: (x0 + padX) * 100, y: (y0 + padY) * 100, w: (x1 - x0 - 2 * padX) * 100, h: (y1 - y0 - 2 * padY) * 100 });
}

/** Neue Größe für einen Button auf einer freien Seite: ungefähr quadratisch. */
function defaultFree(p, cx, cy) {
  const w = 12;
  const h = clampNum((w * p.ratioW) / p.ratioH, 4, 40);
  return fitFree({ x: cx - w / 2, y: cy - h / 2, w, h });
}

function cloneButton(b) {
  const copy = structuredClone(b);
  copy.id = uid('b_');
  for (const list of [copy.actions, copy.actionsOff, copy.holdActions ?? [], copy.releaseActions ?? [], copy.doubleActions ?? [], ...(copy.events ?? []).map((e) => e.actions)]) {
    for (const a of list) a.id = uid('a_');
  }
  for (const e of copy.events ?? []) e.id = uid('e_');
  return copy;
}

export function newButton(row, col, extra = {}) {
  return {
    id: uid('b_'), row, col, rowSpan: 1, colSpan: 1, title: '', icon: { type: 'emoji', value: '' },
    color: '#2b2f3a', colorOn: '#2f9e6b', bgOpacity: 100, textColor: '#ffffff', fontSize: 14, fontFamily: '', bold: true, italic: false, textOutline: 0,
    titlePos: 'bottom', mode: 'press', iconFit: 'contain',
    actions: [], actionsOff: [], holdActions: [], releaseActions: [], doubleActions: [], holdRepeat: 0, events: [],
    iconOn: { type: 'emoji', value: '' }, titleOn: '',
    live: { source: 'none', command: '', variable: '' }, slider: { target: 'volume', input: '', command: '' },
    ...extra,
  };
}

// ---------- Aufbau ----------

export function mountPages(root) {
  root.className = 'view view-pages';
  root.replaceChildren(
    h('aside', { class: 'sidebar' },
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('common.pages')),
        h('div', { class: 'head-buttons' },
          h('button', { class: 'btn small ghost', title: tr('pages.exportImport'), onclick: (e) => {
            const r = e.currentTarget.getBoundingClientRect();
            profileMenu(r.left, r.bottom + 4, page());
          } }, '⋯'),
          h('button', { class: 'btn small', title: tr('pages.newPage'), onclick: addPage }, '＋'))),
      h('ul', { id: 'page-list', class: 'page-list' }),
      h('div', { class: `templates${prefs.get('touchdeck.templatesOpen') === '1' ? '' : ' collapsed'}` },
        h('button', {
          type: 'button', class: 'templates-toggle', title: 'Ein-/ausklappen',
          onclick: (e) => {
            const box = e.currentTarget.parentElement;
            box.classList.toggle('collapsed');
            prefs.set('touchdeck.templatesOpen', box.classList.contains('collapsed') ? '0' : '1');
          },
        }, h('span', { class: 'chevron' }, '▸'), tr('pages.readyMadePages')),
        tplButton('steam', '🎮', tr('pages.steamGames'), tr('pages.allGamesWithCover'), steamTemplate),
        tplButton('system', '📊', tr('common.system'), tr('pages.clockCpuAudioLock'), systemTemplate),
        tplButton('obs', '🎬', tr('pages.obsStudio'), tr('pages.scenesStreamRecording'), obsTemplate),
        h('div', { id: 'plugin-pages' }))),
    h('section', { class: 'workspace' },
      h('div', { class: 'page-toolbar' },
        h('input', { id: 'page-name', class: 'page-name', maxlength: 100, 'aria-label': tr('pages.pageName'),
          oninput: (e) => mutate(() => (page().name = e.target.value || tr('pages.page')), { coalesce: `${S.pageId}:name`, detail: false }) }),
        h('div', { id: 'page-layout' }),
        h('label', { class: 'field-inline grid-only' }, tr('pages.columns'), h('input', { id: 'page-cols', type: 'number', min: 1, max: 16, onchange: (e) => resizePage(page().rows, Number(e.target.value)) })),
        h('label', { class: 'field-inline grid-only' }, tr('pages.rows'), h('input', { id: 'page-rows', type: 'number', min: 1, max: 12, onchange: (e) => resizePage(Number(e.target.value), page().cols) })),
        h('button', { class: 'btn small', title: tr('pages.newButton'), onclick: addButtonSomewhere }, tr('pages.addButton')),
        h('label', { class: 'field-inline free-only' }, tr('pages.ratio'),
          h('select', { id: 'page-ratio', onchange: (e) => setRatio(e.target.value) }, RATIOS.map((r) => h('option', { value: r }, r)))),
        h('label', { class: 'field-inline' }, tr('pages.background'), h('input', { id: 'page-bg', type: 'color',
          oninput: (e) => mutate(() => (page().background = e.target.value), { coalesce: `${S.pageId}:bg`, detail: false }) })),
        h('button', { class: 'btn small ghost', onclick: duplicatePage }, tr('common.duplicate')),
        h('button', { id: 'page-delete', class: 'btn small ghost danger', onclick: deletePage }, tr('common.delete'))),
      // Klick neben die Seite hebt die Auswahl auf (dann erscheinen die Seiten-Einstellungen).
      h('div', { class: 'canvas-wrap', onclick: (e) => e.target === e.currentTarget && S.selId && select(null) }, h('div', { id: 'canvas', class: 'td-grid canvas' })),
      h('p', { id: 'canvas-hint', class: 'hint' }, tr('pages.clickEmptyFieldNew'))),
    h('aside', { id: 'inspector', class: 'inspector' }),
  );
  loadPluginPages();
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
    title: tr('pages.dragToReorder'),
    onclick: () => selectPage(p.id),
    oncontextmenu: (e) => {
      e.preventDefault();
      profileMenu(e.clientX, e.clientY, p);
    },
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
  p.id === S.settings.startPage && h('span', { class: 'badge' }, tr('pages.start')),
  h('span', { class: 'count' }, String(p.buttons.length)))));
}

function renderTemplateState() {
  for (const btn of document.querySelectorAll('.tpl[data-template]')) {
    const exists = S.pages.some((p) => p.template === btn.dataset.template);
    btn.classList.toggle('exists', exists);
    btn.title = exists ? tr('pages.alreadyThereClickTo') : tr('pages.createPage');
  }
}

function renderToolbar() {
  const p = page();
  const free = isFree(p);
  $('.page-toolbar').classList.toggle('is-free', free);
  $('#page-layout').replaceChildren(segmented([['grid', tr('pages.layoutGrid')], ['free', tr('pages.layoutFree')]], free ? 'free' : 'grid', setLayout));
  const ratio = `${p.ratioW ?? 16}:${p.ratioH ?? 10}`;
  const select = $('#page-ratio');
  if (![...select.options].some((o) => o.value === ratio)) select.append(h('option', { value: ratio }, ratio));
  setIfChanged(select, ratio);
  $('#canvas-hint').textContent = free ? tr('pages.freeHint') : tr('pages.clickEmptyFieldNew');
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
  canvas.onpointerdown = null;
  canvas.onpointerup = null;
  canvas.ondblclick = null;
  canvas.oncontextmenu = null;
  if (isFree(p)) return renderFreeCanvas(p, canvas);
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
      // Ein Klick hebt nur die Auswahl auf; neuer Button per Doppelklick, Rechtsklick oder „＋ Button“.
      const cell = h('div', { class: 'cell', style: `grid-row:${r + 1};grid-column:${c + 1}`, title: tr('pages.doubleClickNewButton'),
        onclick: () => S.selId && select(null), ondblclick: () => addButton(r, c),
        oncontextmenu: (e) => {
          e.preventDefault();
          cellMenu(e.clientX, e.clientY, r, c);
        } }, '＋');
      dropHandlers(cell, (id) => moveButton(id, r, c));
      cells.push(cell);
    }
  }

  const buttons = p.buttons.map((b) => {
    const el = buttonElement(b, { on: false, live: b.mode === 'slider' ? { value: 50 } : null, page: p });
    if (b.id === S.selId) el.classList.add('selected');
    addBadges(el, b);
    el.draggable = true;
    el.addEventListener('click', () => select(b.id));
    el.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      if (S.selId !== b.id) select(b.id);
      buttonMenu(e.clientX, e.clientY, b.id);
    });
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

/** Kleine Schilder oben am Button: LIVE, EVENT, HALTEN. */
function addBadges(el, b) {
  const badges = [];
  if (b.mode !== 'slider' && b.live.source !== 'none') badges.push('LIVE');
  if (b.events?.length) badges.push('EVENT');
  if (b.holdActions?.length) badges.push(tr('pages.badgeHold'));
  if (b.doubleActions?.length) badges.push('2×');
  if (badges.length) {
    el.dataset.badges = badges.join(' · ');
    el.classList.add('has-badges');
  }
}

/** Mauszeiger in Prozent der Fläche. */
function pointInCanvas(canvas, e) {
  const r = canvas.getBoundingClientRect();
  return { x: ((e.clientX - r.left) / r.width) * 100, y: ((e.clientY - r.top) / r.height) * 100 };
}

/**
 * Freie Seite: Buttons mit der Maus ziehen (verschieben) oder an der Ecke unten rechts ziehen (Größe).
 * Während des Ziehens ändert sich nur die Anzeige, gespeichert wird beim Loslassen.
 */
function renderFreeCanvas(p, canvas) {
  // Klick auf die leere Fläche hebt nur die Auswahl auf; neue Buttons per Doppelklick,
  // Rechtsklick oder „＋ Button“ – damit ein Verklicken keinen Button anlegt.
  let pressedOnCanvas = false;
  canvas.onpointerdown = (e) => (pressedOnCanvas = e.target === canvas && e.button === 0);
  canvas.onpointerup = (e) => {
    if (!pressedOnCanvas || e.target !== canvas) return;
    pressedOnCanvas = false;
    if (S.selId) select(null);
  };
  canvas.ondblclick = (e) => {
    if (e.target !== canvas) return;
    const { x, y } = pointInCanvas(canvas, e);
    addFreeButton(x, y);
  };
  canvas.oncontextmenu = (e) => {
    if (e.target !== canvas) return;
    e.preventDefault();
    const { x, y } = pointInCanvas(canvas, e);
    contextMenu(e.clientX, e.clientY, [
      { icon: '＋', label: tr('pages.newButton'), onclick: () => addFreeButton(x, y) },
      { icon: '📥', label: tr('pages.pasteHere'), keys: 'Strg+V', disabled: !readClip(CLIP), onclick: () => pasteButton({ x, y }) },
      { icon: '📁', label: tr('pages.createFolderHere'), onclick: () => createFolder({ x, y }) },
    ]);
  };

  const buttons = p.buttons.map((b) => {
    b.free ??= freeFromCell(p, b);
    const el = buttonElement(b, { on: false, live: b.mode === 'slider' ? { value: 50 } : null, page: p });
    el.draggable = false;
    addBadges(el, b);
    if (b.id === S.selId) {
      el.classList.add('selected');
      el.append(h('div', { class: 'resize-handle', title: tr('pages.dragToResize') }));
    }
    el.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      if (S.selId !== b.id) select(b.id);
      buttonMenu(e.clientX, e.clientY, b.id);
    });
    el.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      const resizing = e.target.classList.contains('resize-handle');
      const rect = canvas.getBoundingClientRect();
      const from = { x: e.clientX, y: e.clientY };
      const orig = { ...b.free };
      let next = null;
      el.setPointerCapture(e.pointerId);
      el.classList.add('moving');
      const move = (ev) => {
        if (!next && Math.hypot(ev.clientX - from.x, ev.clientY - from.y) < 4) return;
        const dx = ((ev.clientX - from.x) / rect.width) * 100;
        const dy = ((ev.clientY - from.y) / rect.height) * 100;
        next = resizing
          ? fitFree({ ...orig, w: Math.min(orig.w + dx, 100 - orig.x), h: Math.min(orig.h + dy, 100 - orig.y) })
          : fitFree({ ...orig, x: orig.x + dx, y: orig.y + dy });
        Object.assign(el.style, { left: `${next.x}%`, top: `${next.y}%`, width: `${next.w}%`, height: `${next.h}%` });
      };
      const up = () => {
        el.removeEventListener('pointermove', move);
        el.classList.remove('moving');
        if (next) mutate(() => (b.free = next));
        else select(b.id);
      };
      el.addEventListener('pointermove', move);
      el.addEventListener('pointerup', up, { once: true });
      el.addEventListener('pointercancel', up, { once: true });
    });
    return el;
  });
  canvas.replaceChildren(...buttons);
}

/** „＋ Button“: auf freien Seiten in die Mitte, im Raster ins erste freie Feld. */
function addButtonSomewhere() {
  const p = page();
  if (isFree(p)) return addFreeButton(50, 50);
  const spot = freeCell(p);
  if (!spot) return toast(tr('pages.thereIsNoFree'), 'error');
  addButton(spot.row, spot.col);
}

function addFreeButton(x, y) {
  const p = page();
  let free = defaultFree(p, x, y);
  // Liegt dort schon ein Button genau gleich, etwas versetzen.
  while (p.buttons.some((o) => o.free && o.free.x === free.x && o.free.y === free.y) && free.x < 100 - free.w) free = fitFree({ ...free, x: free.x + 3, y: free.y + 3 });
  const b = newButton(0, 0, { free });
  S.tab = 'look';
  mutate(() => {
    p.buttons.push(b);
    S.selId = b.id;
  });
  $('#inspector input')?.focus();
}

/** Raster ↔ frei. Beim Wechsel ins Raster müssen die Buttons in freie Felder passen. */
function setLayout(layout) {
  const p = page();
  if ((p.layout ?? 'grid') === layout) return;
  if (layout === 'free') {
    return mutate(() => {
      for (const b of p.buttons) b.free = freeFromCell(p, b);
      // Seitenverhältnis wie das bisherige Raster, damit alles gleich aussieht.
      const w = p.colSizes?.length === p.cols ? p.colSizes.reduce((a, n) => a + n, 0) : p.cols;
      const hgt = p.rowSizes?.length === p.rows ? p.rowSizes.reduce((a, n) => a + n, 0) : p.rows;
      const scale = Math.max(1, Math.max(w, hgt) / 32);
      p.ratioW = Math.max(1, Math.round(w / scale));
      p.ratioH = Math.max(1, Math.round(hgt / scale));
      p.layout = 'free';
    });
  }
  // Raster: jeden Button an die Stelle, die seiner freien Lage am nächsten liegt, sonst ins erste freie Feld.
  const placed = { ...p, buttons: [] };
  const plan = [];
  const lost = [];
  for (const b of [...p.buttons].sort((a, c) => (a.free?.y ?? 0) - (c.free?.y ?? 0) || (a.free?.x ?? 0) - (c.free?.x ?? 0))) {
    const f = b.free ?? freeFromCell(p, b);
    const near = {
      row: clampNum(Math.floor((f.y / 100) * p.rows), 0, p.rows - 1),
      col: clampNum(Math.floor((f.x / 100) * p.cols), 0, p.cols - 1),
      rowSpan: 1, colSpan: 1,
    };
    const spot = fits(placed, near) ? near : freeCell(placed);
    if (!spot) {
      lost.push(b);
      continue;
    }
    const rect = { rowSpan: 1, colSpan: 1, ...spot };
    placed.buttons.push({ id: b.id, ...rect });
    plan.push([b, rect]);
  }
  if (lost.length && !confirm(tr('pages.buttonsDontFitGrid', [lost.length]))) return render({ detail: false });
  mutate(() => {
    for (const [b, rect] of plan) Object.assign(b, { row: rect.row, col: rect.col, rowSpan: rect.rowSpan, colSpan: rect.colSpan });
    p.buttons = p.buttons.filter((b) => !lost.includes(b));
    p.layout = 'grid';
    if (!sel()) S.selId = null;
  });
}

function setRatio(value) {
  const [w, hgt] = value.split(':').map(Number);
  mutate(() => Object.assign(page(), { ratioW: w, ratioH: hgt }));
}

/** Pfeiltasten: Button verschieben – frei um 0,5 % (mit Umschalt 5 %), im Raster um ein Feld. */
export function nudgeButton(dx, dy, big) {
  const b = sel();
  const p = page();
  if (!b) return;
  if (isFree(p)) {
    const step = big ? 5 : SNAP;
    return mutate(() => (b.free = fitFree({ ...b.free, x: b.free.x + dx * step, y: b.free.y + dy * step })), { coalesce: `${b.id}:nudge` });
  }
  const rect = { row: b.row + dy, col: b.col + dx, rowSpan: b.rowSpan, colSpan: b.colSpan };
  if (fits(p, rect, [b.id])) mutate(() => Object.assign(b, { row: rect.row, col: rect.col }), { coalesce: `${b.id}:nudge` });
}

// ---------- Profile (.tdprofile): Seiten mit Bildern, Vorlagen und Werten weitergeben ----------

async function exportProfile(pages, name) {
  await save();
  await saveFile({ kind: 'profile', name, pages: pages.map((p) => p.id) });
}

/** Fertige Seiten aus Plugins unter „Fertige Seiten“ zeigen (je .tdprofile-Datei eines eingeschalteten Plugins). */
async function loadPluginPages() {
  const box = $('#plugin-pages');
  if (!box) return;
  const plugins = await api('GET', '/api/plugins').catch(() => []);
  box.replaceChildren(...plugins.filter((p) => p.enabled && p.pages?.length).flatMap((p) => p.pages.map((pg) =>
    h('button', { class: 'tpl', title: tr('pages.pluginPageHint', [p.manifest.name]), onclick: () => importPluginPage(p.id, pg.file) },
      h('span', {}, '🧩'), pg.name, h('small', {}, tr('pages.fromPlugin', [p.manifest.name]))))));
}

/** Seite aus einem Plugin importieren (fügt nur hinzu, überschreibt nichts). */
export async function importPluginPage(id, file) {
  await save();
  await applyImport(() => api('POST', '/api/plugins/import-page', { id, file }));
}

async function importProfile(file) {
  await save();
  await applyImport(async () => api('POST', '/api/profiles/import', { data: await readDataUrl(file) }));
}

async function applyImport(request) {
  try {
    const r = await request();
    pushUndo();
    Object.assign(S, { pages: r.pages, flows: r.flows, events: r.events, values: r.values, revision: r.revision });
    S.pageId = r.created[0] ?? S.pageId;
    S.selId = null;
    S.icons = await api('GET', '/api/icons');
    render();
    toast(tr('pages.profileImported', [r.counts.pages, r.counts.flows, r.counts.values, r.counts.images]), 'ok');
  } catch (err) {
    toast(err.message, 'error');
  }
}

function profileMenu(x, y, p) {
  const input = h('input', { type: 'file', accept: '.tdprofile,.zip', hidden: true, onchange: (e) => e.target.files[0] && importProfile(e.target.files[0]) });
  contextMenu(x, y, [
    { icon: '📤', label: tr('pages.exportThisPage', [p.name]), onclick: () => exportProfile([p], p.name) },
    { icon: '📦', label: tr('pages.exportAllPages'), onclick: () => exportProfile([], 'TouchDeck') },
    '-',
    { icon: '📥', label: tr('pages.importProfile'), onclick: () => input.click() },
    '-',
    { icon: '⌨️', label: tr('pages.shortcutPage'), onclick: () => shortcutPageDialog() },
  ]);
}

// ---------- Tastenkürzel-Seite ----------

/**
 * Liest eine Kürzel-Liste: je Zeile „Bezeichnung = Kürzel“, optional „| Symbol“; # = Kommentar.
 * Auch JSON { name, app, shortcuts: [{ label, keys, icon }] } wird verstanden.
 */
export function parseShortcuts(text) {
  const t = text.trim();
  if (t.startsWith('{')) {
    try {
      const j = JSON.parse(t);
      return { name: String(j.name ?? ''), app: String(j.app ?? ''), items: (j.shortcuts ?? []).map((x) => ({ label: String(x.label ?? x.keys ?? ''), keys: String(x.keys ?? ''), icon: String(x.icon ?? '') })).filter((x) => x.keys) };
    } catch {
      return { items: [] };
    }
  }
  const items = [];
  for (const raw of t.split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const [main, icon = ''] = line.split('|').map((x) => x.trim());
    const eq = main.lastIndexOf('=');
    const label = eq > 0 ? main.slice(0, eq).trim() : main;
    const keys = (eq > 0 ? main.slice(eq + 1) : main).trim().toLowerCase().replace(/\s*\+\s*/g, '+');
    if (keys) items.push({ label, keys, icon });
  }
  return { items };
}

/** Dialog: aus einer Kürzel-Liste eine Seite mit Buttons bauen, die die Kürzel an ein Programm schicken. */
async function shortcutPageDialog() {
  const dlg = h('dialog', { class: 'dialog shortcut-dialog' });
  const name = h('input', { value: tr('pages.shortcutPageName'), maxlength: 100 });
  const listId = `dl-win-${uid('')}`;
  const app = h('input', { list: listId, placeholder: tr('pages.shortcutAppPlaceholder') });
  const winList = h('datalist', { id: listId });
  api('GET', '/api/options/windows').then((items) => winList.replaceChildren(...items.map((o) => h('option', { value: o.value }, o.label)))).catch(() => {});
  const text = h('textarea', { rows: 12, placeholder: tr('pages.shortcutExample') });
  const count = h('span', { class: 'hint-text' });
  const update = () => {
    const n = parseShortcuts(text.value).items.length;
    count.textContent = tr('pages.shortcutCount', [n]);
  };
  text.addEventListener('input', update);
  const file = h('input', { type: 'file', accept: '.txt,.json,.tdkeys', hidden: true, onchange: async (e) => {
    const f = e.target.files[0];
    if (!f) return;
    text.value = await f.text();
    const parsed = parseShortcuts(text.value);
    if (parsed.name) name.value = parsed.name;
    if (parsed.app) app.value = parsed.app;
    update();
  } });
  dlg.append(h('form', { method: 'dialog', class: 'dialog-body' },
    h('h2', {}, tr('pages.shortcutPage')),
    h('p', {}, tr('pages.shortcutIntro')),
    h('label', { class: 'field' }, tr('pages.pageName'), name),
    h('label', { class: 'field' }, tr('pages.shortcutApp'), app, winList),
    h('label', { class: 'field' }, tr('pages.shortcutList'), text),
    h('div', { class: 'row' }, count, h('div', { class: 'spacer' }),
      h('button', { type: 'button', class: 'btn small ghost', onclick: () => { text.value = tr('pages.shortcutExample'); update(); } }, tr('pages.shortcutInsertExample')),
      h('button', { type: 'button', class: 'btn small', onclick: () => file.click() }, tr('pages.shortcutLoadFile')), file),
    h('div', { class: 'dialog-actions' },
      h('button', { value: '', class: 'btn ghost' }, tr('common.cancel')),
      h('div', { class: 'spacer' }),
      h('button', { value: 'ok', class: 'btn primary' }, tr('pages.create')))));
  document.body.append(dlg);
  update();
  dlg.showModal();
  const r = await new Promise((resolve) => dlg.addEventListener('close', () => resolve(dlg.returnValue), { once: true }));
  dlg.remove();
  const { items } = parseShortcuts(text.value);
  if (r !== 'ok' || !items.length) return;
  const cols = Math.min(5, Math.max(3, Math.ceil(Math.sqrt(items.length + 1))));
  const total = items.length + 1;
  const rows = Math.ceil(total / cols);
  const target = app.value.trim();
  const buttons = items.slice(0, 99).map((it, i) => tplBtn(Math.floor(i / cols), i % cols, it.label, it.icon || '⌨️',
    [['hotkey', { keys: it.keys, window: target, back: false }]]));
  const back = items.length;
  buttons.push(tplBtn(Math.floor(back / cols), back % cols, tr('pages.back'), '⬅️', [['go_page', { page: '__back' }]]));
  const pg = { id: uid('p_'), name: name.value.trim() || tr('pages.shortcutPageName'), rows: Math.min(rows, 12), cols, background: '#14161b',
    layout: 'grid', ratioW: 16, ratioH: 10, colSizes: [], rowSizes: [], buttons };
  mutate(() => {
    S.pages.push(pg);
    S.pageId = pg.id;
    S.selId = null;
  });
  toast(tr('pages.pageCreated', [pg.name]), 'ok');
}

// ---------- Seiten ----------

export function selectPage(id) {
  S.pageId = id;
  S.selId = null;
  render();
}

function addPage() {
  const cur = page();
  const p = { id: uid('p_'), name: tr('pages.newPage'), rows: cur?.rows ?? 3, cols: cur?.cols ?? 5, background: cur?.background ?? '#14161b',
    layout: 'grid', ratioW: 16, ratioH: 10, colSizes: [], rowSizes: [], buttons: [] };
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
  copy.name = tr('pages.copy2', [copy.name]);
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
  if (p.buttons.length && !confirm(tr('pages.deletePageWithButtons', [p.name, p.buttons.length]))) return;
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
  if (lost.length && !confirm(tr('pages.buttonSLieOutside', [lost.length]))) return render({ detail: false });
  mutate(() => {
    const fitSizes = (list, n) => (list?.length ? [...list.slice(0, n), ...Array(Math.max(0, n - list.length)).fill(1)] : []);
    p.colSizes = fitSizes(p.colSizes, cols);
    p.rowSizes = fitSizes(p.rowSizes, rows);
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
  if (!fits(p, { row, col, rowSpan: b.rowSpan, colSpan: b.colSpan }, [id])) return toast(tr('pages.buttonDoesntFitThere'), 'error');
  mutate(() => Object.assign(b, { row, col }));
}

function swapButtons(idA, idB) {
  if (idA === idB) return;
  const p = page();
  const a = p.buttons.find((x) => x.id === idA);
  const b = p.buttons.find((x) => x.id === idB);
  const ra = { row: b.row, col: b.col, rowSpan: a.rowSpan, colSpan: a.colSpan };
  const rb = { row: a.row, col: a.col, rowSpan: b.rowSpan, colSpan: b.colSpan };
  if (!fits(p, ra, [idA, idB]) || !fits(p, rb, [idA, idB]) || overlaps(ra, rb)) return toast(tr('pages.twoButtonsCantBe'), 'error');
  mutate(() => {
    Object.assign(a, { row: ra.row, col: ra.col });
    Object.assign(b, { row: rb.row, col: rb.col });
  });
}

export function duplicateButton() {
  const b = sel();
  if (!b) return;
  if (isFree(page())) {
    const copy = { ...cloneButton(b), free: fitFree({ ...b.free, x: b.free.x + 3, y: b.free.y + 3 }) };
    return mutate(() => {
      page().buttons.push(copy);
      S.selId = copy.id;
    });
  }
  const spot = freeCell(page());
  if (!spot) return toast(tr('pages.thereIsNoFree'), 'error');
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

// ---------- Zwischenablage und Rechtsklick-Menü ----------

// Die Zwischenablage bleibt beim Seitenwechsel und Neuladen erhalten (nur in diesem Editor).
const CLIP = 'touchdeck.clipboard';
const STYLE_CLIP = 'touchdeck.styleClipboard';
const STYLE_KEYS = ['color', 'colorOn', 'bgOpacity', 'textColor', 'fontSize', 'fontFamily', 'bold', 'italic', 'textOutline', 'titlePos', 'iconFit', 'iconSize', 'bgImage', 'texFit', 'texVeil', 'overlay'];
const LOGIC_CLIP = 'touchdeck.logicClipboard';
// Logik eines Buttons: alle Aktionslisten (Drücken, Aus, Halten, Loslassen, Doppelt) und seine Events.
const LOGIC_KEYS = ['actions', 'actionsOff', 'holdActions', 'releaseActions', 'doubleActions', 'holdRepeat', 'events'];

const readClip = (key) => {
  try {
    return JSON.parse(prefs.get(key) ?? 'null');
  } catch {
    return null;
  }
};
const findButton = (id) => page().buttons.find((x) => x.id === id);
const label = (b) => (b.title || b.icon.value ? tr('pages.format', [b.title || b.icon.value]) : tr('pages.button'));

export function copyButton(id = S.selId, { quiet = false } = {}) {
  const b = findButton(id);
  if (!b) return false;
  prefs.set(CLIP, JSON.stringify(b));
  if (!quiet) toast(tr('pages.copiedPasteWithCtrl', [label(b)]), 'ok');
  return true;
}

export function cutButton(id = S.selId) {
  const b = findButton(id);
  if (!b || !copyButton(id, { quiet: true })) return;
  mutate(() => {
    page().buttons = page().buttons.filter((x) => x !== b);
    if (S.selId === id) S.selId = null;
  });
  toast(tr('pages.cutPasteWithCtrl', [label(b)]), 'ok');
}

/** Fügt den kopierten Button ein: an row/col, sonst ins erste freie Feld. replace = Button an dieser Stelle ersetzen. */
export function pasteButton({ row, col, replace, x, y } = {}) {
  const src = readClip(CLIP);
  if (!src) return toast(tr('pages.nothingToPasteCopy'), 'error');
  const p = page();
  if (isFree(p)) {
    const old = replace && p.buttons.find((b) => b.id === replace);
    const size = src.free ?? defaultFree(p, 50, 50);
    const free = old?.free ? { ...old.free } : fitFree(x === undefined ? { ...size, x: size.x + 3, y: size.y + 3 } : { ...size, x: x - size.w / 2, y: y - size.h / 2 });
    const copy = { ...newButton(0, 0), ...cloneButton(src), free };
    return mutate(() => {
      if (replace) p.buttons = p.buttons.filter((b) => b.id !== replace);
      p.buttons.push(copy);
      S.selId = copy.id;
    });
  }
  const ignore = replace ? [replace] : [];
  const spot = row === undefined ? freeCell(p) : { row, col };
  if (!spot) return toast(tr('pages.thereIsNoFree'), 'error');
  // Größe übernehmen, wenn sie dort passt, sonst als einzelnes Feld.
  const big = { ...spot, rowSpan: src.rowSpan ?? 1, colSpan: src.colSpan ?? 1 };
  const rect = fits(p, big, ignore) ? big : { ...spot, rowSpan: 1, colSpan: 1 };
  if (!fits(p, rect, ignore)) return toast(tr('pages.buttonDoesntFitThere'), 'error');
  const copy = { ...newButton(rect.row, rect.col), ...cloneButton(src), ...rect };
  mutate(() => {
    if (replace) p.buttons = p.buttons.filter((x) => x.id !== replace);
    p.buttons.push(copy);
    S.selId = copy.id;
  });
}

function copyStyle(id) {
  const b = findButton(id);
  if (!b) return;
  prefs.set(STYLE_CLIP, JSON.stringify(Object.fromEntries(STYLE_KEYS.map((k) => [k, b[k]]))));
  toast(tr('pages.lookCopiedApplyIt'), 'ok');
}

function pasteStyle(id) {
  const b = findButton(id);
  const style = readClip(STYLE_CLIP);
  if (!b || !style) return;
  mutate(() => Object.assign(b, Object.fromEntries(STYLE_KEYS.filter((k) => k in style).map((k) => [k, style[k]]))));
}

function copyLogic(id) {
  const b = findButton(id);
  if (!b) return;
  prefs.set(LOGIC_CLIP, JSON.stringify(Object.fromEntries(LOGIC_KEYS.map((k) => [k, b[k] ?? (k === 'holdRepeat' ? 0 : [])]))));
  toast(tr('pages.logicCopied'), 'ok');
}

function pasteLogic(id) {
  const b = findButton(id);
  const logic = readClip(LOGIC_CLIP);
  if (!b || !logic) return;
  // Neue IDs für Aktionen und Events, damit Original und Kopie unabhängig bleiben.
  const fresh = cloneButton({ ...newButton(0, 0), ...logic });
  mutate(() => Object.assign(b, Object.fromEntries(LOGIC_KEYS.map((k) => [k, fresh[k]]))));
  toast(tr('pages.logicPasted'), 'ok');
}

/** Button auf eine andere Seite verschieben (keep = false) oder dorthin kopieren (keep = true). */
function sendToPage(id, targetId, keep) {
  const b = findButton(id);
  const target = S.pages.find((p) => p.id === targetId);
  if (!b || !target) return;
  if (isFree(target)) {
    const free = b.free ?? freeFromCell(page(), b);
    mutate(() => {
      target.buttons.push({ ...(keep ? cloneButton(b) : b), free: { ...free } });
      if (!keep) {
        page().buttons = page().buttons.filter((x) => x !== b);
        S.selId = null;
      }
    });
    return toast(tr(keep ? 'pages.copiedTo' : 'pages.movedTo', [label(b), target.name]), 'ok');
  }
  const big = { row: b.row, col: b.col, rowSpan: b.rowSpan, colSpan: b.colSpan };
  // Gleiche Stelle wie hier, sonst erstes freies Feld.
  const free = freeCell(target);
  const rect = fits(target, big) ? big : free && { ...free, rowSpan: 1, colSpan: 1 };
  if (!rect) return toast(tr('pages.thereIsNoFree2', [target.name]), 'error');
  mutate(() => {
    target.buttons.push({ ...(keep ? cloneButton(b) : b), ...rect });
    if (!keep) {
      page().buttons = page().buttons.filter((x) => x !== b);
      S.selId = null;
    }
  });
  toast(tr(keep ? 'pages.copiedTo' : 'pages.movedTo', [label(b), target.name]), 'ok');
}

function buttonMenu(x, y, id) {
  const b = findButton(id);
  if (!b) return;
  const others = S.pages.filter((p) => p.id !== S.pageId);
  const pageItems = (keep) => others.map((p) => ({ icon: '📄', label: p.name, onclick: () => sendToPage(id, p.id, keep) }));
  const hasClip = !!readClip(CLIP);
  contextMenu(x, y, [
    { icon: '▶️', label: tr('pages.testActions'), disabled: !b.actions.length, onclick: () => testActions(b.actions, b.title || tr('pages.button'), { buttonId: b.id }) },
    '-',
    { icon: '⧉', label: tr('common.duplicate'), keys: 'Strg+D', onclick: duplicateButton },
    { icon: '📋', label: tr('pages.copy'), keys: 'Strg+C', onclick: () => copyButton(id) },
    { icon: '✂️', label: tr('pages.cut'), keys: 'Strg+X', onclick: () => cutButton(id) },
    { icon: '📥', label: tr('pages.pasteReplacesThisOne'), disabled: !hasClip, onclick: () => pasteButton({ row: b.row, col: b.col, replace: id }) },
    '-',
    { icon: '🎨', label: tr('pages.copyLook'), onclick: () => copyStyle(id) },
    { icon: '🖌️', label: tr('pages.pasteLook'), disabled: !readClip(STYLE_CLIP), onclick: () => pasteStyle(id) },
    { icon: '⚙️', label: tr('pages.copyLogic'), onclick: () => copyLogic(id) },
    { icon: '🔧', label: tr('pages.pasteLogic'), disabled: !readClip(LOGIC_CLIP), onclick: () => pasteLogic(id) },
    '-',
    { icon: '➡️', label: tr('pages.moveToPage'), disabled: !others.length, submenu: pageItems(false) },
    { icon: '📑', label: tr('pages.copyToPage'), disabled: !others.length, submenu: pageItems(true) },
    '-',
    { icon: '🗑️', label: tr('common.delete'), keys: tr('pages.del'), danger: true, onclick: deleteButton },
  ]);
}

function cellMenu(x, y, row, col) {
  contextMenu(x, y, [
    { icon: '＋', label: tr('pages.newButton'), onclick: () => addButton(row, col) },
    { icon: '📥', label: tr('pages.pasteHere'), keys: 'Strg+V', disabled: !readClip(CLIP), onclick: () => pasteButton({ row, col }) },
    { icon: '📁', label: tr('pages.createFolderHere'), onclick: () => createFolder({ row, col }) },
  ]);
}

/**
 * Ordner wie bei Stream Deck: neue Unterseite mit Zurück-Button oben links und hier ein Button,
 * der sie öffnet. Alles normale Aktionen („Gehe zu Seite“), im Editor sichtbar und änderbar.
 */
function createFolder(at) {
  const p = page();
  const name = tr('pages.folder');
  const sub = {
    id: uid('p_'), name: `${p.name} › ${name}`, rows: p.rows, cols: p.cols, background: p.background,
    layout: 'grid', ratioW: p.ratioW ?? 16, ratioH: p.ratioH ?? 10, colSizes: [...(p.colSizes ?? [])], rowSizes: [...(p.rowSizes ?? [])], buttons: [], events: [],
  };
  sub.buttons.push(tplBtn(0, 0, tr('pages.back'), '⬅️', [['go_page', { page: '__back' }]], { color: '#3a3f4b' }));
  const extra = isFree(p) ? { free: defaultFree(p, at.x, at.y) } : {};
  const opener = tplBtn(at.row ?? 0, at.col ?? 0, name, '📁', [['go_page', { page: sub.id }]], extra);
  mutate(() => {
    S.pages.splice(S.pages.indexOf(p) + 1, 0, sub);
    p.buttons.push(opener);
    S.selId = opener.id;
    S.tab = 'look';
  });
  toast(tr('pages.folderCreated', [sub.name]), 'ok');
}

function resizeButton(b, rowSpan, colSpan) {
  const p = page();
  const rect = { row: b.row, col: b.col, rowSpan: Math.max(1, rowSpan || 1), colSpan: Math.max(1, colSpan || 1) };
  if (!fits(p, rect, [b.id])) {
    toast(tr('pages.buttonDoesntFitAt'), 'error');
    return render();
  }
  mutate(() => Object.assign(b, { rowSpan: rect.rowSpan, colSpan: rect.colSpan }));
}

// ---------- Eigenschaften ----------

/** Seite: eigene Spaltenbreiten und Zeilenhöhen (Raster). */
/** Seite (wenn kein Button gewählt ist): Reiter „Logik“ (Events der Seite) und „Raster“ bzw. „Fläche“. */
function pageSettings() {
  const p = page();
  S.pageTab ??= 'logic';
  const count = p.events?.length ?? 0;
  const tab = (id, label) => h('button', { class: S.pageTab === id ? 'active' : '', onclick: () => { S.pageTab = id; render(); } }, label);
  const body = S.pageTab === 'layout' ? layoutSettings(p) : [
    h('p', { class: 'hint-text' }, tr('pages.pageLogicHint')),
    ...eventCards(p, { name: p.name, firstTrigger: 'page_view' }),
  ];
  return h('div', { class: 'page-settings' },
    h('h3', {}, tr('pages.pageLayout')),
    h('div', { class: 'tabs' }, tab('logic', `${tr('pages.pageLogicTab')}${count ? ` (${count})` : ''}`), tab('layout', isFree(p) ? tr('pages.areaTab') : tr('pages.gridTab'))),
    ...body.filter(Boolean));
}

/** Reiter „Raster“: eigene Spaltenbreiten und Zeilenhöhen; „Fläche“: Hinweis zum Format. */
function layoutSettings(p) {
  if (isFree(p)) return [h('p', { class: 'hint-text' }, tr('pages.freeLayoutHint'))];
  const sizeList = (key, count, label) => {
    const list = p[key]?.length === count ? p[key] : Array(count).fill(1);
    return h('div', { class: 'field' }, label,
      h('div', { class: 'size-list' }, list.map((n, i) => h('input', { type: 'number', step: 0.1, min: 0.1, max: 10, value: n, title: `${i + 1}`,
        onchange: (e) => mutate(() => {
          const next = [...list];
          next[i] = clampNum(Math.round((Number(e.target.value) || 1) * 10) / 10, 0.1, 10);
          p[key] = next.every((v) => v === 1) ? [] : next;
        }) }))));
  };
  const custom = p.colSizes?.length || p.rowSizes?.length;
  return [
    sizeList('colSizes', p.cols, tr('pages.colSizes')),
    sizeList('rowSizes', p.rows, tr('pages.rowSizes')),
    h('p', { class: 'hint-text' }, tr('pages.sizesHint')),
    custom && h('button', { class: 'btn small', onclick: () => mutate(() => Object.assign(p, { colSizes: [], rowSizes: [] })) }, tr('pages.allEqual')),
  ];
}

function renderInspector() {
  const root = $('#inspector');
  const b = sel();
  if (!b) {
    root.replaceChildren(empty('▦',
      tr('pages.selectButtonOrClick'),
      tr('pages.withPreviewYouSee')), pageSettings());
    return;
  }
  const counts = {
    actions: b.mode === 'slider' ? null : b.actions.length + (b.mode === 'toggle' ? b.actionsOff.length : 0) + (b.doubleActions?.length ?? 0),
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
      h('h2', {}, b.title || tr('pages.button')),
      b.mode !== 'slider' && h('button', { class: 'btn small primary', title: tr('pages.runActionsOnPc'), onclick: () => testActions(b.actions, tr('pages.button'), { buttonId: b.id }) }, tr('pages.test')),
      h('button', { class: 'btn small ghost', title: tr('pages.deselectHint'), onclick: () => select(null) }, '✕')),
    h('div', { class: 'tabs' },
      tab('look', tr('common.look')),
      tab('actions', b.mode === 'slider' ? tr('pages.slider') : `${tr('pages.press')}${counts.actions ? ` (${counts.actions})` : ''}`),
      tab('events', `${tr('common.event')}${counts.events ? ` (${counts.events})` : ''}`),
      b.mode !== 'slider' && tab('hold', `${tr('pages.hold')}${counts.hold ? ` (${counts.hold})` : ''}`)),
    ...content(b).filter(Boolean),
    h('div', { class: 'insp-footer' },
      h('button', { class: 'btn small', onclick: duplicateButton }, tr('common.duplicate')),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn small ghost danger', onclick: deleteButton }, tr('pages.deleteButton'))),
  );
}

function lookTab(b) {
  const edit = (key, fn, opts) => mutate(fn, { coalesce: `${b.id}:${key}`, ...opts });
  const p = page();
  // Ein Button ist „an“, wenn er ein eingeschalteter Umschalter ist oder seine Live-Anzeige „an“ meldet.
  const canBeOn = b.mode === 'toggle' || (b.mode !== 'slider' && b.live.source !== 'none');
  const onLook = canBeOn && S.lookTarget === 'on';
  const key = onLook ? 'iconOn' : 'icon';
  b.iconOn ??= { type: 'emoji', value: '' };
  const cur = b[key];

  const iconBox = h('div', { class: 'icon-box' });
  if (cur.type === 'image' && cur.value) iconBox.append(h('img', { src: iconUrl(cur.value), alt: '' }));
  else iconBox.textContent = cur.value;

  const upload = h('input', {
    type: 'file', accept: 'image/png,image/jpeg,image/gif,image/webp,image/svg+xml', hidden: true,
    onchange: async (e) => {
      const file = e.target.files[0];
      if (!file) return;
      if (file.size > 5 * 1024 * 1024) return toast(tr('pages.imageIsLargerThan'), 'error');
      const data = await new Promise((res) => {
        const r = new FileReader();
        r.onload = () => res(r.result);
        r.readAsDataURL(file);
      });
      try {
        const { name } = await api('POST', '/api/icons', { data });
        S.icons.push(name);
        mutate(() => (b[key] = { type: 'image', value: name }));
      } catch (err) {
        toast(err.message, 'error');
      }
    },
  });

  return [
    h('label', { class: 'field' }, tr('pages.labelShowsValue'),
      h('input', { value: b.title, maxlength: 200, placeholder: tr('pages.exampleLouder'), oninput: (e) => edit('title', () => (b.title = e.target.value), { detail: false }) })),
    h('div', { class: 'field' }, tr('pages.labelPosition'),
      segmented([['top', tr('pages.top')], ['center', tr('pages.middle')], ['bottom', tr('pages.bottom')], ['hidden', tr('pages.off')]], b.titlePos, (v) => mutate(() => (b.titlePos = v)))),
    h('label', { class: 'field' }, tr('pages.fontSize', [b.fontSize]),
      h('input', { type: 'range', min: 8, max: 64, value: b.fontSize, oninput: (e) => {
        e.target.parentElement.firstChild.textContent = tr('pages.fontSize', [e.target.value]);
        edit('fs', () => (b.fontSize = Number(e.target.value)), { detail: false });
      } })),
    h('div', { class: 'row' },
      h('label', { class: 'field' }, tr('pages.font'),
        h('select', { value: b.fontFamily ?? '', onchange: (e) => mutate(() => (b.fontFamily = e.target.value)) },
          [['', tr('pages.fontNormal')], ['serif', 'Serif'], ['mono', 'Monospace'], ['condensed', tr('pages.fontCondensed')], ['handwriting', tr('pages.fontHandwriting')]]
            .map(([v, l]) => h('option', { value: v }, l)))),
      h('label', { class: 'field' }, tr('pages.textOutline', [b.textOutline ?? 0]),
        h('input', { type: 'range', min: 0, max: 6, value: b.textOutline ?? 0, oninput: (e) => {
          e.target.parentElement.firstChild.textContent = tr('pages.textOutline', [e.target.value]);
          edit('outline', () => (b.textOutline = Number(e.target.value)), { detail: false });
        } }))),
    h('div', { class: 'font-styles' },
      h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: b.bold !== false, onchange: (e) => mutate(() => (b.bold = e.target.checked)) }), h('b', {}, tr('pages.bold'))),
      h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: !!b.italic, onchange: (e) => mutate(() => (b.italic = e.target.checked)) }), h('i', {}, tr('pages.italic')))),

    h('h3', {}, tr('pages.icon')),
    canBeOn && h('div', { class: 'field' },
      segmented([['normal', tr('common.normal')], ['on', tr('pages.whenOn')]], onLook ? 'on' : 'normal', (v) => { S.lookTarget = v; render(); }),
      h('p', { class: 'hint-text' }, b.mode === 'toggle' ? tr('pages.onToggleIsSwitched') : tr('pages.onIsReportedBy'), onLook ? tr('pages.leaveEmptySameIcon') : '')),
    h('div', { class: 'icon-preview' },
      iconBox,
      h('input', { value: cur.type === 'emoji' ? cur.value : '', maxlength: 16, placeholder: tr('pages.enterEmoji'), style: 'flex:1',
        oninput: (e) => edit(`emoji-${key}`, () => (b[key] = { type: 'emoji', value: e.target.value.trim() }), { detail: false }) }),
      h('label', { class: 'btn small', title: tr('pages.uploadYourOwnImage') }, tr('pages.image'), upload),
      cur.value && h('button', { class: 'btn small ghost', title: tr('pages.removeIcon'), onclick: () => mutate(() => (b[key] = { type: 'emoji', value: '' })) }, '✕')),
    !onLook && b.icon.type === 'image' && b.icon.value && h('label', { class: 'switch-row' },
      h('input', { type: 'checkbox', checked: b.iconFit === 'cover', onchange: (e) => mutate(() => (b.iconFit = e.target.checked ? 'cover' : 'contain')) }),
      tr('pages.imageFillsWholeButton')),
    !onLook && h('label', { class: 'field' }, tr('pages.iconSize', [b.iconSize ?? 100]),
      h('input', { type: 'range', min: 30, max: 200, step: 5, value: b.iconSize ?? 100, oninput: (e) => {
        e.target.parentElement.firstChild.textContent = tr('pages.iconSize', [e.target.value]);
        edit('iconSize', () => (b.iconSize = Number(e.target.value)), { detail: false });
      } })),
    h('div', { class: 'emoji-grid' }, EMOJIS.map((em) => h('button', { type: 'button', onclick: () => mutate(() => (b[key] = { type: 'emoji', value: em })) }, em))),
    iconPicker(b, key),
    canBeOn && h('label', { class: 'field' }, tr('pages.labelWhenOnEmpty'),
      h('input', { value: b.titleOn ?? '', maxlength: 200, placeholder: tr('pages.exampleMuted'), oninput: (e) => edit('titleOn', () => (b.titleOn = e.target.value), { detail: false }) })),

    h('h3', {}, tr('pages.colors')),
    h('div', { class: 'colors' },
      h('label', {}, h('input', { type: 'color', value: b.color, oninput: (e) => edit('color', () => (b.color = e.target.value), { detail: false }) }), tr('pages.background')),
      h('label', {}, h('input', { type: 'color', value: b.textColor, oninput: (e) => edit('tc', () => (b.textColor = e.target.value), { detail: false }) }), tr('pages.text')),
      canBeOn && h('label', {}, h('input', { type: 'color', value: b.colorOn, oninput: (e) => edit('con', () => (b.colorOn = e.target.value), { detail: false }) }), tr('pages.whenOn'))),
    h('div', { class: 'swatches' }, SWATCHES.map((c) => h('button', { type: 'button', title: c, style: `background:${c}`, onclick: () => mutate(() => {
      b.color = c;
      if ((b.bgOpacity ?? 100) === 0) b.bgOpacity = 100;
    }) }))),
    h('div', { class: 'row opacity-row' },
      h('label', { class: 'field' }, tr('pages.bgOpacity', [b.bgOpacity ?? 100]),
        h('input', { type: 'range', min: 0, max: 100, step: 5, value: b.bgOpacity ?? 100, oninput: (e) => {
          e.target.parentElement.firstChild.textContent = tr('pages.bgOpacity', [e.target.value]);
          edit('opacity', () => (b.bgOpacity = Number(e.target.value)), { detail: false });
        } })),
      h('button', { class: 'btn small', style: 'align-self:end', onclick: () => mutate(() => (b.bgOpacity = (b.bgOpacity ?? 100) === 0 ? 100 : 0)) },
        (b.bgOpacity ?? 100) === 0 ? tr('pages.opaque') : tr('pages.transparent'))),

    h('h3', {}, tr('pages.texture')),
    textureSection(b),

    h('h3', {}, tr('pages.sizeBehavior')),
    isFree(p) ? freeSizeFields(b) : h('div', { class: 'row' },
      h('label', { class: 'field' }, tr('pages.widthFields'),
        h('input', { type: 'number', min: 1, max: p.cols - b.col, value: b.colSpan, onchange: (e) => resizeButton(b, b.rowSpan, Number(e.target.value)) })),
      h('label', { class: 'field' }, tr('pages.heightFields'),
        h('input', { type: 'number', min: 1, max: p.rows - b.row, value: b.rowSpan, onchange: (e) => resizeButton(b, Number(e.target.value), b.colSpan) }))),
    h('div', { class: 'field' }, tr('pages.type'),
      segmented([['press', tr('pages.press')], ['toggle', tr('pages.toggle')], ['slider', tr('pages.slider')], ['web', tr('pages.web')]], b.mode, (v) => mutate(() => (b.mode = v)))),
    b.mode === 'web' && webSection(b),
    b.mode !== 'slider' && b.mode !== 'web' && liveSection(b),
  ];
}

// Ergebnis der Prüfung je Adresse, damit nicht bei jedem Zeichnen neu gefragt wird.
const webChecks = new Map();

/** Hinweis, wenn die Seite das Einbetten verbietet oder nicht erreichbar ist. */
function webCheck(url) {
  if (!url) return null;
  const box = h('p', { class: 'hint-text' });
  const show = (r) => {
    if (!r || r.ok) return box.replaceChildren();
    box.classList.add('error');
    box.textContent = r.reason === 'blocked' ? tr('pages.webBlocked') : r.reason === 'unreachable' ? tr('pages.webUnreachable') : tr('pages.webBadUrl');
  };
  if (webChecks.has(url)) show(webChecks.get(url));
  else api('POST', '/api/web-check', { url }).then((r) => { webChecks.set(url, r); show(r); }).catch(() => {});
  return box;
}

/** Graph: Verlauf der Live-Anzeige als Linie, Fläche oder Balken. */
function graphSettings(b) {
  const live = b.live;
  const set = (k, v, opts) => mutate(() => (live[k] = v), { coalesce: `${b.id}:graph-${k}`, ...opts });
  const num = (v) => (v === '' || v === null || v === undefined ? null : Number(String(v).replace(',', '.')));
  return h('div', {},
    h('label', { class: 'field' }, tr('pages.graph'),
      h('select', { value: live.graph ?? '', onchange: (e) => set('graph', e.target.value, { detail: true }) },
        [['', tr('pages.graphNone')], ['line', tr('pages.graphLine')], ['area', tr('pages.graphArea')], ['bars', tr('pages.graphBars')]].map(([v, l]) => h('option', { value: v }, l)))),
    live.graph && h('div', { class: 'row' },
      h('label', { class: 'field' }, tr('pages.graphPoints'),
        h('input', { type: 'number', min: 10, max: 300, value: live.graphPoints ?? 60, onchange: (e) => set('graphPoints', Math.max(10, Math.min(300, Number(e.target.value) || 60))) })),
      h('label', { class: 'field' }, tr('pages.graphInterval'),
        h('input', { type: 'number', min: 1, max: 3600, value: live.graphInterval ?? 2, onchange: (e) => set('graphInterval', Math.max(1, Math.min(3600, Number(e.target.value) || 2))) }))),
    live.graph && h('div', { class: 'row' },
      h('label', { class: 'field' }, tr('pages.graphMin'),
        h('input', { value: live.graphMin ?? '', placeholder: tr('pages.graphAuto'), onchange: (e) => set('graphMin', num(e.target.value)) })),
      h('label', { class: 'field' }, tr('pages.graphMax'),
        h('input', { value: live.graphMax ?? '', placeholder: tr('pages.graphAuto'), onchange: (e) => set('graphMax', num(e.target.value)) })),
      h('label', { class: 'field' }, tr('pages.graphColor'),
        h('input', { type: 'color', value: live.graphColor || '#5aa9e6', onchange: (e) => set('graphColor', e.target.value) }))),
    live.graph && h('p', { class: 'hint-text' }, tr('pages.graphHint')));
}

/** Webseite im Button: Adresse, Zoom, bedienbar und automatisch neu laden. */
function webSection(b) {
  b.web ??= { url: '', zoom: 100, interactive: true, refresh: 0 };
  const set = (k, v, opts) => mutate(() => (b.web[k] = v), { coalesce: `${b.id}:web-${k}`, ...opts });
  return h('div', { class: 'field-group' },
    h('label', { class: 'field' }, tr('pages.webUrl'),
      h('input', { value: b.web.url, placeholder: 'https://…', onchange: (e) => {
        // „www.beispiel.de“ ohne Schema: https:// davor, sonst würde die Adresse verworfen.
        const v = e.target.value.trim();
        set('url', v && !v.includes('://') ? `https://${v}` : v, { detail: true });
      } })),
    webCheck(b.web.url),
    h('div', { class: 'row' },
      h('label', { class: 'field' }, tr('pages.webZoom'),
        h('input', { type: 'number', min: 25, max: 300, step: 5, value: b.web.zoom ?? 100, onchange: (e) => set('zoom', Number(e.target.value) || 100) })),
      h('label', { class: 'field' }, tr('pages.webRefresh'),
        h('input', { type: 'number', min: 0, max: 86400, value: b.web.refresh ?? 0, onchange: (e) => set('refresh', Math.max(0, Number(e.target.value) || 0)) }))),
    h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked: b.web.interactive !== false, onchange: (e) => set('interactive', e.target.checked) }), tr('pages.webInteractive')),
    h('p', { class: 'hint-text' }, tr('pages.webHint')));
}

/** Textur: Hintergrundbild unter Symbol und Text und ein Overlay darüber. */
function textureSection(b) {
  b.bgImage ??= { type: 'emoji', value: '' };
  const has = b.bgImage.type === 'image' && b.bgImage.value;
  const upload = h('input', {
    type: 'file', accept: 'image/png,image/jpeg,image/gif,image/webp,image/svg+xml', hidden: true,
    onchange: async (e) => {
      const file = e.target.files[0];
      if (!file) return;
      if (file.size > 5 * 1024 * 1024) return toast(tr('pages.imageIsLargerThan'), 'error');
      try {
        const { name } = await api('POST', '/api/icons', { data: await readDataUrl(file) });
        S.icons.push(name);
        mutate(() => (b.bgImage = { type: 'image', value: name }));
      } catch (err) {
        toast(err.message, 'error');
      }
    },
  });
  const overlays = [['', tr('pages.overlayNone')], ['gloss', tr('pages.overlayGloss')], ['frame', tr('pages.overlayFrame')], ['stripes', tr('pages.overlayStripes')],
    ['dots', tr('pages.overlayDots')], ['vignette', tr('pages.overlayVignette')], ['glow', tr('pages.overlayGlow')]];
  return h('div', {},
    h('div', { class: 'icon-preview' },
      h('div', { class: 'icon-box' }, has && h('img', { src: iconUrl(b.bgImage.value), alt: '' })),
      h('span', { class: 'hint-text', style: 'flex:1;margin:0' }, has ? b.bgImage.value : tr('pages.noBackgroundImage')),
      h('label', { class: 'btn small' }, tr('pages.image'), upload),
      has && h('button', { class: 'btn small ghost', title: tr('pages.removeBackgroundImage'), onclick: () => mutate(() => (b.bgImage = { type: 'emoji', value: '' })) }, '✕')),
    iconPicker(b, 'bgImage', 'showBgPicker'),
    has && h('label', { class: 'field' }, tr('pages.texFit'),
      h('select', { value: b.texFit ?? 'cover', onchange: (e) => mutate(() => (b.texFit = e.target.value)) },
        [['cover', tr('pages.texCover')], ['contain', tr('pages.texContain')], ['stretch', tr('pages.texStretch')], ['tile', tr('pages.texTile')]].map(([v, l]) => h('option', { value: v }, l)))),
    has && h('label', { class: 'field' }, tr('pages.texVeil', [b.texVeil ?? 35]),
      h('input', { type: 'range', min: 0, max: 100, step: 5, value: b.texVeil ?? 35, oninput: (e) => {
        e.target.parentElement.firstChild.textContent = tr('pages.texVeil', [e.target.value]);
        mutate(() => (b.texVeil = Number(e.target.value)), { coalesce: `${b.id}:veil`, detail: false });
      } })),
    h('label', { class: 'field' }, tr('pages.overlay'),
      h('select', { value: b.overlay ?? '', onchange: (e) => mutate(() => (b.overlay = e.target.value)) }, overlays.map(([v, l]) => h('option', { value: v }, l)))),
    h('p', { class: 'hint-text' }, tr('pages.textureHint')));
}

/** Lage und Größe in Prozent der Seite (freie Seiten). */
function freeSizeFields(b) {
  const field = (key, label) => h('label', { class: 'field' }, label,
    h('input', { type: 'number', step: SNAP, min: 0, max: 100, value: b.free[key],
      onchange: (e) => mutate(() => (b.free = fitFree({ ...b.free, [key]: Number(e.target.value) || 0 }))) }));
  return h('div', {},
    h('div', { class: 'row' }, field('x', tr('pages.posX')), field('y', tr('pages.posY'))),
    h('div', { class: 'row' }, field('w', tr('pages.widthPercent')), field('h', tr('pages.heightPercent'))),
    h('p', { class: 'hint-text' }, tr('pages.freeSizeHint')));
}

function liveSection(b) {
  const hints = {
    steam_running: tr('pages.lightsUpWhileGame'),
    obs_scene: tr('pages.lightsUpWhenScene'),
    temp: tr('pages.lightsUpFrom80'),
    mic: tr('pages.lightsUpWhenMicrophone'),
    command: tr('pages.firstLineOfOutput'),
    variable: tr('pages.lightsUpWhenValue'),
  };
  return h('div', {},
    h('h3', {}, tr('pages.liveDisplay')),
    h('label', { class: 'field' }, tr('pages.showOnButton'),
      h('select', { value: b.live.source, onchange: (e) => mutate(() => (b.live.source = e.target.value)) },
        S.liveSources.map((o) => h('option', { value: o.value }, o.label)))),
    (['cpu', 'ram', 'temp', 'volume', 'timer', 'variable', 'gpu_temp', 'gpu_load', 'disk'].includes(b.live.source) || b.live.source.startsWith('plugin:')) && h('label', { class: 'switch-row' },
      h('input', { type: 'checkbox', checked: !!b.live.bar, onchange: (e) => mutate(() => (b.live.bar = e.target.checked)) }), tr('pages.showAsBar')),
    !['none', 'clock', 'date', 'page_indicator', 'track'].includes(b.live.source) && graphSettings(b),
    b.live.source === 'timer' && h('label', { class: 'field' }, tr('pages.timerName'),
      h('input', { value: b.live.variable || '', placeholder: 'Timer', oninput: (e) => mutate(() => (b.live.variable = e.target.value.trim()), { coalesce: `${b.id}:timer`, detail: false }) })),
    b.live.source === 'command' && h('label', { class: 'field' }, tr('pages.command'),
      h('input', { value: b.live.command, placeholder: tr('pages.exampleUptimeP'), oninput: (e) => mutate(() => (b.live.command = e.target.value), { coalesce: `${b.id}:livecmd`, detail: false }) })),
    b.live.source === 'variable' && dynamicSelect({ label: tr('pages.value'), source: 'variables', allowNew: true }, b.live.variable,
      (v) => mutate(() => (b.live.variable = v), { coalesce: `${b.id}:livevar`, detail: false })),
    hints[b.live.source] && h('p', { class: 'hint-text' }, hints[b.live.source]));
}

function pressTab(b) {
  b.doubleActions ??= [];
  const context = b.title || tr('pages.button');
  const double = [
    actionList(b, 'doubleActions', tr('pages.whenDoublePressed'), { buttonId: b.id, compact: true, context }),
    h('p', { class: 'hint-text' }, tr('pages.doublePressHint')),
  ];
  if (b.mode === 'toggle') {
    return [
      actionList(b, 'actions', tr('pages.whenSwitchedOn'), { buttonId: b.id, compact: true, context }),
      actionList(b, 'actionsOff', tr('pages.whenSwitchedOff'), { buttonId: b.id, compact: true, context }),
      ...double,
    ];
  }
  return [actionList(b, 'actions', tr('pages.whenPressed'), { buttonId: b.id, compact: true, context, hint: b.holdActions.length ? tr('pages.shortTapRunsThese') : '' }), ...double];
}

function sliderTab(b) {
  const sl = b.slider;
  const edit = (fn, key) => mutate(fn, { coalesce: key && `${b.id}:${key}`, detail: !key });
  return [
    h('p', { class: 'hint-text' }, tr('pages.swipeOverButtonOn')),
    h('label', { class: 'field' }, tr('pages.whatShouldSliderControl'),
      h('select', { value: sl.target, onchange: (e) => edit(() => (sl.target = e.target.value)) }, S.sliderTargets.map((o) => h('option', { value: o.value }, o.label)))),
    sl.target === 'obs_volume' && dynamicSelect({ key: 'input', label: tr('pages.obsAudioSource'), source: 'obs-inputs' }, sl.input, (v) => edit(() => (sl.input = v))),
    sl.target === 'command' && h('label', { class: 'field' }, tr('pages.commandIsReplacedBy'),
      h('textarea', { value: sl.command, placeholder: tr('pages.exampleDdcutilSetvcp10'), oninput: (e) => edit(() => (sl.command = e.target.value), 'slcmd') })),

    h('h3', {}, tr('pages.sliderLook')),
    h('label', { class: 'field' }, tr('pages.sliderShape'),
      h('select', { value: sl.shape ?? 'fill', onchange: (e) => edit(() => (sl.shape = e.target.value)) },
        [['fill', tr('pages.shapeFill')], ['rail', tr('pages.shapeRail')], ['fader', tr('pages.shapeFader')], ['knob', tr('pages.shapeKnob')], ['dial', tr('pages.shapeDial')]].map(([v, l]) => h('option', { value: v }, l)))),
    h('div', { class: 'row' },
      h('div', { class: 'field' }, tr('pages.sliderColor'),
        h('div', { class: 'colors' },
          h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: !!sl.color, onchange: (e) => edit(() => (sl.color = e.target.checked ? '#5b6cff' : '')) }), tr('pages.ownColor')),
          sl.color && h('input', { type: 'color', value: sl.color, oninput: (e) => edit(() => (sl.color = e.target.value), 'slcolor') }))),
      (sl.shape ?? 'fill') !== 'fill' && h('label', { class: 'field' }, tr('pages.sliderThickness', [sl.thickness ?? 10]),
        h('input', { type: 'range', min: 2, max: 30, value: sl.thickness ?? 10, oninput: (e) => {
          e.target.parentElement.firstChild.textContent = tr('pages.sliderThickness', [e.target.value]);
          edit(() => (sl.thickness = Number(e.target.value)), 'slthick');
        } }))),
    h('p', { class: 'hint-text' }, tr('pages.sliderShapeHint')),
  ];
}

/** Event-Karten für einen Button oder eine Seite (owner.events). */
function eventCards(owner, { buttonId, name, firstTrigger }) {
  owner.events ??= [];
  return [
    ...owner.events.map((ev, i) => h('div', { class: `event-card${ev.enabled ? '' : ' disabled'}` },
      h('div', { class: 'list-title' },
        h('h3', {}, tr('pages.event', [i + 1])),
        h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: ev.enabled, onchange: (e) => mutate(() => (ev.enabled = e.target.checked)) }), tr('common.active')),
        h('button', { class: 'btn icon ghost danger', title: tr('pages.removeEvent'), onclick: () => mutate(() => owner.events.splice(i, 1)) }, '✕')),
      h('p', { class: 'muted-note' }, triggerSummary(ev.trigger)),
      triggerEditor(ev.trigger),
      actionList(ev, 'actions', tr('common.then'), { buttonId, compact: true, context: tr('pages.event2', [name, i + 1]) }))),
    h('button', { class: 'btn', onclick: () => mutate(() => owner.events.push({ id: uid('e_'), enabled: true, trigger: newTrigger(firstTrigger), actions: [] })) }, tr('pages.addEvent')),
  ];
}

function eventsTab(b) {
  return [h('p', { class: 'hint-text' }, tr('pages.buttonReactsToEvents')), ...eventCards(b, { buttonId: b.id, name: b.title || 'Button' })];
}

function holdTab(b) {
  return [
    h('p', { class: 'hint-text' }, tr('pages.longPressFromAbout')),
    actionList(b, 'holdActions', tr('pages.whileHeld'), { buttonId: b.id, compact: true, context: b.title || tr('pages.button') }),
    h('label', { class: 'field' }, tr('pages.repeatWhileHeldMs'),
      h('input', { type: 'number', min: 0, max: 10000, step: 50, value: b.holdRepeat, onchange: (e) => mutate(() => (b.holdRepeat = Math.max(0, Number(e.target.value) || 0))) })),
    actionList(b, 'releaseActions', tr('pages.onReleaseAfterHolding'), { buttonId: b.id, compact: true, context: b.title || tr('pages.button') }),
  ];
}

// ---------- Fertige Seiten ----------

function tplBtn(row, col, title, emoji, actions = [], extra = {}) {
  return newButton(row, col, { title, icon: { type: 'emoji', value: emoji }, actions: actions.map(([type, params = {}]) => ({ id: uid('a_'), type, enabled: true, params })), ...extra });
}

const TEMPLATE_NAMES = { steam: tr('pages.steamGames'), system: tr('common.system'), obs: 'OBS' };

/** Fragt vor dem Anlegen nach; gibt es die Seite schon: aktualisieren, öffnen oder abbrechen. */
async function askTemplate(template, createText) {
  const existing = S.pages.filter((p) => p.template === template);
  const name = TEMPLATE_NAMES[template];
  if (!existing.length) return ask(tr('pages.createPage2', [name]), createText, [{ value: 'create', label: tr('pages.create'), kind: 'primary' }]);
  const r = await ask(tr('pages.alreadyExists', [name]),
    `${existing.length > 1 ? tr('pages.thereArePagesFrom', [existing.length]) : tr('pages.thisPageAlreadyExists')} ${tr('pages.updatingCreatesItAnew')}`,
    [{ value: 'open', label: tr('pages.openExisting') }, { value: 'update', label: tr('common.update'), kind: 'primary' }]);
  if (r === 'open') selectPage(existing[0].id);
  return r === 'update' ? 'update' : null;
}

/** extra: Werte und globale Events, die die Seite braucht (werden nur angelegt, wenn es sie noch nicht gibt). */
function addTemplatePage(pg, template, extra = {}) {
  pg.template = template;
  const existing = S.pages.find((p) => p.template === template);
  mutate(() => {
    for (const [name, meta] of Object.entries(extra.values ?? {})) S.values[name] ??= meta;
    for (const ev of extra.events ?? []) if (!S.events.some((e) => e.name === ev.name)) S.events.push(ev);
    if (existing) {
      pg.id = existing.id;
      S.pages = S.pages.filter((p) => p.template !== template || p === existing).map((p) => (p === existing ? pg : p));
    } else {
      S.pages.push(pg);
    }
    S.pageId = pg.id;
    S.selId = null;
  });
  toast(tr(existing ? 'pages.pageUpdated' : 'pages.pageCreated', [pg.name]), 'ok');
}

// Bausteine für die fertigen Seiten: nur normale Aktionen und Events, damit man im Editor sieht, was ein Button tut.
const act = (type, params = {}) => ({ id: uid('a_'), type, enabled: true, params });
const onEvent = (type, params, actions) => ({ id: uid('e_'), enabled: true, trigger: { type, params }, actions });
const look = (params) => act('change_visuals', { target: 'self', ...params });
const restoreLook = () => act('restore_visuals', { target: 'self' });
const liveShow = (source, bar = false) => ({ live: { source, command: '', variable: '', bar } });

/** Ton- oder Mikrofon-Button: bei „stumm“ anderes Aussehen, bei „laut“ wieder das normale. */
function muteEvents(target, mutedLook) {
  return {
    events: [
      onEvent('audio_mute', { target, state: 'muted' }, [look(mutedLook)]),
      onEvent('audio_mute', { target, state: 'unmuted' }, [restoreLook()]),
    ],
  };
}

export function muteButton(row, col) {
  return tplBtn(row, col, tr('pages.soundOn'), '🔊', [['volume', { op: 'mute' }]],
    muteEvents('output', { icon: '🔇', title: tr('pages.muted'), color: '#8f3b3b' }));
}

async function systemTemplate() {
  if (!(await askTemplate('system', tr('pages.pageWithClockCpu')))) return;
  addTemplatePage({
    id: uid('p_'), name: tr('common.system'), rows: 4, cols: 5, background: '#14161b',
    buttons: [
      // Zahlen wie Uhrzeit oder CPU gibt es nur als Live-Anzeige (Reiter „Aussehen“), mit Balken.
      tplBtn(0, 0, tr('pages.clock'), '🕐', [], liveShow('clock')),
      tplBtn(0, 1, 'CPU', '🧠', [], liveShow('cpu', true)),
      tplBtn(0, 2, 'RAM', '💾', [], liveShow('ram', true)),
      tplBtn(0, 3, tr('pages.temperature'), '🌡️', [], { ...liveShow('temp', true), colorOn: '#a33a2a' }),
      tplBtn(0, 4, tr('pages.volume'), '', [], { rowSpan: 4, mode: 'slider', color: '#3b3f8f', slider: { target: 'volume', input: '', command: '', shape: 'fader', color: '', thickness: 10 } }),
      tplBtn(1, 0, tr('pages.music'), '⏯️', [['media', { op: 'play-pause' }]], { colSpan: 2, ...liveShow('track'), colorOn: '#2f6e4f' }),
      muteButton(1, 2),
      tplBtn(1, 3, tr('pages.microphone'), '🎙️', [['mic_mute', { op: 'toggle' }]],
        muteEvents('input', { icon: '🔇', title: tr('pages.micOff'), color: '#8f3b3b' })),
      tplBtn(2, 0, tr('pages.output'), '🎧', [['audio_output', { target: 'next' }]]),
      tplBtn(2, 1, tr('pages.photo'), '📸', [['screenshot', { mode: 'dialog' }]]),
      tplBtn(2, 2, tr('pages.nightLight'), '🌙', [['night_light', { mode: 'toggle' }]]),
      tplBtn(2, 3, tr('pages.lock'), '🔒', [['lock_screen']], { color: '#5b3fa0' }),
      tplBtn(3, 0, tr('pages.gpu'), '🎮', [], { ...liveShow('gpu_temp', true), colorOn: '#a33a2a' }),
      tplBtn(3, 1, tr('pages.disk'), '💽', [], liveShow('disk', true)),
      tplBtn(3, 2, tr('pages.network'), '📶', [], liveShow('net')),
      tplBtn(3, 3, tr('pages.screenOff'), '🖥️', [['screen', { op: 'off' }]], { color: '#30343f' }),
    ],
  }, 'system');
}

async function obsTemplate() {
  if (!(await askTemplate('obs', tr('pages.oneButtonPerObs')))) return;
  let scenes = [];
  try {
    scenes = (await api('GET', '/api/options/obs-scenes')).map((o) => o.value);
  } catch (err) {
    toast(tr('pages.obsIsNotReachable', [err.message]), 'error');
  }
  if (!S.apps) S.apps = await api('GET', '/api/apps').catch(() => []);
  const obsApp = S.apps.find((a) => /obs/i.test(a.name));
  const cols = 5;
  const sceneList = scenes.length ? scenes.slice(0, cols * 2) : [''];
  const sceneRows = Math.ceil(sceneList.length / cols);
  // Die aktive Szene steht im Wert „OBS-Szene“ (gesetzt vom Event unten); jeder Szenen-Button vergleicht damit.
  const sceneValue = tr('pages.obsSceneValue');
  const buttons = sceneList.map((scene, i) => tplBtn(Math.floor(i / cols), i % cols, scene || tr('pages.scene'), '🎬',
    [['obs_scene', { scene }]], {
      events: [onEvent('value_change', { name: sceneValue, op: 'any', value: '' }, [
        restoreLook(),
        act('condition', { name: sceneValue, op: 'eq', value: scene }),
        look({ color: '#2f7a4a' }),
      ])],
    }));
  const r = sceneRows;
  buttons.push(
    tplBtn(r, 0, tr('pages.stream'), '📡', [['obs_stream', { op: 'toggle' }]], {
      events: [
        onEvent('obs_stream', { state: 'started' }, [look({ icon: '🔴', title: 'LIVE', color: '#b3262d' })]),
        onEvent('obs_stream', { state: 'stopped' }, [restoreLook()]),
      ],
    }),
    tplBtn(r, 1, tr('pages.recording'), '⏺️', [['obs_record', { op: 'toggle' }]], {
      events: [
        onEvent('obs_record', { state: 'started' }, [look({ title: 'REC', color: '#b3262d' })]),
        onEvent('obs_record', { state: 'resumed' }, [look({ title: 'REC', color: '#b3262d' })]),
        onEvent('obs_record', { state: 'paused' }, [look({ title: tr('pages.pause'), color: '#a3501d' })]),
        onEvent('obs_record', { state: 'stopped' }, [restoreLook()]),
      ],
    }),
    tplBtn(r, 2, tr('pages.pause'), '⏸️', [['obs_record', { op: 'pause' }]]),
    tplBtn(r, 3, tr('pages.clip'), '⏪', [['obs_replay', { op: 'save' }]]),
    obsApp
      ? tplBtn(r, 4, tr('pages.startObs'), '🚀', [['launch_app', { app: obsApp.id, appName: obsApp.name }]])
      : tplBtn(r, 4, tr('pages.virtualCamera'), '📷', [['obs_misc', { op: 'virtualcam' }]]),
  );
  addTemplatePage({ id: uid('p_'), name: 'OBS', rows: sceneRows + 1, cols, background: '#14161b', buttons }, 'obs', {
    values: { [sceneValue]: { default: '', description: tr('pages.obsSceneValueDescription'), folder: 'OBS', resetOnStart: true } },
    events: [{
      id: uid('ev_'), name: tr('pages.rememberObsScene'), enabled: true, cooldown: 0,
      trigger: { type: 'obs_scene', params: { scene: '' } },
      actions: [act('var_set', { name: sceneValue, value: getLang() === 'de' ? '{Event.Szene}' : '{Event.Scene}' })],
    }],
  });
}

/** Für Vorlagen, die der Server erzeugt (Steam): Ergebnis übernehmen. */
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
  if (!(await askTemplate('steam', tr('pages.allInstalledSteamGames')))) return;
  await serverTemplate('/api/steam/pages', (r) => {
    S.pageId = r.created[0];
    toast(tr(r.updated ? 'pages.steamPageSUpdated' : 'pages.steamPageSCreated', [r.created.length]), 'ok');
  });
}

