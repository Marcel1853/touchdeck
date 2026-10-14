// Live-Werte auf Buttons (Uhrzeit, CPU, Lautstärke, Stream-Status …) und Schieberegler.
import * as linux from '../system/linux.js';
import * as steam from '../integrations/steam.js';

const opt = (value, label) => ({ value, label });

/** Quellen für Live-Anzeigen. Jede liefert { text, on?, value? }; „on“ lässt den Button leuchten. */
export const LIVE_SOURCES = [
  opt('none', 'Keine'),
  opt('clock', 'Uhrzeit'),
  opt('date', 'Datum'),
  opt('cpu', 'CPU-Auslastung'),
  opt('ram', 'Arbeitsspeicher'),
  opt('temp', 'CPU-Temperatur'),
  opt('volume', 'Lautstärke'),
  opt('mic', 'Mikrofon stumm?'),
  opt('track', 'Aktueller Musiktitel'),
  opt('steam_updates', 'Steam-Updates'),
  opt('steam_running', 'Spiel läuft (aus Aktion)'),
  opt('obs_stream', 'OBS: Stream läuft?'),
  opt('obs_record', 'OBS: Aufnahme läuft?'),
  opt('obs_scene', 'OBS: Szene aktiv? (aus Aktion)'),
  opt('twitch_live', 'Twitch: live?'),
  opt('twitch_viewers', 'Twitch: Zuschauer'),
  opt('twitch_followers', 'Twitch: Follower'),
  opt('variable', 'Wert einer Variable'),
  opt('command', 'Ausgabe eines Befehls'),
];

export const SLIDER_TARGETS = [
  opt('volume', 'Lautstärke (PC)'),
  opt('mic', 'Mikrofon-Pegel'),
  opt('obs_volume', 'OBS-Audioquelle'),
  opt('command', 'Eigener Befehl ({value} = 0–100)'),
];

const pad = (n) => String(n).padStart(2, '0');

/** Liest pro Durchlauf jede Quelle nur einmal, auch wenn mehrere Buttons sie anzeigen. */
class Snapshot {
  constructor(obs) {
    this.obs = obs;
    this.cache = new Map();
  }

  once(key, fn) {
    if (!this.cache.has(key)) this.cache.set(key, Promise.resolve().then(fn));
    return this.cache.get(key);
  }

  sink = () => this.once('sink', () => linux.getVolume('@DEFAULT_AUDIO_SINK@'));
  source = () => this.once('source', () => linux.getVolume('@DEFAULT_AUDIO_SOURCE@'));
  obsStatus = () => this.once('obs', () => this.obs.status());
  obsScene = () => this.once('obsScene', async () => (await this.obs.call('GetCurrentProgramScene')).currentProgramSceneName);
  running = () => this.once('running', () => steam.runningGames());
}

// Befehle für Live-Anzeigen höchstens alle 5 Sekunden ausführen.
const commandCache = new Map();
async function commandOutput(cmd) {
  const hit = commandCache.get(cmd);
  if (hit && Date.now() - hit.at < 5000) return hit.text;
  const text = (await linux.shell(cmd, { wait: true, timeout: 4000 })).split('\n')[0].slice(0, 60);
  commandCache.set(cmd, { at: Date.now(), text });
  return text;
}

const firstParam = (button, type, key) => button.actions.find((a) => a.type === type)?.params?.[key];

const truthy = (v) => v === true || ['true', '1', 'an', 'ja', 'on'].includes(String(v).toLowerCase());

async function liveValue(button, snap) {
  const { source, command, variable } = button.live;
  const now = new Date();
  switch (source) {
    case 'clock':
      return { text: `${pad(now.getHours())}:${pad(now.getMinutes())}` };
    case 'date':
      return { text: now.toLocaleDateString('de-DE', { weekday: 'short', day: 'numeric', month: 'short' }) };
    case 'cpu': {
      const v = await snap.once('cpu', () => linux.cpuUsage());
      return { text: v === null ? '…' : `${v} %`, value: v };
    }
    case 'ram': {
      const r = await snap.once('ram', () => linux.ramUsage());
      return { text: `${r.percent} %`, value: r.percent };
    }
    case 'temp': {
      const t = await snap.once('temp', () => linux.cpuTemp());
      return { text: t === null ? '–' : `${t} °C`, on: t !== null && t >= 80 };
    }
    case 'volume': {
      const v = await snap.sink();
      return { text: v.muted ? 'Stumm' : `${v.volume} %`, on: v.muted, value: v.volume };
    }
    case 'mic': {
      const v = await snap.source();
      return { text: v.muted ? 'Stumm' : 'An', on: v.muted };
    }
    case 'track': {
      const t = await snap.once('track', () => linux.currentTrack());
      return t ? { text: [t.title, t.artist].filter(Boolean).join(' – ') || 'Unbekannt', on: t.playing } : { text: 'Keine Wiedergabe' };
    }
    case 'steam_updates': {
      const u = await snap.once('steamUpdates', () => steam.updateStatus());
      if (u.running.length) return { text: `Lädt ${u.running.length}`, on: true };
      return { text: u.pending.length ? `${u.pending.length} Update${u.pending.length > 1 ? 's' : ''}` : 'Aktuell', on: u.pending.length > 0 };
    }
    case 'steam_running': {
      const game = firstParam(button, 'steam_game', 'game');
      const on = !!game && (await snap.running()).has(String(game));
      return { text: on ? 'Läuft' : '', on };
    }
    case 'obs_stream':
    case 'obs_record': {
      try {
        const s = await snap.obsStatus();
        if (source === 'obs_stream') return { text: s.streaming ? 'LIVE' : 'Offline', on: s.streaming };
        return { text: s.paused ? 'Pause' : s.recording ? 'REC' : 'Aus', on: s.recording };
      } catch {
        return { text: 'OBS aus' };
      }
    }
    case 'obs_scene': {
      try {
        const want = firstParam(button, 'obs_scene', 'scene');
        const on = !!want && (await snap.obsScene()) === want;
        return { text: on ? 'Aktiv' : '', on };
      } catch {
        return { text: '' };
      }
    }
    case 'command':
      return { text: command ? await commandOutput(command) : '' };
    case 'twitch_live':
    case 'twitch_viewers':
    case 'twitch_followers': {
      if (!snap.twitch?.connected) return { text: 'Twitch aus' };
      if (source === 'twitch_followers') return { text: String(await snap.twitch.followers()) };
      const s = await snap.twitch.stream();
      if (source === 'twitch_live') return { text: s ? 'LIVE' : 'Offline', on: !!s };
      return { text: s ? `${s.viewer_count} 👁` : '–', on: !!s };
    }
    case 'variable': {
      const v = snap.variables?.[variable];
      return { text: v === undefined ? '–' : String(v), on: truthy(v) };
    }
    default:
      return null;
  }
}

async function sliderValue(button, snap) {
  const { target, input } = button.slider;
  if (target === 'volume') return { value: (await snap.sink()).volume };
  if (target === 'mic') return { value: (await snap.source()).volume };
  if (target === 'obs_volume' && input) {
    try {
      const r = await snap.obs.call('GetInputVolume', { inputName: input });
      return { value: Math.round(Math.sqrt(r.inputVolumeMul) * 100) };
    } catch {
      return { value: null, text: 'OBS aus' };
    }
  }
  return null;
}

/** Berechnet die Live-Werte aller Buttons, die welche brauchen. */
export async function collectLive(pages, obs, variables = {}, twitch = null) {
  const snap = new Snapshot(obs);
  snap.variables = variables;
  snap.twitch = twitch;
  const values = {};
  const jobs = [];
  for (const page of pages) {
    for (const b of page.buttons) {
      const job = b.mode === 'slider' ? sliderValue(b, snap) : b.live.source !== 'none' ? liveValue(b, snap) : null;
      if (!job) continue;
      jobs.push(job.then((v) => v && (values[b.id] = v)).catch((err) => (values[b.id] = { text: '⚠', error: err.message })));
    }
  }
  await Promise.all(jobs);
  return values;
}

/** Setzt den Wert eines Schiebereglers (0–100). */
export async function applySlider(button, value, obs) {
  const v = Math.max(0, Math.min(100, Math.round(Number(value))));
  const { target, input, command } = button.slider;
  switch (target) {
    case 'volume':
      return linux.run('wpctl', ['set-volume', '@DEFAULT_AUDIO_SINK@', `${v}%`]);
    case 'mic':
      return linux.run('wpctl', ['set-volume', '@DEFAULT_AUDIO_SOURCE@', `${v}%`]);
    case 'obs_volume':
      if (!input) throw new Error('Keine OBS-Audioquelle gewählt.');
      // Quadratische Kurve: fühlt sich gleichmäßiger an als die lineare Verstärkung.
      return obs.call('SetInputVolume', { inputName: input, inputVolumeMul: (v / 100) ** 2 });
    case 'command':
      if (!command) throw new Error('Kein Befehl angegeben.');
      return linux.shell(command.replaceAll('{value}', String(v)));
    default:
      throw new Error('Unbekanntes Ziel für den Regler.');
  }
}
