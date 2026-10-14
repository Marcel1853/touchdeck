// Protokoll: die letzten Einträge im Speicher, alles zusätzlich in logs/touchdeck.log (wird bei 2 MB rotiert).
import fs from 'node:fs';
import path from 'node:path';

const LEVELS = { debug: 0, info: 1, warn: 2, error: 3 };
const KEEP = 1500;
const MAX_FILE = 2 * 1024 * 1024;

export class Log {
  constructor(dir, getLevel = () => 'info') {
    this.dir = dir;
    this.file = path.join(dir, 'touchdeck.log');
    this.getLevel = getLevel;
    this.entries = [];
    this.listeners = new Set();
    this.seq = 0;
    fs.mkdirSync(dir, { recursive: true });
  }

  onEntry(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  add(level, source, message) {
    if (LEVELS[level] < LEVELS[this.getLevel()]) return;
    const entry = { id: ++this.seq, time: new Date().toISOString(), level, source, message: String(message).slice(0, 2000) };
    this.entries.push(entry);
    if (this.entries.length > KEEP) this.entries.splice(0, this.entries.length - KEEP);
    this.#append(entry);
    for (const fn of this.listeners) fn(entry);
    if (level === 'error' || process.env.TOUCHDECK_DEBUG) console.log(`[${level}] ${source}: ${entry.message}`);
  }

  #append(e) {
    try {
      if (fs.existsSync(this.file) && fs.statSync(this.file).size > MAX_FILE) fs.renameSync(this.file, `${this.file}.1`);
      fs.appendFileSync(this.file, `${e.time} ${e.level.toUpperCase().padEnd(5)} [${e.source}] ${e.message}\n`);
    } catch {
      // Ein volles Laufwerk soll TouchDeck nicht lahmlegen.
    }
  }

  debug(source, msg) { this.add('debug', source, msg); }
  info(source, msg) { this.add('info', source, msg); }
  warn(source, msg) { this.add('warn', source, msg); }
  error(source, msg) { this.add('error', source, msg); }

  clear() {
    this.entries = [];
  }
}
