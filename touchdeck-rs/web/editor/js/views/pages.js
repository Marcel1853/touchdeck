// Bereich „Seiten“: Seitenliste, Raster mit Buttons und die Eigenschaften des gewählten Buttons.
import { t as tr, getLang } from '/shared/i18n.js';
import { buttonElement, setupGrid, iconUrl } from '/shared/render.js';
import { $, h, api, toast, ask, uid, segmented, setIfChanged, empty, prefs, contextMenu } from '../dom.js';
import { S, page, sel, mutate, render, save, pushUndo } from '../state.js';
import { iconPicker } from './iconpacks.js';
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
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('common.pages')), h('button', { class: 'btn small', title: tr('pages.newPage'), onclick: addPage }, '＋')),
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
        tplButton('touchportal', '⤵️', tr('pages.touchPortal'), tr('pages.importPages'), importTouchPortal))),
    h('section', { class: 'workspace' },
      h('div', { class: 'page-toolbar' },
        h('input', { id: 'page-name', class: 'page-name', maxlength: 100, 'aria-label': tr('pages.pageName'),
          oninput: (e) => mutate(() => (page().name = e.target.value || tr('pages.page')), { coalesce: `${S.pageId}:name`, detail: false }) }),
        h('label', { class: 'field-inline' }, tr('pages.columns'), h('input', { id: 'page-cols', type: 'number', min: 1, max: 16, onchange: (e) => resizePage(page().rows, Number(e.target.value)) })),
        h('label', { class: 'field-inline' }, tr('pages.rows'), h('input', { id: 'page-rows', type: 'number', min: 1, max: 12, onchange: (e) => resizePage(Number(e.target.value), page().cols) })),
        h('label', { class: 'field-inline' }, tr('pages.background'), h('input', { id: 'page-bg', type: 'color',
          oninput: (e) => mutate(() => (page().background = e.target.value), { coalesce: `${S.pageId}:bg`, detail: false }) })),
        h('button', { class: 'btn small ghost', onclick: duplicatePage }, tr('common.duplicate')),
        h('button', { id: 'page-delete', class: 'btn small ghost danger', onclick: deletePage }, tr('common.delete'))),
      h('div', { class: 'canvas-wrap' }, h('div', { id: 'canvas', class: 'td-grid canvas' })),
      h('p', { class: 'hint' }, tr('pages.clickEmptyFieldNew'))),
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
    title: tr('pages.dragToReorder'),
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
      const cell = h('div', { class: 'cell', style: `grid-row:${r + 1};grid-column:${c + 1}`, title: tr('pages.newButton'), onclick: () => addButton(r, c),
        oncontextmenu: (e) => {
          e.preventDefault();
          cellMenu(e.clientX, e.clientY, r, c);
        } }, '＋');
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

// ---------- Seiten ----------

export function selectPage(id) {
  S.pageId = id;
  S.selId = null;
  render();
}

function addPage() {
  const cur = page();
  const p = { id: uid('p_'), name: tr('pages.newPage'), rows: cur?.rows ?? 3, cols: cur?.cols ?? 5, background: cur?.background ?? '#14161b', buttons: [] };
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
const STYLE_KEYS = ['color', 'colorOn', 'textColor', 'fontSize', 'titlePos', 'iconFit'];

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
export function pasteButton({ row, col, replace } = {}) {
  const src = readClip(CLIP);
  if (!src) return toast(tr('pages.nothingToPasteCopy'), 'error');
  const p = page();
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

/** Button auf eine andere Seite verschieben (keep = false) oder dorthin kopieren (keep = true). */
function sendToPage(id, targetId, keep) {
  const b = findButton(id);
  const target = S.pages.find((p) => p.id === targetId);
  if (!b || !target) return;
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
  ]);
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

function renderInspector() {
  const root = $('#inspector');
  const b = sel();
  if (!b) {
    root.replaceChildren(empty('▦',
      tr('pages.selectButtonOrClick'),
      tr('pages.withPreviewYouSee')));
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
      h('h2', {}, b.title || tr('pages.button')),
      b.mode !== 'slider' && h('button', { class: 'btn small primary', title: tr('pages.runActionsOnPc'), onclick: () => testActions(b.actions, tr('pages.button'), { buttonId: b.id }) }, tr('pages.test'))),
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
    h('div', { class: 'emoji-grid' }, EMOJIS.map((em) => h('button', { type: 'button', onclick: () => mutate(() => (b[key] = { type: 'emoji', value: em })) }, em))),
    iconPicker(b, key),
    canBeOn && h('label', { class: 'field' }, tr('pages.labelWhenOnEmpty'),
      h('input', { value: b.titleOn ?? '', maxlength: 200, placeholder: tr('pages.exampleMuted'), oninput: (e) => edit('titleOn', () => (b.titleOn = e.target.value), { detail: false }) })),

    h('h3', {}, tr('pages.colors')),
    h('div', { class: 'colors' },
      h('label', {}, h('input', { type: 'color', value: b.color, oninput: (e) => edit('color', () => (b.color = e.target.value), { detail: false }) }), tr('pages.background')),
      h('label', {}, h('input', { type: 'color', value: b.textColor, oninput: (e) => edit('tc', () => (b.textColor = e.target.value), { detail: false }) }), tr('pages.text')),
      canBeOn && h('label', {}, h('input', { type: 'color', value: b.colorOn, oninput: (e) => edit('con', () => (b.colorOn = e.target.value), { detail: false }) }), tr('pages.whenOn'))),
    h('div', { class: 'swatches' }, SWATCHES.map((c) => h('button', { type: 'button', title: c, style: `background:${c}`, onclick: () => mutate(() => (b.color = c)) }))),

    h('h3', {}, tr('pages.sizeBehavior')),
    h('div', { class: 'row' },
      h('label', { class: 'field' }, tr('pages.widthFields'),
        h('input', { type: 'number', min: 1, max: p.cols - b.col, value: b.colSpan, onchange: (e) => resizeButton(b, b.rowSpan, Number(e.target.value)) })),
      h('label', { class: 'field' }, tr('pages.heightFields'),
        h('input', { type: 'number', min: 1, max: p.rows - b.row, value: b.rowSpan, onchange: (e) => resizeButton(b, Number(e.target.value), b.colSpan) }))),
    h('div', { class: 'field' }, tr('pages.type'),
      segmented([['press', tr('pages.press')], ['toggle', tr('pages.toggle')], ['slider', tr('pages.slider')]], b.mode, (v) => mutate(() => (b.mode = v)))),
    b.mode !== 'slider' && liveSection(b),
  ];
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
    b.live.source === 'command' && h('label', { class: 'field' }, tr('pages.command'),
      h('input', { value: b.live.command, placeholder: tr('pages.exampleUptimeP'), oninput: (e) => mutate(() => (b.live.command = e.target.value), { coalesce: `${b.id}:livecmd`, detail: false }) })),
    b.live.source === 'variable' && dynamicSelect({ label: tr('pages.value'), source: 'variables', allowNew: true }, b.live.variable,
      (v) => mutate(() => (b.live.variable = v), { coalesce: `${b.id}:livevar`, detail: false })),
    hints[b.live.source] && h('p', { class: 'hint-text' }, hints[b.live.source]));
}

function pressTab(b) {
  if (b.mode === 'toggle') {
    return [
      actionList(b, 'actions', tr('pages.whenSwitchedOn'), { buttonId: b.id, compact: true, context: b.title || tr('pages.button') }),
      actionList(b, 'actionsOff', tr('pages.whenSwitchedOff'), { buttonId: b.id, compact: true, context: b.title || tr('pages.button') }),
    ];
  }
  return [actionList(b, 'actions', tr('pages.whenPressed'), { buttonId: b.id, compact: true, context: b.title || tr('pages.button'), hint: b.holdActions.length ? tr('pages.shortTapRunsThese') : '' })];
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
  ];
}

function eventsTab(b) {
  return [
    h('p', { class: 'hint-text' }, tr('pages.buttonReactsToEvents')),
    ...b.events.map((ev, i) => h('div', { class: `event-card${ev.enabled ? '' : ' disabled'}` },
      h('div', { class: 'list-title' },
        h('h3', {}, tr('pages.event', [i + 1])),
        h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: ev.enabled, onchange: (e) => mutate(() => (ev.enabled = e.target.checked)) }), tr('common.active')),
        h('button', { class: 'btn icon ghost danger', title: tr('pages.removeEvent'), onclick: () => mutate(() => b.events.splice(i, 1)) }, '✕')),
      h('p', { class: 'muted-note' }, triggerSummary(ev.trigger)),
      triggerEditor(ev.trigger),
      actionList(ev, 'actions', tr('common.then'), { buttonId: b.id, compact: true, context: tr('pages.event2', [b.title || 'Button', i + 1]) }))),
    h('button', { class: 'btn', onclick: () => mutate(() => b.events.push({ id: uid('e_'), enabled: true, trigger: newTrigger(), actions: [] })) }, tr('pages.addEvent')),
  ];
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

const TEMPLATE_NAMES = { steam: tr('pages.steamGames'), system: tr('common.system'), obs: 'OBS', touchportal: tr('pages.touchPortal') };

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
const liveShow = (source) => ({ live: { source, command: '', variable: '' } });

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
    id: uid('p_'), name: tr('common.system'), rows: 3, cols: 5, background: '#14161b',
    buttons: [
      // Zahlen wie Uhrzeit oder CPU gibt es nur als Live-Anzeige (Reiter „Aussehen“).
      tplBtn(0, 0, tr('pages.clock'), '🕐', [], liveShow('clock')),
      tplBtn(0, 1, 'CPU', '🧠', [], liveShow('cpu')),
      tplBtn(0, 2, 'RAM', '💾', [], liveShow('ram')),
      tplBtn(0, 3, tr('pages.temperature'), '🌡️', [], { ...liveShow('temp'), colorOn: '#a33a2a' }),
      tplBtn(0, 4, tr('pages.volume'), '', [], { rowSpan: 3, mode: 'slider', color: '#3b3f8f' }),
      tplBtn(1, 0, tr('pages.music'), '⏯️', [['media', { op: 'play-pause' }]], { colSpan: 2, ...liveShow('track'), colorOn: '#2f6e4f' }),
      muteButton(1, 2),
      tplBtn(1, 3, tr('pages.microphone'), '🎙️', [['mic_mute', { op: 'toggle' }]],
        muteEvents('input', { icon: '🔇', title: tr('pages.micOff'), color: '#8f3b3b' })),
      tplBtn(2, 0, tr('pages.output'), '🎧', [['audio_output', { target: 'next' }]]),
      tplBtn(2, 1, tr('pages.photo'), '📸', [['screenshot', { mode: 'dialog' }]]),
      tplBtn(2, 2, tr('pages.nightLight'), '🌙', [['night_light', { mode: 'toggle' }]]),
      tplBtn(2, 3, tr('pages.lock'), '🔒', [['lock_screen']], { color: '#5b3fa0' }),
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
  if (!(await askTemplate('steam', tr('pages.allInstalledSteamGames')))) return;
  await serverTemplate('/api/steam/pages', (r) => {
    S.pageId = r.created[0];
    toast(tr(r.updated ? 'pages.steamPageSUpdated' : 'pages.steamPageSCreated', [r.created.length]), 'ok');
  });
}

async function importTouchPortal() {
  if (!(await askTemplate('touchportal', tr('pages.importsAllPagesFrom')))) return;
  await serverTemplate('/api/import/touchportal', (r) => {
    S.pageId = S.pages.find((p) => p.template === 'touchportal')?.id ?? S.pageId;
    const st = r.stats;
    const missing = Object.entries(st.unsupported);
    toast(`${tr(r.updated ? 'pages.updatedPageSButtons' : 'pages.importedPageSButtons', [st.pages, st.buttons, st.actions, st.icons])}${missing.length ? `\n${tr('pages.notSupportedYet', [missing.map(([k, n]) => `${k} (${n}×)`).join(', ')])}` : ''}`, 'ok');
  });
}
