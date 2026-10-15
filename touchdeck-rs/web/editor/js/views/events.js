// Bereich „Events“: Wenn <Auslöser>, dann <Aktionen> – unabhängig von Buttons.
import { t as tr } from '/shared/i18n.js';
import { h, uid, prompt, empty } from '../dom.js';
import { S, mutate, render } from '../state.js';
import { actionList, testActions, triggerEditor, triggerSummary, newTrigger } from '../actions-ui.js';

async function addEvent() {
  const name = await prompt(tr('Neues Event'), tr('Name des Events (z. B. „Bei Streamstart Szene wechseln“):'));
  if (!name) return;
  const ev = { id: uid('ev_'), name, enabled: true, trigger: newTrigger('obs_stream'), cooldown: 0, actions: [] };
  mutate(() => {
    S.events.push(ev);
    S.eventId = ev.id;
  });
}

function buttonEventCount() {
  return S.pages.reduce((n, pg) => n + pg.buttons.reduce((m, b) => m + (b.events?.length ?? 0), 0), 0);
}

export function mountEvents(root) {
  root.className = 'view view-master';
  root.replaceChildren(
    h('aside', { class: 'sidebar' },
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('Events')), h('button', { class: 'btn small', title: tr('Neues Event'), onclick: addEvent }, '＋')),
      h('ul', { id: 'event-list', class: 'page-list' }),
      h('p', { id: 'event-note', class: 'hint-text sidebar-note' })),
    h('section', { id: 'event-detail', class: 'detail' }),
  );
}

export function renderEvents({ detail = true } = {}) {
  const ev = S.events.find((e) => e.id === S.eventId) ?? S.events[0];
  S.eventId = ev?.id ?? null;
  document.getElementById('event-list').replaceChildren(...S.events.map((e) => h('li', { class: `${e === ev ? 'active' : ''}${e.enabled ? '' : ' off'}`, onclick: () => { S.eventId = e.id; S.expanded.clear(); render(); } },
    h('span', { class: `dot ${e.enabled ? 'on' : ''}` }),
    h('span', { class: 'name' }, e.name, h('small', {}, S.triggerMap.get(e.trigger.type)?.label ?? '')))));
  const bc = buttonEventCount();
  document.getElementById('event-note').textContent = bc ? tr('Dazu {0} Event(s) direkt an Buttons (Tab „Event“).', [bc]) : '';
  if (!detail) return;
  const root = document.getElementById('event-detail');
  if (!ev) {
    root.replaceChildren(empty('⚡',
      tr('Events führen Aktionen automatisch aus – ohne dass jemand einen Button drückt.'),
      tr('Zum Beispiel: „Wenn der OBS-Stream startet, Szene wechseln und Chat-Hinweis setzen“, „Jeden Tag um 18:00 Musik leiser“ oder „Wenn der Wert Tore größer als 3 wird, Sound abspielen“.')),
    h('div', { class: 'center' }, h('button', { class: 'btn primary', onclick: addEvent }, tr('＋ Erstes Event anlegen'))));
    return;
  }
  root.replaceChildren(
    h('div', { class: 'detail-head' },
      h('input', { class: 'page-name', value: ev.name, maxlength: 100, oninput: (e) => mutate(() => (ev.name = e.target.value || tr('Event')), { coalesce: `${ev.id}:name`, detail: false }) }),
      h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: ev.enabled, onchange: (e) => mutate(() => (ev.enabled = e.target.checked)) }), 'aktiv'),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn primary small', title: tr('Aktionen jetzt ausführen, ohne auf den Auslöser zu warten'), onclick: () => testActions(ev.actions, ev.name) }, tr('▶ Jetzt ausführen')),
      h('button', { class: 'btn ghost danger small', onclick: () => confirm(tr('Event „{0}“ löschen?', [ev.name])) && mutate(() => { S.events = S.events.filter((x) => x !== ev); S.eventId = null; }) }, tr('Löschen'))),
    h('div', { class: 'event-when' },
      h('p', { class: 'muted-note' }, triggerSummary(ev.trigger)),
      triggerEditor(ev.trigger),
      h('label', { class: 'field' }, tr('Abklingzeit in Sekunden (0 = keine) – so oft darf das Event höchstens laufen'),
        h('input', { type: 'number', min: 0, max: 86400, value: ev.cooldown, onchange: (e) => mutate(() => (ev.cooldown = Math.max(0, Number(e.target.value) || 0))) }))),
    actionList(ev, 'actions', tr('Dann …'), { context: tr('Event „{0}“', [ev.name]) }),
  );
}
