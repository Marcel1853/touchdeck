// Bereich „Vorlagen“: wiederverwendbare Aktionslisten, die Buttons und Events mit „Vorlage ausführen“ aufrufen.
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
      check(where, [...b.actions, ...b.actionsOff, ...(b.holdActions ?? []), ...(b.releaseActions ?? []), ...(b.events ?? []).flatMap((e) => e.actions)]);
    }
  }
  for (const e of S.events) check(`Event „${e.name}“`, e.actions);
  for (const f of S.flows) if (f.id !== id) check(`Vorlage „${f.name}“`, f.actions);
  return found;
}

async function addFlow() {
  const name = await prompt('Neue Vorlage', 'Name der Vorlage (z. B. „Stream starten“):');
  if (!name) return;
  const flow = { id: uid('f_'), name, actions: [] };
  mutate(() => {
    S.flows.push(flow);
    S.flowId = flow.id;
  });
}

function deleteFlow(flow) {
  const used = usages(flow.id);
  if (!confirm(used.length ? `„${flow.name}“ wird noch benutzt von:\n${used.join('\n')}\n\nTrotzdem löschen?` : `Vorlage „${flow.name}“ löschen?`)) return;
  mutate(() => {
    S.flows = S.flows.filter((f) => f !== flow);
    S.flowId = S.flows[0]?.id ?? null;
  });
}

export function mountFlows(root) {
  root.className = 'view view-master';
  root.replaceChildren(
    h('aside', { class: 'sidebar' },
      h('div', { class: 'sidebar-head' }, h('h2', {}, 'Vorlagen'), h('button', { class: 'btn small', title: 'Neue Vorlage', onclick: addFlow }, '＋')),
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
      'Vorlagen sind Aktionslisten, die du einmal baust und überall wiederverwendest.',
      'Ein Button oder Event ruft sie mit „Logik › Vorlage ausführen“ auf – auch mehrfach hintereinander (Wiederholen).'),
    h('div', { class: 'center' }, h('button', { class: 'btn primary', onclick: addFlow }, '＋ Erste Vorlage anlegen')));
    return;
  }
  const used = usages(flow.id);
  root.replaceChildren(
    h('div', { class: 'detail-head' },
      h('input', { class: 'page-name', value: flow.name, maxlength: 100, oninput: (e) => mutate(() => (flow.name = e.target.value || 'Vorlage'), { coalesce: `${flow.id}:name`, detail: false }) }),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn primary small', onclick: () => testActions(flow.actions, flow.name) }, '▶ Ausführen'),
      h('button', { class: 'btn ghost danger small', onclick: () => deleteFlow(flow) }, 'Löschen')),
    h('p', { class: 'hint-text' }, used.length ? `Benutzt von: ${used.join(', ')}` : 'Noch nirgends benutzt. Füge in einem Button oder Event „Vorlage ausführen“ hinzu.'),
    actionList(flow, 'actions', 'Aktionen', { context: `Vorlage „${flow.name}“` }),
  );
}
