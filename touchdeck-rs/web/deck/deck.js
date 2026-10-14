// Bedienfeld: zeigt die Seiten an und schickt Drücken, Halten und Regler über WebSocket an den PC.
import { buttonElement, setupGrid } from '/shared/render.js';

const $ = (id) => document.getElementById(id);
const grid = $('grid');
const HOLD_DELAY = 450;
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

function setConnected(ok, text) {
  $('status').classList.toggle('ok', ok);
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

const element = (b) => {
  const el = buttonElement(effective(b), { on: !!state.toggles[b.id], live: liveFor(b), variables: state.variables });
  if (b.holdActions?.length) el.dataset.hold = '1';
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
  currentPage = page.id;
  $('page-name').textContent = page.name;
  $('back').hidden = history_.length === 0;
  applyOrientation(state.deck.orientation);
  document.body.className = `anim-${state.deck.pressAnimation}${state.deck.showLabels ? '' : ' hide-labels'}${page.cols > page.rows + 1 ? ' wide-page' : ''}`;
  setupGrid(grid, page);
  document.body.style.background = state.overrides?.pages?.[page.id]?.background ?? page.background;
  grid.replaceChildren(...page.buttons.map(element));
}

/** Live-Werte einzeln austauschen, damit laufende Berührungen nicht unterbrochen werden. */
function updateLive(values) {
  state.live = values;
  const page = state.pages.find((p) => p.id === currentPage);
  for (const b of page?.buttons ?? []) {
    const el = grid.querySelector(`[data-id="${CSS.escape(b.id)}"]`);
    if (!el || el.classList.contains('pressed') || el.classList.contains('sliding')) continue;
    el.replaceWith(element(b));
  }
}

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
  if (pageId === '__back') {
    if (history_.length) currentPage = history_.pop();
  } else if (pageId && pageId !== currentPage) {
    history_.push(currentPage);
    currentPage = pageId;
  }
  render();
}

const send = (msg) => {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    toast('Nicht verbunden', true);
    return false;
  }
  ws.send(JSON.stringify({ ...msg, pageId: currentPage }));
  return true;
};

// ---------- Regler ----------

let lastSlideSent = 0;
let slideTimer = null;

function sliderValue(el, e) {
  const r = el.getBoundingClientRect();
  const ratio = el.dataset.vertical ? (r.bottom - e.clientY) / r.height : (e.clientX - r.left) / r.width;
  return Math.round(Math.max(0, Math.min(1, ratio)) * 100);
}

function sendSlide(el, value, final = false) {
  el.style.setProperty('--val', value);
  const live = el.querySelector('.td-live');
  if (live) live.textContent = `${value} %`;
  clearTimeout(slideTimer);
  const go = () => {
    lastSlideSent = Date.now();
    ws?.readyState === WebSocket.OPEN && ws.send(JSON.stringify({ type: 'slide', pageId: currentPage, buttonId: el.dataset.id, value }));
  };
  // Höchstens alle 60 ms senden, den letzten Wert aber immer.
  if (final || Date.now() - lastSlideSent > 60) go();
  else slideTimer = setTimeout(go, 60);
}

// ---------- Drücken und Halten ----------

// Zeiger-Ereignisse statt click: reagiert sofort und funktioniert mit Maus und Touch gleich.
grid.addEventListener('pointerdown', (e) => {
  const el = e.target.closest('.td-btn');
  if (!el || el.classList.contains('is-blank')) return;
  el.setPointerCapture(e.pointerId);
  if (el.classList.contains('is-slider')) {
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
    send({ type: 'press', reqId: ++reqId, buttonId: el.dataset.id });
  }
};
grid.addEventListener('pointerup', (e) => release(e, true));
grid.addEventListener('pointercancel', (e) => release(e, false));
grid.addEventListener('contextmenu', (e) => e.preventDefault());

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
    c.textContent = '⚙ Verbindung ändern';
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
  else document.documentElement.requestFullscreen?.().then(() => applyOrientation(state?.deck.orientation)).catch(() => toast('Vollbild wird hier nicht unterstützt', true));
});

// ---------- PIN ----------

function askPin(wrong) {
  return new Promise((resolve) => {
    $('pin-box').hidden = false;
    $('pin-hint').textContent = wrong ? 'Die PIN stimmt nicht. Bitte noch einmal.' : 'Dieser PC ist mit einer PIN geschützt.';
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
  };
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.type === 'state') {
      // Neue TouchDeck-Version am PC: einmal neu laden, damit auch das Bedienfeld aktuell ist.
      if (state?.version && msg.version && msg.version !== state.version) return location.reload();
      state = msg;
      render();
    } else if (msg.type === 'live') {
      if (state) updateLive(msg.values);
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
      ? 'Nicht gekoppelt. Bitte den QR-Code im TouchDeck-Editor am PC scannen.'
      : 'Verbindung verloren – verbinde neu …';
    setConnected(false, text);
    setTimeout(connect, retryDelay);
    retryDelay = Math.min(retryDelay * 2, 8000);
  };
}

connect();
