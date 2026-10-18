// Bereich „Vorlagen“: wiederverwendbare Aktionslisten, die Buttons und Events mit „Vorlage ausführen“ aufrufen.
import { t as tr } from '/shared/i18n.js';
import { h, uid, prompt, empty, ask, contextMenu, toast } from '../dom.js';
import { renderFolderList, moveToFolderItem, addFolder, clearSelection } from '../folders.js';
import { S, mutate, render } from '../state.js';
import { actionList, testActions } from '../actions-ui.js';

/** Wo wird eine Vorlage aufgerufen? */
function usages(id) {
  const found = [];
  const check = (where, list) => list.some((a) => a.type === 'run_flow' && a.params.flow === id) && found.push(where);
  for (const pg of S.pages) {
    for (const b of pg.buttons) {
      const where = `${pg.name} › ${b.title || 'Button'}`;
      check(where, [...b.actions, ...b.actionsOff, ...(b.holdActions ?? []), ...(b.releaseActions ?? []), ...(b.doubleActions ?? []), ...(b.events ?? []).flatMap((e) => e.actions)]);
    }
  }
  for (const e of S.events) check(tr('common.event2', [e.name]), e.actions);
  for (const f of S.flows) if (f.id !== id) check(tr('common.flow', [f.name]), f.actions);
  return found;
}

async function addFlow() {
  const name = await prompt(tr('flows.newFlow'), tr('flows.nameOfFlowStart'));
  if (!name) return;
  const flow = { id: uid('f_'), name, inputs: [], actions: [] };
  mutate(() => {
    S.flows.push(flow);
    S.flowId = flow.id;
  });
}

/** Rechtsklick auf eine oder mehrere Vorlagen. */
function flowMenu(items, x, y) {
  const one = items.length === 1 ? items[0] : null;
  contextMenu(x, y, [
    one && { icon: '📂', label: tr('pages.openPage'), onclick: () => { S.flowId = one.id; render(); } },
    one && { icon: '✏️', label: tr('pages.renamePage'), onclick: async () => {
      const name = await prompt(tr('pages.renamePage'), tr('flows.nameOfFlowStart'), one.name);
      if (name) mutate(() => (one.name = name.slice(0, 100)));
    } },
    one && { icon: '⧉', label: tr('common.duplicate'), onclick: () => mutate(() => {
      const copy = { ...structuredClone(one), id: uid('f_'), name: tr('pages.copy2', [one.name]) };
      S.flows.splice(S.flows.indexOf(one) + 1, 0, copy);
      S.flowId = copy.id;
    }) },
    moveToFolderItem('flows', S.flows, items),
    '-',
    { icon: '🗑', label: one ? tr('common.delete') : tr('flows.deleteSelected', [items.length]), danger: true, onclick: () => deleteFlows(items) },
  ]);
}

/** Mehrere Vorlagen löschen; benutzte werden genannt. */
async function deleteFlows(list) {
  const used = list.flatMap((f) => usages(f.id).map((u) => `${f.name}: ${u}`));
  const r = await ask(tr('flows.deleteFlowsTitle', [list.length]), [list.map((f) => f.name).join(', '), used.length ? `\n\n${tr('flows.stillUsed')}\n${used.slice(0, 10).join('\n')}` : ''].join(''),
    [{ value: 'delete', label: tr('common.delete'), kind: 'danger' }]);
  if (r !== 'delete') return false;
  const ids = new Set(list.map((f) => f.id));
  mutate(() => {
    S.flows = S.flows.filter((f) => !ids.has(f.id));
    if (ids.has(S.flowId)) S.flowId = S.flows[0]?.id ?? null;
  });
  clearSelection('flows');
  toast(tr('flows.deleted', [list.length]), 'ok');
  return true;
}

function deleteFlow(flow) {
  const used = usages(flow.id);
  if (!confirm(used.length ? tr('flows.isStillUsedBy', [flow.name, used.join('\n')]) : tr('flows.deleteFlow', [flow.name]))) return;
  mutate(() => {
    S.flows = S.flows.filter((f) => f !== flow);
    S.flowId = S.flows[0]?.id ?? null;
  });
}

export function mountFlows(root) {
  root.className = 'view view-master';
  root.replaceChildren(
    h('aside', { class: 'sidebar' },
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('common.flows')),
        h('div', { class: 'head-buttons' },
          h('button', { class: 'btn small ghost', title: tr('folders.newFolder'), onclick: () => addFolder('flows') }, '📁'),
          h('button', { class: 'btn small', title: tr('flows.newFlow'), onclick: addFlow }, '＋'))),
      h('ul', { id: 'flow-list', class: 'page-list' })),
    h('section', { id: 'flow-detail', class: 'detail' }),
  );
}

export function renderFlows({ detail = true } = {}) {
  const flow = S.flows.find((f) => f.id === S.flowId) ?? S.flows[0];
  S.flowId = flow?.id ?? null;
  renderFolderList(document.getElementById('flow-list'), {
    kind: 'flows',
    items: S.flows,
    activeId: flow?.id,
    open: (f) => { S.flowId = f.id; S.expanded.clear(); render(); },
    row: (f) => [h('span', { class: 'name' }, f.name), h('span', { class: 'count' }, String(f.actions.length))],
    menu: (items, x, y) => flowMenu(items, x, y),
    deleteItems: (items) => deleteFlows(items),
  });
  if (!detail) return;
  const root = document.getElementById('flow-detail');
  if (!flow) {
    root.replaceChildren(empty('🧩',
      tr('flows.flowsAreActionLists'),
      tr('flows.buttonOrEventCalls')),
    h('div', { class: 'center' }, h('button', { class: 'btn primary', onclick: addFlow }, tr('flows.createFirstFlow'))));
    return;
  }
  const used = usages(flow.id);
  root.replaceChildren(
    h('div', { class: 'detail-head' },
      h('input', { class: 'page-name', value: flow.name, maxlength: 100, oninput: (e) => mutate(() => (flow.name = e.target.value || tr('flows.flow')), { coalesce: `${flow.id}:name`, detail: false }) }),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn primary small', title: tr('flows.runWithDefaults'), onclick: () => testActions(flow.actions, flow.name, { flowId: flow.id }) }, tr('flows.run')),
      h('button', { class: 'btn ghost danger small', onclick: () => deleteFlow(flow) }, tr('common.delete'))),
    h('p', { class: 'hint-text' }, used.length ? tr('flows.usedBy', [used.join(', ')]) : tr('flows.notUsedAnywhereYet')),
    inputsEditor(flow),
    actionList(flow, 'actions', tr('flows.actions'), { context: tr('common.flow', [flow.name]) }),
  );
}

/** Eingaben der Vorlage: Name und Standardwert; im Ablauf als {Eingabe.Name}. */
function inputsEditor(flow) {
  flow.inputs ??= [];
  const set = (i, k, v) => mutate(() => (flow.inputs[i][k] = v), { coalesce: `${flow.id}:in${i}${k}`, detail: false });
  return h('div', { class: 'action-section' },
    h('div', { class: 'list-title' }, h('h3', {}, tr('flows.inputs')),
      flow.inputs.length < 10 && h('button', { class: 'btn small', onclick: () => mutate(() => flow.inputs.push({ name: `${tr('flows.inputName')} ${flow.inputs.length + 1}`, default: '' })) }, tr('flows.addInput'))),
    h('p', { class: 'hint-text' }, tr('flows.inputsHint')),
    flow.inputs.map((inp, i) => h('div', { class: 'flow-input' },
      h('input', { value: inp.name, maxlength: 40, title: tr('flows.inputName'), oninput: (e) => set(i, 'name', e.target.value.replace(/[{}.]/g, '')) }),
      h('input', { value: inp.default, placeholder: tr('flows.inputDefault'), title: tr('flows.inputDefault'), oninput: (e) => set(i, 'default', e.target.value) }),
      h('code', {}, `{${tr('flows.placeholderPrefix')}.${inp.name}}`),
      h('button', { class: 'btn icon ghost danger', title: tr('common.remove'), onclick: () => mutate(() => flow.inputs.splice(i, 1)) }, '✕'))));
}
