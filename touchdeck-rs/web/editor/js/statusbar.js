// Statusleiste unten im Editor: verbundene Geräte, OBS, Twitch, Discord und die Adresse dieses PCs.
// Fragt den Stand alle paar Sekunden ab, solange das Fenster sichtbar ist.
import { t as tr } from '/shared/i18n.js';
import { $, h, api } from './dom.js';

const INTERVAL = 4000;
let timer = null;

/** Ein Eintrag: farbiger Punkt (ok, warn, error, off) und Text, Details im Tooltip. */
const chip = (state, text, title, onclick) =>
  h(onclick ? 'button' : 'span', { class: `status-chip ${state}`, title, ...(onclick && { type: 'button', onclick }) }, h('i', { class: 'dot' }), text);

function render(info, openSettings) {
  const devices = info.deviceList ?? [];
  const deviceNames = devices.map((d) => `${d.name || tr('editor.unknownDevice')} (${d.ip})${d.page ? ` – ${d.page}` : ''}`);
  const twitch = info.twitch ?? {};
  const discord = info.discord ?? {};
  const integration = (s) => (s.state === 'connected' ? 'ok' : s.state === 'error' ? 'error' : s.state === 'waiting' ? 'warn' : 'off');
  const address = info.lan?.[0] ? `${info.lan[0]}:${info.port}` : `localhost:${info.port}`;
  $('#statusbar').replaceChildren(...[
    chip(devices.length ? 'ok' : 'off',
      devices.length === 0 ? tr('editor.noDeviceConnected')
        : devices.length === 1 ? tr('editor.oneDeviceConnected', [devices[0].name || tr('editor.unknownDevice')])
          : tr('editor.devicesConnected', [devices.length]),
      deviceNames.join('\n') || tr('editor.noDeviceConnected')),
    devices.length > 1 && h('span', { class: 'status-names' }, devices.map((d) => d.name || d.ip).join(' · ')),
    h('span', { class: 'spacer' }),
    chip(info.obsConnected ? 'ok' : 'off', 'OBS', info.obsConnected ? tr('settings.connected') : tr('settings.notConnected'), openSettings),
    chip(integration(twitch), 'Twitch', twitch.state === 'connected' ? `${tr('settings.connected')}: ${twitch.displayName || twitch.login}` : twitch.error || tr('settings.notConnected'), openSettings),
    chip(discord.state === 'connected' ? (discord.running ? 'ok' : 'warn') : integration(discord), 'Discord',
      discord.state === 'connected' ? (discord.running ? tr('settings.connected') : tr('editor.discordNotRunning')) : discord.error || tr('settings.notConnected'), openSettings),
    h('span', { class: 'status-sep' }),
    h('span', { class: 'status-address', title: (info.lan ?? []).map((ip) => `http://${ip}:${info.port}`).join('\n') }, address),
    h('span', { class: 'status-version' }, `v${info.version}`),
  ].filter(Boolean));
}

async function refresh(openSettings) {
  clearTimeout(timer);
  if (document.visibilityState === 'visible') {
    try {
      render(await api('GET', '/api/info'), openSettings);
    } catch {
      $('#statusbar').replaceChildren(chip('error', tr('editor.serverNotReachable'), ''));
    }
  }
  timer = setTimeout(() => refresh(openSettings), INTERVAL);
}

/** Startet die Statusleiste. openSettings: öffnet die Einstellungen (Klick auf OBS/Twitch/Discord). */
export function startStatusBar(openSettings) {
  document.addEventListener('visibilitychange', () => refresh(openSettings));
  refresh(openSettings);
}
