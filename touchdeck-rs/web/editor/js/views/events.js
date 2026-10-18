// Bereich „Events“: Wenn <Auslöser>, dann <Aktionen> – unabhängig von Buttons.
import { t as tr } from '/shared/i18n.js';
import { h, uid, prompt, empty, ask, contextMenu, toast } from '../dom.js';
import { renderFolderList, moveToFolderItem, addFolder, clearSelection } from '../folders.js';
import { S, mutate, render } from '../state.js';
import { actionList, testActions, triggerEditor, triggerSummary, newTrigger } from '../actions-ui.js';

async function addEvent() {
  const name = await prompt(tr('events.newEvent'), tr('events.nameOfEventSwitch'));
  if (!name) return;
  const ev = { id: uid('ev_'), name, enabled: true, trigger: newTrigger('obs_stream'), cooldown: 0, actions: [] };
  mutate(() => {
    S.events.push(ev);
    S.eventId = ev.id;
  });
}

/** Rechtsklick auf ein oder mehrere Events. */
function eventMenu(items, x, y) {
  const one = items.length === 1 ? items[0] : null;
  const allOn = items.every((e) => e.enabled);
  contextMenu(x, y, [
    one && { icon: '📂', label: tr('pages.openPage'), onclick: () => { S.eventId = one.id; render(); } },
    one && { icon: '✏️', label: tr('pages.renamePage'), onclick: async () => {
      const name = await prompt(tr('pages.renamePage'), tr('events.nameOfEventSwitch'), one.name);
      if (name) mutate(() => (one.name = name.slice(0, 100)));
    } },
    one && { icon: '⧉', label: tr('common.duplicate'), onclick: () => mutate(() => {
      const copy = { ...structuredClone(one), id: uid('ev_'), name: tr('pages.copy2', [one.name]) };
      S.events.splice(S.events.indexOf(one) + 1, 0, copy);
      S.eventId = copy.id;
    }) },
    one && { icon: '▶', label: tr('events.runNow'), onclick: () => testActions(one.actions, one.name) },
    { icon: allOn ? '⏸' : '⏵', label: allOn ? tr('events.disable', [items.length]) : tr('events.enable', [items.length]), onclick: () => mutate(() => items.forEach((e) => (e.enabled = !allOn))) },
    moveToFolderItem('events', S.events, items),
    '-',
    { icon: '🗑', label: one ? tr('common.delete') : tr('events.deleteSelected', [items.length]), danger: true, onclick: () => deleteEvents(items) },
  ]);
}

async function deleteEvents(list) {
  const r = await ask(tr('events.deleteEventsTitle', [list.length]), list.map((e) => e.name).join(', '), [{ value: 'delete', label: tr('common.delete'), kind: 'danger' }]);
  if (r !== 'delete') return false;
  const ids = new Set(list.map((e) => e.id));
  mutate(() => {
    S.events = S.events.filter((e) => !ids.has(e.id));
    if (ids.has(S.eventId)) S.eventId = S.events[0]?.id ?? null;
  });
  clearSelection('events');
  toast(tr('flows.deleted', [list.length]), 'ok');
  return true;
}

function buttonEventCount() {
  return S.pages.reduce((n, pg) => n + pg.buttons.reduce((m, b) => m + (b.events?.length ?? 0), 0), 0);
}

export function mountEvents(root) {
  root.className = 'view view-master';
  root.replaceChildren(
    h('aside', { class: 'sidebar' },
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('common.events')),
        h('div', { class: 'head-buttons' },
          h('button', { class: 'btn small ghost', title: tr('folders.newFolder'), onclick: () => addFolder('events') }, '📁'),
          h('button', { class: 'btn small', title: tr('events.newEvent'), onclick: addEvent }, '＋'))),
      h('ul', { id: 'event-list', class: 'page-list' }),
      h('p', { id: 'event-note', class: 'hint-text sidebar-note' })),
    h('section', { id: 'event-detail', class: 'detail' }),
  );
}

export function renderEvents({ detail = true } = {}) {
  const ev = S.events.find((e) => e.id === S.eventId) ?? S.events[0];
  S.eventId = ev?.id ?? null;
  renderFolderList(document.getElementById('event-list'), {
    kind: 'events',
    items: S.events,
    activeId: ev?.id,
    open: (e) => { S.eventId = e.id; S.expanded.clear(); render(); },
    row: (e) => [h('span', { class: `dot ${e.enabled ? 'on' : ''}` }),
      h('span', { class: `name${e.enabled ? '' : ' off'}` }, e.name, h('small', {}, S.triggerMap.get(e.trigger.type)?.label ?? ''))],
    menu: (items, x, y) => eventMenu(items, x, y),
    deleteItems: (items) => deleteEvents(items),
  });
  const bc = buttonEventCount();
  document.getElementById('event-note').textContent = bc ? tr('events.plusEventSDirectly', [bc]) : '';
  if (!detail) return;
  const root = document.getElementById('event-detail');
  if (!ev) {
    root.replaceChildren(empty('⚡',
      tr('events.eventsRunActionsAutomatically'),
      tr('events.forExampleWhenObs')),
    h('div', { class: 'center' }, h('button', { class: 'btn primary', onclick: addEvent }, tr('events.createFirstEvent'))));
    return;
  }
  root.replaceChildren(
    h('div', { class: 'detail-head' },
      h('input', { class: 'page-name', value: ev.name, maxlength: 100, oninput: (e) => mutate(() => (ev.name = e.target.value || tr('common.event')), { coalesce: `${ev.id}:name`, detail: false }) }),
      h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: ev.enabled, onchange: (e) => mutate(() => (ev.enabled = e.target.checked)) }), tr('common.active')),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn primary small', title: tr('events.runActionsNowWithout'), onclick: () => testActions(ev.actions, ev.name) }, tr('events.runNow')),
      h('button', { class: 'btn ghost danger small', onclick: () => confirm(tr('events.deleteEvent', [ev.name])) && mutate(() => { S.events = S.events.filter((x) => x !== ev); S.eventId = null; }) }, tr('common.delete'))),
    h('div', { class: 'event-when' },
      h('p', { class: 'muted-note' }, triggerSummary(ev.trigger)),
      triggerEditor(ev.trigger),
      h('label', { class: 'field' }, tr('events.cooldownInSeconds0'),
        h('input', { type: 'number', min: 0, max: 86400, value: ev.cooldown, onchange: (e) => mutate(() => (ev.cooldown = Math.max(0, Number(e.target.value) || 0))) }))),
    actionList(ev, 'actions', tr('common.then'), { context: tr('common.event2', [ev.name]) }),
  );
}
