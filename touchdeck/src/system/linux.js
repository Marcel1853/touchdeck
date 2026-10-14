// Anbindung an den Linux-Desktop: Programme, Medien (MPRIS), Lautstärke (PipeWire), Tastatur (X11).
import { spawn, execFile } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Im gepackten Programm liegt das Skript außerhalb des app.asar-Archivs, damit Python es lesen kann.
const HELPER = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', 'helpers', 'xinput.py').replace(/app\.asar([\\/])/, 'app.asar.unpacked$1');

export function expandHome(p) {
  return p === '~' || p.startsWith('~/') ? path.join(os.homedir(), p.slice(1)) : p;
}

export function run(cmd, args, { timeout = 10000, input } = {}) {
  return new Promise((resolve, reject) => {
    const child = execFile(cmd, args, { timeout, maxBuffer: 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) {
        const msg = (stderr || err.message).toString().trim();
        reject(new Error(err.code === 'ENOENT' ? `Programm „${cmd}“ ist nicht installiert.` : msg));
      } else resolve(stdout.toString());
    });
    if (input !== undefined) child.stdin.end(input);
  });
}

export function spawnDetached(cmd, args, opts = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { detached: true, stdio: 'ignore', ...opts });
    child.once('error', (err) => reject(new Error(err.code === 'ENOENT' ? `Programm „${cmd}“ ist nicht installiert.` : err.message)));
    child.once('spawn', () => {
      child.unref();
      resolve();
    });
  });
}

export const openWithDefault = (target) => spawnDetached('xdg-open', [target]);

export async function shell(command, { wait = false, timeout = 30000 } = {}) {
  const cwd = os.homedir();
  if (!wait) return spawnDetached('sh', ['-c', command], { cwd });
  return new Promise((resolve, reject) => {
    execFile('sh', ['-c', command], { cwd, timeout, maxBuffer: 1024 * 1024 }, (err, stdout, stderr) => {
      const out = `${stdout}${stderr}`.trim();
      if (err) reject(new Error(out || err.message));
      else resolve(out);
    });
  });
}

// ---------- Tastatur ----------

export const sendHotkey = (keys) => run('python3', ['-I', HELPER, 'hotkey', keys]);
export const typeText = (text) => run('python3', ['-I', HELPER, 'type', text], { timeout: 60000 });

// ---------- Medien über MPRIS (D-Bus) ----------

async function mprisPlayers() {
  const out = await run('gdbus', ['call', '--session', '--dest', 'org.freedesktop.DBus', '--object-path', '/org/freedesktop/DBus', '--method', 'org.freedesktop.DBus.ListNames']);
  return [...out.matchAll(/'(org\.mpris\.MediaPlayer2\.[^']+)'/g)].map((m) => m[1]);
}

async function playbackStatus(name) {
  try {
    const out = await run('gdbus', ['call', '--session', '--dest', name, '--object-path', '/org/mpris/MediaPlayer2', '--method', 'org.freedesktop.DBus.Properties.Get', 'org.mpris.MediaPlayer2.Player', 'PlaybackStatus'], { timeout: 2000 });
    return out.match(/'(\w+)'/)?.[1] ?? 'Unknown';
  } catch {
    return 'Unknown';
  }
}

const MPRIS_METHODS = { 'play-pause': 'PlayPause', next: 'Next', previous: 'Previous', stop: 'Stop', play: 'Play', pause: 'Pause' };

export async function media(op) {
  const method = MPRIS_METHODS[op];
  if (!method) throw new Error(`Unbekannte Medienaktion: ${op}`);
  const players = await mprisPlayers();
  if (players.length === 0) throw new Error('Kein Mediaplayer gefunden.');
  // Bevorzugt den Player, der gerade spielt, sonst einen pausierten.
  const states = await Promise.all(players.map(playbackStatus));
  const pick = players[states.indexOf('Playing')] ?? players[states.indexOf('Paused')] ?? players[0];
  await run('gdbus', ['call', '--session', '--dest', pick, '--object-path', '/org/mpris/MediaPlayer2', '--method', `org.mpris.MediaPlayer2.Player.${method}`]);
  return pick.replace('org.mpris.MediaPlayer2.', '').replace(/\.instance\d+$/, '');
}

// ---------- Lautstärke über PipeWire (wpctl) ----------

const SINK = '@DEFAULT_AUDIO_SINK@';
const SOURCE = '@DEFAULT_AUDIO_SOURCE@';

export async function getVolume(target = SINK) {
  const out = await run('wpctl', ['get-volume', target]);
  const m = out.match(/Volume:\s*([\d.]+)/);
  return { volume: m ? Math.round(Number(m[1]) * 100) : null, muted: out.includes('[MUTED]') };
}

export async function volume(op, { step = 5, value = 50 } = {}) {
  const s = Math.max(1, Math.min(50, Number(step) || 5));
  switch (op) {
    case 'up':
      await run('wpctl', ['set-volume', '-l', '1.0', SINK, `${s}%+`]);
      break;
    case 'down':
      await run('wpctl', ['set-volume', SINK, `${s}%-`]);
      break;
    case 'set':
      await run('wpctl', ['set-volume', SINK, `${Math.max(0, Math.min(100, Number(value) || 0))}%`]);
      break;
    case 'mute':
      await run('wpctl', ['set-mute', SINK, 'toggle']);
      break;
    default:
      throw new Error(`Unbekannte Lautstärke-Aktion: ${op}`);
  }
  return getVolume(SINK);
}

export async function mic(op) {
  const arg = { toggle: 'toggle', mute: '1', unmute: '0' }[op] ?? 'toggle';
  await run('wpctl', ['set-mute', SOURCE, arg]);
  return getVolume(SOURCE);
}

// ---------- Installierte Programme (.desktop-Dateien) ----------

function appDirs() {
  const dataHome = process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local/share');
  const dataDirs = (process.env.XDG_DATA_DIRS || '/usr/local/share:/usr/share').split(':');
  return [
    path.join(dataHome, 'applications'),
    path.join(dataHome, 'flatpak/exports/share/applications'),
    '/var/lib/flatpak/exports/share/applications',
    '/var/lib/snapd/desktop/applications',
    ...dataDirs.map((d) => path.join(d, 'applications')),
  ];
}

function parseDesktopFile(file) {
  const text = fs.readFileSync(file, 'utf8');
  const section = text.split(/^\[/m).find((s) => s.startsWith('Desktop Entry]'));
  if (!section) return null;
  const get = (key) => section.match(new RegExp(`^${key}=(.*)$`, 'm'))?.[1]?.trim();
  if (get('Type') !== 'Application' || get('NoDisplay') === 'true' || get('Hidden') === 'true') return null;
  const lang = (process.env.LANG || 'de').split(/[_.]/)[0];
  return { name: get(`Name\\[${lang}\\]`) || get('Name') || path.basename(file), icon: get('Icon') || '', comment: get(`Comment\\[${lang}\\]`) || get('Comment') || '' };
}

let appCache = null;
export function listApps() {
  if (appCache) return appCache;
  const apps = new Map();
  for (const dir of appDirs()) {
    let entries;
    try {
      entries = fs.readdirSync(dir);
    } catch {
      continue;
    }
    for (const f of entries) {
      if (!f.endsWith('.desktop') || apps.has(f)) continue;
      try {
        const info = parseDesktopFile(path.join(dir, f));
        if (info) apps.set(f, { id: f.replace(/\.desktop$/, ''), ...info });
      } catch {
        // Unlesbare Dateien überspringen.
      }
    }
  }
  appCache = [...apps.values()].sort((a, b) => a.name.localeCompare(b.name, 'de'));
  setTimeout(() => (appCache = null), 60000).unref();
  return appCache;
}

export const launchApp = (id) => spawnDetached('gtk-launch', [id]);

export const notify = (title, body) => run('notify-send', ['-a', 'TouchDeck', title || 'TouchDeck', body || '']);

/** Ist die Firewall ufw aktiv? (Dann müssen die Ports für das Handy freigegeben werden.) */
export async function firewallActive() {
  try {
    return (await run('systemctl', ['is-active', 'ufw'], { timeout: 2000 })).trim() === 'active';
  } catch {
    return false;
  }
}

/** Öffnet die Firewall fürs Heimnetz; pkexec fragt dabei grafisch nach dem Passwort. */
export async function openFirewall(port) {
  const rules = ['192.168.0.0/16', '10.0.0.0/8', '172.16.0.0/12'].flatMap((net) => [
    `ufw allow from ${net} to any port ${Number(port)} proto tcp comment TouchDeck`,
    `ufw allow from ${net} to any port 8769 proto udp comment TouchDeck`,
  ]);
  try {
    await run('pkexec', ['sh', '-c', rules.join(' && ')], { timeout: 120000 });
  } catch (err) {
    throw new Error(/dismissed|Not authorized|126|127/i.test(err.message) ? 'Abgebrochen – die Firewall wurde nicht geändert.' : err.message);
  }
}

export function lanAddresses() {
  return Object.values(os.networkInterfaces())
    .flat()
    .filter((i) => i && i.family === 'IPv4' && !i.internal)
    .map((i) => i.address);
}

// ---------- Audiogeräte ----------

/** Liest Ausgänge (sinks) oder Eingänge (sources) aus `wpctl status`. Die Nummern ändern sich nach Neustarts, gemerkt wird daher der Name. */
export async function audioDevices(kind = 'sinks') {
  const out = await run('wpctl', ['status']);
  const audio = out.split(/^Video$/m)[0];
  const block = audio.split(new RegExp(`${kind === 'sinks' ? 'Sinks' : 'Sources'}:`))[1]?.split(/[├└]─/)[0] ?? '';
  return [...block.matchAll(/^\s*│\s+(\*)?\s+(\d+)\.\s+(.+?)\s*(?:\[vol:[^\]]*\])?\s*$/gm)]
    .map((m) => ({ id: m[2], value: m[3].trim(), label: m[3].trim(), isDefault: !!m[1] }));
}

export async function setDefaultAudio(kind, target) {
  const devices = await audioDevices(kind);
  if (devices.length === 0) throw new Error('Keine Audiogeräte gefunden.');
  let next;
  if (target === 'next') {
    const i = devices.findIndex((d) => d.isDefault);
    next = devices[(i + 1) % devices.length];
  } else {
    next = devices.find((d) => d.value === String(target));
    if (!next) throw new Error('Das gewählte Audiogerät ist gerade nicht vorhanden.');
  }
  await run('wpctl', ['set-default', next.id]);
  return next.label;
}

// ---------- Systemwerte ----------

let lastCpu = null;
/** CPU-Auslastung seit dem letzten Aufruf (braucht zwei Messpunkte). */
export function cpuUsage() {
  const parts = fs.readFileSync('/proc/stat', 'utf8').split('\n')[0].trim().split(/\s+/).slice(1).map(Number);
  const idle = parts[3] + (parts[4] ?? 0);
  const total = parts.reduce((a, b) => a + b, 0);
  const prev = lastCpu;
  lastCpu = { idle, total };
  if (!prev || total === prev.total) return null;
  return Math.round(100 * (1 - (idle - prev.idle) / (total - prev.total)));
}

export function ramUsage() {
  const info = Object.fromEntries(fs.readFileSync('/proc/meminfo', 'utf8').split('\n').map((l) => l.split(/:\s+/)).filter((p) => p.length === 2).map(([k, v]) => [k, Number.parseInt(v, 10)]));
  return { percent: Math.round(100 * (1 - info.MemAvailable / info.MemTotal)), usedGb: (info.MemTotal - info.MemAvailable) / 1048576 };
}

/** CPU-Temperatur aus hwmon (k10temp, coretemp, zenpower …). */
export function cpuTemp() {
  const base = '/sys/class/hwmon';
  let dirs;
  try {
    dirs = fs.readdirSync(base);
  } catch {
    return null;
  }
  for (const d of dirs) {
    try {
      const name = fs.readFileSync(path.join(base, d, 'name'), 'utf8').trim();
      if (!['k10temp', 'coretemp', 'zenpower', 'cpu_thermal', 'acpitz'].includes(name)) continue;
      return Math.round(Number(fs.readFileSync(path.join(base, d, 'temp1_input'), 'utf8')) / 1000);
    } catch {
      // nächster Sensor
    }
  }
  return null;
}

/** Aktueller Titel des gerade spielenden Players. */
export async function currentTrack() {
  const players = await mprisPlayers();
  const states = await Promise.all(players.map(playbackStatus));
  const pick = players[states.indexOf('Playing')] ?? players[states.indexOf('Paused')];
  if (!pick) return null;
  const out = await run('gdbus', ['call', '--session', '--dest', pick, '--object-path', '/org/mpris/MediaPlayer2', '--method', 'org.freedesktop.DBus.Properties.Get', 'org.mpris.MediaPlayer2.Player', 'Metadata'], { timeout: 2000 });
  const title = out.match(/'xesam:title': <'((?:[^'\\]|\\.)*)'>/)?.[1];
  const artist = out.match(/'xesam:artist': <\['((?:[^'\\]|\\.)*)'/)?.[1];
  return { title: title ?? '', artist: artist ?? '', playing: states[players.indexOf(pick)] === 'Playing' };
}

// ---------- Desktop & System ----------

export function screenshot(mode) {
  const flags = { full: [], area: ['-a'], window: ['-w'], dialog: ['-i'] }[mode] ?? ['-i'];
  return spawnDetached('gnome-screenshot', flags);
}

export async function lockScreen() {
  try {
    await run('cinnamon-screensaver-command', ['-l']);
  } catch {
    await run('loginctl', ['lock-session']);
  }
}

export function power(op) {
  switch (op) {
    case 'suspend':
      return run('systemctl', ['suspend']);
    case 'reboot':
      return run('systemctl', ['reboot']);
    case 'poweroff':
      return run('systemctl', ['poweroff']);
    case 'logout':
      return spawnDetached('cinnamon-session-quit', ['--logout']);
    default:
      throw new Error(`Unbekannte Energie-Aktion: ${op}`);
  }
}

async function gsettingsBool(schema, key, mode) {
  const cur = (await run('gsettings', ['get', schema, key])).trim() === 'true';
  const next = mode === 'toggle' ? !cur : mode === 'on';
  await run('gsettings', ['set', schema, key, String(next)]);
  return next;
}

export const nightLight = (mode) => gsettingsBool('org.cinnamon.settings-daemon.plugins.color', 'night-light-enabled', mode);

/** Hell/Dunkel umschalten: Farbschema plus passendes Mint-Y-Thema. */
export async function darkMode(mode) {
  const theme = (await run('gsettings', ['get', 'org.cinnamon.desktop.interface', 'gtk-theme'])).trim().replace(/'/g, '');
  const isDark = /-Dark/i.test(theme);
  const dark = mode === 'toggle' ? !isDark : mode === 'on';
  const nextTheme = dark ? (isDark ? theme : theme.replace(/^(Mint-Y)(-|$)/, '$1-Dark$2')) : theme.replace(/-Dark/i, '');
  await run('gsettings', ['set', 'org.x.apps.portal', 'color-scheme', dark ? 'prefer-dark' : 'prefer-light']).catch(() => {});
  if (nextTheme !== theme) {
    await run('gsettings', ['set', 'org.cinnamon.desktop.interface', 'gtk-theme', nextTheme]);
    await run('gsettings', ['set', 'org.cinnamon.theme', 'name', nextTheme]).catch(() => {});
  }
  return dark;
}

/** Wake-on-LAN: weckt einen Rechner im Netz über sein „Magic Packet“. */
export async function wakeOnLan(mac) {
  const hex = String(mac).replace(/[^0-9a-f]/gi, '');
  if (hex.length !== 12) throw new Error('Ungültige MAC-Adresse (Beispiel: 00:11:22:33:44:55).');
  const { createSocket } = await import('node:dgram');
  const packet = Buffer.concat([Buffer.alloc(6, 0xff), ...Array(16).fill(Buffer.from(hex, 'hex'))]);
  const sock = createSocket('udp4');
  await new Promise((resolve, reject) => {
    sock.bind(() => {
      sock.setBroadcast(true);
      sock.send(packet, 9, '255.255.255.255', (err) => (sock.close(), err ? reject(err) : resolve()));
    });
  });
}

// ---------- Maus & Fenster (X11) ----------

export const mouseClick = (which) => run('python3', ['-I', HELPER, 'click', which]);
export const mouseMove = (x, y, relative) => run('python3', ['-I', HELPER, relative ? 'moverel' : 'move', String(Math.round(x)), String(Math.round(y))]);
export const holdKeys = (keys, ms) => run('python3', ['-I', HELPER, 'hold', keys, String(ms)], { timeout: 65000 });
export const activateWindow = async (query) => (await run('python3', ['-I', HELPER, 'activate', query])).trim();

/** Offene Fenster, je Programm (WM_CLASS) ein Eintrag. */
export async function openWindows() {
  const list = JSON.parse(await run('python3', ['-I', HELPER, 'windows']));
  const byClass = new Map();
  for (const w of list) {
    if (!w.class || /^(Nemo-desktop|xfdesktop|desktop_window)$/i.test(w.class) || byClass.has(w.class)) continue;
    byClass.set(w.class, { value: w.class, label: `${w.class} – ${w.title.slice(0, 50)}` });
  }
  return [...byClass.values()].sort((a, b) => a.label.localeCompare(b.label, 'de'));
}

/** Beendet ein Programm über seinen Prozessnamen (wie pkill). */
export async function killProcess(name, force) {
  if (!/^[\w .+-]{1,64}$/.test(name ?? '')) throw new Error('Ungültiger Programmname.');
  try {
    await run('pkill', [force ? '-KILL' : '-TERM', '-x', name]);
  } catch {
    throw new Error(`Kein laufendes Programm „${name}“ gefunden.`);
  }
}

// ---------- Soundboard ----------

const sounds = new Set();

/** Spielt eine Audiodatei über PipeWire ab; mehrere Sounds können gleichzeitig laufen. */
export function playSound(file, volume = 100, { exclusive = false } = {}) {
  const f = expandHome(file ?? '');
  if (!f || !fs.existsSync(f)) throw new Error(`Audiodatei nicht gefunden: ${file}`);
  if (exclusive) stopSounds();
  const child = spawn('pw-play', ['--volume', String(Math.max(0, Math.min(100, Number(volume) || 0)) / 100), f], { stdio: 'ignore' });
  sounds.add(child);
  child.on('exit', () => sounds.delete(child));
  child.on('error', () => sounds.delete(child));
}

export function stopSounds() {
  for (const child of sounds) child.kill();
  sounds.clear();
}

// ---------- Dateien ----------

export function writeFile(file, text, mode) {
  const f = expandHome(file ?? '');
  if (!f) throw new Error('Kein Dateipfad angegeben.');
  fs.mkdirSync(path.dirname(f), { recursive: true });
  if (mode === 'append') fs.appendFileSync(f, `${text}\n`);
  else fs.writeFileSync(f, text);
}

// Erster Messpunkt schon beim Start, damit die CPU-Anzeige nicht mit „…“ beginnt.
try {
  cpuUsage();
} catch {
  // /proc/stat nicht lesbar – dann eben erst beim ersten Abruf.
}
