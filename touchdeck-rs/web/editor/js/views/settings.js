// Bereich „Einstellungen“: Allgemein, Handy & Sicherheit, Bedienfeld, Aussehen, Integrationen, Icon-Pakete, Backups, Protokoll, Information.
import { t as tr } from '/shared/i18n.js';
import { h, api, toast, ask, segmented } from '../dom.js';
import { S, render, loadState } from '../state.js';
import { openPairing } from '../main.js';
import { iconPacksSection } from './iconpacks.js';

const SECTIONS = [
  ['general', '⚙️', tr('Allgemein')],
  ['security', '🔐', tr('Handy & Sicherheit')],
  ['deck', '📱', tr('Bedienfeld')],
  ['look', '🎨', tr('Aussehen')],
  ['integrations', '🔌', tr('Integrationen')],
  ['iconpacks', '🖼️', tr('Icon-Pakete')],
  ['backups', '💾', tr('Backups')],
  ['logs', '📜', tr('Protokoll')],
  ['info', 'ℹ️', tr('Information')],
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
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('Einstellungen'))),
      h('ul', { id: 'settings-nav', class: 'page-list' })),
    h('section', { id: 'settings-detail', class: 'detail settings' }),
  );
}

export function renderSettings({ detail = true } = {}) {
  document.getElementById('settings-nav').replaceChildren(...SECTIONS.map(([id, icon, label]) => h('li', { class: id === section ? 'active' : '', onclick: () => { section = id; render(); } },
    h('span', {}, icon), h('span', { class: 'name' }, label))));
  if (!detail) return;
  const build = { general, security, deck, look, integrations, iconpacks: iconPacksSection, backups: backupsSection, logs: logsSection, info: infoSection }[section];
  document.getElementById('settings-detail').replaceChildren(...build().filter(Boolean));
}

const title = (text, sub) => [h('h1', {}, text), sub && h('p', { class: 'hint-text' }, sub)];
const check = (label, checked, onchange) => h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked, onchange: (e) => onchange(e.target.checked) }), label);
const card = (...children) => h('div', { class: 'settings-card' }, ...children);

function general() {
  const st = S.settings;
  return [
    ...title(tr('Allgemein')),
    card(
      h('div', { class: 'field' }, tr('Sprache / Language'),
        segmented([['de', 'Deutsch'], ['en', 'English']], st.language ?? 'de', async (v) => {
          if (v !== st.language && await updateSettings({ language: v })) location.reload();
        })),
      h('p', { class: 'hint-text' }, tr('Gilt für Editor, Bedienfeld und die Android-App. Voreingestellt ist die Sprache des Systems.'))),
    card(
      h('label', { class: 'field' }, tr('Startseite auf dem Handy'),
        h('select', { value: st.startPage, onchange: (e) => updateSettings({ startPage: e.target.value }) }, S.pages.map((p) => h('option', { value: p.id }, p.name)))),
      h('p', { class: 'hint-text' }, tr('Beim Desktop-Programm stellst du „Beim Anmelden starten“ über das Symbol in der Systemleiste ein.'))),
    card(
      h('h3', {}, tr('Optische Änderungen zurücksetzen')),
      h('p', { class: 'hint-text' }, tr('Aktionen aus „Visuelles“ verändern Buttons zur Laufzeit. Hier stellst du überall das ursprüngliche Aussehen wieder her.')),
      h('button', { class: 'btn', onclick: async () => { await api('POST', '/api/overrides/reset', {}); toast(tr('Alle Buttons sehen wieder aus wie entworfen ✓'), 'ok'); } }, tr('Alles zurücksetzen'))),
  ];
}

function security() {
  const st = S.settings;
  return [
    ...title(tr('Handy & Sicherheit'), tr('Der Editor ist nur an diesem PC erreichbar. Handys brauchen den Schlüssel aus dem QR-Code – und, wenn gesetzt, die PIN.')),
    card(
      check(tr('Zugriff aus dem WLAN erlauben'), st.lanEnabled, (v) => updateSettings({ lanEnabled: v })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn primary', onclick: openPairing }, tr('📱 QR-Code anzeigen')),
        h('button', { class: 'btn ghost danger', onclick: async () => {
          if (await ask(tr('Neuen Schlüssel erzeugen?'), tr('Alle gekoppelten Handys werden getrennt und müssen den QR-Code neu scannen.'), [{ value: 'yes', label: tr('Neu erzeugen'), kind: 'primary' }])) {
            await updateSettings({ regenerateToken: true });
            toast(tr('Neuer Schlüssel erzeugt ✓'), 'ok');
          }
        } }, tr('Neuen Schlüssel erzeugen')))),
    card(
      h('h3', {}, 'PIN'),
      h('p', { class: 'hint-text' }, tr('Zusätzlicher Schutz: Wer den QR-Code sieht, braucht trotzdem die PIN. Das Handy fragt einmal danach und merkt sie sich.')),
      h('div', { class: 'hotkey' },
        h('input', { id: 'pin-input', type: 'password', inputmode: 'numeric', maxlength: 8, placeholder: st.pin ? tr('•••• (gesetzt)') : tr('4–8 Ziffern'), autocomplete: 'off' }),
        h('button', { class: 'btn', onclick: async () => {
          const pin = document.getElementById('pin-input').value.trim();
          if (!/^\d{4,8}$/.test(pin)) return toast(tr('Die PIN muss aus 4 bis 8 Ziffern bestehen.'), 'error');
          if (await updateSettings({ pin })) toast(tr('PIN gesetzt ✓ – gekoppelte Handys fragen jetzt danach.'), 'ok');
        } }, tr('PIN setzen')),
        st.pin && h('button', { class: 'btn ghost danger', onclick: async () => (await updateSettings({ pin: '' })) && toast(tr('PIN entfernt'), 'ok') }, tr('Entfernen')))),
  ];
}

function deck() {
  const d = S.settings.deck;
  const set = (k, v) => updateSettings({ deck: { ...d, [k]: v } });
  return [
    ...title(tr('Bedienfeld'), tr('So verhält sich das Bedienfeld auf dem Handy.')),
    card(
      h('label', { class: 'field' }, tr('Druck-Animation'),
        h('select', { value: d.pressAnimation, onchange: (e) => set('pressAnimation', e.target.value) },
          [['shrink', tr('Schrumpfen')], ['squeeze', tr('Quetschen')], ['wobble', tr('Wackeln')], ['glow', tr('Leuchten')], ['none', tr('Keine')]].map(([v, l]) => h('option', { value: v }, l)))),
      check(tr('Vibrieren beim Drücken'), d.vibrate, (v) => set('vibrate', v)),
      check(tr('Klick-Ton beim Drücken'), d.sound, (v) => set('sound', v)),
      check(tr('Beschriftungen anzeigen'), d.showLabels, (v) => set('showLabels', v)),
      h('div', { class: 'field' }, tr('Ausrichtung auf Handy und Tablet'),
        segmented([['auto', tr('Automatisch')], ['portrait', tr('Hochformat')], ['landscape', tr('Querformat')]], d.orientation ?? 'auto', (v) => set('orientation', v))),
      h('p', { class: 'hint-text' }, tr('Die Android-App dreht sich sofort in die gewählte Richtung. „Automatisch“ folgt der Drehsperre des Geräts. Im Browser gilt die Einstellung nur im Vollbild.'))),
  ];
}

function look() {
  const t = S.settings.theme;
  const accents = ['#7c8cff', '#3ccf91', '#ff8a4c', '#ff5c8a', '#46b4ff', '#b26cff', '#f5c542'];
  return [
    ...title(tr('Aussehen'), tr('Thema des Editors.')),
    card(
      h('div', { class: 'field' }, tr('Thema'),
        h('div', { class: 'segmented' }, [['dark', tr('🌙 Dunkel')], ['light', tr('☀️ Hell')], ['system', tr('🖥️ Wie das System')]].map(([v, l]) =>
          h('button', { type: 'button', class: t.mode === v ? 'active' : '', onclick: () => updateSettings({ theme: { ...t, mode: v } }) }, l)))),
      h('div', { class: 'field' }, tr('Akzentfarbe'),
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
    ...title(tr('Integrationen')),
    card(
      h('h3', {}, tr('🎬 OBS Studio')),
      h('p', { class: 'hint-text' }, tr('In OBS unter Werkzeuge → WebSocket-Servereinstellungen den Server aktivieren. „Aus OBS übernehmen“ liest Port und Passwort direkt aus OBS (auch Flatpak).')),
      h('div', { class: 'row' },
        h('label', { class: 'field' }, tr('Adresse'), h('input', { id: 'obs-host', value: o.host, onchange: () => { forget(); updateSettings({ obs: fields() }); } })),
        h('label', { class: 'field', style: 'max-width:110px' }, tr('Port'), h('input', { id: 'obs-port', type: 'number', value: o.port, onchange: () => { forget(); updateSettings({ obs: fields() }); } }))),
      h('label', { class: 'field' }, tr('Passwort'), h('input', { id: 'obs-password', type: 'password', value: o.password, autocomplete: 'off', onchange: () => { forget(); updateSettings({ obs: fields() }); } })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn', onclick: async () => {
          try {
            const r = await api('POST', '/api/obs/autoconfig', {});
            S.settings = r.settings;
            forget();
            render();
            toast(r.serverEnabled ? tr('Port und Passwort aus OBS übernommen ✓') : tr('Übernommen ✓ – aber in OBS ist der WebSocket-Server noch aus.'), r.serverEnabled ? 'ok' : 'error');
          } catch (err) {
            toast(err.message, 'error');
          }
        } }, tr('Aus OBS übernehmen')),
        h('button', { class: 'btn', onclick: async () => {
          try {
            const r = await api('POST', '/api/obs/test', fields());
            toast(tr('Verbunden mit OBS {0} ✓ ({1} Szenen)', [r.version, r.scenes]), 'ok');
          } catch (err) {
            toast(err.message, 'error');
          }
        } }, tr('Verbindung testen')))),
    twitchCard(),
    discordCard(),
  ];
}

let discordStatus = null;

function discordCard() {
  const dc = S.settings.discord ?? {};
  if (!discordStatus) {
    api('GET', '/api/discord/status').then((st) => {
      discordStatus = st;
      if (section === 'integrations') render();
    }).catch(() => {});
  }
  const st = discordStatus ?? { state: 'idle' };
  const connect = async (e) => {
    const clientId = document.getElementById('discord-client').value.trim();
    const clientSecret = document.getElementById('discord-secret').value.trim();
    if (!/^\d{15,25}$/.test(clientId)) return toast(tr('Die Client-ID besteht nur aus Ziffern (ca. 19 Stellen).'), 'error');
    if (!clientSecret && !dc.hasSecret) return toast(tr('Bitte auch das Client-Secret eintragen.'), 'error');
    e.target.disabled = true;
    e.target.textContent = tr('Bitte in Discord „Autorisieren“ klicken …');
    try {
      discordStatus = await api('POST', '/api/discord/connect', { clientId, clientSecret });
      toast(tr('Mit Discord verbunden ✓'), 'ok');
    } catch (err) {
      toast(err.message, 'error');
      discordStatus = null;
    }
    render();
  };
  let body;
  if (st.state === 'connected') {
    body = [
      h('p', {}, tr('✅ Verbunden als '), h('b', {}, st.user || tr('Discord-Nutzer')), st.running ? tr(' · Discord läuft') : tr(' · Discord ist gerade nicht geöffnet')),
      h('p', { class: 'hint-text' }, tr('Aktionen unter „Kommunikation › Discord“, Auslöser bei den Events, Live-Anzeigen „Discord: stumm?“, „taub?“, „Sprachkanal“. TouchDeck verbindet sich automatisch, sobald Discord läuft.')),
      h('button', { class: 'btn ghost danger', onclick: async () => { discordStatus = await api('POST', '/api/discord/disconnect', {}); toast(tr('Discord getrennt'), 'ok'); render(); } }, tr('Verbindung trennen')),
    ];
  } else {
    body = [
      h('details', { class: 'howto', open: !dc.clientId },
        h('summary', {}, tr('So richtest du es ein (einmalig, ca. 3 Minuten)')),
        h('ol', {},
          h('li', {}, tr('Auf '), h('a', { href: 'https://discord.com/developers/applications', target: '_blank', rel: 'noopener' }, 'discord.com/developers/applications'), tr(' mit deinem Discord-Konto anmelden und „New Application“ anlegen (Name z. B. „TouchDeck“).')),
          h('li', {}, tr('Links „OAuth2“ öffnen, unter „Redirects“ '), h('code', {}, 'http://localhost'), tr(' eintragen und speichern.')),
          h('li', {}, tr('Auf derselben Seite „Client ID“ kopieren und mit „Reset Secret“ ein „Client Secret“ erzeugen und kopieren.')),
          h('li', {}, tr('Beides hier eintragen, Discord am PC geöffnet lassen und „Mit Discord verbinden“ klicken. In Discord erscheint eine Abfrage – „Autorisieren“.')))),
      h('div', { class: 'row' },
        h('label', { class: 'field' }, tr('Client-ID'), h('input', { id: 'discord-client', value: dc.clientId ?? '', placeholder: tr('z. B. 1234567890123456789'), autocomplete: 'off', spellcheck: 'false' })),
        h('label', { class: 'field' }, tr('Client-Secret'), h('input', { id: 'discord-secret', type: 'password', placeholder: dc.hasSecret ? tr('•••••• (gespeichert)') : '', autocomplete: 'off' }))),
      st.error && h('p', { class: 'hint-text error' }, st.error),
      h('button', { class: 'btn primary', onclick: connect }, tr('💬 Mit Discord verbinden')),
    ];
  }
  return card(
    h('h3', {}, tr('💬 Discord')),
    h('p', { class: 'hint-text' }, tr('Sprachsteuerung des Discord-Programms an diesem PC (stumm, taub, Sprachkanal, Lautstärke). Jeder nutzt dafür eine eigene Discord-Anwendung. Nachrichten per Webhook gehen auch ohne Verbindung.')),
    ...body);
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
    if (!/^[a-z0-9]{20,40}$/i.test(clientId)) return toast(tr('Das sieht nicht nach einer Client-ID aus (ca. 30 Buchstaben und Ziffern).'), 'error');
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
      h('p', {}, tr('✅ Verbunden als '), h('b', {}, st.displayName ?? st.login), st.events ? tr(' · Ereignisse aktiv') : ''),
      h('p', { class: 'hint-text' }, tr('Twitch-Aktionen findest du unter „Streaming › Twitch“, Auslöser bei den Events; Live-Anzeigen „Twitch: live?“, „Zuschauer“, „Follower“.')),
      h('button', { class: 'btn ghost danger', onclick: async () => { twitchStatus = await api('POST', '/api/twitch/disconnect', {}); toast(tr('Twitch getrennt'), 'ok'); render(); } }, tr('Verbindung trennen')),
    ];
  } else if (st.state === 'waiting') {
    body = [
      h('p', {}, tr('Öffne '), h('a', { href: st.verificationUri, target: '_blank', rel: 'noopener' }, 'twitch.tv/activate'), tr(' und gib diesen Code ein:')),
      h('div', { class: 'twitch-code' }, st.userCode),
      h('p', { class: 'hint-text' }, tr('Warte auf Bestätigung … (der Code gilt etwa 30 Minuten)')),
      h('button', { class: 'btn ghost', onclick: async () => { twitchStatus = await api('POST', '/api/twitch/cancel', {}); render(); } }, tr('Abbrechen')),
    ];
  } else {
    body = [
      h('details', { class: 'howto', open: !tw.clientId },
        h('summary', {}, tr('So bekommst du eine Client-ID (einmalig, ca. 5 Minuten)')),
        h('ol', {},
          h('li', {}, tr('Auf '), h('a', { href: 'https://dev.twitch.tv/console/apps/create', target: '_blank', rel: 'noopener' }, 'dev.twitch.tv/console'), tr(' mit deinem Twitch-Konto anmelden (Twitch verlangt dafür die Zwei-Faktor-Anmeldung).')),
          h('li', {}, tr('„Deine Anwendung registrieren“: Name frei wählbar (z. B. „TouchDeck von dir“), OAuth-Redirect-URL '), h('code', {}, 'http://localhost'), tr(', Kategorie „Application Integration“, Client-Typ '), h('b', {}, tr('„Öffentlich“')), '.'),
          h('li', {}, tr('Erstellen → „Verwalten“ → die Client-ID kopieren und hier einfügen. Ein Client-Geheimnis brauchst du nicht.')))),
      h('label', { class: 'field' }, tr('Client-ID deiner Twitch-Anwendung'),
        h('input', { id: 'twitch-client', value: tw.clientId, placeholder: tr('z. B. gp762nuuoqcoxypju8c569th9wz7q5'), autocomplete: 'off', spellcheck: 'false' })),
      st.error && h('p', { class: 'hint-text error' }, st.error),
      h('button', { class: 'btn primary', onclick: connect }, tr('💜 Mit Twitch verbinden')),
    ];
  }
  return card(
    h('h3', {}, tr('💜 Twitch')),
    h('p', { class: 'hint-text' }, tr('Jeder verbindet sein eigenes Twitch-Konto über eine eigene Twitch-Anwendung. Die Anmeldung bleibt nur auf diesem PC gespeichert.')),
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
    if (!(await ask(tr('Sicherung wiederherstellen?'), tr('Der aktuelle Stand wird vorher automatisch gesichert. Danach gilt „{0}“.', [name]), [{ value: 'yes', label: tr('Wiederherstellen'), kind: 'primary' }]))) return;
    try {
      const r = await api('POST', '/api/backups/restore', { name });
      backups = r.list;
      await loadState();
      toast(tr('Sicherung wiederhergestellt ✓'), 'ok');
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  const upload = h('input', { type: 'file', accept: 'application/json', hidden: true, onchange: async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    try {
      const config = JSON.parse(await file.text());
      if (!(await ask(tr('Datei wiederherstellen?'), tr('„{0}“ ersetzt den aktuellen Stand (der vorher automatisch gesichert wird).', [file.name]), [{ value: 'yes', label: tr('Wiederherstellen'), kind: 'primary' }]))) return;
      const r = await api('POST', '/api/backups/restore', { config });
      backups = r.list;
      await loadState();
      toast(tr('Sicherung wiederhergestellt ✓'), 'ok');
    } catch (err) {
      toast(err.message.includes('JSON') ? tr('Die Datei ist keine gültige Sicherung.') : err.message, 'error');
    }
  } });
  return [
    ...title(tr('Backups'), tr('Sicherungen von Seiten, Werten, Vorlagen, Events und Einstellungen.')),
    card(
      check(tr('Automatisch einmal am Tag sichern'), b.auto, (v) => updateSettings({ backups: { ...b, auto: v } })),
      h('label', { class: 'field', style: 'max-width:260px' }, tr('Automatische Sicherungen behalten'),
        h('input', { type: 'number', min: 1, max: 100, value: b.keep, onchange: (e) => updateSettings({ backups: { ...b, keep: Number(e.target.value) } }) })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn primary', onclick: async () => {
          const r = await api('POST', '/api/backups', { note: 'manuell' });
          backups = r.list;
          render();
          toast(tr('Sicherung angelegt ✓'), 'ok');
        } }, tr('Jetzt sichern')),
        h('label', { class: 'btn' }, tr('Aus Datei wiederherstellen …'), upload))),
    card(
      h('h3', {}, tr('Vorhandene Sicherungen')),
      !backups ? h('p', { class: 'muted-note' }, tr('Wird geladen …'))
        : !backups.length ? h('p', { class: 'muted-note' }, tr('Noch keine Sicherungen.'))
          : h('table', { class: 'values-table' }, h('tbody', {}, backups.map((bk) => h('tr', {},
            h('td', {}, new Date(bk.time).toLocaleString('de-DE')),
            h('td', {}, bk.note || '–'),
            h('td', {}, tr('{0} KB', [Math.round(bk.size / 1024)])),
            h('td', { class: 'right' },
              h('button', { class: 'btn small', onclick: () => restore(bk.name) }, tr('Wiederherstellen')),
              h('a', { class: 'btn small ghost', href: `/backups/download?name=${encodeURIComponent(bk.name)}`, download: bk.name }, tr('Herunterladen')),
              h('button', { class: 'btn icon ghost danger', title: tr('Löschen'), onclick: async () => {
                const r = await api('POST', '/api/backups/delete', { name: bk.name });
                backups = r.list;
                render();
              } }, '✕'))))))),
  ];
}

function logsSection() {
  const l = S.settings.logs;
  return [
    ...title(tr('Protokoll'), tr('Wie ausführlich TouchDeck mitschreibt. Die Einträge siehst du im Bereich „Logs“; die Datei liegt unter logs/touchdeck.log im Datenordner.')),
    card(h('label', { class: 'field' }, tr('Ausführlichkeit'),
      h('select', { value: l.level, onchange: (e) => updateSettings({ logs: { level: e.target.value } }) },
        [['debug', tr('Alles (für Fehlersuche)')], ['info', tr('Normal')], ['warn', tr('Nur Warnungen und Fehler')], ['error', tr('Nur Fehler')]].map(([v, t]) => h('option', { value: v }, t))))),
  ];
}

function infoSection() {
  if (!info) {
    api('GET', '/api/info').then((i) => {
      info = i;
      render();
      setTimeout(() => (info = null), 5000);
    }).catch((err) => toast(err.message, 'error'));
    return [...title(tr('Information')), h('p', { class: 'muted-note' }, tr('Wird geladen …'))];
  }
  const up = `${Math.floor(info.uptime / 3600)} h ${Math.floor((info.uptime % 3600) / 60)} min`;
  const rows = [
    [tr('Version'), tr('TouchDeck {0}', [info.version])],
    [tr('System'), tr('{0} · {1}', [info.platform, info.host])],
    [tr('Läuft seit'), up],
    [tr('Adresse'), info.lan.map((ip) => `http://${ip}:${info.port}`).join(', ') || tr('kein Netzwerk')],
    [tr('Verbundene Handys'), String(info.devices)],
    ['OBS', info.obsConnected ? tr('verbunden') : tr('nicht verbunden')],
    [tr('Inhalt'), tr('{0} Seiten · {1} Buttons · {2} Vorlagen · {3} Events · {4} Werte', [info.counts.pages, info.counts.buttons, info.counts.flows, info.counts.events, info.counts.values])],
    [tr('Datenordner'), info.dataDir],
  ];
  return [
    ...title(tr('Information')),
    card(h('table', { class: 'info-table' }, h('tbody', {}, rows.map(([k, v]) => h('tr', {}, h('th', {}, k), h('td', {}, v)))))),
  ];
}

export function resetSettingsCache() {
  info = null;
  backups = null;
  twitchStatus = null;
  discordStatus = null;
}
