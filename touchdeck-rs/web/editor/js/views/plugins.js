// Bereich „Plugins“: installierte Plugins, Marktplatz (öffentliche GitHub-Repos) und Testmodus für Entwickler.
import { t as tr, getLang } from '/shared/i18n.js';
import { h, api, toast, ask, prompt } from '../dom.js';
import { S, render, loadState, save } from '../state.js';
import { importPluginPage } from './pages.js';

/** Seite im Plugin-Wiki in der passenden Sprache. */
const wikiPage = (de, en) => `https://github.com/Marcel1853/touchdeck-plugins/wiki/${getLang() === 'de' ? de : en}`;

const TABS = [['installed', tr('plugins.installed')], ['market', tr('plugins.marketplace')], ['dev', tr('plugins.develop')], ['advanced', tr('plugins.advanced')]];
let tab = 'installed';
let plugins = null;
let market = null;
let config = { advanced: false, confirm: 'AUF EIGENE GEFAHR' };

async function refresh() {
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

function pluginCard(p) {
  const m = p.manifest;
  const local = String(p.source ?? '').startsWith('local:');
  const repo = String(p.source ?? '').replace(/^github:/, '');
  const status = { running: [tr('plugins.running'), 'ok'], off: [tr('plugins.off'), ''], blocked: [tr('plugins.blocked'), 'warn'] }[p.status.state] ?? [tr('common.error'), 'error'];
  return h('div', { class: 'settings-card plugin' },
    h('div', { class: 'plugin-head' },
      h('div', {},
        h('b', {}, m.name), m.version && h('span', { class: 'muted' }, ` ${m.version}`),
        h('span', { class: `status-badge ${status[1]}` }, status[0]),
        local && h('span', { class: 'status-badge warn' }, tr('plugins.testMode')),
        p.native && h('span', { class: 'status-badge error' }, tr('plugins.fullRights')),
        h('p', { class: 'hint-text' }, m.description || ''),
        h('p', { class: 'hint-text' }, [m.author && tr('plugins.by', [m.author]), m.license && tr('plugins.license', [m.license]), local ? tr('plugins.folder', [p.source.slice(6)]) : null].filter(Boolean).join(' · '),
          !local && p.url && h('span', {}, m.author ? ' · ' : '', h('a', { href: p.url, target: '_blank', rel: 'noopener' }, tr('plugins.format', [repo, p.ref]))))),
      h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked: p.enabled,
        onchange: (e) => call('/api/plugins/enable', { id: p.id, enabled: e.target.checked }) }), tr('plugins.on'))),
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
      h('button', { class: 'btn small ghost danger', onclick: async () => {
        const a = await ask(tr('plugins.removePlugin'), tr('plugins.willBeRemovedButtons', [m.name]), [{ value: 'remove', label: tr('common.remove'), kind: 'danger' }]);
        if (a === 'remove') call('/api/plugins/remove', { id: p.id }, tr('plugins.pluginRemoved'));
      } }, tr('plugins.remove'))));
}

function installedTab() {
  if (!plugins.length) return [h('div', { class: 'settings-card' }, h('p', {}, tr('plugins.noPluginsInstalledYet')))];
  return plugins.map(pluginCard);
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

function marketTab() {
  if (market === null) {
    api('GET', '/api/plugins/market').then((r) => (market = r)).catch((err) => (market = { plugins: [], note: err.message })).then(render);
    return [repoInput(), h('p', { class: 'hint-text' }, tr('plugins.loadingMarketplace'))];
  }
  return [
    repoInput(),
    market.note && h('p', { class: 'hint-text' }, market.note),
    !market.note && !market.plugins.length && h('p', { class: 'hint-text' }, tr('plugins.thereAreNoPlugins'),
      h('a', { href: wikiPage('Veröffentlichen', 'Publishing'), target: '_blank', rel: 'noopener' }, tr('plugins.pluginWiki')), '.'),
    ...market.plugins.map((p) => {
      const installed = plugins.some((x) => x.source === `github:${p.repo}`);
      return h('div', { class: 'settings-card plugin' },
        h('div', { class: 'plugin-head' },
          h('div', {}, h('b', {}, p.name || p.repo), h('p', { class: 'hint-text' }, p.description),
            h('p', { class: 'hint-text' }, p.author && tr('plugins.by2', [p.author]), p.license && tr('plugins.license2', [p.license]), h('a', { href: `https://github.com/${p.repo}`, target: '_blank', rel: 'noopener' }, p.repo))),
          h('button', { class: `btn small ${installed ? '' : 'primary'}`, onclick: () => installFromGitHub(p.repo) }, installed ? tr('common.update') : tr('plugins.install'))));
    }),
  ];
}

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
  root.replaceChildren(h('section', { id: 'plugins-detail', class: 'detail settings' }));
  if (plugins === null) refresh();
}

export function renderPlugins() {
  const el = document.getElementById('plugins-detail');
  if (!el) return;
  const head = [h('h1', {}, tr('common.plugins')), h('p', { class: 'hint-text' }, tr('plugins.extensionsByOthersNew')),
    h('div', { class: 'segmented plugin-tabs' }, TABS.map(([id, label]) => h('button', { type: 'button', class: id === tab ? 'active' : '', onclick: () => { tab = id; if (id === 'market') market = null; render(); } }, label)))];
  if (plugins === null) return el.replaceChildren(...head, h('p', { class: 'hint-text' }, tr('common.loading')));
  const body = { installed: installedTab, market: marketTab, dev: devTab, advanced: advancedTab }[tab]();
  el.replaceChildren(...head, ...body.filter(Boolean));
}
