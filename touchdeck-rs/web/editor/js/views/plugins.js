// Bereich „Plugins“: installierte Plugins, Marktplatz (öffentliche GitHub-Repos) und Testmodus für Entwickler.
import { h, api, toast, ask, prompt } from '../dom.js';
import { S, render, loadState, save } from '../state.js';

const TABS = [['installed', 'Installiert'], ['market', 'Marktplatz'], ['dev', 'Entwickeln'], ['advanced', 'Erweitert']];
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
    toast('Prüfe das Plugin auf GitHub …');
    preview = await api('POST', '/api/plugins/preview', { repo });
  } catch (err) {
    return toast(err.message, 'error');
  }
  const m = preview.manifest;
  if (preview.native && !config.advanced) {
    return ask(`⚠️ ${m.name}`, 'Dieses Plugin ist ein eigenes Programm mit vollen Rechten. Solche Plugins laufen nur im erweiterten Modus (Plugins › Erweitert), den du bewusst und auf eigene Gefahr einschalten musst.',
      [{ value: 'go', label: 'Zum erweiterten Modus', kind: 'primary' }]).then((a) => { if (a === 'go') { tab = 'advanced'; render(); } });
  }
  const perms = preview.permissions.length ? preview.permissions.map((p) => `• ${p}`).join('\n') : '• keine besonderen Rechte';
  const existing = plugins?.find((p) => p.id === m.id);
  const answer = await ask(`🧱 ${m.name} ${m.version ? `(${m.version})` : ''}`,
    `${preview.native ? '⚠️ EIGENES PROGRAMM MIT VOLLEN RECHTEN\n\n' : ''}${preview.listed ? '' : '⚠️ Nicht im Marktplatz – nicht angesehen, Installation auf eigene Gefahr.\n\n'}${m.description || 'Keine Beschreibung.'}\n\nVon: ${m.author || 'unbekannt'}\nLizenz: ${m.license || 'keine Angabe (alle Rechte beim Autor)'}\nQuelle: ${preview.url} (Stand: ${preview.ref})\n\nDas Plugin möchte:\n${perms}\n\n`
    + (preview.native ? 'Erweiterte Plugins laufen NICHT abgeschottet.' : 'Plugins laufen abgeschottet und können nur das oben Genannte. Installiere trotzdem nur Plugins, deren Quelle du vertraust.'),
    [{ value: 'install', label: existing ? 'Aktualisieren' : 'Installieren', kind: 'primary' }]);
  if (answer !== 'install') return;
  // Zweite Bestätigung für Plugins mit vollen Rechten.
  if (preview.native) {
    const sure = await ask('⚠️ Volle Rechte geben?', `„${m.name}“ ist ein eigenes Programm und kann auf deinem PC alles, was du auch kannst: Dateien lesen und löschen, Programme starten, Daten ins Internet schicken. TouchDeck kann es nicht einschränken.\n\nNur installieren, wenn du der Quelle wirklich vertraust: ${preview.url}`,
      [{ value: 'yes', label: 'Ja, volle Rechte geben', kind: 'danger' }]);
    if (sure !== 'yes') return;
  }
  const r = await call('/api/plugins/install', { repo: preview.repo, digest: preview.digest, acceptRisk: !!preview.native }, `${m.name} ${existing ? 'aktualisiert' : 'installiert'} ✓`);
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
  const status = { running: ['läuft', 'ok'], off: ['aus', ''], blocked: ['gesperrt', 'warn'] }[p.status.state] ?? ['Fehler', 'error'];
  return h('div', { class: 'settings-card plugin' },
    h('div', { class: 'plugin-head' },
      h('div', {},
        h('b', {}, m.name), m.version && h('span', { class: 'muted' }, ` ${m.version}`),
        h('span', { class: `status-badge ${status[1]}` }, status[0]),
        local && h('span', { class: 'status-badge warn' }, 'Testmodus'),
        p.native && h('span', { class: 'status-badge error' }, 'volle Rechte'),
        h('p', { class: 'hint-text' }, m.description || ''),
        h('p', { class: 'hint-text' }, [m.author && `Von ${m.author}`, m.license && `Lizenz: ${m.license}`, local ? `Ordner: ${p.source.slice(6)}` : null].filter(Boolean).join(' · '),
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
    !market.note && !market.plugins.length && h('p', { class: 'hint-text' }, 'Im Marktplatz sind noch keine Plugins. Wie du deins einträgst, steht im ',
      h('a', { href: 'https://github.com/Marcel1853/touchdeck-plugins/wiki/Veröffentlichen', target: '_blank', rel: 'noopener' }, 'Plugin-Wiki'), '.'),
    ...market.plugins.map((p) => {
      const installed = plugins.some((x) => x.source === `github:${p.repo}`);
      return h('div', { class: 'settings-card plugin' },
        h('div', { class: 'plugin-head' },
          h('div', {}, h('b', {}, p.name || p.repo), h('p', { class: 'hint-text' }, p.description),
            h('p', { class: 'hint-text' }, p.author && `Von ${p.author} · `, p.license && `Lizenz: ${p.license} · `, h('a', { href: `https://github.com/${p.repo}`, target: '_blank', rel: 'noopener' }, p.repo))),
          h('button', { class: `btn small ${installed ? '' : 'primary'}`, onclick: () => installFromGitHub(p.repo) }, installed ? 'Aktualisieren' : 'Installieren')));
    }),
  ];
}

async function setAdvanced(on) {
  if (on) {
    const first = await ask('⚠️ Erweiterter Modus', 'Im erweiterten Modus dürfen Plugins eigene Programme mitbringen. Diese laufen mit vollen Rechten: Sie können Dateien lesen und löschen, Programme starten und Daten ins Internet schicken. TouchDeck kann sie nicht einschränken oder prüfen.\n\nDas geschieht auf eigene Gefahr. Installiere nur Plugins, deren Quelltext du dir angesehen hast oder deren Entwickler du vertraust.',
      [{ value: 'next', label: 'Verstanden, weiter', kind: 'danger' }]);
    if (first !== 'next') return render();
    const typed = await prompt('Zweite Bestätigung', `Zum Einschalten bitte „${config.confirm}“ eintippen:`);
    if (typed === null) return render();
    if (typed.trim() !== config.confirm) {
      toast('Nicht eingeschaltet – der Text stimmt nicht.', 'error');
      return render();
    }
    const r = await call('/api/plugins/advanced', { enabled: true, confirm: typed }, 'Erweiterter Modus eingeschaltet');
    if (r) config.advanced = r.advanced;
  } else {
    const r = await call('/api/plugins/advanced', { enabled: false }, 'Erweiterter Modus ausgeschaltet – Plugins mit vollen Rechten sind gestoppt.');
    if (r) config.advanced = r.advanced;
  }
  render();
}

function advancedTab() {
  const natives = plugins.filter((p) => p.native);
  return [
    h('div', { class: `settings-card ${config.advanced ? 'danger-card' : ''}` },
      h('h3', {}, 'Erweiterte Plugins (volle Rechte)'),
      h('p', {}, 'Normale Plugins laufen abgeschottet. Erweiterte Plugins sind eigene Programme (z. B. in Python oder Node.js), die TouchDeck startet – so mächtig wie Plugins anderer Makro- und Streaming-Programme, aber damit eben auch in der Lage, Schaden anzurichten.'),
      h('p', { class: 'hint-text' }, 'Auch erweiterte Plugins gibt es nur aus öffentlichen GitHub-Repos (oder im Testmodus aus einem eigenen Ordner). Jedes einzelne muss beim Installieren noch einmal bestätigt werden.'),
      h('label', { class: 'switch-row' },
        h('input', { type: 'checkbox', checked: config.advanced, onchange: (e) => setAdvanced(e.target.checked) }),
        config.advanced ? 'Erweiterter Modus ist eingeschaltet (auf eigene Gefahr)' : 'Erweiterten Modus einschalten …')),
    natives.length > 0 && h('p', { class: 'hint-text' }, `${natives.length} erweiterte(s) Plugin(s) installiert${config.advanced ? '' : ' – gesperrt, solange der Modus aus ist'}.`),
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
      h('p', { class: 'hint-text' }, 'Ein Ordner (bzw. ein öffentliches GitHub-Repo) mit plugin.json und plugin.js. Die vollständige Anleitung mit Beispielen für normale und erweiterte Plugins steht im ',
        h('a', { href: 'https://github.com/Marcel1853/touchdeck-plugins/wiki', target: '_blank', rel: 'noopener' }, 'Plugin-Wiki'), ', die Beispiele zum Kopieren unter ',
        h('a', { href: 'https://github.com/Marcel1853/touchdeck-plugins/tree/main/examples', target: '_blank', rel: 'noopener' }, 'touchdeck-plugins/examples'), '.'),
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
  const body = { installed: installedTab, market: marketTab, dev: devTab, advanced: advancedTab }[tab]();
  el.replaceChildren(...head, ...body.filter(Boolean));
}
