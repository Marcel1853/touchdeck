// Läuft als eigener Prozess neben dem Fenster: Server, Live-Abfragen und Aktionen
// können so das Editor-Fenster nie ausbremsen.
import { Store } from '../src/core/store.js';
import { createServer } from '../src/server/server.js';

const port = Number.parseInt(process.env.TOUCHDECK_PORT, 10) || 8770;
const report = (msg) => process.parentPort.postMessage(msg);

try {
  const store = new Store(process.env.TOUCHDECK_DATA);
  // Kopplungsanfrage vom Handy: das Hauptprogramm holt das Fenster nach vorn.
  await createServer(store, { port, onPairRequest: (r) => report({ type: 'pair-request', ...r }) });
  report({ type: 'ready', port, dataDir: store.dataDir });
} catch (err) {
  report({ type: 'error', code: err.code, message: err.message });
  process.exit(1);
}
