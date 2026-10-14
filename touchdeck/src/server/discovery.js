// Automatisches Finden im WLAN: Die Handy-App ruft per UDP-Broadcast, TouchDeck antwortet mit Name und Port.
// Der Kopplungsschlüssel wird dabei nie verschickt – den gibt es erst nach Bestätigung am PC.
import dgram from 'node:dgram';
import os from 'node:os';

export const DISCOVERY_PORT = 8769;
const QUESTION = 'TOUCHDECK_DISCOVER';

export function startDiscovery({ port, version, enabled, log }) {
  const sock = dgram.createSocket({ type: 'udp4', reuseAddr: true });
  sock.on('message', (msg, rinfo) => {
    if (!enabled() || msg.toString().trim() !== QUESTION) return;
    const answer = JSON.stringify({ app: 'touchdeck', name: os.hostname(), port, version });
    sock.send(answer, rinfo.port, rinfo.address);
  });
  sock.on('error', (err) => {
    log.warn('Suche', `Automatisches Finden nicht verfügbar: ${err.message}`);
    sock.close();
  });
  sock.bind(DISCOVERY_PORT, () => log.debug('Suche', `Antworte auf Suchanfragen (UDP ${DISCOVERY_PORT})`));
  return () => {
    try {
      sock.close();
    } catch {
      // schon geschlossen
    }
  };
}
