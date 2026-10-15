// Kleine Helfer für DOM, Server-Aufrufe, Dialoge und Meldungen.

export const $ = (sel, root = document) => root.querySelector(sel);

/** DOM-Baukasten: h('div', { class: 'x', onclick }, kind1, kind2). */
export function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  let value;
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k === 'value') value = v;
    else if (k === 'checked') el.checked = !!v;
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v === true ? '' : v);
  }
  el.append(...children.flat(Infinity).filter((c) => c !== null && c !== undefined && c !== false));
  if (value !== undefined) el.value = value;
  return el;
}

export async function api(method, url, body) {
  const res = await fetch(url, {
    method,
    headers: { 'X-TouchDeck': '1', ...(body !== undefined && { 'Content-Type': 'application/json' }) },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw Object.assign(new Error(data.error || res.statusText), { status: res.status });
  return data;
}

export function toast(text, kind = '') {
  const t = h('div', { class: `toast ${kind}` }, text);
  $('#toasts').append(t);
  setTimeout(() => t.remove(), kind === 'error' ? 6000 : 3500);
}

/** Fragt mit eigenen Knöpfen nach. buttons: [{ value, label, kind }]; liefert den gewählten Wert oder null. */
export function ask(title, text, buttons) {
  const dlg = $('#dlg-ask');
  $('#ask-title').textContent = title;
  $('#ask-text').textContent = text;
  $('#ask-buttons').replaceChildren(
    h('button', { value: '', class: 'btn ghost' }, tr('common.cancel')),
    h('div', { class: 'spacer' }),
    ...buttons.map((b) => h('button', { value: b.value, class: `btn ${b.kind ?? ''}` }, b.label)),
  );
  dlg.returnValue = '';
  dlg.showModal();
  dlg.querySelector('.btn:last-child').focus();
  return new Promise((resolve) => dlg.addEventListener('close', () => resolve(dlg.returnValue || null), { once: true }));
}

/** Kurze Eingabe per Dialog (z. B. Name einer neuen Vorlage). */
export async function prompt(title, text, initial = '') {
  const dlg = $('#dlg-ask');
  const input = h('input', { value: initial, style: 'margin-top:10px' });
  $('#ask-title').textContent = title;
  $('#ask-text').replaceChildren(text, input);
  $('#ask-buttons').replaceChildren(
    h('button', { value: '', class: 'btn ghost' }, tr('common.cancel')),
    h('div', { class: 'spacer' }),
    h('button', { value: 'ok', class: 'btn primary' }, 'OK'),
  );
  dlg.returnValue = '';
  dlg.showModal();
  input.select();
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      dlg.close('ok');
    }
  });
  const r = await new Promise((resolve) => dlg.addEventListener('close', () => resolve(dlg.returnValue), { once: true }));
  return r === 'ok' ? input.value.trim() : null;
}

export const uid = (prefix) => prefix + Math.random().toString(16).slice(2, 14);

/**
 * Kontextmenü (Rechtsklick) an Position x/y. items: { icon, label, keys, onclick, disabled, danger }
 * oder '-' für eine Trennlinie; { label, submenu: [...] } öffnet ein Untermenü.
 */
export function contextMenu(x, y, items) {
  closeContextMenu();
  const menu = h('div', { class: 'ctx-menu', role: 'menu' });
  const close = () => closeContextMenu();
  menu.append(...items.filter(Boolean).map((it) => {
    if (it === '-') return h('div', { class: 'ctx-sep' });
    return h('button', {
      type: 'button', role: 'menuitem', class: `ctx-item${it.danger ? ' danger' : ''}`, disabled: it.disabled,
      onclick: (e) => {
        e.stopPropagation();
        if (it.submenu) {
          const r = e.currentTarget.getBoundingClientRect();
          return contextMenu(r.right - 4, r.top - 6, it.submenu);
        }
        close();
        it.onclick?.();
      },
    },
    h('span', { class: 'ctx-icon' }, it.icon ?? ''),
    h('span', { class: 'ctx-label' }, it.label),
    h('span', { class: 'ctx-keys' }, it.submenu ? '▸' : it.keys ?? ''));
  }));
  document.body.append(menu);
  // Im Fenster halten.
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(8, Math.min(x, innerWidth - r.width - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(y, innerHeight - r.height - 8))}px`;
  menu.querySelector('.ctx-item:not([disabled])')?.focus();
  menu.addEventListener('keydown', (e) => {
    const list = [...menu.querySelectorAll('.ctx-item:not([disabled])')];
    const i = list.indexOf(document.activeElement);
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      list[(i + (e.key === 'ArrowDown' ? 1 : list.length - 1)) % list.length]?.focus();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  });
  setTimeout(() => {
    document.addEventListener('pointerdown', onOutside, true);
    window.addEventListener('blur', close, { once: true });
  });
}

function onOutside(e) {
  if (!e.target.closest('.ctx-menu')) closeContextMenu();
}

export function closeContextMenu() {
  document.querySelector('.ctx-menu')?.remove();
  document.removeEventListener('pointerdown', onOutside, true);
}

export function segmented(options, current, onPick) {
  return h('div', { class: 'segmented' }, options.map(([value, label]) => h('button', { type: 'button', class: value === current ? 'active' : '', onclick: () => onPick(value) }, label)));
}

export function setIfChanged(input, value) {
  if (document.activeElement !== input && input.value !== String(value)) input.value = value;
}

export const empty = (icon, ...lines) => h('div', { class: 'empty' }, h('div', { class: 'empty-icon' }, icon), lines.map((l) => h('p', {}, l)));

const store = {
  get: (k) => {
    try {
      return localStorage.getItem(k);
    } catch {
      return null;
    }
  },
  set: (k, v) => {
    try {
      if (v === null) localStorage.removeItem(k);
      else localStorage.setItem(k, v);
    } catch {
      // Ohne Speicher gelten eben die Standardwerte.
    }
  },
};
export const prefs = store;

/**
 * Ziehgriff zwischen zwei Spalten: ändert die Breite von panel über die CSS-Variable varName an root.
 * invert = true, wenn das Panel rechts vom Griff liegt. Breite wird gemerkt, Doppelklick = Standard.
 */
export function resizer(root, panel, varName, { min = 160, max = 640, invert = false, key }) {
  const saved = Number(store.get(key));
  if (saved) root.style.setProperty(varName, `${saved}px`);
  const el = h('div', { class: 'resizer', role: 'separator', title: tr('editor.dragToResizeDouble') });
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // Ohne Zeiger-Erfassung funktioniert das Ziehen trotzdem, solange der Zeiger auf dem Griff bleibt.
    }
    const startX = e.clientX;
    const startW = panel.getBoundingClientRect().width;
    el.classList.add('active');
    document.body.classList.add('resizing');
    const move = (ev) => {
      const w = Math.round(Math.min(max, Math.max(min, startW + (ev.clientX - startX) * (invert ? -1 : 1))));
      root.style.setProperty(varName, `${w}px`);
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.classList.remove('active');
      document.body.classList.remove('resizing');
      store.set(key, String(Math.round(panel.getBoundingClientRect().width)));
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up, { once: true });
    el.addEventListener('pointercancel', up, { once: true });
  });
  el.addEventListener('dblclick', () => {
    root.style.removeProperty(varName);
    store.set(key, null);
  });
  return el;
}import { t as tr } from '/shared/i18n.js';

