// Einstieg des Editors: Navigation zwischen den Bereichen, Kopplungsdialog, Tastenkürzel, Start.
import { t as tr } from '/shared/i18n.js';
import { $, h, api, toast, ask, resizer } from './dom.js';
import { S, setRenderer, render, loadState, watchServer, undo, redo, sel, onPairRequest } from './state.js';
import { mountPages, renderPages, duplicateButton, deleteButton, select, copyButton, cutButton, pasteButton } from './views/pages.js';
import { mountValues, renderValues } from './views/values.js';
import { mountFlows, renderFlows } from './views/flows.js';
import { mountEvents, renderEvents } from './views/events.js';
import { mountLogs, unmountLogs, renderLogs } from './views/logs.js';
import { loadIconPacks } from './views/iconpacks.js';
import { mountPlugins, renderPlugins } from './views/plugins.js';
import { mountSettings, renderSettings, applyTheme, updateSettings, resetSettingsCache } from './views/settings.js';

const VIEWS = {
  pages: { icon: '▦', label: tr('Seiten'), mount: mountPages, render: renderPages },
  values: { icon: '🏷️', label: tr('Werte'), mount: mountValues, render: renderValues },
  events: { icon: '⚡', label: tr('Events'), mount: mountEvents, render: renderEvents },
  flows: { icon: '🧩', label: tr('Vorlagen'), mount: mountFlows, render: renderFlows },
  logs: { icon: '📜', label: tr('Logs'), mount: mountLogs, render: renderLogs, unmount: unmountLogs },
  plugins: { icon: '🧱', label: tr('Plugins'), mount: mountPlugins, render: renderPlugins },
  settings: { icon: '⚙️', label: tr('Einstellungen'), mount: mountSettings, render: renderSettings },
};

export function showView(name) {
  VIEWS[S.view]?.unmount?.();
  S.view = name;
  S.expanded.clear();
  if (name === 'settings') resetSettingsCache();
  const view = VIEWS[name];
  view.mount($('#view'));
  addResizers($('#view'));
  setRenderer(view.render);
  for (const b of document.querySelectorAll('.rail button[data-view]')) b.classList.toggle('active', b.dataset.view === name);
  render();
}

/** Ziehgriffe zwischen die Spalten setzen (Seiten: links und rechts, sonst nach der Liste). */
function addResizers(root) {
  const side = root.querySelector(':scope > .sidebar');
  if (side) side.after(resizer(root, side, '--side-w', { min: 170, max: 480, key: `touchdeck.width.${S.view}.side` }));
  const insp = root.querySelector(':scope > .inspector');
  if (insp) insp.before(resizer(root, insp, '--insp-w', { min: 300, max: 760, invert: true, key: 'touchdeck.width.pages.insp' }));
}

function buildRail() {
  $('#rail').replaceChildren(...Object.entries(VIEWS).map(([id, v]) => h('button', { 'data-view': id, title: v.label, onclick: () => showView(id) },
    h('span', { class: 'rail-icon' }, v.icon), h('span', { class: 'rail-label' }, v.label))));
}

// ---------- Handy koppeln ----------

export async function openPairing() {
  $('#dlg-pair').showModal();
  await refreshPairing();
}

async function refreshPairing() {
  try {
    const r = await api('GET', '/api/pairing');
    $('#pair-lan').checked = r.lanEnabled;
    $('#pair-qr').innerHTML = r.qr && r.lanEnabled ? r.qr : '';
    $('#pair-qr').hidden = !(r.qr && r.lanEnabled);
    $('#pair-app').replaceChildren(...(r.apkQr && r.lanEnabled
      ? [h('div', { class: 'qr small' }), h('div', {},
        h('b', {}, tr('Android-App')),
        h('p', { class: 'hint-text' }, tr('QR-Code scannen, App herunterladen und installieren (einmalig „Unbekannte Apps installieren“ erlauben). Die App findet diesen PC dann von selbst.')),
        h('code', {}, r.apkUrl))]
      : []));
    if (r.apkQr && r.lanEnabled) $('#pair-app .qr').innerHTML = r.apkQr;
    $('#pair-urls').replaceChildren(...(r.lanEnabled
      ? [...(r.urls.length ? r.urls.map((u) => h('code', {}, u)) : [h('p', {}, tr('Kein Netzwerk gefunden. Ist der PC mit dem WLAN/LAN verbunden?'))]),
        r.pin && h('p', { class: 'hint-text' }, tr('🔐 Eine PIN ist gesetzt – das Handy fragt beim ersten Verbinden danach.')),
        r.firewall && h('div', { class: 'firewall-hint' },
          h('b', {}, tr('🧱 Die Firewall (ufw) ist aktiv.')),
          h('p', {}, tr('Damit Handys TouchDeck finden und erreichen, muss sie fürs Heimnetz geöffnet werden. Das geht mit einem Klick (fragt dein Passwort ab) – oder von Hand im Terminal:')),
          h('button', { type: 'button', class: 'btn primary small', onclick: async (e) => {
            e.target.disabled = true;
            e.target.textContent = tr('Warte auf Passwort …');
            try {
              await api('POST', '/api/firewall/open', {});
              toast(tr('Firewall fürs Heimnetz geöffnet ✓'), 'ok');
              refreshPairing();
            } catch (err) {
              toast(err.message, 'error');
              e.target.disabled = false;
              e.target.textContent = tr('Jetzt freigeben');
            }
          } }, tr('Jetzt freigeben')),
          h('code', { style: 'margin-top:8px' }, r.firewall))].filter(Boolean)
      : [h('p', {}, tr('Der Zugriff aus dem WLAN ist ausgeschaltet.'))]));
  } catch (err) {
    toast(err.message, 'error');
  }
}

function bind() {
  $('#undo').onclick = undo;
  $('#redo').onclick = redo;
  $('#open-preview').onclick = () => window.open('/deck/', 'touchdeck-vorschau', 'width=480,height=820');
  $('#open-pair').onclick = openPairing;
  $('#pair-lan').onchange = async (e) => {
    await updateSettings({ lanEnabled: e.target.checked });
    refreshPairing();
  };
  $('#pair-settings').onclick = () => {
    $('#dlg-pair').close();
    showView('settings');
  };

  document.addEventListener('keydown', (e) => {
    const typing = e.target.closest('input, textarea, select') || document.querySelector('dialog[open], .ctx-menu');
    if (typing) return;
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    if (ctrl && key === 'z') {
      e.preventDefault();
      e.shiftKey ? redo() : undo();
    } else if (ctrl && key === 'y') {
      e.preventDefault();
      redo();
    } else if (S.view === 'pages' && ctrl && key === 'd') {
      e.preventDefault();
      duplicateButton();
    } else if (S.view === 'pages' && ctrl && key === 'c' && sel() && !getSelection().toString()) {
      e.preventDefault();
      copyButton();
    } else if (S.view === 'pages' && ctrl && key === 'x' && sel()) {
      e.preventDefault();
      cutButton();
    } else if (S.view === 'pages' && ctrl && key === 'v') {
      e.preventDefault();
      pasteButton();
    } else if (S.view === 'pages' && (e.key === 'Delete' || e.key === 'Backspace') && sel()) {
      e.preventDefault();
      deleteButton();
    } else if (S.view === 'pages' && e.key === 'Escape' && sel()) {
      select(null);
    } else if (ctrl && /^[1-7]$/.test(key)) {
      e.preventDefault();
      showView(Object.keys(VIEWS)[Number(key) - 1]);
    }
  });
  matchMedia('(prefers-color-scheme: light)').addEventListener('change', applyTheme);
  // Ein Handy möchte sich verbinden: hier bestätigen.
  onPairRequest(async (r) => {
    const answer = await ask(tr('📱 Neues Gerät'), tr('„{0}“ ({1}) möchte sich mit TouchDeck verbinden. Erlauben?', [r.device, r.ip]),
      [{ value: 'deny', label: tr('Ablehnen') }, { value: 'allow', label: tr('Erlauben'), kind: 'primary' }]);
    await api('POST', '/api/pair/answer', { id: r.id, allow: answer === 'allow' }).catch((err) => toast(err.message, 'error'));
    if (answer === 'allow') toast(tr('{0} ist jetzt verbunden ✓', [r.device]), 'ok');
  });
}

async function init() {
  buildRail();
  bind();
  try {
    await loadState();
    S.icons = await api('GET', '/api/icons');
    await loadIconPacks();
    applyTheme();
    showView('pages');
    watchServer();
    // Das Desktop-Programm öffnet über #pair den Kopplungsdialog (Menü in der Systemleiste).
    const onHash = () => {
      if (location.hash !== '#pair') return;
      history.replaceState(null, '', location.pathname);
      if (!$('#dlg-pair').open) openPairing();
    };
    window.addEventListener('hashchange', onHash);
    onHash();
  } catch (err) {
    $('#save-state').textContent = tr('Verbindung zum Server fehlgeschlagen: {0}', [err.message]);
    $('#save-state').classList.add('error');
  }
}

init();
