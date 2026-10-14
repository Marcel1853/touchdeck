// Steam: installierte Spiele aus allen Bibliotheken lesen, Coverbilder finden, Updates und laufende Spiele erkennen.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { openWithDefault } from '../system/linux.js';

const STEAM_ROOTS = [
  path.join(os.homedir(), '.local/share/Steam'),
  path.join(os.homedir(), '.steam/steam'),
  path.join(os.homedir(), '.var/app/com.valvesoftware.Steam/.local/share/Steam'),
];

// Werkzeuge, die Steam als „App“ führt, aber keine Spiele sind.
const NOT_GAMES = /^(Proton|Steam Linux Runtime|Steamworks Common|Steam VR|SteamVR)/i;

// StateFlags im App-Manifest: Bit 2 = Update nötig, Bit 1024 = Update läuft.
const FLAG_UPDATE_REQUIRED = 2;
const FLAG_UPDATE_RUNNING = 1024;

function steamRoot() {
  return STEAM_ROOTS.find((r) => fs.existsSync(path.join(r, 'steamapps', 'libraryfolders.vdf')));
}

function libraryPaths() {
  const root = steamRoot();
  if (!root) return [];
  const paths = new Set([fs.realpathSync(root)]);
  const vdf = fs.readFileSync(path.join(root, 'steamapps', 'libraryfolders.vdf'), 'utf8');
  for (const m of vdf.matchAll(/"path"\s+"([^"]+)"/g)) paths.add(m[1]);
  return [...paths];
}

/** Alle installierten Apps inkl. Werkzeuge, mit Update-Status. */
function allApps() {
  const apps = new Map();
  for (const lib of libraryPaths()) {
    const dir = path.join(lib, 'steamapps');
    let files;
    try {
      files = fs.readdirSync(dir).filter((f) => /^appmanifest_\d+\.acf$/.test(f));
    } catch {
      continue; // Bibliothek auf nicht eingehängtem Laufwerk
    }
    for (const f of files) {
      try {
        const text = fs.readFileSync(path.join(dir, f), 'utf8');
        const appid = text.match(/"appid"\s+"(\d+)"/)?.[1];
        const name = text.match(/"name"\s+"([^"]+)"/)?.[1];
        const flags = Number(text.match(/"StateFlags"\s+"(\d+)"/)?.[1] ?? 0);
        if (appid && name) apps.set(appid, { appid, name, flags, isGame: !NOT_GAMES.test(name) });
      } catch {
        // Defekte Manifeste überspringen.
      }
    }
  }
  return [...apps.values()];
}

export function listGames() {
  return allApps()
    .filter((a) => a.isGame)
    .map((a) => ({ value: a.appid, label: a.name }))
    .sort((a, b) => a.label.localeCompare(b.label, 'de'));
}

export function updateStatus() {
  const apps = allApps();
  return {
    pending: apps.filter((a) => a.flags & FLAG_UPDATE_REQUIRED).map((a) => a.name),
    running: apps.filter((a) => a.flags & FLAG_UPDATE_RUNNING).map((a) => a.name),
  };
}

/** Coverbild aus dem Steam-Cache (neues Layout: Unterordner je Spiel, altes: appid_library_600x900.jpg). */
export function coverImage(appid) {
  const root = steamRoot();
  if (!root) return null;
  const cache = path.join(root, 'appcache', 'librarycache');
  const wanted = ['library_capsule.jpg', 'library_600x900.jpg', 'library_header.jpg', 'header.jpg'];
  const dir = path.join(cache, String(appid));
  if (fs.existsSync(dir)) {
    const found = new Map();
    const walk = (d) => {
      for (const e of fs.readdirSync(d, { withFileTypes: true })) {
        if (e.isDirectory()) walk(path.join(d, e.name));
        else if (wanted.includes(e.name) && !found.has(e.name)) found.set(e.name, path.join(d, e.name));
      }
    };
    walk(dir);
    for (const w of wanted) if (found.has(w)) return found.get(w);
  }
  for (const w of wanted) {
    const old = path.join(cache, `${appid}_${w}`);
    if (fs.existsSync(old)) return old;
  }
  return null;
}

/** App-IDs der gerade laufenden Spiele (Steam startet sie über „reaper SteamLaunch AppId=…“). */
export function runningGames() {
  const ids = new Set();
  for (const pid of fs.readdirSync('/proc')) {
    if (!/^\d+$/.test(pid)) continue;
    try {
      const cmd = fs.readFileSync(`/proc/${pid}/cmdline`, 'utf8');
      const m = cmd.match(/SteamLaunch\0AppId=(\d+)/);
      if (m) ids.add(m[1]);
    } catch {
      // Prozess bereits beendet oder fremder Benutzer
    }
  }
  return ids;
}

export function runGame(appid) {
  if (!/^\d+$/.test(String(appid))) throw new Error('Kein Spiel ausgewählt.');
  return openWithDefault(`steam://rungameid/${appid}`);
}

const STEAM_VIEWS = {
  library: 'steam://open/games',
  downloads: 'steam://open/downloads',
  bigpicture: 'steam://open/bigpicture',
  friends: 'steam://open/friends',
  store: 'steam://store',
  screenshots: 'steam://open/screenshots',
};

export const openSteam = (what) => openWithDefault(STEAM_VIEWS[what] ?? STEAM_VIEWS.library);
