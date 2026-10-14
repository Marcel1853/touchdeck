// Erzeugt Seiten mit allen installierten Steam-Spielen (Coverbild, Start-Aktion, „läuft“-Anzeige).
import fs from 'node:fs';
import path from 'node:path';
import { newId, sanitizePage } from '../core/store.js';
import { listGames, coverImage } from './steam.js';

const COLS = 6;
const MAX_ROWS = 4;

const action = (type, params) => ({ id: newId('a_'), type, enabled: true, params });

function gameButton(game, row, col, iconDir) {
  const src = coverImage(game.value);
  let icon = { type: 'emoji', value: '🎮' };
  if (src) {
    const name = `steam_${game.value}${path.extname(src)}`;
    fs.copyFileSync(src, path.join(iconDir, name));
    icon = { type: 'image', value: name };
  }
  return {
    row, col, title: game.label, icon,
    iconFit: src ? 'cover' : 'contain',
    titlePos: src ? 'hidden' : 'bottom',
    color: '#1b2838', colorOn: '#4c8a2f',
    actions: [action('steam_game', { game: game.value })],
    live: { source: 'steam_running' },
  };
}

/** reuseIds: IDs vorhandener Steam-Seiten, damit Verweise darauf gültig bleiben. */
export function buildSteamPages(iconDir, reuseIds = []) {
  const games = listGames();
  if (games.length === 0) throw Object.assign(new Error('Keine Steam-Spiele gefunden.'), { status: 404 });
  // Letzte Zeile jeder Seite: Navigation und Steam-Funktionen.
  const perPage = COLS * (MAX_ROWS - 1);
  const chunks = [];
  for (let i = 0; i < games.length; i += perPage) chunks.push(games.slice(i, i + perPage));
  const ids = chunks.map((_, i) => reuseIds[i] ?? newId('p_'));

  return chunks.map((chunk, n) => {
    const rows = Math.ceil(chunk.length / COLS) + 1;
    const last = rows - 1;
    const buttons = chunk.map((g, i) => gameButton(g, Math.floor(i / COLS), i % COLS, iconDir));
    const tool = (col, title, emoji, actions, extra = {}) => buttons.push({ row: last, col, title, icon: { type: 'emoji', value: emoji }, color: '#2a475e', actions, ...extra });
    tool(0, 'Bibliothek', '📚', [action('steam_open', { view: 'library' })]);
    tool(1, 'Downloads', '⬇️', [action('steam_open', { view: 'downloads' })], { live: { source: 'steam_updates' }, colorOn: '#a3501d' });
    tool(2, 'Big Picture', '📺', [action('steam_open', { view: 'bigpicture' })]);
    tool(3, 'Freunde', '👥', [action('steam_open', { view: 'friends' })]);
    if (n > 0) tool(4, 'Zurück', '◀️', [action('go_page', { page: ids[n - 1] })]);
    if (n < chunks.length - 1) tool(5, 'Weiter', '▶️', [action('go_page', { page: ids[n + 1] })]);
    return sanitizePage({
      id: ids[n],
      name: chunks.length > 1 ? `Steam-Spiele ${n + 1}` : 'Steam-Spiele',
      rows,
      cols: COLS,
      background: '#171a21',
      template: 'steam',
      buttons,
    });
  });
}
