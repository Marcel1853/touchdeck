// Übernimmt Seiten aus Touch Portal (~/.config/TouchPortal/pages/*.tml).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { newId, sanitizePage } from '../core/store.js';

export const DEFAULT_TP_DIR = path.join(os.homedir(), '.config', 'TouchPortal');

// Java-AWT-Tastencodes (KeyEvent.VK_*) → Tastennamen von TouchDeck.
const AWT_KEYS = {
  8: 'backspace', 9: 'tab', 10: 'enter', 16: 'shift', 17: 'ctrl', 18: 'alt', 19: 'pause', 20: 'capslock',
  27: 'esc', 32: 'space', 33: 'pageup', 34: 'pagedown', 35: 'end', 36: 'home', 37: 'left', 38: 'up',
  39: 'right', 40: 'down', 44: 'comma', 45: 'minus', 46: 'period', 127: 'delete', 144: 'numlock',
  145: 'scrolllock', 154: 'print', 155: 'insert', 157: 'super', 521: 'plus', 524: 'super', 525: 'menu',
  65406: 'altgr',
};
const awtKey = (code) => {
  if (code >= 65 && code <= 90) return String.fromCharCode(code + 32);
  if (code >= 48 && code <= 57) return String.fromCharCode(code);
  if (code >= 112 && code <= 123) return `f${code - 111}`;
  if (code >= 61440 && code <= 61451) return `f${code - 61427}`;
  return AWT_KEYS[code];
};

// Java speichert Farben als vorzeichenbehaftete ARGB-Ganzzahl.
const argbToHex = (n, fallback) => (Number.isInteger(n) ? `#${((n >>> 0) & 0xffffff).toString(16).padStart(6, '0')}` : fallback);

function convertAction(a, pageIdByName) {
  const act = (type, params) => ({ id: newId('a_'), type, enabled: a.kIEn !== false, params });
  switch (a.KEY_TYPE) {
    case 'OPEN_URL_ACTION':
      return act('open_url', { url: a.KEY_URL ?? '' });
    case 'OPEN_FILE_ACTION':
      return act('open_path', { path: a.KEY_FP ?? '' });
    case 'OPEN_FOLDER_ACTION':
      return act('open_path', { path: a.KEY_PATH ?? '' });
    case 'START_APPLICATION_ACTION': {
      const cmd = [a.KEY_PATH, a.KEY_COMMANDLINE].filter(Boolean).map((s) => (s.includes(' ') && s === a.KEY_PATH ? `"${s}"` : s)).join(' ');
      return act('run_command', { command: cmd, wait: false });
    }
    case 'START_BATCH_SCRIPT_ACTION':
      return act('run_command', { command: [a.kP && `"${a.kP}"`, a.kC].filter(Boolean).join(' '), wait: false });
    case 'KEY_TEXT_WRITE':
      return act('type_text', { text: a.KEY_TEXT_TO_WRITE ?? '' });
    case 'KEY_PRESS_ACTION': {
      const keys = (a.KEY_KEYCODES ?? []).map(awtKey);
      if (keys.length && keys.every(Boolean)) return act('hotkey', { keys: keys.join('+') });
      break;
    }
    case 'GOTO_PAGE_ACTION': {
      const name = String(a.KEY_PAGE_NAME ?? '').replace(/\.tml$/, '').split(/[\\/]/).pop();
      if (name === '(Previous Page)') return act('go_page', { page: '__back' });
      const id = pageIdByName.get(name);
      if (id) return act('go_page', { page: id });
      break;
    }
    default:
  }
  return act('unsupported', { original: String(a.KEY_TYPE ?? 'unbekannt') });
}

function iconFor(tpButton, tpDir, iconDir, copied) {
  const name = tpButton.I;
  if (!name || typeof name !== 'string') return { type: 'emoji', value: '' };
  const src = path.join(tpDir, 'icons', path.basename(name));
  if (!fs.existsSync(src)) return { type: 'emoji', value: '' };
  const target = `tp_${path.basename(name)}`;
  if (!copied.has(target)) {
    fs.copyFileSync(src, path.join(iconDir, target));
    copied.add(target);
  }
  return { type: 'image', value: target };
}

const displayName = (name) => (name === '(main)' ? 'Hauptseite (Touch Portal)' : name);

function findPageFiles(dir) {
  const result = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.tml') && e.name !== 'null.tml') result.push(p);
    }
  };
  walk(dir);
  return result;
}

/** Liest alle Touch-Portal-Seiten und liefert TouchDeck-Seiten samt Statistik.
 * existingIds: Seitenname → ID bereits importierter Seiten, damit ein erneuter Import sie ersetzt. */
export function importTouchPortal(tpDir, iconDir, existingIds = new Map()) {
  const pagesDir = path.join(tpDir, 'pages');
  if (!fs.existsSync(pagesDir)) throw new Error(`Kein Touch-Portal-Ordner gefunden unter ${pagesDir}`);
  const raw = findPageFiles(pagesDir).map((file) => {
    const name = path.basename(file, '.tml');
    return { name, id: existingIds.get(displayName(name)) ?? newId('p_'), json: JSON.parse(fs.readFileSync(file, 'utf8')) };
  });
  const pageIdByName = new Map(raw.map((r) => [r.name, r.id]));
  const copied = new Set();
  const stats = { pages: 0, buttons: 0, actions: 0, unsupported: new Map() };

  const pages = raw.map(({ name, id, json }) => {
    const cols = json.KEY_COLUMNS ?? 4;
    const rows = json.KEY_ROWS ?? 2;
    const buttons = [];
    // BUTTONS[x][y]: x ist die Spalte, y die Zeile.
    (json.BUTTONS ?? []).forEach((column, x) => (column ?? []).forEach((b, y) => {
      if (!b || !b.id || x >= cols || y >= rows) return;
      const actions = (b.A ?? []).map((a) => convertAction(a, pageIdByName));
      for (const a of actions) {
        stats.actions++;
        if (a.type === 'unsupported') stats.unsupported.set(a.params.original, (stats.unsupported.get(a.params.original) ?? 0) + 1);
      }
      buttons.push({
        id: newId('b_'),
        row: y,
        col: x,
        rowSpan: b.ROWS ?? 1,
        colSpan: b.COLS ?? 1,
        title: b.T ?? '',
        icon: iconFor(b, tpDir, iconDir, copied),
        color: argbToHex(b.BG, '#2b2f3a'),
        textColor: argbToHex(b.TC, '#ffffff'),
        fontSize: b.TS > 0 ? b.TS : 14,
        titlePos: ({ 1: 'top', 2: 'bottom', 3: 'center' })[b.TP] ?? 'bottom',
        actions,
      });
      stats.buttons++;
    }));
    stats.pages++;
    return sanitizePage({
      id,
      name: displayName(name),
      template: 'touchportal',
      rows,
      cols,
      background: argbToHex(json.BG, '#14161b'),
      buttons,
    });
  });

  return {
    pages,
    stats: { ...stats, icons: copied.size, unsupported: Object.fromEntries(stats.unsupported) },
  };
}
