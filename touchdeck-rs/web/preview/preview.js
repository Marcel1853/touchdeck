// Vorschau am PC: das Bedienfeld in der Größe eines bestimmten Handys oder Tablets.
// Größen in CSS-Pixeln (so, wie die Geräte sie dem Bedienfeld melden); verbundene Geräte melden ihre
// echte Größe und erscheinen oben in der Liste.
import { t, setLang, translateDom } from '/shared/i18n.js';

const $ = (id) => document.getElementById(id);
// Gängige Geräte in CSS-Pixeln (Hochformat), nach Gruppen. Quelle: Viewport-Tabellen wie yesviz.com.
const PRESETS = [
  ['iPhone', [['iPhone SE', 375, 667], ['iPhone 13 / 14', 390, 844], ['iPhone 15 / 16', 393, 852], ['iPhone 16 Pro', 402, 874],
    ['iPhone 15 Plus / 16 Plus', 430, 932], ['iPhone 16 Pro Max', 440, 956]]],
  ['Samsung', [['Galaxy S24', 360, 780], ['Galaxy S24+', 384, 832], ['Galaxy S24 Ultra', 412, 892], ['Galaxy A55', 412, 915]]],
  ['Google', [['Pixel 8', 412, 915], ['Pixel 9 Pro XL', 412, 920]]],
  ['iPad', [['iPad mini', 744, 1133], ['iPad 10. Gen. / iPad Air 11"', 820, 1180], ['iPad Pro 11"', 834, 1210], ['iPad Pro 13"', 1032, 1376]]],
  ['Android-Tablets', [['Galaxy Tab S9', 800, 1280], ['Tablet 10 Zoll', 800, 1280]]],
];
let devices = [];
let landscape = false; // gedreht gegenüber der gemeldeten Lage

const store = {
  get: (k) => { try { return localStorage.getItem(`touchdeck.preview.${k}`); } catch { return null; } },
  set: (k, v) => { try { localStorage.setItem(`touchdeck.preview.${k}`, v); } catch { /* egal */ } },
};

/** Alle Einträge: [id, Bezeichnung, Breite, Höhe, Gruppe]. */
function options() {
  const known = devices.map(([name, w, h]) => [`dev:${name}`, `${name} (${w} × ${h})`, w, h, t('preview.yourDevices')]);
  const fixed = PRESETS.flatMap(([group, list]) => list.map(([name, w, h]) => [`p:${name}`, `${name} (${w} × ${h})`, w, h, group]));
  const custom = customSize();
  return [...known, ...fixed,
    ['custom', `${t('preview.custom')} (${custom[0]} × ${custom[1]})`, custom[0], custom[1], t('preview.other')],
    ['free', t('preview.free'), 0, 0, t('preview.other')]];
}

/** Eigene Größe (gemerkt). */
function customSize() {
  const v = (store.get('custom') ?? '400x800').split('x').map(Number);
  return [v[0] || 400, v[1] || 800];
}

function fillSelect() {
  const sel = $('device');
  const value = sel.value;
  const groups = new Map();
  for (const [id, label, , , group] of options()) {
    if (!groups.has(group)) groups.set(group, Object.assign(document.createElement('optgroup'), { label: group }));
    groups.get(group).append(Object.assign(document.createElement('option'), { value: id, textContent: label }));
  }
  sel.replaceChildren(...groups.values());
  if (value) sel.value = value;
}

function current() {
  const list = options();
  return list.find(([id]) => id === $('device').value) ?? list[0];
}

function layout() {
  const [id, , w0, h0] = current();
  document.body.classList.toggle('free', id === 'free');
  $('custom-size').hidden = id !== 'custom';
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
  fillSelect();
  // Letzte Auswahl merken.
  const saved = store.get('device');
  if (saved && options().some(([id]) => id === saved)) $('device').value = saved;
  landscape = store.get('landscape') === '1';
  $('zoom').value = store.get('zoom') ?? 'fit';
  $('device').onchange = () => { store.set('device', $('device').value); landscape = false; store.set('landscape', '0'); layout(); };
  $('rotate').onclick = () => { landscape = !landscape; store.set('landscape', landscape ? '1' : '0'); layout(); };
  $('zoom').onchange = () => { store.set('zoom', $('zoom').value); layout(); };
  // Eigene Größe: Breite × Höhe in CSS-Pixeln.
  const [cw, ch] = customSize();
  $('custom-w').value = cw;
  $('custom-h').value = ch;
  const setCustom = () => {
    const w = Math.max(200, Math.min(4000, Number($('custom-w').value) || 400));
    const h = Math.max(200, Math.min(4000, Number($('custom-h').value) || 800));
    store.set('custom', `${w}x${h}`);
    fillSelect();
    layout();
  };
  $('custom-w').onchange = setCustom;
  $('custom-h').onchange = setCustom;
  window.addEventListener('resize', layout);
  layout();
}

init();
