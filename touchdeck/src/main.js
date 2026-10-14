#!/usr/bin/env node
// Startet TouchDeck: lädt die Daten, startet den Server und öffnet den Editor im Browser.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { Store } from './core/store.js';
import { createServer } from './server/server.js';
import { openWithDefault } from './system/linux.js';

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');

const { values: args } = parseArgs({
  options: {
    port: { type: 'string', default: process.env.TOUCHDECK_PORT || '8770' },
    data: { type: 'string', default: process.env.TOUCHDECK_DATA || path.join(ROOT, 'data') },
    'no-open': { type: 'boolean', default: false },
    help: { type: 'boolean', short: 'h', default: false },
  },
});

if (args.help) {
  console.log(`TouchDeck – Makro-Fernbedienung für Linux

  --port <n>    Port des Servers (Standard 8770, oder TOUCHDECK_PORT)
  --data <dir>  Datenordner (Standard ./data, oder TOUCHDECK_DATA)
  --no-open     Editor nicht automatisch im Browser öffnen`);
  process.exit(0);
}

const port = Number.parseInt(args.port, 10);
const store = new Store(path.resolve(args.data));

const editorUrl = `http://localhost:${port}/editor/`;

try {
  await createServer(store, { port });
} catch (err) {
  if (err.code !== 'EADDRINUSE') {
    console.error(err.message);
    process.exit(1);
  }
  // Läuft TouchDeck schon (z. B. per Autostart), nur dessen Editor öffnen.
  const running = await fetch(`http://localhost:${port}/api/state`, { signal: AbortSignal.timeout(2000) }).then((r) => r.ok).catch(() => false);
  if (!running) {
    console.error(`Port ${port} ist von einem anderen Programm belegt. Anderen Port wählen: --port <n>`);
    process.exit(1);
  }
  console.log(`TouchDeck läuft bereits – öffne den Editor: ${editorUrl}`);
  if (!args['no-open']) await openWithDefault(editorUrl).catch(() => {});
  process.exit(0);
}

console.log(`TouchDeck läuft.\n  Editor:  ${editorUrl}\n  Daten:   ${store.dataDir}`);
if (!args['no-open']) openWithDefault(editorUrl).catch(() => {});

const stop = () => process.exit(0);
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
