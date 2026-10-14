// Bereich „Einstellungen“: Allgemein, Handy & Sicherheit, Bedienfeld, Aussehen, Integrationen, Backups, Protokoll, Information.
import { h, api, toast, ask } from '../dom.js';
import { S, render, loadState } from '../state.js';
import { openPairing } from '../main.js';

const SECTIONS = [
  ['general', '⚙️', 'Allgemein'],
  ['security', '🔐', 'Handy & Sicherheit'],
  ['deck', '📱', 'Bedienfeld'],
  ['look', '🎨', 'Aussehen'],
  ['integrations', '🔌', 'Integrationen'],
  ['backups', '💾', 'Backups'],
  ['logs', '📜', 'Protokoll'],
  ['info', 'ℹ️', 'Information'],
];
let section = 'general';
let info = null;
let backups = null;

export async function updateSettings(patch) {
  try {
    const r = await api('PUT', '/api/settings', patch);
    S.settings = r.settings;
    applyTheme();
    render();
    return true;
  } catch (err) {
    toast(err.message, 'error');
    return false;
  }
}

/** Thema und Akzentfarbe auf den Editor anwenden. */
export function applyTheme() {
  const t = S.settings.theme ?? { mode: 'dark', accent: '#7c8cff' };
  const mode = t.mode === 'system' ? (matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark') : t.mode;
  document.documentElement.dataset.theme = mode;
  document.documentElement.style.setProperty('--accent', t.accent);
}

export function mountSettings(root) {
  root.className = 'view view-master';
  root.replaceChildren(
    h('aside', { class: 'sidebar' },
      h('div', { class: 'sidebar-head' }, h('h2', {}, 'Einstellungen')),
      h('ul', { id: 'settings-nav', class: 'page-list' })),
    h('section', { id: 'settings-detail', class: 'detail settings' }),
  );
}

export function renderSettings({ detail = true } = {}) {
  document.getElementById('settings-nav').replaceChildren(...SECTIONS.map(([id, icon, label]) => h('li', { class: id === section ? 'active' : '', onclick: () => { section = id; render(); } },
    h('span', {}, icon), h('span', { class: 'name' }, label))));
  if (!detail) return;
  const build = { general, security, deck, look, integrations, backups: backupsSection, logs: logsSection, info: infoSection }[section];
  document.getElementById('settings-detail').replaceChildren(...build().filter(Boolean));
}

const title = (text, sub) => [h('h1', {}, text), sub && h('p', { class: 'hint-text' }, sub)];
const check = (label, checked, onchange) => h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked, onchange: (e) => onchange(e.target.checked) }), label);
const card = (...children) => h('div', { class: 'settings-card' }, ...children);

function general() {
  const st = S.settings;
  return [
    ...title('Allgemein'),
    card(
      h('label', { class: 'field' }, 'Startseite auf dem Handy',
        h('select', { value: st.startPage, onchange: (e) => updateSettings({ startPage: e.target.value }) }, S.pages.map((p) => h('option', { value: p.id }, p.name)))),
      h('p', { class: 'hint-text' }, 'Beim Desktop-Programm stellst du „Beim Anmelden starten“ über das Symbol in der Systemleiste ein.')),
    card(
      h('h3', {}, 'Optische Änderungen zurücksetzen'),
      h('p', { class: 'hint-text' }, 'Aktionen aus „Visuelles“ verändern Buttons zur Laufzeit. Hier stellst du überall das ursprüngliche Aussehen wieder her.'),
      h('button', { class: 'btn', onclick: async () => { await api('POST', '/api/overrides/reset', {}); toast('Alle Buttons sehen wieder aus wie entworfen ✓', 'ok'); } }, 'Alles zurücksetzen')),
  ];
}

function security() {
  const st = S.settings;
  return [
    ...title('Handy & Sicherheit', 'Der Editor ist nur an diesem PC erreichbar. Handys brauchen den Schlüssel aus dem QR-Code – und, wenn gesetzt, die PIN.'),
    card(
      check('Zugriff aus dem WLAN erlauben', st.lanEnabled, (v) => updateSettings({ lanEnabled: v })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn primary', onclick: openPairing }, '📱 QR-Code anzeigen'),
        h('button', { class: 'btn ghost danger', onclick: async () => {
          if (await ask('Neuen Schlüssel erzeugen?', 'Alle gekoppelten Handys werden getrennt und müssen den QR-Code neu scannen.', [{ value: 'yes', label: 'Neu erzeugen', kind: 'primary' }])) {
            await updateSettings({ regenerateToken: true });
            toast('Neuer Schlüssel erzeugt ✓', 'ok');
          }
        } }, 'Neuen Schlüssel erzeugen'))),
    card(
      h('h3', {}, 'PIN'),
      h('p', { class: 'hint-text' }, 'Zusätzlicher Schutz: Wer den QR-Code sieht, braucht trotzdem die PIN. Das Handy fragt einmal danach und merkt sie sich.'),
      h('div', { class: 'hotkey' },
        h('input', { id: 'pin-input', type: 'password', inputmode: 'numeric', maxlength: 8, placeholder: st.pin ? '•••• (gesetzt)' : '4–8 Ziffern', autocomplete: 'off' }),
        h('button', { class: 'btn', onclick: async () => {
          const pin = document.getElementById('pin-input').value.trim();
          if (!/^\d{4,8}$/.test(pin)) return toast('Die PIN muss aus 4 bis 8 Ziffern bestehen.', 'error');
          if (await updateSettings({ pin })) toast('PIN gesetzt ✓ – gekoppelte Handys fragen jetzt danach.', 'ok');
        } }, 'PIN setzen'),
        st.pin && h('button', { class: 'btn ghost danger', onclick: async () => (await updateSettings({ pin: '' })) && toast('PIN entfernt', 'ok') }, 'Entfernen'))),
  ];
}

function deck() {
  const d = S.settings.deck;
  const set = (k, v) => updateSettings({ deck: { ...d, [k]: v } });
  return [
    ...title('Bedienfeld', 'So verhält sich das Bedienfeld auf dem Handy.'),
    card(
      h('label', { class: 'field' }, 'Druck-Animation',
        h('select', { value: d.pressAnimation, onchange: (e) => set('pressAnimation', e.target.value) },
          [['shrink', 'Schrumpfen'], ['squeeze', 'Quetschen'], ['wobble', 'Wackeln'], ['glow', 'Leuchten'], ['none', 'Keine']].map(([v, l]) => h('option', { value: v }, l)))),
      check('Vibrieren beim Drücken', d.vibrate, (v) => set('vibrate', v)),
      check('Klick-Ton beim Drücken', d.sound, (v) => set('sound', v)),
      check('Beschriftungen anzeigen', d.showLabels, (v) => set('showLabels', v))),
  ];
}

function look() {
  const t = S.settings.theme;
  const accents = ['#7c8cff', '#3ccf91', '#ff8a4c', '#ff5c8a', '#46b4ff', '#b26cff', '#f5c542'];
  return [
    ...title('Aussehen', 'Thema des Editors.'),
    card(
      h('div', { class: 'field' }, 'Thema',
        h('div', { class: 'segmented' }, [['dark', '🌙 Dunkel'], ['light', '☀️ Hell'], ['system', '🖥️ Wie das System']].map(([v, l]) =>
          h('button', { type: 'button', class: t.mode === v ? 'active' : '', onclick: () => updateSettings({ theme: { ...t, mode: v } }) }, l)))),
      h('div', { class: 'field' }, 'Akzentfarbe',
        h('div', { class: 'swatches big' },
          accents.map((c) => h('button', { type: 'button', class: c === t.accent ? 'active' : '', style: `background:${c}`, title: c, onclick: () => updateSettings({ theme: { ...t, accent: c } }) })),
          h('input', { type: 'color', value: t.accent, onchange: (e) => updateSettings({ theme: { ...t, accent: e.target.value } }) })))),
  ];
}

function integrations() {
  const o = S.settings.obs;
  const fields = () => ({ host: document.getElementById('obs-host').value.trim() || '127.0.0.1', port: Number(document.getElementById('obs-port').value) || 4455, password: document.getElementById('obs-password').value });
  const forget = () => {
    for (const k of Object.keys(S.options)) if (k.startsWith('obs-')) delete S.options[k];
  };
  return [
    ...title('Integrationen'),
    card(
      h('h3', {}, '🎬 OBS Studio'),
      h('p', { class: 'hint-text' }, 'In OBS unter Werkzeuge → WebSocket-Servereinstellungen den Server aktivieren. „Aus OBS übernehmen“ liest Port und Passwort direkt aus OBS (auch Flatpak).'),
      h('div', { class: 'row' },
        h('label', { class: 'field' }, 'Adresse', h('input', { id: 'obs-host', value: o.host, onchange: () => { forget(); updateSettings({ obs: fields() }); } })),
        h('label', { class: 'field', style: 'max-width:110px' }, 'Port', h('input', { id: 'obs-port', type: 'number', value: o.port, onchange: () => { forget(); updateSettings({ obs: fields() }); } }))),
      h('label', { class: 'field' }, 'Passwort', h('input', { id: 'obs-password', type: 'password', value: o.password, autocomplete: 'off', onchange: () => { forget(); updateSettings({ obs: fields() }); } })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn', onclick: async () => {
          try {
            const r = await api('POST', '/api/obs/autoconfig', {});
            S.settings = r.settings;
            forget();
            render();
            toast(r.serverEnabled ? 'Port und Passwort aus OBS übernommen ✓' : 'Übernommen ✓ – aber in OBS ist der WebSocket-Server noch aus.', r.serverEnabled ? 'ok' : 'error');
          } catch (err) {
            toast(err.message, 'error');
          }
        } }, 'Aus OBS übernehmen'),
        h('button', { class: 'btn', onclick: async () => {
          try {
            const r = await api('POST', '/api/obs/test', fields());
            toast(`Verbunden mit OBS ${r.version} ✓ (${r.scenes} Szenen)`, 'ok');
          } catch (err) {
            toast(err.message, 'error');
          }
        } }, 'Verbindung testen'))),
    twitchCard(),
  ];
}

let twitchStatus = null;
let twitchPoll = null;

async function refreshTwitch() {
  try {
    twitchStatus = await api('GET', '/api/twitch/status');
  } catch (err) {
    twitchStatus = { state: 'error', error: err.message };
  }
  if (section === 'integrations') render();
  clearTimeout(twitchPoll);
  // Während auf die Bestätigung gewartet wird, alle 2 s nachsehen.
  if (twitchStatus.state === 'waiting') twitchPoll = setTimeout(refreshTwitch, 2000);
}

function twitchCard() {
  const tw = S.settings.twitch;
  if (!twitchStatus) refreshTwitch();
  const st = twitchStatus ?? { state: 'idle' };
  const connect = async () => {
    const clientId = document.getElementById('twitch-client').value.trim();
    if (!/^[a-z0-9]{20,40}$/i.test(clientId)) return toast('Das sieht nicht nach einer Client-ID aus (ca. 30 Buchstaben und Ziffern).', 'error');
    try {
      twitchStatus = await api('POST', '/api/twitch/connect', { clientId });
      S.settings.twitch.clientId = clientId;
      render();
      refreshTwitch();
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  let body;
  if (st.state === 'connected') {
    body = [
      h('p', {}, '✅ Verbunden als ', h('b', {}, st.displayName ?? st.login), st.events ? ' · Ereignisse aktiv' : ''),
      h('p', { class: 'hint-text' }, 'Twitch-Aktionen findest du unter „Streaming › Twitch“, Auslöser bei den Events; Live-Anzeigen „Twitch: live?“, „Zuschauer“, „Follower“.'),
      h('button', { class: 'btn ghost danger', onclick: async () => { twitchStatus = await api('POST', '/api/twitch/disconnect', {}); toast('Twitch getrennt', 'ok'); render(); } }, 'Verbindung trennen'),
    ];
  } else if (st.state === 'waiting') {
    body = [
      h('p', {}, 'Öffne ', h('a', { href: st.verificationUri, target: '_blank', rel: 'noopener' }, 'twitch.tv/activate'), ' und gib diesen Code ein:'),
      h('div', { class: 'twitch-code' }, st.userCode),
      h('p', { class: 'hint-text' }, 'Warte auf Bestätigung … (der Code gilt etwa 30 Minuten)'),
      h('button', { class: 'btn ghost', onclick: async () => { twitchStatus = await api('POST', '/api/twitch/cancel', {}); render(); } }, 'Abbrechen'),
    ];
  } else {
    body = [
      h('details', { class: 'howto', open: !tw.clientId },
        h('summary', {}, 'So bekommst du eine Client-ID (einmalig, ca. 5 Minuten)'),
        h('ol', {},
          h('li', {}, 'Auf ', h('a', { href: 'https://dev.twitch.tv/console/apps/create', target: '_blank', rel: 'noopener' }, 'dev.twitch.tv/console'), ' mit deinem Twitch-Konto anmelden (Twitch verlangt dafür die Zwei-Faktor-Anmeldung).'),
          h('li', {}, '„Deine Anwendung registrieren“: Name frei wählbar (z. B. „TouchDeck von dir“), OAuth-Redirect-URL ', h('code', {}, 'http://localhost'), ', Kategorie „Application Integration“, Client-Typ ', h('b', {}, '„Öffentlich“'), '.'),
          h('li', {}, 'Erstellen → „Verwalten“ → die Client-ID kopieren und hier einfügen. Ein Client-Geheimnis brauchst du nicht.'))),
      h('label', { class: 'field' }, 'Client-ID deiner Twitch-Anwendung',
        h('input', { id: 'twitch-client', value: tw.clientId, placeholder: 'z. B. gp762nuuoqcoxypju8c569th9wz7q5', autocomplete: 'off', spellcheck: 'false' })),
      st.error && h('p', { class: 'hint-text error' }, st.error),
      h('button', { class: 'btn primary', onclick: connect }, '💜 Mit Twitch verbinden'),
    ];
  }
  return card(
    h('h3', {}, '💜 Twitch'),
    h('p', { class: 'hint-text' }, 'Jeder verbindet sein eigenes Twitch-Konto über eine eigene Twitch-Anwendung. Die Anmeldung bleibt nur auf diesem PC gespeichert.'),
    ...body);
}

function backupsSection() {
  const b = S.settings.backups;
  if (!backups) {
    api('GET', '/api/backups').then((list) => {
      backups = list;
      render();
    }).catch((err) => toast(err.message, 'error'));
  }
  const restore = async (name) => {
    if (!(await ask('Sicherung wiederherstellen?', `Der aktuelle Stand wird vorher automatisch gesichert. Danach gilt „${name}“.`, [{ value: 'yes', label: 'Wiederherstellen', kind: 'primary' }]))) return;
    try {
      const r = await api('POST', '/api/backups/restore', { name });
      backups = r.list;
      await loadState();
      toast('Sicherung wiederhergestellt ✓', 'ok');
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  const upload = h('input', { type: 'file', accept: 'application/json', hidden: true, onchange: async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    try {
      const config = JSON.parse(await file.text());
      if (!(await ask('Datei wiederherstellen?', `„${file.name}“ ersetzt den aktuellen Stand (der vorher automatisch gesichert wird).`, [{ value: 'yes', label: 'Wiederherstellen', kind: 'primary' }]))) return;
      const r = await api('POST', '/api/backups/restore', { config });
      backups = r.list;
      await loadState();
      toast('Sicherung wiederhergestellt ✓', 'ok');
    } catch (err) {
      toast(err.message.includes('JSON') ? 'Die Datei ist keine gültige Sicherung.' : err.message, 'error');
    }
  } });
  return [
    ...title('Backups', 'Sicherungen von Seiten, Werten, Vorlagen, Events und Einstellungen.'),
    card(
      check('Automatisch einmal am Tag sichern', b.auto, (v) => updateSettings({ backups: { ...b, auto: v } })),
      h('label', { class: 'field', style: 'max-width:260px' }, 'Automatische Sicherungen behalten',
        h('input', { type: 'number', min: 1, max: 100, value: b.keep, onchange: (e) => updateSettings({ backups: { ...b, keep: Number(e.target.value) } }) })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn primary', onclick: async () => {
          const r = await api('POST', '/api/backups', { note: 'manuell' });
          backups = r.list;
          render();
          toast('Sicherung angelegt ✓', 'ok');
        } }, 'Jetzt sichern'),
        h('label', { class: 'btn' }, 'Aus Datei wiederherstellen …', upload))),
    card(
      h('h3', {}, 'Vorhandene Sicherungen'),
      !backups ? h('p', { class: 'muted-note' }, 'Wird geladen …')
        : !backups.length ? h('p', { class: 'muted-note' }, 'Noch keine Sicherungen.')
          : h('table', { class: 'values-table' }, h('tbody', {}, backups.map((bk) => h('tr', {},
            h('td', {}, new Date(bk.time).toLocaleString('de-DE')),
            h('td', {}, bk.note || '–'),
            h('td', {}, `${Math.round(bk.size / 1024)} KB`),
            h('td', { class: 'right' },
              h('button', { class: 'btn small', onclick: () => restore(bk.name) }, 'Wiederherstellen'),
              h('a', { class: 'btn small ghost', href: `/backups/download?name=${encodeURIComponent(bk.name)}`, download: bk.name }, 'Herunterladen'),
              h('button', { class: 'btn icon ghost danger', title: 'Löschen', onclick: async () => {
                const r = await api('POST', '/api/backups/delete', { name: bk.name });
                backups = r.list;
                render();
              } }, '✕'))))))),
  ];
}

function logsSection() {
  const l = S.settings.logs;
  return [
    ...title('Protokoll', 'Wie ausführlich TouchDeck mitschreibt. Die Einträge siehst du im Bereich „Logs“; die Datei liegt unter logs/touchdeck.log im Datenordner.'),
    card(h('label', { class: 'field' }, 'Ausführlichkeit',
      h('select', { value: l.level, onchange: (e) => updateSettings({ logs: { level: e.target.value } }) },
        [['debug', 'Alles (für Fehlersuche)'], ['info', 'Normal'], ['warn', 'Nur Warnungen und Fehler'], ['error', 'Nur Fehler']].map(([v, t]) => h('option', { value: v }, t))))),
  ];
}

function infoSection() {
  if (!info) {
    api('GET', '/api/info').then((i) => {
      info = i;
      render();
      setTimeout(() => (info = null), 5000);
    }).catch((err) => toast(err.message, 'error'));
    return [...title('Information'), h('p', { class: 'muted-note' }, 'Wird geladen …')];
  }
  const up = `${Math.floor(info.uptime / 3600)} h ${Math.floor((info.uptime % 3600) / 60)} min`;
  const rows = [
    ['Version', `TouchDeck ${info.version}`],
    ['Laufzeit', info.electron ? `Electron ${info.electron} · Node ${info.node}` : `Node ${info.node}`],
    ['System', `${info.platform} · ${info.host}`],
    ['Läuft seit', up],
    ['Adresse', info.lan.map((ip) => `http://${ip}:${info.port}`).join(', ') || 'kein Netzwerk'],
    ['Verbundene Handys', String(info.devices)],
    ['OBS', info.obsConnected ? 'verbunden' : 'nicht verbunden'],
    ['Inhalt', `${info.counts.pages} Seiten · ${info.counts.buttons} Buttons · ${info.counts.flows} Vorlagen · ${info.counts.events} Events · ${info.counts.values} Werte`],
    ['Datenordner', info.dataDir],
  ];
  return [
    ...title('Information'),
    card(h('table', { class: 'info-table' }, h('tbody', {}, rows.map(([k, v]) => h('tr', {}, h('th', {}, k), h('td', {}, v)))))),
  ];
}

export function resetSettingsCache() {
  info = null;
  backups = null;
  twitchStatus = null;
}
