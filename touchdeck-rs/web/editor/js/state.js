// Zustand des Editors, Rückgängig/Wiederholen, automatisches Speichern und Abgleich mit dem Server.
import { t as tr } from '/shared/i18n.js';
import { $, api, toast } from './dom.js';

export const S = {
  view: 'pages',
  // Projekt (wird gespeichert und ist rückgängig machbar)
  pages: [],
  flows: [],
  events: [],
  values: {},
  // Laufzeit
  variables: {},
  settings: {},
  revision: 0,
  // Beschreibungen vom Server
  actionTypes: [],
  typeMap: new Map(),
  triggerTypes: [],
  triggerMap: new Map(),
  liveSources: [],
  sliderTargets: [],
  version: '',
  // Auswahl und Anzeige
  pageId: null,
  selId: null,
  tab: 'look',
  flowId: null,
  eventId: null,
  expanded: new Set(),
  options: {},
  apps: null,
  icons: [],
  iconPacks: [],
  logs: [],
  showIcons: false,
};

export const page = () => S.pages.find((p) => p.id === S.pageId) ?? S.pages[0];
export const sel = () => page()?.buttons.find((b) => b.id === S.selId);

// ---------- Darstellen ----------

let renderer = () => {};
/** Der aktive Bereich meldet hier seine Zeichenfunktion an. */
export function setRenderer(fn) {
  renderer = fn;
}
const renderHooks = new Set();
/** Weitere Bereiche (z. B. der Aktions-Editor), die nach jeder Änderung mit neu gezeichnet werden. */
export const onRender = (fn) => renderHooks.add(fn);

/** detail=false zeichnet die Eingabespalte nicht neu (damit Tippen den Fokus behält). */
export function render({ detail = true } = {}) {
  renderer({ detail });
  for (const fn of renderHooks) fn({ detail });
  $('#undo').disabled = !undoStack.length;
  $('#redo').disabled = !redoStack.length;
}

// ---------- Rückgängig ----------

const undoStack = [];
const redoStack = [];
let lastCoalesce = null;
let lastEdit = 0;

const snapshot = () => JSON.stringify({ pages: S.pages, flows: S.flows, events: S.events, values: S.values, folders: S.folders });
function apply(json) {
  Object.assign(S, JSON.parse(json));
  if (!S.pages.some((p) => p.id === S.pageId)) S.pageId = S.pages[0].id;
  if (!sel()) S.selId = null;
}

/** Jede Änderung am Projekt läuft hierüber: Rückgängig-Schritt, Speichern, Neuzeichnen. */
export function mutate(fn, { coalesce = null, detail = true } = {}) {
  const now = Date.now();
  if (!(coalesce && coalesce === lastCoalesce && now - lastEdit < 1200)) {
    undoStack.push(snapshot());
    if (undoStack.length > 100) undoStack.shift();
    redoStack.length = 0;
  }
  lastCoalesce = coalesce;
  lastEdit = now;
  fn();
  scheduleSave();
  render({ detail });
}

function restore(from, to) {
  if (!from.length) return;
  to.push(snapshot());
  apply(from.pop());
  lastCoalesce = null;
  scheduleSave();
  render();
}
export const undo = () => restore(undoStack, redoStack);
export const redo = () => restore(redoStack, undoStack);
export const pushUndo = () => undoStack.push(snapshot());

// ---------- Speichern ----------

let saveTimer = null;
let saving = null;
let dirty = false;

function setSaveState(text, isError = false) {
  const el = $('#save-state');
  el.textContent = text;
  el.classList.toggle('error', isError);
}

function scheduleSave() {
  dirty = true;
  setSaveState(tr('editor.savingShortly'));
  clearTimeout(saveTimer);
  saveTimer = setTimeout(save, 400);
}

export async function save() {
  clearTimeout(saveTimer);
  if (saving) await saving;
  if (!dirty) return;
  dirty = false;
  saving = api('PUT', '/api/pages', { pages: S.pages, flows: S.flows, events: S.events, values: S.values, folders: S.folders, revision: S.revision })
    .then((r) => {
      S.revision = r.revision;
      setSaveState(dirty ? tr('editor.savingShortly') : tr('editor.allChangesSaved'));
    })
    .catch(async (err) => {
      if (err.status === 409) {
        // Ein anderes Fenster war schneller: dessen Stand übernehmen statt ihn zu überschreiben.
        await loadState();
        toast(tr('editor.somethingWasChangedIn'), 'error');
        return;
      }
      dirty = true;
      setSaveState(tr('editor.savingFailed', [err.message]), true);
    })
    .finally(() => (saving = null));
  await saving;
}

window.addEventListener('beforeunload', (e) => {
  if (dirty) e.preventDefault();
});

// ---------- Laden und Abgleich ----------

/** Lädt den kompletten Stand vom Server (beim Start oder wenn ein anderes Fenster etwas geändert hat). */
export async function loadState({ keepUndo = false } = {}) {
  const st = await api('GET', '/api/state');
  Object.assign(S, {
    pages: st.pages, flows: st.flows, events: st.events, values: st.values, variables: st.variables, folders: { pages: [], flows: [], events: [], ...st.folders },
    settings: st.settings, revision: st.revision, version: st.version, port: st.port,
    actionTypes: st.actionTypes, typeMap: new Map(st.actionTypes.map((t) => [t.type, t])),
    triggerTypes: st.triggerTypes, triggerMap: new Map(st.triggerTypes.map((t) => [t.type, t])),
    liveSources: st.liveSources, sliderTargets: st.sliderTargets,
  });
  S.pageId ??= st.startPage;
  if (!S.pages.some((p) => p.id === S.pageId)) S.pageId = st.startPage;
  if (!sel()) S.selId = null;
  dirty = false;
  if (!keepUndo) {
    undoStack.length = 0;
    redoStack.length = 0;
  }
  setSaveState(tr('editor.allChangesSaved'));
  render();
}

const logListeners = new Set();
export const onLog = (fn) => logListeners.add(fn);
const pairListeners = new Set();
export const onPairRequest = (fn) => pairListeners.add(fn);

/** Live-Verbindung: neue Werte, Log-Einträge und Änderungen aus anderen Fenstern. */
export function watchServer() {
  const ws = new WebSocket(`ws://${location.host}/ws?role=editor`);
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.type === 'pair-request') {
      for (const fn of pairListeners) fn(msg);
    } else if (msg.type === 'log') {
      S.logs.push(msg.entry);
      if (S.logs.length > 1500) S.logs.shift();
      for (const fn of logListeners) fn(msg.entry);
    } else if (msg.type === 'vars') {
      for (const [name, value] of Object.entries(msg.values)) {
        if (value === null) delete S.variables[name];
        else S.variables[name] = value;
      }
      if (S.view === 'values') render({ detail: !document.activeElement?.closest('.values-table') });
    } else if (msg.type === 'state') {
      const valuesChanged = JSON.stringify(msg.variables) !== JSON.stringify(S.variables);
      S.variables = msg.variables;
      if (msg.revision !== S.revision && !dirty && !saving) loadState({ keepUndo: true }).catch(() => {});
      else if (valuesChanged && S.view === 'values') render({ detail: !document.activeElement?.closest('.values-table') });
    }
  };
  ws.onclose = () => setTimeout(watchServer, 3000);
}

export { setSaveState };
