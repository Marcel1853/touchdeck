// Bereich „Plugins“: installierte Plugins, Marktplatz (öffentliche GitHub-Repos) und Testmodus für Entwickler.
import { t as tr, getLang } from '/shared/i18n.js';
import { h, api, toast, ask, prompt, contextMenu } from '../dom.js';
import { S, render, loadState, save } from '../state.js';
import { importPluginPage } from './pages.js';
import { renderMarkdown } from '../markdown.js';

/** Seite im Plugin-Wiki in der passenden Sprache. */
const wikiPage = (de, en) => `https://github.com/Marcel1853/touchdeck-plugins/wiki/${getLang() === 'de' ? de : en}`;

const TABS = [['installed', tr('plugins.installed')], ['market', tr('plugins.marketplace')], ['dev', tr('plugins.develop')], ['advanced', tr('plugins.advanced')]];
let tab = 'installed';
let plugins = null;
let market = null;
let config = { advanced: false, confirm: 'AUF EIGENE GEFAHR' };

async function refresh() {
  updatesInfo = null;
  try {
    plugins = await api('GET', '/api/plugins');
    config = await api('GET', '/api/plugins/config');
  } catch (err) {
    plugins = [];
    toast(err.message, 'error');
  }
  render();
}

/** Nach Änderungen: neue Aktionen und Auslöser der Plugins in den Editor holen. */
async function applyChange(r) {
  if (r?.plugins) plugins = r.plugins;
  await save();
  await loadState({ keepUndo: true });
  render();
}

async function call(path, body, okText) {
  try {
    const r = await api('POST', path, body);
    await applyChange(r);
    if (okText) toast(okText, 'ok');
    return r;
  } catch (err) {
    toast(err.message, 'error');
    return null;
  }
}

/** GitHub-Repo prüfen, Berechtigungen zeigen, nach Bestätigung installieren. */
/** GitHub-Adresse eines Plugins; bei „owner/repo/ordner“ die Ordner-Ansicht. */
function githubUrl(repo) {
  const [owner, name, ...path] = repo.split('/');
  return path.length ? `https://github.com/${owner}/${name}/tree/HEAD/${path.join('/')}` : `https://github.com/${owner}/${name}`;
}

/** Beschreibung (README) eines Marktplatz-Plugins in einem eigenen Fenster, mit Installieren-Knopf. */
async function showReadme(p, installed) {
  const body = h('div', { class: 'readme-body' }, h('p', { class: 'hint-text' }, tr('common.loading')));
  const dlg = h('dialog', { class: 'dialog readme-dialog' },
    h('div', { class: 'readme-head' },
      h('div', {}, h('h2', {}, p.name || p.repo), h('p', { class: 'hint-text' }, [p.author && tr('plugins.by2', [p.author]), p.license && tr('plugins.license2', [p.license])].filter(Boolean).join(' ').replace(/[\s·]+$/, ''))),
      h('div', { class: 'spacer' }),
      h('button', { class: `btn small ${installed ? '' : 'primary'}`, onclick: () => { dlg.close(); installFromGitHub(p.repo); } }, installed ? tr('common.update') : tr('plugins.install')),
      h('button', { class: 'btn small ghost', title: tr('common.close'), onclick: () => dlg.close() }, '✕')),
    body);
  dlg.addEventListener('close', () => dlg.remove());
  document.body.append(dlg);
  dlg.showModal();
  try {
    const r = await api('GET', `/api/plugins/readme?repo=${encodeURIComponent(p.repo)}`);
    body.replaceChildren(renderMarkdown(r.text, r.base),
      h('p', { class: 'hint-text' }, h('a', { href: r.page, target: '_blank', rel: 'noopener' }, tr('plugins.viewOnGithub'))));
  } catch (err) {
    body.replaceChildren(h('p', { class: 'hint-text' }, p.description || ''), h('p', { class: 'hint-text error' }, err.message));
  }
}

async function installFromGitHub(repo) {
  let preview;
  try {
    toast(tr('plugins.checkingPluginOnGithub'));
    preview = await api('POST', '/api/plugins/preview', { repo });
  } catch (err) {
    return toast(err.message, 'error');
  }
  const m = preview.manifest;
  if (preview.native && !config.advanced) {
    return ask(tr('plugins.format3', [m.name]), tr('plugins.thisPluginIsProgram'),
      [{ value: 'go', label: tr('plugins.goToAdvancedMode'), kind: 'primary' }]).then((a) => { if (a === 'go') { tab = 'advanced'; render(); } });
  }
  const perms = preview.permissions.length ? preview.permissions.map((p) => tr('plugins.format2', [p])).join('\n') : tr('plugins.noSpecialRights');
  const existing = plugins?.find((p) => p.id === m.id);
  const intro = [
    preview.native && tr('plugins.ownProgramWithFull'),
    !preview.listed && tr('plugins.notInMarketplaceNot'),
    m.description || tr('plugins.noDescription'),
  ].filter(Boolean).join('\n\n');
  const facts = tr('plugins.byLicenseSourceVersion', [m.author || tr('plugins.unknown'), m.license || tr('plugins.notStatedAllRights'), preview.url, preview.ref]);
  const answer = await ask(`🧱 ${m.name}${m.version ? ` (${m.version})` : ''}`,
    `${intro}\n\n${facts}\n\n${tr('plugins.pluginWantsTo')}\n${perms}\n\n`
    + (preview.native ? tr('plugins.advancedPluginsAreNot') : tr('plugins.pluginsAreSandboxedAnd')),
    [{ value: 'install', label: existing ? tr('common.update') : tr('plugins.install'), kind: 'primary' }]);
  if (answer !== 'install') return;
  // Zweite Bestätigung für Plugins mit vollen Rechten.
  if (preview.native) {
    const sure = await ask(tr('plugins.giveFullRights'), tr('plugins.isProgramOfIts', [m.name, preview.url]),
      [{ value: 'yes', label: tr('plugins.yesGiveFullRights'), kind: 'danger' }]);
    if (sure !== 'yes') return;
  }
  const r = await call('/api/plugins/install', { repo: preview.repo, digest: preview.digest, acceptRisk: !!preview.native }, tr(existing ? 'plugins.updated' : 'plugins.installed2', [m.name]));
  if (r) tab = 'installed';
  render();
}

function settingsForm(p) {
  const fields = p.manifest.settings ?? [];
  if (!fields.length) return null;
  const values = { ...(p.settings ?? {}) };
  return h('div', { class: 'plugin-settings' },
    h('h3', {}, tr('common.settings')),
    fields.map((f) => {
      const v = values[f.key] ?? f.default ?? '';
      if (f.type === 'checkbox') {
        return h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked: v === true || v === 'true', onchange: (e) => (values[f.key] = e.target.checked) }), f.label || f.key);
      }
      return h('label', { class: 'field' }, f.label || f.key,
        h('input', { type: f.type === 'password' ? 'password' : f.type === 'number' ? 'number' : 'text', value: v, placeholder: f.placeholder ?? '', oninput: (e) => (values[f.key] = e.target.value) }));
    }),
    h('button', { class: 'btn small primary', onclick: () => call('/api/plugins/settings', { id: p.id, settings: values }, tr('plugins.settingsSaved')) }, tr('common.save')));
}

// Aufgeklappte Plugin-Karten (wie bei den Icon-Paketen).
const openPlugins = new Set();

function removePlugin(p) {
  ask(tr('plugins.removePlugin'), tr('plugins.willBeRemovedButtons', [p.manifest.name]), [{ value: 'remove', label: tr('common.remove'), kind: 'danger' }])
    .then((a) => a === 'remove' && call('/api/plugins/remove', { id: p.id }, tr('plugins.pluginRemoved')));
}

/** Rechtsklick auf ein Plugin. */
function pluginMenu(p, x, y) {
  const local = String(p.source ?? '').startsWith('local:');
  const repo = String(p.source ?? '').replace(/^github:/, '');
  const open = openPlugins.has(p.id);
  contextMenu(x, y, [
    { icon: open ? '▸' : '▾', label: open ? tr('plugins.collapse') : tr('plugins.expand'), onclick: () => { open ? openPlugins.delete(p.id) : openPlugins.add(p.id); render(); } },
    { icon: p.enabled ? '⏸' : '⏵', label: p.enabled ? tr('plugins.turnOff') : tr('plugins.turnOn'), onclick: () => call('/api/plugins/enable', { id: p.id, enabled: !p.enabled }) },
    local
      ? { icon: '↻', label: tr('plugins.reload'), onclick: () => call('/api/plugins/reload', { id: p.id }, tr('plugins.reloaded')) }
      : { icon: '⬆', label: tr('plugins.checkForUpdates'), onclick: () => installFromGitHub(repo) },
    ...(p.pages ?? []).map((pg) => ({ icon: '📄', label: `${pg.name} ${tr('plugins.importPage')}`, onclick: () => importPluginPage(p.id, pg.file) })),
    p.url && { icon: '↗', label: tr('plugins.viewOnGithub'), onclick: () => window.open(p.url, '_blank', 'noopener') },
    '-',
    { icon: '🗑', label: tr('plugins.remove'), danger: true, onclick: () => removePlugin(p) },
  ]);
}

function pluginCard(p) {
  const m = p.manifest;
  const local = String(p.source ?? '').startsWith('local:');
  const repo = String(p.source ?? '').replace(/^github:/, '');
  const status = { running: [tr('plugins.running'), 'ok'], off: [tr('plugins.off'), ''], blocked: [tr('plugins.blocked'), 'warn'] }[p.status.state] ?? [tr('common.error'), 'error'];
  const open = openPlugins.has(p.id);
  const toggle = () => { open ? openPlugins.delete(p.id) : openPlugins.add(p.id); render(); };
  const head = h('div', { class: 'plugin-head', oncontextmenu: (e) => { e.preventDefault(); pluginMenu(p, e.clientX, e.clientY); } },
    h('div', { class: 'plugin-title', onclick: toggle },
      h('span', { class: 'chevron' }, open ? '▾' : '▸'),
      h('b', {}, m.name), m.version && h('span', { class: 'muted' }, ` ${m.version}`),
      h('span', { class: `status-badge ${status[1]}` }, status[0]),
      local && h('span', { class: 'status-badge warn' }, tr('plugins.testMode')),
      p.native && h('span', { class: 'status-badge error' }, tr('plugins.fullRights')),
      p.update && h('span', { class: 'status-badge update-badge' }, tr('plugins.updateTo', [p.update.latest])),
      h('p', { class: 'hint-text' }, m.description || '')),
    h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked: p.enabled,
      onchange: (e) => call('/api/plugins/enable', { id: p.id, enabled: e.target.checked }) }), tr('plugins.on')));
  // Update und geänderte fertige Seiten auch im zugeklappten Zustand zeigen.
  const extras = [
    p.update && h('div', { class: 'plugin-update-row' },
      h('button', { class: 'btn small primary', onclick: () => updatePlugin(p) }, `⬆ ${tr('plugins.updateNow', [p.update.latest])}`),
      h('span', { class: 'hint-text' }, tr('plugins.updateKeepsSettings'))),
    p.pagesChanged?.length > 0 && h('div', { class: 'plugin-pages-changed' },
      h('p', {}, tr('plugins.pagesChangedQuestion', [m.name])),
      h('p', { class: 'hint-text' }, p.pagesChanged.join(', ')),
      h('div', { class: 'plugin-update-row' },
        h('button', { class: 'btn small primary', onclick: () => updatePages(p, 'replace') }, tr('plugins.pagesReplace')),
        h('button', { class: 'btn small', onclick: () => updatePages(p, 'add') }, tr('plugins.pagesAdd')),
        h('button', { class: 'btn small ghost', onclick: () => updatePages(p, 'keep') }, tr('plugins.pagesKeep'))),
      h('p', { class: 'hint-text' }, tr('plugins.pagesReplaceHint'))),
  ];
  if (!open) return h('div', { class: 'settings-card plugin' }, head, ...extras, p.status.error && h('p', { class: 'hint-text error' }, p.status.error));
  return h('div', { class: 'settings-card plugin open', oncontextmenu: (e) => { if (e.target.closest('input, textarea, select')) return; e.preventDefault(); pluginMenu(p, e.clientX, e.clientY); } },
    head,
    ...extras,
    h('p', { class: 'hint-text' }, [m.author && tr('plugins.by', [m.author]), m.license && tr('plugins.license', [m.license]), local ? tr('plugins.folder', [p.source.slice(6)]) : null].filter(Boolean).join(' · '),
      !local && p.url && h('span', {}, m.author ? ' · ' : '', h('a', { href: p.url, target: '_blank', rel: 'noopener' }, tr('plugins.format', [repo, p.ref])))),
    p.status.error && h('p', { class: 'hint-text error' }, p.status.error),
    h('p', { class: 'hint-text' }, tr('plugins.allowed'), p.permissions.length ? p.permissions.join(' · ') : tr('plugins.nothingSpecial')),
    h('p', { class: 'hint-text' }, tr('plugins.actionSTriggerS', [m.actions.length, m.triggers.length, m.name])),
    settingsForm(p),
    p.pages?.length > 0 && h('div', { class: 'plugin-pages' },
      h('p', { class: 'hint-text' }, tr('plugins.readyPages')),
      p.pages.map((pg) => h('button', { class: 'btn small', onclick: () => importPluginPage(p.id, pg.file).then(() => toast(tr('plugins.pageImportedHint'), 'ok')) }, `📄 ${pg.name} ${tr('plugins.importPage')}`))),
    h('div', { class: 'pack-actions' },
      local
        ? h('button', { class: 'btn small', onclick: () => call('/api/plugins/reload', { id: p.id }, tr('plugins.reloaded')) }, tr('plugins.reload'))
        : h('button', { class: 'btn small', onclick: () => installFromGitHub(repo) }, tr('plugins.checkForUpdates')),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn small ghost danger', onclick: () => removePlugin(p) }, tr('plugins.remove'))));
}

function installedTab() {
  if (!plugins.length) return [h('div', { class: 'settings-card' }, h('p', {}, tr('plugins.noPluginsInstalledYet')))];
  return [
    h('div', { class: 'market-bar updates-bar' },
      h('button', { class: 'btn small', title: tr('plugins.checkUpdatesHint'), onclick: checkUpdates }, `⟳ ${tr('plugins.checkUpdates')}`)),
    ...plugins.map(pluginCard),
  ];
}

/** Jetzt nach Updates suchen (Plugins über GitHub/Marktplatz und TouchDeck selbst). */
async function checkUpdates() {
  try {
    const r = await api('POST', '/api/updates/check', {});
    toast(r.plugins.length ? tr('plugins.updatesFound', [r.plugins.length]) : tr('plugins.noUpdates'), 'ok');
    await refresh();
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function updatePlugin(p) {
  try {
    toast(tr('plugins.updating', [p.manifest.name]));
    const r = await api('POST', '/api/plugins/update', { id: p.id });
    toast(tr('plugins.updatedTo', [p.manifest.name, p.update.latest]), 'ok');
    await applyChange(r);
  } catch (err) {
    toast(err.message, 'error');
  }
}

/** Geänderte fertige Seiten: ersetzen, zusätzlich importieren oder behalten. */
async function updatePages(p, mode) {
  try {
    const r = await api('POST', '/api/plugins/update-pages', { id: p.id, mode });
    if (mode !== 'keep') toast(tr('plugins.pagesUpdatedToast'), 'ok');
    await applyChange(r);
  } catch (err) {
    toast(err.message, 'error');
  }
}

function repoInput() {
  let repo = '';
  return h('div', { class: 'settings-card' },
    h('h3', {}, tr('plugins.installFromGithub')),
    h('p', { class: 'hint-text' }, tr('plugins.pluginsHaveToBe')),
    h('div', { class: 'row' },
      h('input', { placeholder: tr('plugins.ownerRepoOrHttps'), oninput: (e) => (repo = e.target.value), onkeydown: (e) => e.key === 'Enter' && installFromGitHub(repo) }),
      h('button', { class: 'btn primary small', onclick: () => installFromGitHub(repo) }, tr('plugins.check'))));
}

/**
 * Installiertes Plugin zu einem Marktplatz-Eintrag: vom Marktplatz (github:…), aus einem Ordner mit
 * demselben Pfad (Testmodus, z. B. …/plugins/factorio) oder mit demselben Namen.
 */
function installedFor(p) {
  const sub = p.repo.split('/').slice(2).join('/');
  const name = (p.name ?? '').toLowerCase();
  return plugins.find((x) => x.source === `github:${p.repo}`)
    ?? plugins.find((x) => sub && x.source?.startsWith('local:') && x.source.replace(/\/+$/, '').endsWith(`/${sub}`))
    ?? plugins.find((x) => name && String(x.manifest?.name ?? '').toLowerCase() === name);
}

function marketTab() {
  if (market === null || market === 'fresh') {
    const fresh = market === 'fresh';
    market = 'loading';
    api('GET', `/api/plugins/market${fresh ? '?fresh=1' : ''}`).then((r) => (market = r)).catch((err) => (market = { plugins: [], note: err.message })).then(render);
  }
  if (market === 'loading') {
    return [repoInput(), h('p', { class: 'hint-text' }, tr('plugins.loadingMarketplace'))];
  }
  return [
    repoInput(),
    h('div', { class: 'market-bar' },
      h('button', { class: 'btn small', title: tr('plugins.reloadMarketHint'), onclick: () => { market = 'fresh'; render(); } }, `⟳ ${tr('plugins.reloadMarket')}`)),
    market.note && h('p', { class: 'hint-text' }, market.note),
    !market.note && !market.plugins.length && h('p', { class: 'hint-text' }, tr('plugins.thereAreNoPlugins'),
      h('a', { href: wikiPage('Veröffentlichen', 'Publishing'), target: '_blank', rel: 'noopener' }, tr('plugins.pluginWiki')), '.'),
    ...market.plugins.map((p) => {
      const mine = installedFor(p);
      const installed = mine?.source === `github:${p.repo}`;
      // Plugin für ein anderes Betriebssystem: Hinweis statt Installieren.
      const foreign = p.platforms?.length > 0 && !p.platforms.includes(market.platform);
      return h('div', { class: `settings-card plugin${mine ? ' is-installed' : ''}` },
        h('div', { class: 'plugin-head' },
          h('div', {}, h('b', {}, p.name || p.repo),
            mine && h('span', { class: 'installed-badge', title: mine.source ?? '' },
              `✓ ${tr('plugins.installedBadge')}${mine.manifest?.version ? ` · ${mine.manifest.version}` : ''}${installed ? '' : ` · ${tr('plugins.installedLocal')}`}`),
            mine?.update && h('span', { class: 'installed-badge update-badge' }, tr('plugins.updateTo', [mine.update.latest])),
            foreign && h('span', { class: 'installed-badge update-badge', title: tr('plugins.onlyForHint', [systemName(market.platform)]) },
              tr('plugins.onlyFor', [p.platforms.map(systemName).join(', ')])),
            h('p', { class: 'hint-text' }, p.description),
            h('p', { class: 'hint-text' }, p.author && tr('plugins.by2', [p.author]), p.license && tr('plugins.license2', [p.license]), h('a', { href: githubUrl(p.repo), target: '_blank', rel: 'noopener' }, p.repo))),
          h('div', { class: 'plugin-buttons' },
            h('button', { class: 'btn small', onclick: () => showReadme(p, installed) }, tr('plugins.readMore')),
            // Aus einem Ordner (Testmodus) installiert: die Marktplatz-Fassung kann sie ersetzen.
            h('button', { class: `btn small ${mine || foreign ? '' : 'primary'}`, disabled: foreign && !mine,
              title: foreign ? tr('plugins.onlyForHint', [systemName(market.platform)]) : mine && !installed ? tr('plugins.takeFromMarketHint') : '', onclick: () => installFromGitHub(p.repo) },
              installed ? tr('common.update') : mine ? tr('plugins.takeFromMarket') : tr('plugins.install')))));
    }),
  ];
}

const systemName = (p) => ({ linux: 'Linux', windows: 'Windows' })[p] ?? p;

async function setAdvanced(on) {
  if (on) {
    const first = await ask(tr('plugins.advancedMode'), tr('plugins.inAdvancedModePlugins'),
      [{ value: 'next', label: tr('plugins.understoodContinue'), kind: 'danger' }]);
    if (first !== 'next') return render();
    const typed = await prompt(tr('plugins.secondConfirmation'), tr('plugins.toTurnItOn', [config.confirm]));
    if (typed === null) return render();
    if (typed.trim() !== config.confirm) {
      toast(tr('plugins.notTurnedOnText'), 'error');
      return render();
    }
    const r = await call('/api/plugins/advanced', { enabled: true, confirm: typed }, tr('plugins.advancedModeTurnedOn'));
    if (r) config.advanced = r.advanced;
  } else {
    const r = await call('/api/plugins/advanced', { enabled: false }, tr('plugins.advancedModeTurnedOff'));
    if (r) config.advanced = r.advanced;
  }
  render();
}

function advancedTab() {
  const natives = plugins.filter((p) => p.native);
  return [
    h('div', { class: `settings-card ${config.advanced ? 'danger-card' : ''}` },
      h('h3', {}, tr('plugins.advancedPluginsFullRights')),
      h('p', {}, tr('plugins.regularPluginsAreSandboxed')),
      h('p', { class: 'hint-text' }, tr('plugins.advancedPluginsTooOnly')),
      h('label', { class: 'switch-row' },
        h('input', { type: 'checkbox', checked: config.advanced, onchange: (e) => setAdvanced(e.target.checked) }),
        config.advanced ? tr('plugins.advancedModeIsOn') : tr('plugins.turnOnAdvancedMode'))),
    natives.length > 0 && h('p', { class: 'hint-text' }, tr(config.advanced ? 'plugins.advancedPluginSInstalled2' : 'plugins.advancedPluginSInstalled', [natives.length])),
  ];
}

function devTab() {
  let path = '';
  return [
    h('div', { class: 'settings-card' },
      h('h3', {}, tr('plugins.testPluginFromFolder')),
      h('p', { class: 'hint-text' }, tr('plugins.forDevelopmentLoadPlugin')),
      h('div', { class: 'row' },
        h('input', { placeholder: '~/mein-plugin', oninput: (e) => (path = e.target.value), onkeydown: (e) => e.key === 'Enter' && call('/api/plugins/dev', { path }, tr('plugins.pluginLoadedInTest')).then((r) => r && ((tab = 'installed'), render())) }),
        h('button', { class: 'btn primary small', onclick: () => call('/api/plugins/dev', { path }, tr('plugins.pluginLoadedInTest')).then((r) => r && ((tab = 'installed'), render())) }, tr('plugins.load')),
        h('label', { class: 'btn small' }, tr('plugins.chooseZip'), h('input', { type: 'file', accept: '.zip', hidden: true, onchange: async (e) => {
          const file = e.target.files[0];
          if (!file) return;
          const data = await new Promise((ok) => { const fr = new FileReader(); fr.onload = () => ok(fr.result); fr.readAsDataURL(file); });
          const r = await call('/api/plugins/dev', { data }, tr('plugins.pluginLoadedInTest'));
          if (r) { tab = 'installed'; render(); }
        } })))),
    h('div', { class: 'settings-card' },
      h('h3', {}, tr('plugins.whatPluginLooksLike')),
      h('p', { class: 'hint-text' }, tr('plugins.folderOrPublicGithub'),
        h('a', { href: wikiPage('Startseite', 'Home'), target: '_blank', rel: 'noopener' }, tr('plugins.pluginWiki')), tr('plugins.examplesToCopyAre'),
        h('a', { href: 'https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples', target: '_blank', rel: 'noopener' }, 'touchdeck-plugins/examples'), '.'),
      h('pre', { class: 'code' }, `// plugin.js
td.action('greet', (params, info) => \`Hello \${params.name}!\`);   // ${tr('plugins.action')}
td.every(60000, () => td.values.set('Time', new Date().toLocaleTimeString()));  // ${tr('plugins.timer')}
td.fire('done', { Name: 'x' });              // ${tr('plugins.ownTriggerForEvents')}
td.http({ url: 'https://api.example.com/x' }).json();   // ${tr('plugins.onlyAllowedAddresses')}
td.values.get('Name'); td.notify('Title', 'Text'); td.log('…'); td.settings.apiKey`)),
  ];
}

export function mountPlugins(root) {
  root.className = 'view view-plugins';
  root.replaceChildren(h('section', { id: 'plugins-detail', class: 'detail settings' }), h('aside', { id: 'plugins-aside', class: 'plugins-aside' }));
  if (plugins === null) refresh();
}

// Stand der Updates für die rechte Spalte (neu geladen mit der Plugin-Liste).
let updatesInfo = null;

/** Rechte Spalte: Updates auf einen Blick, Suche, Einstellungen, neue TouchDeck-Version. */
function updatesPanel() {
  if (updatesInfo === null) {
    updatesInfo = 'loading';
    api('GET', '/api/updates').then((u) => { updatesInfo = u; render(); }).catch(() => (updatesInfo = { plugins: [] }));
    return [];
  }
  if (updatesInfo === 'loading') return [];
  const u = updatesInfo;
  const st = S.settings ?? {};
  const when = u.checked ? new Date(u.checked * 1000).toLocaleString(getLang(), { dateStyle: 'short', timeStyle: 'short' }) : tr('plugins.neverChecked');
  const setSetting = async (patch) => {
    try {
      const r = await api('PUT', '/api/settings', patch);
      S.settings = r.settings;
      render();
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  return [
    h('h2', {}, tr('plugins.updatesPanel')),
    h('p', { class: 'hint-text' }, tr('plugins.lastChecked', [when])),
    h('button', { class: 'btn small', onclick: checkUpdates }, `⟳ ${tr('plugins.checkUpdates')}`),
    u.plugins.length
      ? h('div', { class: 'aside-list' }, u.plugins.map((x) => {
        const p = (plugins ?? []).find((y) => y.id === x.id);
        return h('div', { class: 'aside-item' },
          h('div', {}, h('b', {}, x.name), h('span', { class: 'hint-text' }, ` ${x.current} → ${x.latest}`)),
          p && h('button', { class: 'btn small primary', onclick: () => updatePlugin(p) }, tr('plugins.updateShort')));
      }))
      : h('p', { class: 'aside-ok' }, `✓ ${tr('plugins.noUpdates')}`),
    h('p', { class: 'hint-text' }, tr('plugins.updatableCount', [u.updatable ?? 0])),
    u.release?.version && h('div', { class: 'aside-release' },
      h('b', {}, tr('settings.newReleaseAvailable', [u.release.version])), ' ',
      h('a', { href: u.release.url, target: '_blank', rel: 'noopener' }, tr('settings.download'))),
    h('h3', {}, tr('plugins.updateSettings')),
    h('label', { class: 'field' }, tr('settings.pluginUpdates'),
      h('select', { value: st.pluginUpdates ?? 'notify', onchange: (e) => setSetting({ pluginUpdates: e.target.value }) },
        [['notify', tr('settings.pluginUpdatesNotify')], ['auto', tr('settings.pluginUpdatesAuto')], ['off', tr('settings.pluginUpdatesOff')]].map(([v, l]) => h('option', { value: v }, l)))),
    h('label', { class: 'field' }, tr('settings.pluginPages'),
      h('select', { value: st.pluginPages ?? 'ask', onchange: (e) => setSetting({ pluginPages: e.target.value }) },
        [['ask', tr('settings.pluginPagesAsk')], ['replace', tr('settings.pluginPagesReplace')], ['keep', tr('settings.pluginPagesKeep')]].map(([v, l]) => h('option', { value: v }, l)))),
    h('p', { class: 'hint-text' }, tr('plugins.updatesPanelHint')),
  ];
}

export function renderPlugins() {
  const el = document.getElementById('plugins-detail');
  if (!el) return;
  const head = [h('h1', {}, tr('common.plugins')), h('p', { class: 'hint-text' }, tr('plugins.extensionsByOthersNew')),
    h('div', { class: 'segmented plugin-tabs' }, TABS.map(([id, label]) => h('button', { type: 'button', class: id === tab ? 'active' : '', onclick: () => { tab = id; if (id === 'market') market = null; render(); } }, label)))];
  if (plugins === null) return el.replaceChildren(...head, h('p', { class: 'hint-text' }, tr('common.loading')));
  const body = { installed: installedTab, market: marketTab, dev: devTab, advanced: advancedTab }[tab]();
  el.replaceChildren(...head, ...body.filter(Boolean));
  document.getElementById('plugins-aside')?.replaceChildren(...updatesPanel().filter(Boolean));
}
