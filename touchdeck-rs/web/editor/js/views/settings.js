// Bereich „Einstellungen“: Allgemein, Handy & Sicherheit, Bedienfeld, Aussehen, Integrationen, Backups, Protokoll, Information.
import { t as tr } from '/shared/i18n.js';
import { h, api, toast, ask, segmented } from '../dom.js';
import { S, render, loadState } from '../state.js';
import { openPairing } from '../main.js';

const SECTIONS = [
  ['general', '⚙️', tr('settings.general')],
  ['security', '🔐', tr('settings.phoneSecurity')],
  ['deck', '📱', tr('settings.deck')],
  ['look', '🎨', tr('common.look')],
  ['integrations', '🔌', tr('settings.integrations')],
  ['backups', '💾', tr('settings.backups')],
  ['logs', '📜', tr('settings.log')],
  ['info', 'ℹ️', tr('settings.information')],
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
      h('div', { class: 'sidebar-head' }, h('h2', {}, tr('common.settings'))),
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
    ...title(tr('settings.general')),
    card(
      h('div', { class: 'field' }, tr('settings.spracheLanguage'),
        segmented([['de', 'Deutsch'], ['en', 'English']], st.language ?? 'de', async (v) => {
          if (v !== st.language && await updateSettings({ language: v })) location.reload();
        })),
      h('p', { class: 'hint-text' }, tr('settings.appliesToEditorDeck'))),
    card(
      h('label', { class: 'field' }, tr('settings.startPageOnPhone'),
        h('select', { value: st.startPage, onchange: (e) => updateSettings({ startPage: e.target.value }) }, S.pages.map((p) => h('option', { value: p.id }, p.name)))),
      h('p', { class: 'hint-text' }, tr('settings.inDesktopProgramYou'))),
    card(
      h('h3', {}, tr('settings.resetVisualChanges')),
      h('p', { class: 'hint-text' }, tr('settings.actionsFromVisualsChange')),
      h('button', { class: 'btn', onclick: async () => { await api('POST', '/api/overrides/reset', {}); toast(tr('settings.allButtonsLookAs'), 'ok'); } }, tr('settings.resetEverything'))),
  ];
}

function security() {
  const st = S.settings;
  return [
    ...title(tr('settings.phoneSecurity'), tr('settings.editorCanOnlyBe')),
    card(
      check(tr('common.allowAccessFromWi'), st.lanEnabled, (v) => updateSettings({ lanEnabled: v })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn primary', onclick: openPairing }, tr('settings.showQrCode')),
        h('button', { class: 'btn ghost danger', onclick: async () => {
          if (await ask(tr('settings.createNewKey2'), tr('settings.allPairedPhonesWill'), [{ value: 'yes', label: tr('settings.createNew'), kind: 'primary' }])) {
            await updateSettings({ regenerateToken: true });
            toast(tr('settings.newKeyCreated'), 'ok');
          }
        } }, tr('settings.createNewKey')))),
    card(
      h('h3', {}, 'PIN'),
      h('p', { class: 'hint-text' }, tr('settings.extraProtectionWhoeverSees')),
      h('div', { class: 'hotkey' },
        h('input', { id: 'pin-input', type: 'password', inputmode: 'numeric', maxlength: 8, placeholder: st.pin ? tr('settings.set') : tr('settings.n48Digits'), autocomplete: 'off' }),
        h('button', { class: 'btn', onclick: async () => {
          const pin = document.getElementById('pin-input').value.trim();
          if (!/^\d{4,8}$/.test(pin)) return toast(tr('settings.pinMustConsistOf'), 'error');
          if (await updateSettings({ pin })) toast(tr('settings.pinSetPairedPhones'), 'ok');
        } }, tr('settings.setPin')),
        st.pin && h('button', { class: 'btn ghost danger', onclick: async () => (await updateSettings({ pin: '' })) && toast(tr('settings.pinRemoved'), 'ok') }, tr('common.remove')))),
  ];
}

function deck() {
  const d = S.settings.deck;
  const set = (k, v) => updateSettings({ deck: { ...d, [k]: v } });
  return [
    ...title(tr('settings.deck'), tr('settings.howDeckBehavesOn')),
    card(
      h('label', { class: 'field' }, tr('settings.pressAnimation'),
        h('select', { value: d.pressAnimation, onchange: (e) => set('pressAnimation', e.target.value) },
          [['shrink', tr('settings.shrink')], ['squeeze', tr('settings.squeeze')], ['wobble', tr('settings.wobble')], ['glow', tr('settings.glow')], ['none', tr('settings.none')]].map(([v, l]) => h('option', { value: v }, l)))),
      check(tr('settings.vibrateOnPress'), d.vibrate, (v) => set('vibrate', v)),
      check(tr('settings.clickSoundOnPress'), d.sound, (v) => set('sound', v)),
      check(tr('settings.showLabels'), d.showLabels, (v) => set('showLabels', v)),
      h('div', { class: 'field' }, tr('settings.orientationOnPhoneAnd'),
        segmented([['auto', tr('settings.automatic')], ['portrait', tr('settings.portrait')], ['landscape', tr('settings.landscape')]], d.orientation ?? 'auto', (v) => set('orientation', v))),
      h('p', { class: 'hint-text' }, tr('settings.androidAppTurnsTo')),
      h('div', { class: 'field' }, tr('settings.cellShape'),
        segmented([['fill', tr('settings.cellShapeFill')], ['square', tr('settings.cellShapeSquare')]], d.cellShape ?? 'fill', (v) => set('cellShape', v))),
      h('p', { class: 'hint-text' }, tr('settings.cellShapeHint'))),
  ];
}

function look() {
  const t = S.settings.theme;
  const accents = ['#7c8cff', '#3ccf91', '#ff8a4c', '#ff5c8a', '#46b4ff', '#b26cff', '#f5c542'];
  return [
    ...title(tr('common.look'), tr('settings.themeOfEditor')),
    card(
      h('div', { class: 'field' }, tr('settings.theme'),
        h('div', { class: 'segmented' }, [['dark', tr('settings.dark')], ['light', tr('settings.light')], ['system', tr('settings.likeSystem')]].map(([v, l]) =>
          h('button', { type: 'button', class: t.mode === v ? 'active' : '', onclick: () => updateSettings({ theme: { ...t, mode: v } }) }, l)))),
      h('div', { class: 'field' }, tr('settings.accentColor'),
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
    ...title(tr('settings.integrations')),
    card(
      h('h3', {}, tr('settings.obsStudio')),
      h('p', { class: 'hint-text' }, tr('settings.inObsEnableServer')),
      h('div', { class: 'row' },
        h('label', { class: 'field' }, tr('settings.address'), h('input', { id: 'obs-host', value: o.host, onchange: () => { forget(); updateSettings({ obs: fields() }); } })),
        h('label', { class: 'field', style: 'max-width:110px' }, tr('settings.port'), h('input', { id: 'obs-port', type: 'number', value: o.port, onchange: () => { forget(); updateSettings({ obs: fields() }); } }))),
      h('label', { class: 'field' }, tr('settings.password'), h('input', { id: 'obs-password', type: 'password', value: o.password, autocomplete: 'off', onchange: () => { forget(); updateSettings({ obs: fields() }); } })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn', onclick: async () => {
          try {
            const r = await api('POST', '/api/obs/autoconfig', {});
            S.settings = r.settings;
            forget();
            render();
            toast(r.serverEnabled ? tr('settings.portAndPasswordTaken') : tr('settings.takenOverButWebsocket'), r.serverEnabled ? 'ok' : 'error');
          } catch (err) {
            toast(err.message, 'error');
          }
        } }, tr('settings.takeOverFromObs')),
        h('button', { class: 'btn', onclick: async () => {
          try {
            const r = await api('POST', '/api/obs/test', fields());
            toast(tr('settings.connectedToObsScenes', [r.version, r.scenes]), 'ok');
          } catch (err) {
            toast(err.message, 'error');
          }
        } }, tr('settings.testConnection')))),
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
    if (!/^\d{15,25}$/.test(clientId)) return toast(tr('settings.clientIdConsistsOf'), 'error');
    if (!clientSecret && !dc.hasSecret) return toast(tr('settings.pleaseAlsoEnterClient'), 'error');
    e.target.disabled = true;
    e.target.textContent = tr('settings.pleaseClickAuthorizeIn');
    try {
      discordStatus = await api('POST', '/api/discord/connect', { clientId, clientSecret });
      toast(tr('settings.connectedToDiscord'), 'ok');
    } catch (err) {
      toast(err.message, 'error');
      discordStatus = null;
    }
    render();
  };
  let body;
  if (st.state === 'connected') {
    body = [
      h('p', {}, tr('settings.connectedAs'), h('b', {}, st.user || tr('settings.discordUser')), st.running ? tr('settings.discordIsRunning') : tr('settings.discordIsNotOpen')),
      h('p', { class: 'hint-text' }, tr('settings.actionsUnderCommunicationDiscord')),
      h('button', { class: 'btn ghost danger', onclick: async () => { discordStatus = await api('POST', '/api/discord/disconnect', {}); toast(tr('settings.discordDisconnected'), 'ok'); render(); } }, tr('settings.disconnect')),
    ];
  } else {
    body = [
      h('details', { class: 'howto', open: !dc.clientId },
        h('summary', {}, tr('settings.howToSetIt')),
        h('ol', {},
          h('li', {}, tr('settings.on'), h('a', { href: 'https://discord.com/developers/applications', target: '_blank', rel: 'noopener' }, 'discord.com/developers/applications'), tr('settings.signInWithYour')),
          h('li', {}, tr('settings.openOauth2OnLeft'), h('code', {}, 'http://localhost'), tr('settings.underRedirectsAndSave')),
          h('li', {}, tr('settings.onSamePageCopy')),
          h('li', {}, tr('settings.enterBothHereKeep')))),
      h('div', { class: 'row' },
        h('label', { class: 'field' }, tr('settings.clientId'), h('input', { id: 'discord-client', value: dc.clientId ?? '', placeholder: tr('settings.example1234567890123456789'), autocomplete: 'off', spellcheck: 'false' })),
        h('label', { class: 'field' }, tr('settings.clientSecret'), h('input', { id: 'discord-secret', type: 'password', placeholder: dc.hasSecret ? tr('settings.saved') : '', autocomplete: 'off' }))),
      st.error && h('p', { class: 'hint-text error' }, st.error),
      h('button', { class: 'btn primary', onclick: connect }, tr('settings.connectToDiscord')),
    ];
  }
  return card(
    h('h3', {}, tr('settings.discord')),
    h('p', { class: 'hint-text' }, tr('settings.voiceControlOfDiscord')),
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
    if (!/^[a-z0-9]{20,40}$/i.test(clientId)) return toast(tr('settings.thatDoesntLookLike'), 'error');
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
      h('p', {}, tr('settings.connectedAs'), h('b', {}, st.displayName ?? st.login), st.events ? tr('settings.eventsActive') : ''),
      h('p', { class: 'hint-text' }, tr('settings.youFindTwitchActions')),
      h('button', { class: 'btn ghost danger', onclick: async () => { twitchStatus = await api('POST', '/api/twitch/disconnect', {}); toast(tr('settings.twitchDisconnected'), 'ok'); render(); } }, tr('settings.disconnect')),
    ];
  } else if (st.state === 'waiting') {
    body = [
      h('p', {}, tr('settings.open'), h('a', { href: st.verificationUri, target: '_blank', rel: 'noopener' }, 'twitch.tv/activate'), tr('settings.andEnterThisCode')),
      h('div', { class: 'twitch-code' }, st.userCode),
      h('p', { class: 'hint-text' }, tr('settings.waitingForConfirmationCode')),
      h('button', { class: 'btn ghost', onclick: async () => { twitchStatus = await api('POST', '/api/twitch/cancel', {}); render(); } }, tr('common.cancel')),
    ];
  } else {
    body = [
      h('details', { class: 'howto', open: !tw.clientId },
        h('summary', {}, tr('settings.howToGetClient')),
        h('ol', {},
          h('li', {}, tr('settings.on'), h('a', { href: 'https://dev.twitch.tv/console/apps/create', target: '_blank', rel: 'noopener' }, 'dev.twitch.tv/console'), tr('settings.signInWithYour2')),
          h('li', {}, tr('settings.registerYourApplicationAny'), h('code', {}, 'http://localhost'), tr('settings.categoryApplicationIntegrationClient'), h('b', {}, tr('settings.public')), '.'),
          h('li', {}, tr('settings.createManageCopyClient')))),
      h('label', { class: 'field' }, tr('settings.clientIdOfYour'),
        h('input', { id: 'twitch-client', value: tw.clientId, placeholder: tr('settings.exampleGp762nuuoqcoxypju8c569th9wz7q5'), autocomplete: 'off', spellcheck: 'false' })),
      st.error && h('p', { class: 'hint-text error' }, st.error),
      h('button', { class: 'btn primary', onclick: connect }, tr('settings.connectToTwitch')),
    ];
  }
  return card(
    h('h3', {}, tr('settings.twitch')),
    h('p', { class: 'hint-text' }, tr('settings.everyoneConnectsTheirOwn')),
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
    if (!(await ask(tr('settings.restoreBackup'), tr('settings.currentStateIsBacked', [name]), [{ value: 'yes', label: tr('settings.restore'), kind: 'primary' }]))) return;
    try {
      const r = await api('POST', '/api/backups/restore', { name });
      backups = r.list;
      await loadState();
      toast(tr('settings.backupRestored'), 'ok');
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  const upload = h('input', { type: 'file', accept: 'application/json', hidden: true, onchange: async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    try {
      const config = JSON.parse(await file.text());
      if (!(await ask(tr('settings.restoreFromFile2'), tr('settings.replacesCurrentStateWhich', [file.name]), [{ value: 'yes', label: tr('settings.restore'), kind: 'primary' }]))) return;
      const r = await api('POST', '/api/backups/restore', { config });
      backups = r.list;
      await loadState();
      toast(tr('settings.backupRestored'), 'ok');
    } catch (err) {
      toast(err.message.includes('JSON') ? tr('settings.fileIsNotValid') : err.message, 'error');
    }
  } });
  return [
    ...title(tr('settings.backups'), tr('settings.backupsOfPagesValues')),
    card(
      check(tr('settings.backUpAutomaticallyOnce'), b.auto, (v) => updateSettings({ backups: { ...b, auto: v } })),
      h('label', { class: 'field', style: 'max-width:260px' }, tr('settings.keepAutomaticBackups'),
        h('input', { type: 'number', min: 1, max: 100, value: b.keep, onchange: (e) => updateSettings({ backups: { ...b, keep: Number(e.target.value) } }) })),
      h('div', { class: 'row-buttons' },
        h('button', { class: 'btn primary', onclick: async () => {
          const r = await api('POST', '/api/backups', { note: 'manuell' });
          backups = r.list;
          render();
          toast(tr('settings.backupCreated'), 'ok');
        } }, tr('settings.backUpNow')),
        h('label', { class: 'btn' }, tr('settings.restoreFromFile'), upload))),
    card(
      h('h3', {}, tr('settings.existingBackups')),
      !backups ? h('p', { class: 'muted-note' }, tr('common.loading'))
        : !backups.length ? h('p', { class: 'muted-note' }, tr('settings.noBackupsYet'))
          : h('table', { class: 'values-table' }, h('tbody', {}, backups.map((bk) => h('tr', {},
            h('td', {}, new Date(bk.time).toLocaleString('de-DE')),
            h('td', {}, bk.note || '–'),
            h('td', {}, tr('settings.kb', [Math.round(bk.size / 1024)])),
            h('td', { class: 'right' },
              h('button', { class: 'btn small', onclick: () => restore(bk.name) }, tr('settings.restore')),
              h('a', { class: 'btn small ghost', href: `/backups/download?name=${encodeURIComponent(bk.name)}`, download: bk.name }, tr('settings.download')),
              h('button', { class: 'btn icon ghost danger', title: tr('common.delete'), onclick: async () => {
                const r = await api('POST', '/api/backups/delete', { name: bk.name });
                backups = r.list;
                render();
              } }, '✕'))))))),
  ];
}

function logsSection() {
  const l = S.settings.logs;
  return [
    ...title(tr('settings.log'), tr('settings.howDetailedTouchdeckLogs')),
    card(h('label', { class: 'field' }, tr('settings.detail'),
      h('select', { value: l.level, onchange: (e) => updateSettings({ logs: { level: e.target.value } }) },
        [['debug', tr('settings.everythingForTroubleshooting')], ['info', tr('common.normal')], ['warn', tr('settings.warningsAndErrorsOnly')], ['error', tr('settings.errorsOnly')]].map(([v, t]) => h('option', { value: v }, t))))),
  ];
}

function infoSection() {
  if (!info) {
    api('GET', '/api/info').then((i) => {
      info = i;
      render();
      setTimeout(() => (info = null), 5000);
    }).catch((err) => toast(err.message, 'error'));
    return [...title(tr('settings.information')), h('p', { class: 'muted-note' }, tr('common.loading'))];
  }
  const up = `${Math.floor(info.uptime / 3600)} h ${Math.floor((info.uptime % 3600) / 60)} min`;
  const rows = [
    [tr('settings.version'), tr('settings.touchdeck', [info.version])],
    [tr('common.system'), tr('settings.format', [info.platform, info.host])],
    [tr('settings.runningFor'), up],
    [tr('settings.address'), info.lan.map((ip) => `http://${ip}:${info.port}`).join(', ') || tr('settings.noNetwork')],
    [tr('settings.connectedPhones'), String(info.devices)],
    ['OBS', info.obsConnected ? tr('settings.connected') : tr('settings.notConnected')],
    [tr('settings.content'), tr('settings.pagesButtonsFlowsEvents', [info.counts.pages, info.counts.buttons, info.counts.flows, info.counts.events, info.counts.values])],
    [tr('settings.dataFolder'), info.dataDir],
  ];
  return [
    ...title(tr('settings.information')),
    card(h('table', { class: 'info-table' }, h('tbody', {}, rows.map(([k, v]) => h('tr', {}, h('th', {}, k), h('td', {}, v)))))),
  ];
}

export function resetSettingsCache() {
  info = null;
  backups = null;
  twitchStatus = null;
  discordStatus = null;
}
