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
    h('button', { value: '', class: 'btn ghost' }, 'Abbrechen'),
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
    h('button', { value: '', class: 'btn ghost' }, 'Abbrechen'),
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

export function segmented(options, current, onPick) {
  return h('div', { class: 'segmented' }, options.map(([value, label]) => h('button', { type: 'button', class: value === current ? 'active' : '', onclick: () => onPick(value) }, label)));
}

export function setIfChanged(input, value) {
  if (document.activeElement !== input && input.value !== String(value)) input.value = value;
}

export const empty = (icon, ...lines) => h('div', { class: 'empty' }, h('div', { class: 'empty-icon' }, icon), lines.map((l) => h('p', {}, l)));
