// Bedienfeld: zeigt die Seiten an und schickt Drücken, Halten und Regler über WebSocket an den PC.
import { t as tr, getLang } from '/shared/i18n.js';
import { buttonElement, setupGrid } from '/shared/render.js';

const $ = (id) => document.getElementById(id);
const grid = $('grid');
const HOLD_DELAY = 450;
// Doppelt drücken: so lange auf ein zweites Tippen warten (nur bei Buttons mit Doppelt-Aktionen).
const DOUBLE_DELAY = 300;
const pendingTaps = new Map();
// In der Android-App stellt die App diese Brücke bereit (Vibration, Verbindung ändern).
const app = window.TouchDeckApp ?? null;
if (app) document.documentElement.classList.add('in-app');

// Schlüssel und PIN merken, damit die Startbildschirm-Verknüpfung ohne sie auskommt.
const store = {
  get: (k) => {
    try {
      return localStorage.getItem(`touchdeck.${k}`);
    } catch {
      return null;
    }
  },
  set: (k, v) => {
    try {
      if (v === null) localStorage.removeItem(`touchdeck.${k}`);
      else localStorage.setItem(`touchdeck.${k}`, v);
    } catch {
      // Ohne Speicher funktioniert es trotzdem, solange der Schlüssel in der Adresse steht.
    }
  },
};
const params = new URLSearchParams(location.search);
let token = params.get('token');
if (token) store.set('token', token);
else token = store.get('token');
let pin = store.get('pin');
if (params.has('token')) history.replaceState(null, '', location.pathname);

let state = null;
let currentPage = null;
let announcedPage = null; // die Seite, die der Server als angezeigt kennt
const history_ = [];
let ws = null;
let reqId = 0;
let retryDelay = 500;

function toast(text, isError = false) {
  const t = $('toast');
  t.textContent = text;
  t.className = `show${isError ? ' error' : ''}`;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.className = ''), isError ? 3500 : 1800);
}

let pcName = '';
function setConnected(ok, text) {
  $('status').classList.toggle('ok', ok);
  $('foot-dot').classList.toggle('ok', ok);
  $('foot-text').textContent = ok ? tr('deck.connectedTo', [pcName || location.hostname]) : tr('deck.disconnectedShort');
  $('overlay').classList.toggle('hidden', ok);
  if (text) $('overlay-text').textContent = text;
}

let audioCtx = null;
function click() {
  try {
    audioCtx ??= new AudioContext();
    const o = audioCtx.createOscillator();
    const g = audioCtx.createGain();
    o.frequency.value = 900;
    g.gain.setValueAtTime(0.08, audioCtx.currentTime);
    g.gain.exponentialRampToValueAtTime(0.0001, audioCtx.currentTime + 0.05);
    o.connect(g).connect(audioCtx.destination);
    o.start();
    o.stop(audioCtx.currentTime + 0.06);
  } catch {
    // Ton ist nur Zugabe.
  }
}

const vibrate = (ms) => (app ? app.vibrate(ms) : navigator.vibrate?.(ms));

const feedback = (ms = 15) => {
  if (state.deck.vibrate) vibrate(ms);
  if (state.deck.sound) click();
};

// ---------- Darstellen ----------

/** Button so, wie er gerade aussehen soll: Entwurf plus optische Änderungen aus „Visuelles“. */
const effective = (b) => ({ ...b, ...state.overrides?.buttons?.[b.id] });

/** Live-Wert eines Buttons; solange er noch nicht da ist, ein Platzhalter, damit nichts springt. */
const liveFor = (b) => state.live?.[b.id] ?? (b.mode !== 'slider' && b.live && b.live.source !== 'none' ? { text: '…' } : null);

const element = (b, page) => {
  const el = buttonElement(effective(b), { on: !!state.toggles[b.id], live: liveFor(b), variables: state.variables, page });
  if (b.holdActions?.length) el.dataset.hold = '1';
  if (b.doubleActions?.length) el.dataset.double = '1';
  return el;
};

/** Ausrichtung aus den Einstellungen: in der App sofort, im Browser nur im Vollbild möglich. */
let appliedOrientation = null;
function applyOrientation(mode = 'auto') {
  if (app?.setOrientation) {
    if (mode !== appliedOrientation) app.setOrientation(mode);
    appliedOrientation = mode;
    return;
  }
  if (!document.fullscreenElement || !screen.orientation?.lock) return;
  if (mode === 'auto') screen.orientation.unlock?.();
  else screen.orientation.lock(mode).catch(() => {});
}

function render() {
  if (!state) return;
  const page = state.pages.find((p) => p.id === currentPage) ?? state.pages.find((p) => p.id === state.startPage) ?? state.pages[0];
  // Dem Server melden, welche Seite angezeigt wird – nur dafür werden Live-Werte abgefragt.
  // Verglichen wird mit der zuletzt gemeldeten Seite (goTo setzt currentPage schon vorher).
  if (announcedPage !== page.id && ws?.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'view', pageId: page.id }));
    announcedPage = page.id;
  }
  showRotateHint(page.id);
  currentPage = page.id;
  $('page-name').textContent = page.name;
  $('back').hidden = history_.length === 0;
  applyOrientation(state.deck.orientation);
  document.body.className = `anim-${state.deck.pressAnimation}${state.deck.showLabels ? '' : ' hide-labels'}${page.cols > page.rows + 1 ? ' wide-page' : ''}${state.deck.footer ? ' with-footer' : ''}`;
  // „Fläche füllen“: Felder dürfen bis 1,4-mal so breit und 1,25-mal so hoch wie quadratisch werden.
  // Freie Seiten immer im gewählten Format zeigen, sonst verschiebt sich die Gestaltung.
  const square = state.deck.cellShape === 'square' || page.layout === 'free';
  grid.style.setProperty('--stretch-w', square ? 1 : 1.4);
  grid.style.setProperty('--stretch-h', square ? 1 : 1.25);
  setupGrid(grid, page);
  document.body.style.background = state.overrides?.pages?.[page.id]?.background ?? page.background;
  grid.replaceChildren(...page.buttons.map((b) => element(b, page)));
}

/** Hinweis „Gerät quer halten“ beim Öffnen einer breiten Seite kurz zeigen, dann ausblenden. */
let hintShownFor = null;
let hintTimer = null;
function showRotateHint(pageId) {
  if (hintShownFor === pageId) return;
  hintShownFor = pageId;
  const el = $('rotate-hint');
  el.classList.remove('gone');
  clearTimeout(hintTimer);
  hintTimer = setTimeout(() => el.classList.add('gone'), 4000);
}

/**
 * Nur die Buttons der angezeigten Seite neu zeichnen, für die changed(b) gilt – einzeln, damit laufende
 * Berührungen nicht unterbrochen werden und das Tablet nicht bei jedem Wert die ganze Seite neu aufbaut.
 */
function refreshButtons(changed) {
  const page = state.pages.find((p) => p.id === currentPage);
  for (const b of page?.buttons ?? []) {
    if (!changed(b)) continue;
    const el = grid.querySelector(`[data-id="${CSS.escape(b.id)}"]`);
    // Webseiten nicht neu aufbauen, sonst lädt die Seite bei jedem Live-Wert neu.
    if (!el || el.classList.contains('pressed') || el.classList.contains('sliding') || el.classList.contains('is-web')) continue;
    el.replaceWith(element(b, page));
  }
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** Live-Werte: full = alle Werte der Seite, sonst nur geänderte (werden eingemischt). */
function updateLive(values, full = true) {
  const before = state.live ?? {};
  // Graphen kommen nach dem ersten Mal nur mit neuen Punkten: an den bisherigen Verlauf anhängen.
  for (const [id, v] of Object.entries(values)) {
    if (!v?.seriesAppend) continue;
    const prev = before[id]?.series ?? [];
    v.series = v.seriesAppend.map((a, i) => ({ name: prev[i]?.name ?? '', values: [...(prev[i]?.values ?? []), ...a.values].slice(-a.len) }));
    delete v.seriesAppend;
  }
  state.live = full ? values : { ...before, ...values };
  refreshButtons((b) => !same(before[b.id], state.live[b.id]));
}

/** Geänderte Werte ({Name} in Titeln, Live-Quelle „Wert“): nur Buttons neu zeichnen, die sie benutzen. */
function updateVariables(values) {
  for (const [name, value] of Object.entries(values)) {
    if (value === null) delete state.variables[name];
    else state.variables[name] = value;
  }
  const names = Object.keys(values);
  refreshButtons((b) => {
    const text = JSON.stringify(effective(b));
    return names.some((n) => text.includes(`{${n}}`) || text.includes(`"variable":${JSON.stringify(n)}`));
  });
}

function updateToggles(toggles) {
  const before = state.toggles ?? {};
  state.toggles = toggles;
  refreshButtons((b) => !same(before[b.id], toggles[b.id]));
}

function updateOverrides(overrides) {
  const before = state.overrides ?? {};
  state.overrides = overrides;
  refreshButtons((b) => !same(before.buttons?.[b.id], overrides.buttons?.[b.id]));
  const page = state.pages.find((p) => p.id === currentPage);
  if (page) document.body.style.background = overrides.pages?.[page.id]?.background ?? page.background;
}

/** Webseiten-Buttons mit „Neu laden alle n Sekunden“. */
setInterval(() => {
  const now = Date.now();
  for (const f of grid.querySelectorAll('iframe.td-web[data-refresh]')) {
    const due = Number(f.dataset.loaded ?? now) + Number(f.dataset.refresh) * 1000;
    if (!f.dataset.loaded) f.dataset.loaded = now;
    else if (now >= due) {
      f.dataset.loaded = now;
      f.src = f.src;
    }
  }
}, 1000);

/** Für die Zurück-Taste der Android-App: eine Seite zurück, falls möglich. */
window.deckBack = () => {
  if (!$('page-menu').hidden) {
    $('page-menu').hidden = true;
    return true;
  }
  if (!history_.length) return false;
  goTo('__back');
  return true;
};

function goTo(pageId) {
  // Nächste / vorherige Seite in der Reihenfolge der Seitenliste (ringsum).
  if (pageId === '__next' || pageId === '__prev') {
    const ids = state.pages.map((p) => p.id);
    const i = ids.indexOf(currentPage);
    pageId = ids[(i + (pageId === '__next' ? 1 : ids.length - 1)) % ids.length];
  }
  if (pageId === '__back') {
    if (history_.length) currentPage = history_.pop();
  } else if (pageId && pageId !== currentPage) {
    history_.push(currentPage);
    currentPage = pageId;
  }
  render();
}

/** Name dieses Geräts für die Statusleiste am PC: aus der App, sonst aus der Browser-Kennung. */
function deviceName() {
  try {
    const fromApp = app?.deviceName?.();
    if (fromApp) return fromApp;
  } catch {}
  const ua = navigator.userAgent;
  const model = ua.match(/Android [\d.]+; ([^;)]+)/)?.[1]?.trim();
  if (model && model !== 'K') return model;
  if (/iPad/.test(ua)) return 'iPad';
  if (/iPhone/.test(ua)) return 'iPhone';
  if (/Android/.test(ua)) return 'Android';
  return `Browser (${/Firefox/.test(ua) ? 'Firefox' : /Chrome/.test(ua) ? 'Chrome' : /Safari/.test(ua) ? 'Safari' : navigator.platform || '?'})`;
}

const send = (msg) => {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    toast(tr('deck.notConnected'), true);
    return false;
  }
  ws.send(JSON.stringify({ ...msg, pageId: currentPage }));
  return true;
};

// ---------- Regler ----------

let lastSlideSent = 0;
let slideTimer = null;

/** Drehwinkel um die Mitte von r als Wert 0–100 (unten links 0 %, unten rechts 100 %). */
function knobValue(r, e) {
  const deg = (Math.atan2(e.clientX - (r.left + r.width / 2), r.top + r.height / 2 - e.clientY) * 180) / Math.PI;
  let from = (deg + 360 + 135) % 360;
  if (from > 270) from = from > 315 ? 0 : 270;
  return Math.round((from / 270) * 100);
}

function sliderValue(el, e) {
  // Fader + Drehknopf: der Teil, an dem das Ziehen begann (2 = Knopf).
  if (el.dataset.part === '2') return knobValue(el.querySelector('.td-combo-knob').getBoundingClientRect(), e);
  const r = el.dataset.knob ? (el.querySelector('.td-knob')?.getBoundingClientRect() ?? el.getBoundingClientRect()) : el.getBoundingClientRect();
  if (el.dataset.knob) {
    // Winkel um die Mitte: 0 % unten links (225°), 100 % unten rechts (135°), 270° dazwischen im Uhrzeigersinn.
    const deg = (Math.atan2(e.clientX - (r.left + r.width / 2), r.top + r.height / 2 - e.clientY) * 180) / Math.PI;
    let from = (deg + 360 + 135) % 360;
    if (from > 270) from = from > 315 ? 0 : 270;
    return Math.round((from / 270) * 100);
  }
  // Schiene/Fader: Wert an der Schiene messen, nicht am ganzen Button.
  const rail = el.querySelector('.td-rail')?.getBoundingClientRect();
  const box = rail && rail.height > 0 && rail.width > 0 ? rail : r;
  const ratio = el.dataset.vertical ? (box.bottom - e.clientY) / box.height : (e.clientX - box.left) / box.width;
  return Math.round(Math.max(0, Math.min(1, ratio)) * 100);
}

function sendSlide(el, value, final = false) {
  const part = el.dataset.part === '2' ? 2 : 1;
  el.style.setProperty(part === 2 ? '--val2' : '--val', value);
  const live = el.querySelector('.td-live');
  if (live && part === 1) live.textContent = `${value} %`;
  clearTimeout(slideTimer);
  const go = () => {
    lastSlideSent = Date.now();
    ws?.readyState === WebSocket.OPEN && ws.send(JSON.stringify({ type: 'slide', pageId: currentPage, buttonId: el.dataset.id, value, part }));
  };
  // Höchstens alle 60 ms senden, den letzten Wert aber immer.
  if (final || Date.now() - lastSlideSent > 60) go();
  else slideTimer = setTimeout(go, 60);
}

// ---------- Drücken und Halten ----------

// Zeiger-Ereignisse statt click: reagiert sofort und funktioniert mit Maus und Touch gleich.
grid.addEventListener('pointerdown', (e) => {
  const el = e.target.closest('.td-btn');
  // Leere Felder und Anzeigen reagieren nicht auf Antippen.
  if (!el || el.classList.contains('is-blank') || el.classList.contains('is-display')) return;
  el.setPointerCapture(e.pointerId);
  if (el.classList.contains('is-slider')) {
    const knob = el.querySelector('.td-combo-knob')?.getBoundingClientRect();
    el.dataset.part = knob && e.clientX >= knob.left && e.clientX <= knob.right && e.clientY >= knob.top && e.clientY <= knob.bottom ? '2' : '1';
    el.classList.add('sliding');
    if (state.deck.vibrate) vibrate(8);
    sendSlide(el, sliderValue(el, e));
    return;
  }
  el.classList.add('pressed');
  if (el.dataset.hold) {
    // Langes Drücken: nach kurzer Zeit die Halten-Aktionen statt der normalen auslösen.
    el.holdTimer = setTimeout(() => {
      el.holding = true;
      el.classList.add('holding');
      feedback(35);
      send({ type: 'hold', phase: 'start', buttonId: el.dataset.id });
    }, HOLD_DELAY);
  }
});
grid.addEventListener('pointermove', (e) => {
  const el = e.target.closest('.td-btn.sliding');
  if (el) sendSlide(el, sliderValue(el, e));
});
const release = (e, fire) => {
  const el = e.target.closest('.td-btn');
  if (el?.classList.contains('sliding')) {
    el.classList.remove('sliding');
    sendSlide(el, sliderValue(el, e), true);
    return;
  }
  if (!el?.classList.contains('pressed')) return;
  el.classList.remove('pressed', 'holding');
  clearTimeout(el.holdTimer);
  if (el.holding) {
    el.holding = false;
    send({ type: 'hold', phase: 'end', buttonId: el.dataset.id });
    return;
  }
  const r = el.getBoundingClientRect();
  if (fire && e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom) {
    feedback();
    const id = el.dataset.id;
    if (!el.dataset.double) return send({ type: 'press', reqId: ++reqId, buttonId: id });
    // Zweites Tippen innerhalb der Wartezeit: Doppelt-Aktionen, sonst nach der Wartezeit die normalen.
    if (pendingTaps.has(id)) {
      clearTimeout(pendingTaps.get(id));
      pendingTaps.delete(id);
      return send({ type: 'press', reqId: ++reqId, buttonId: id, double: true });
    }
    pendingTaps.set(id, setTimeout(() => {
      pendingTaps.delete(id);
      send({ type: 'press', reqId: ++reqId, buttonId: id });
    }, DOUBLE_DELAY));
  }
};
grid.addEventListener('pointerup', (e) => release(e, true));
grid.addEventListener('pointercancel', (e) => release(e, false));
grid.addEventListener('contextmenu', (e) => e.preventDefault());
// In der App: über die Fußleiste zum Verbinden-Bildschirm (anderen PC wählen).
if (app?.openConnection) {
  $('foot-switch').hidden = false;
  $('foot-switch').addEventListener('click', () => app.openConnection());
}

// Nach dem Drehen oder einer Größenänderung neu zeichnen – manche Android-WebViews berechnen
// die an der Buttongröße ausgerichteten Schriftgrößen sonst erst beim nächsten Neuzeichnen.
let resizeTimer = null;
window.addEventListener('resize', () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => state && render(), 120);
});

// ---------- Kopfzeile ----------

$('back').addEventListener('click', () => goTo('__back'));
$('page-name').addEventListener('click', () => {
  if (!state) return;
  $('page-menu-list').replaceChildren(...state.pages.map((p) => {
    const b = document.createElement('button');
    b.textContent = p.name;
    b.className = p.id === currentPage ? 'active' : '';
    b.onclick = () => {
      $('page-menu').hidden = true;
      goTo(p.id);
    };
    return b;
  }));
  if (app) {
    const c = document.createElement('button');
    c.textContent = tr('deck.changeConnection');
    c.className = 'menu-extra';
    c.onclick = () => app.openConnection();
    $('page-menu-list').append(c);
  }
  $('page-menu').hidden = false;
});
$('page-menu').addEventListener('click', (e) => {
  if (e.target.id === 'page-menu') $('page-menu').hidden = true;
});
$('fullscreen').addEventListener('click', () => {
  if (document.fullscreenElement) document.exitFullscreen();
  else document.documentElement.requestFullscreen?.().then(() => applyOrientation(state?.deck.orientation)).catch(() => toast(tr('deck.fullscreenIsNotSupported'), true));
});

// ---------- PIN ----------

function askPin(wrong) {
  return new Promise((resolve) => {
    $('pin-box').hidden = false;
    $('pin-hint').textContent = wrong ? tr('deck.pinIsWrongPlease') : tr('deck.thisPcIsProtected');
    const input = $('pin-input');
    input.value = '';
    input.focus();
    $('pin-form').onsubmit = (e) => {
      e.preventDefault();
      $('pin-box').hidden = true;
      resolve(input.value.trim());
    };
  });
}

/** Vor dem Verbinden prüfen, ob der Schlüssel reicht oder eine PIN fehlt. */
async function checkAccess() {
  if (!token) return true;
  const q = new URLSearchParams({ token, ...(pin && { pin }) });
  const res = await fetch(`/api/state?${q}`).catch(() => null);
  if (!res || res.ok) return true;
  const body = await res.json().catch(() => ({}));
  if (!body.pin) return true;
  pin = await askPin(!!pin);
  store.set('pin', pin);
  return checkAccess();
}

// ---------- Verbindung ----------

async function connect() {
  await checkAccess();
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  const q = new URLSearchParams({ ...(token && { token }), ...(pin && { pin }) });
  ws = new WebSocket(`${proto}://${location.host}/ws${q.size ? `?${q}` : ''}`);
  ws.onopen = () => {
    retryDelay = 500;
    announcedPage = null;
    setConnected(true);
    ws.send(JSON.stringify({ type: 'hello', name: deviceName() }));
    // Name des PCs für die Fußleiste.
    fetch('/api/hello').then((r) => r.json()).then((h) => {
      pcName = h.name ?? '';
      setConnected(true);
    }).catch(() => {});
  };
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.type === 'state') {
      // Neue TouchDeck-Version am PC: einmal neu laden, damit auch das Bedienfeld aktuell ist.
      if (state?.version && msg.version && msg.version !== state.version) return location.reload();
      // TouchDeck am PC neu gestartet (z. B. neuer Build): neu laden, sonst läuft hier noch alter Code.
      if (state?.build && msg.build && msg.build !== state.build) return location.reload();
      // Andere Sprache am PC eingestellt: neu laden, damit alle Texte passen.
      if (msg.language && msg.language !== getLang()) return location.reload();
      // Der App die Sprache sagen, damit auch ihr Verbinden-Bildschirm passt.
      if (msg.language) app?.setLanguage?.(msg.language);
      state = msg;
      render();
    } else if (msg.type === 'goto') {
      // „Seite auf allen Geräten anzeigen“ (z. B. aus „Programm im Vordergrund“).
      if (msg.pageId && msg.pageId !== currentPage) goTo(msg.pageId);
    } else if (msg.type === 'live') {
      if (state) updateLive(msg.values, msg.full !== false);
    } else if (msg.type === 'vars') {
      if (state) updateVariables(msg.values);
    } else if (msg.type === 'toggles') {
      if (state) updateToggles(msg.toggles);
    } else if (msg.type === 'overrides') {
      if (state) updateOverrides(msg.overrides);
    } else if (msg.type === 'result') {
      const el = grid.querySelector(`[data-id="${CSS.escape(msg.buttonId)}"]`);
      if (!msg.ok) {
        el?.classList.add('failed');
        setTimeout(() => el?.classList.remove('failed'), 500);
        toast(msg.error, true);
      } else if (msg.messages?.length) {
        toast(msg.messages.join(' · '));
      }
      if (msg.navigate) goTo(msg.navigate);
    }
  };
  ws.onclose = (ev) => {
    const text = ev.code === 1006 && !state && !token
      ? tr('deck.notPairedPleaseScan')
      : tr('deck.connectionLostReconnecting');
    setConnected(false, text);
    setTimeout(connect, retryDelay);
    retryDelay = Math.min(retryDelay * 2, 8000);
  };
}

connect();
