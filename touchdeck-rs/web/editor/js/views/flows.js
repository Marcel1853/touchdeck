// Bereich „Vorlagen“: wiederverwendbare Aktionslisten, die Buttons und Events mit „Vorlage ausführen“ aufrufen.
import { t as tr } from '/shared/i18n.js';
import { h, uid, prompt, empty } from '../dom.js';
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
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('common.flows')), h('button', { class: 'btn small', title: tr('flows.newFlow'), onclick: addFlow }, '＋')),
      h('ul', { id: 'flow-list', class: 'page-list' })),
    h('section', { id: 'flow-detail', class: 'detail' }),
  );
}

export function renderFlows({ detail = true } = {}) {
  const flow = S.flows.find((f) => f.id === S.flowId) ?? S.flows[0];
  S.flowId = flow?.id ?? null;
  document.getElementById('flow-list').replaceChildren(...S.flows.map((f) => h('li', { class: f === flow ? 'active' : '', onclick: () => { S.flowId = f.id; S.expanded.clear(); render(); } },
    h('span', { class: 'name' }, f.name), h('span', { class: 'count' }, String(f.actions.length)))));
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
      h('button', { class: 'btn primary small', onclick: () => testActions(flow.actions, flow.name) }, tr('flows.run')),
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
