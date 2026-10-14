// Bereich „Plugins“: installierte Plugins, Marktplatz (öffentliche GitHub-Repos) und Testmodus für Entwickler.
import { h, api, toast, ask } from '../dom.js';
import { S, render, loadState, save } from '../state.js';

const TABS = [['installed', 'Installiert'], ['market', 'Marktplatz'], ['dev', 'Entwickeln']];
let tab = 'installed';
let plugins = null;
let market = null;
let available = true;

async function refresh() {
  try {
    plugins = await api('GET', '/api/plugins');
  } catch {
    available = false;
    plugins = [];
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
    toast('Prüfe das Plugin auf GitHub …');
    preview = await api('POST', '/api/plugins/preview', { repo });
  } catch (err) {
    return toast(err.message, 'error');
  }
  const m = preview.manifest;
  const perms = preview.permissions.length ? preview.permissions.map((p) => `• ${p}`).join('\n') : '• keine besonderen Rechte';
  const existing = plugins?.find((p) => p.id === m.id);
  const answer = await ask(`🧱 ${m.name} ${m.version ? `(${m.version})` : ''}`,
    `${m.description || 'Keine Beschreibung.'}\n\nVon: ${m.author || 'unbekannt'}\nQuelle: ${preview.url} (Stand: ${preview.ref})\n\nDas Plugin möchte:\n${perms}\n\n`
    + 'Plugins laufen abgeschottet und können nur das oben Genannte. Installiere trotzdem nur Plugins, deren Quelle du vertraust.',
    [{ value: 'install', label: existing ? 'Aktualisieren' : 'Installieren', kind: 'primary' }]);
  if (answer !== 'install') return;
  const r = await call('/api/plugins/install', { repo: preview.repo, digest: preview.digest }, `${m.name} ${existing ? 'aktualisiert' : 'installiert'} ✓`);
  if (r) tab = 'installed';
  render();
}

function settingsForm(p) {
  const fields = p.manifest.settings ?? [];
  if (!fields.length) return null;
  const values = { ...(p.settings ?? {}) };
  return h('div', { class: 'plugin-settings' },
    h('h3', {}, 'Einstellungen'),
    fields.map((f) => {
      const v = values[f.key] ?? f.default ?? '';
      if (f.type === 'checkbox') {
        return h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked: v === true || v === 'true', onchange: (e) => (values[f.key] = e.target.checked) }), f.label || f.key);
      }
      return h('label', { class: 'field' }, f.label || f.key,
        h('input', { type: f.type === 'password' ? 'password' : f.type === 'number' ? 'number' : 'text', value: v, placeholder: f.placeholder ?? '', oninput: (e) => (values[f.key] = e.target.value) }));
    }),
    h('button', { class: 'btn small primary', onclick: () => call('/api/plugins/settings', { id: p.id, settings: values }, 'Einstellungen gespeichert ✓') }, 'Speichern'));
}

function pluginCard(p) {
  const m = p.manifest;
  const local = String(p.source ?? '').startsWith('local:');
  const repo = String(p.source ?? '').replace(/^github:/, '');
  const status = p.status.state === 'running' ? ['läuft', 'ok'] : p.status.state === 'off' ? ['aus', ''] : ['Fehler', 'error'];
  return h('div', { class: 'settings-card plugin' },
    h('div', { class: 'plugin-head' },
      h('div', {},
        h('b', {}, m.name), m.version && h('span', { class: 'muted' }, ` ${m.version}`),
        h('span', { class: `status-badge ${status[1]}` }, status[0]),
        local && h('span', { class: 'status-badge warn' }, 'Testmodus'),
        h('p', { class: 'hint-text' }, m.description || ''),
        h('p', { class: 'hint-text' }, [m.author && `Von ${m.author}`, local ? `Ordner: ${p.source.slice(6)}` : null].filter(Boolean).join(' · '),
          !local && p.url && h('span', {}, m.author ? ' · ' : '', h('a', { href: p.url, target: '_blank', rel: 'noopener' }, `${repo} @ ${p.ref}`)))),
      h('label', { class: 'switch-row' }, h('input', { type: 'checkbox', checked: p.enabled,
        onchange: (e) => call('/api/plugins/enable', { id: p.id, enabled: e.target.checked }) }), 'an')),
    p.status.error && h('p', { class: 'hint-text error' }, p.status.error),
    h('p', { class: 'hint-text' }, 'Darf: ', p.permissions.length ? p.permissions.join(' · ') : 'nichts Besonderes'),
    h('p', { class: 'hint-text' }, `${m.actions.length} Aktion(en), ${m.triggers.length} Auslöser – zu finden im Aktions-Editor unter „Plugins › ${m.name}“.`),
    settingsForm(p),
    h('div', { class: 'pack-actions' },
      local
        ? h('button', { class: 'btn small', onclick: () => call('/api/plugins/reload', { id: p.id }, 'Neu geladen ✓') }, '↻ Neu laden')
        : h('button', { class: 'btn small', onclick: () => installFromGitHub(repo) }, '⟳ Nach Update suchen'),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn small ghost danger', onclick: async () => {
        const a = await ask('Plugin entfernen?', `„${m.name}“ wird entfernt. Buttons mit Aktionen dieses Plugins melden danach einen Fehler.`, [{ value: 'remove', label: 'Entfernen', kind: 'danger' }]);
        if (a === 'remove') call('/api/plugins/remove', { id: p.id }, 'Plugin entfernt');
      } }, '🗑️ Entfernen')));
}

function installedTab() {
  if (!plugins.length) return [h('div', { class: 'settings-card' }, h('p', {}, 'Noch keine Plugins installiert. Im Marktplatz findest du welche – oder gib ein GitHub-Repo an.'))];
  return plugins.map(pluginCard);
}

function repoInput() {
  let repo = '';
  return h('div', { class: 'settings-card' },
    h('h3', {}, 'Aus GitHub installieren'),
    h('p', { class: 'hint-text' }, 'Plugins müssen öffentlich auf GitHub liegen – so kann jeder den Quelltext prüfen. Installiert wird die neueste Veröffentlichung (Release), sonst der Hauptzweig.'),
    h('div', { class: 'row' },
      h('input', { placeholder: 'besitzer/repo oder https://github.com/…', oninput: (e) => (repo = e.target.value), onkeydown: (e) => e.key === 'Enter' && installFromGitHub(repo) }),
      h('button', { class: 'btn primary small', onclick: () => installFromGitHub(repo) }, 'Prüfen')));
}

function marketTab() {
  if (market === null) {
    api('GET', '/api/plugins/market').then((r) => (market = r)).catch((err) => (market = { plugins: [], note: err.message })).then(render);
    return [repoInput(), h('p', { class: 'hint-text' }, 'Marktplatz wird geladen …')];
  }
  return [
    repoInput(),
    market.note && h('p', { class: 'hint-text' }, market.note),
    ...market.plugins.map((p) => {
      const installed = plugins.some((x) => x.source === `github:${p.repo}`);
      return h('div', { class: 'settings-card plugin' },
        h('div', { class: 'plugin-head' },
          h('div', {}, h('b', {}, p.name || p.repo), h('p', { class: 'hint-text' }, p.description),
            h('p', { class: 'hint-text' }, p.author && `Von ${p.author} · `, h('a', { href: `https://github.com/${p.repo}`, target: '_blank', rel: 'noopener' }, p.repo))),
          h('button', { class: `btn small ${installed ? '' : 'primary'}`, onclick: () => installFromGitHub(p.repo) }, installed ? 'Aktualisieren' : 'Installieren')));
    }),
  ];
}

function devTab() {
  let path = '';
  return [
    h('div', { class: 'settings-card' },
      h('h3', {}, 'Plugin aus einem Ordner testen'),
      h('p', { class: 'hint-text' }, 'Zum Entwickeln: Ein Plugin direkt aus einem Ordner auf diesem PC laden, bevor es auf GitHub veröffentlicht ist. Nach Änderungen auf „Neu laden“ klicken. Es gelten dieselben Grenzen wie für installierte Plugins.'),
      h('div', { class: 'row' },
        h('input', { placeholder: '~/mein-plugin', oninput: (e) => (path = e.target.value), onkeydown: (e) => e.key === 'Enter' && call('/api/plugins/dev', { path }, 'Plugin im Testmodus geladen ✓').then((r) => r && ((tab = 'installed'), render())) }),
        h('button', { class: 'btn primary small', onclick: () => call('/api/plugins/dev', { path }, 'Plugin im Testmodus geladen ✓').then((r) => r && ((tab = 'installed'), render())) }, 'Laden'))),
    h('div', { class: 'settings-card' },
      h('h3', {}, 'So sieht ein Plugin aus'),
      h('p', { class: 'hint-text' }, 'Ein Ordner (bzw. ein öffentliches GitHub-Repo) mit plugin.json und plugin.js. Ein vollständiges Beispiel liegt im TouchDeck-Quelltext unter touchdeck-rs/plugins/beispiel.'),
      h('pre', { class: 'code' }, `// plugin.js
td.action('gruessen', (params, info) => \`Hallo \${params.name}!\`);   // Aktion
td.every(60000, () => td.values.set('Uhr', new Date().toLocaleTimeString()));  // Zeitgeber
td.fire('fertig', { Name: 'x' });            // eigener Auslöser für Events
td.http({ url: 'https://api.example.com/x' }).json();   // nur erlaubte Adressen
td.values.get('Name'); td.notify('Titel', 'Text'); td.log('…'); td.settings.apiKey`)),
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
  const head = [h('h1', {}, 'Plugins'), h('p', { class: 'hint-text' }, 'Erweiterungen von anderen: neue Aktionen und Auslöser. Plugins laufen abgeschottet und dürfen nur, was sie beim Installieren angeben.'),
    h('div', { class: 'segmented plugin-tabs' }, TABS.map(([id, label]) => h('button', { type: 'button', class: id === tab ? 'active' : '', onclick: () => { tab = id; if (id === 'market') market = null; render(); } }, label)))];
  if (plugins === null) return el.replaceChildren(...head, h('p', { class: 'hint-text' }, 'Wird geladen …'));
  if (!available) return el.replaceChildren(...head, h('div', { class: 'settings-card' }, h('p', {}, 'Plugins gibt es nur in der Rust-Fassung von TouchDeck.')));
  const body = { installed: installedTab, market: marketTab, dev: devTab }[tab]();
  el.replaceChildren(...head, ...body.filter(Boolean));
}
