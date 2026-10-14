// Sicherungen der Konfiguration (Seiten, Werte, Vorlagen, Events, Einstellungen) im Ordner backups/.
import fs from 'node:fs';
import path from 'node:path';

const PATTERN = /^touchdeck-(\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2})(?:-(.+))?\.json$/;
const DAY = 24 * 60 * 60 * 1000;

export class Backups {
  constructor(store, log) {
    this.store = store;
    this.log = log;
    this.dir = path.join(store.dataDir, 'backups');
    fs.mkdirSync(this.dir, { recursive: true });
  }

  list() {
    return fs.readdirSync(this.dir)
      .filter((f) => PATTERN.test(f))
      .map((name) => {
        const st = fs.statSync(path.join(this.dir, name));
        return { name, size: st.size, time: st.mtime.toISOString(), note: name.match(PATTERN)[2]?.replace(/-/g, ' ') ?? '' };
      })
      .sort((a, b) => b.time.localeCompare(a.time));
  }

  create(note = '') {
    const d = new Date();
    const two = (n) => String(n).padStart(2, '0');
    const stamp = `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}_${two(d.getHours())}-${two(d.getMinutes())}-${two(d.getSeconds())}`;
    const suffix = note ? `-${note.toLowerCase().replace(/[^a-z0-9äöüß]+/g, '-').replace(/^-|-$/g, '').slice(0, 30)}` : '';
    const name = `touchdeck-${stamp}${suffix}.json`;
    fs.writeFileSync(path.join(this.dir, name), JSON.stringify(this.store.config, null, 2), { mode: 0o600 });
    this.prune();
    this.log.info('Backup', `Sicherung angelegt: ${name}`);
    return name;
  }

  /** Nur die neuesten N automatischen Sicherungen behalten; benannte bleiben. */
  prune() {
    const keep = this.store.settings.backups.keep;
    const auto = this.list().filter((b) => b.note === 'automatisch');
    for (const b of auto.slice(keep)) fs.rmSync(path.join(this.dir, b.name), { force: true });
  }

  /** Einmal am Tag automatisch sichern, wenn eingeschaltet. */
  autoBackup() {
    if (!this.store.settings.backups.auto) return;
    const last = this.list().find((b) => b.note === 'automatisch');
    if (!last || Date.now() - Date.parse(last.time) > DAY) this.create('automatisch');
  }

  read(name) {
    if (!PATTERN.test(name)) throw Object.assign(new Error('Unbekannte Sicherung.'), { status: 404 });
    const file = path.join(this.dir, name);
    if (!fs.existsSync(file)) throw Object.assign(new Error('Unbekannte Sicherung.'), { status: 404 });
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  }

  restore(nameOrConfig) {
    const cfg = typeof nameOrConfig === 'string' ? this.read(nameOrConfig) : nameOrConfig;
    if (!cfg || !Array.isArray(cfg.pages)) throw Object.assign(new Error('Das ist keine TouchDeck-Sicherung.'), { status: 400 });
    this.create('vor Wiederherstellung');
    this.store.restore(cfg);
    this.log.info('Backup', `Wiederhergestellt: ${typeof nameOrConfig === 'string' ? nameOrConfig : 'hochgeladene Datei'}`);
  }

  remove(name) {
    if (!PATTERN.test(name)) throw Object.assign(new Error('Unbekannte Sicherung.'), { status: 404 });
    fs.rmSync(path.join(this.dir, name), { force: true });
  }
}
