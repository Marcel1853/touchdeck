// Vorschau am PC: das Bedienfeld in der Größe eines bestimmten Handys oder Tablets.
// Größen in CSS-Pixeln (so, wie die Geräte sie dem Bedienfeld melden); verbundene Geräte melden ihre
// echte Größe und erscheinen oben in der Liste.
import { t, setLang, translateDom } from '/shared/i18n.js';

const $ = (id) => document.getElementById(id);
const PRESETS = [
  ['phoneSmall', 360, 780],
  ['phoneLarge', 412, 915],
  ['tablet8', 800, 1280],
  ['tablet11', 834, 1194],
  ['tablet13', 1024, 1366],
  ['desktop', 1280, 800],
];
let devices = [];
let landscape = false; // gedreht gegenüber der gemeldeten Lage

const store = {
  get: (k) => { try { return localStorage.getItem(`touchdeck.preview.${k}`); } catch { return null; } },
  set: (k, v) => { try { localStorage.setItem(`touchdeck.preview.${k}`, v); } catch { /* egal */ } },
};

function options() {
  const known = devices.map(([name, w, h]) => [`dev:${name}`, `📱 ${name} (${w} × ${h})`, w, h]);
  const fixed = PRESETS.map(([id, w, h]) => [id, `${t(`preview.${id}`)} (${w} × ${h})`, w, h]);
  return [...known, ...fixed, ['free', t('preview.free'), 0, 0]];
}

function current() {
  const list = options();
  return list.find(([id]) => id === $('device').value) ?? list[0];
}

function layout() {
  const [id, , w0, h0] = current();
  document.body.classList.toggle('free', id === 'free');
  const frame = $('frame');
  const deck = $('deck');
  if (id === 'free') {
    frame.style.width = frame.style.height = '';
    $('size').textContent = '';
    return;
  }
  // So wie das Gerät es meldet; ⟳ dreht (Seiten vertauschen).
  const [w, h] = landscape ? [h0, w0] : [w0, h0];
  const stage = $('stage').getBoundingClientRect();
  const z = $('zoom').value;
  const scale = z === 'fit' ? Math.min(1, (stage.width - 60) / w, (stage.height - 60) / h) : Number(z);
  deck.style.width = `${w}px`;
  deck.style.height = `${h}px`;
  deck.style.transform = `scale(${scale})`;
  frame.style.width = `${Math.round(w * scale)}px`;
  frame.style.height = `${Math.round(h * scale)}px`;
  $('size').textContent = `${w} × ${h} · ${Math.round(scale * 100)} %`;
}

async function init() {
  const hello = await fetch('/api/hello').then((r) => r.json()).catch(() => ({}));
  await setLang(hello.language ?? 'de');
  translateDom();
  document.title = t('preview.title');
  const sizes = await fetch('/api/device-sizes').then((r) => r.json()).catch(() => ({}));
  devices = Object.entries(sizes).map(([name, [w, h]]) => [name, w, h]);
  $('device').replaceChildren(...options().map(([id, label]) => Object.assign(document.createElement('option'), { value: id, textContent: label })));
  // Letzte Auswahl merken.
  const saved = store.get('device');
  if (saved && options().some(([id]) => id === saved)) $('device').value = saved;
  landscape = store.get('landscape') === '1';
  $('zoom').value = store.get('zoom') ?? 'fit';
  $('device').onchange = () => { store.set('device', $('device').value); landscape = false; store.set('landscape', '0'); layout(); };
  $('rotate').onclick = () => { landscape = !landscape; store.set('landscape', landscape ? '1' : '0'); layout(); };
  $('zoom').onchange = () => { store.set('zoom', $('zoom').value); layout(); };
  window.addEventListener('resize', layout);
  layout();
}

init();
