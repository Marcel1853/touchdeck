// Icon-Pakete: eigene Bildersammlungen anlegen, füllen, exportieren (.tdicons) und importieren.
// Verwaltung in den Einstellungen, Auswahl beim Button (iconPicker).
import { t as tr } from '/shared/i18n.js';
import { h, api, toast, ask } from '../dom.js';
import { S, render, mutate } from '../state.js';
import { iconUrl } from '/shared/render.js';

const UPLOAD_BATCH = 20;

export const packIcon = (packId, file) => `packs/${packId}/${file}`;

/** Wie viele Buttons Bilder aus diesem Paket nutzen. */
const usage = (packId) => S.pages.flatMap((p) => p.buttons).filter((b) => b.icon.type === 'image' && b.icon.value.startsWith(`packs/${packId}/`)).length;

export async function loadIconPacks() {
  try {
    S.iconPacks = await api('GET', '/api/iconpacks');
  } catch (err) {
    S.iconPacks = [];
    toast(err.message, 'error');
  }
}

export const readDataUrl = (file) => new Promise((resolve, reject) => {
  const r = new FileReader();
  r.onload = () => resolve(r.result);
  r.onerror = () => reject(new Error(tr('iconpacks.cannotBeRead', [file.name])));
  r.readAsDataURL(file);
});

async function uploadImages(pack, fileList) {
  const files = [...fileList].filter((f) => f.size <= 5 * 1024 * 1024);
  const skipped = fileList.length - files.length;
  let saved = 0;
  try {
    for (let i = 0; i < files.length; i += UPLOAD_BATCH) {
      const batch = await Promise.all(files.slice(i, i + UPLOAD_BATCH).map(async (f) => ({ name: f.name, data: await readDataUrl(f) })));
      const r = await api('POST', '/api/iconpacks/add', { id: pack.id, files: batch });
      S.iconPacks = r.packs;
      saved += r.saved.length;
    }
    toast(`${tr(saved === 1 ? 'iconpacks.oneImageAddedTo' : 'iconpacks.imagesAddedTo', [saved, pack.name])}${skipped ? ` ${tr('iconpacks.largerThan5Mb', [skipped])}` : ''} ✓`, 'ok');
  } catch (err) {
    toast(err.message, 'error');
  }
  render();
}

async function importPack(file) {
  try {
    const r = await api('POST', '/api/iconpacks/import', { data: await readDataUrl(file), fileName: file.name });
    S.iconPacks = r.packs;
    S.openPack = r.id;
    toast(tr('iconpacks.iconPackWithImages', [r.count]), 'ok');
    render();
  } catch (err) {
    toast(err.message, 'error');
  }
}

function packForm(pack, onSave) {
  const f = { name: pack?.name ?? '', author: pack?.author ?? '', link: pack?.link ?? '', license: pack?.license ?? '' };
  const input = (key, label, placeholder) => h('label', { class: 'field' }, label,
    h('input', { value: f[key], placeholder, maxlength: key === 'link' ? 300 : 120, oninput: (e) => (f[key] = e.target.value) }));
  return h('div', { class: 'pack-form' },
    input('name', tr('common.name'), tr('iconpacks.exampleMyStreamingSet')),
    h('div', { class: 'row' }, input('author', tr('iconpacks.byOptional'), tr('iconpacks.yourName')), input('license', tr('iconpacks.licenseOptional'), tr('iconpacks.exampleCc0CcBy'))),
    input('link', tr('iconpacks.linkOptional'), 'https://…'),
    h('button', { class: 'btn primary small', onclick: () => onSave(f) }, pack ? tr('common.save') : tr('iconpacks.createPack')));
}

function packCard(pack) {
  const open = S.openPack === pack.id;
  const upload = h('input', { type: 'file', multiple: true, hidden: true, accept: 'image/png,image/jpeg,image/gif,image/webp,image/svg+xml',
    onchange: (e) => uploadImages(pack, e.target.files) });
  const head = h('div', { class: 'pack-head', onclick: () => { S.openPack = open ? null : pack.id; render(); } },
    h('div', { class: 'pack-preview' }, pack.icons.slice(0, 4).map((i) => h('img', { src: iconUrl(packIcon(pack.id, i)), alt: '', loading: 'lazy' }))),
    h('div', { class: 'pack-title' },
      h('b', {}, pack.name),
      h('small', {}, [tr('iconpacks.images', [pack.icons.length]), pack.author && tr('iconpacks.by', [pack.author]), pack.license].filter(Boolean).join(' · '))),
    h('span', { class: 'chevron' }, open ? '▾' : '▸'));
  if (!open) return h('div', { class: 'settings-card pack' }, head);

  return h('div', { class: 'settings-card pack open' },
    head,
    h('div', { class: 'pack-actions' },
      h('label', { class: 'btn small primary' }, tr('iconpacks.addImages'), upload),
      h('a', { class: 'btn small', href: `/iconpacks/export?id=${encodeURIComponent(pack.id)}`, download: '' }, tr('iconpacks.export')),
      h('button', { class: 'btn small ghost', onclick: () => { S.editPack = S.editPack === pack.id ? null : pack.id; render(); } }, tr('iconpacks.description')),
      h('div', { class: 'spacer' }),
      h('button', { class: 'btn small ghost danger', onclick: async () => {
        const used = usage(pack.id);
        const answer = await ask(tr('iconpacks.deleteIconPack'), `${tr('iconpacks.withImagesWillBe', [pack.name, pack.icons.length])}${used ? ` ${tr('iconpacks.buttonSUseImages', [used])}` : ''}`,
          [{ value: 'delete', label: tr('common.delete'), kind: 'danger' }]);
        if (answer !== 'delete') return;
        try {
          S.iconPacks = (await api('POST', '/api/iconpacks/delete', { id: pack.id })).packs;
          render();
        } catch (err) {
          toast(err.message, 'error');
        }
      } }, tr('iconpacks.deletePack'))),
    S.editPack === pack.id && packForm(pack, async (f) => {
      try {
        S.iconPacks = (await api('POST', '/api/iconpacks/update', { id: pack.id, ...f })).packs;
        S.editPack = null;
        render();
      } catch (err) {
        toast(err.message, 'error');
      }
    }),
    pack.link && h('p', { class: 'hint-text' }, h('a', { href: pack.link, target: '_blank', rel: 'noopener' }, pack.link)),
    pack.icons.length
      ? h('div', { class: 'icon-grid pack-icons' }, pack.icons.map((i) => h('div', { class: 'pack-icon', title: i },
        h('img', { src: iconUrl(packIcon(pack.id, i)), alt: '', loading: 'lazy' }),
        h('button', { class: 'remove', title: tr('iconpacks.removeImage'), onclick: async () => {
          try {
            S.iconPacks = (await api('POST', '/api/iconpacks/remove-icon', { id: pack.id, file: i })).packs;
            render();
          } catch (err) {
            toast(err.message, 'error');
          }
        } }, '✕'))))
      : h('p', { class: 'hint-text' }, tr('iconpacks.stillEmptyUseAdd')));
}

/** Eigener Bereich „Icon-Pakete“ in der linken Leiste (wie Plugins). */
export function mountIconPacks(root) {
  root.className = 'view view-plugins';
  root.replaceChildren(h('section', { id: 'iconpacks-detail', class: 'detail settings' }));
}

export function renderIconPacks() {
  const el = document.getElementById('iconpacks-detail');
  if (el) el.replaceChildren(...iconPacksSection().filter(Boolean));
}

// ---------- Icon-Editor: Bild aus Hintergrund, Symbol und Text zusammenbauen ----------

const SIZE = 256;

function drawIcon(canvas, o) {
  const c = canvas.getContext('2d');
  c.clearRect(0, 0, SIZE, SIZE);
  c.save();
  c.beginPath();
  if (o.shape === 'circle') c.arc(SIZE / 2, SIZE / 2, SIZE / 2, 0, Math.PI * 2);
  else if (o.shape === 'rounded') c.roundRect(0, 0, SIZE, SIZE, 44);
  else c.rect(0, 0, SIZE, SIZE);
  c.clip();
  if (o.background !== 'none') {
    const g = c.createLinearGradient(0, 0, 0, SIZE);
    g.addColorStop(0, o.color1);
    g.addColorStop(1, o.background === 'gradient' ? o.color2 : o.color1);
    c.fillStyle = g;
    c.fillRect(0, 0, SIZE, SIZE);
  }
  c.textAlign = 'center';
  c.textBaseline = 'middle';
  const hasText = o.text.trim() && o.textPos !== 'none';
  const emojiY = !hasText ? SIZE / 2 : o.textPos === 'top' ? SIZE * 0.58 : SIZE * 0.42;
  if (o.emoji) {
    c.font = `${Math.round(SIZE * o.emojiSize / 100)}px "Noto Color Emoji", "Apple Color Emoji", "Segoe UI Emoji", sans-serif`;
    c.fillText(o.emoji, SIZE / 2, emojiY);
  }
  if (hasText) {
    c.font = `700 ${o.textSize}px system-ui, sans-serif`;
    const y = o.textPos === 'top' ? SIZE * 0.14 : SIZE * 0.86;
    if (o.outline) {
      c.lineWidth = 6;
      c.strokeStyle = 'rgb(0 0 0 / 0.8)';
      c.lineJoin = 'round';
      c.strokeText(o.text, SIZE / 2, y, SIZE - 20);
    }
    c.fillStyle = o.textColor;
    c.fillText(o.text, SIZE / 2, y, SIZE - 20);
  }
  c.restore();
}

/** Kleiner Editor: Vorschau zeichnet sich bei jeder Eingabe neu, Speichern legt ein PNG an. */
function iconEditor() {
  S.iconEditor ??= { background: 'gradient', color1: '#5b6cff', color2: '#23264a', shape: 'rounded', emoji: '🎮', emojiSize: 55, text: '', textColor: '#ffffff', textSize: 34, textPos: 'bottom', outline: true, target: 'own' };
  const o = S.iconEditor;
  const canvas = h('canvas', { width: SIZE, height: SIZE, class: 'icon-editor-canvas' });
  const redraw = () => drawIcon(canvas, o);
  const set = (k, v) => { o[k] = v; redraw(); };
  const field = (label, input) => h('label', { class: 'field' }, label, input);
  const select = (k, options) => h('select', { value: o[k], onchange: (e) => set(k, e.target.value) }, options.map(([v, l]) => h('option', { value: v }, l)));
  const save = async () => {
    const data = canvas.toDataURL('image/png');
    try {
      if (o.target === 'own') {
        const { name } = await api('POST', '/api/icons', { data });
        S.icons.push(name);
        toast(tr('iconpacks.iconSavedOwn', [name]), 'ok');
      } else {
        const pack = S.iconPacks.find((p) => p.id === o.target);
        const base = (o.text || 'icon').replace(/[^a-zA-Z0-9_-]+/g, '_').replace(/^_+|_+$/g, '').slice(0, 30) || 'icon';
        const file = `${base}_${Date.now().toString(36)}.png`;
        const r = await api('POST', '/api/iconpacks/add', { id: o.target, files: [{ name: file, data }] });
        S.iconPacks = r.packs;
        toast(tr('iconpacks.iconSavedPack', [pack?.name ?? '']), 'ok');
      }
      render();
    } catch (err) {
      toast(err.message, 'error');
    }
  };
  requestAnimationFrame(redraw);
  return h('div', { class: 'settings-card icon-editor' },
    h('div', { class: 'icon-editor-preview' }, canvas),
    h('div', { class: 'icon-editor-form' },
      h('div', { class: 'row' },
        field(tr('iconpacks.background'), select('background', [['gradient', tr('iconpacks.gradient')], ['solid', tr('iconpacks.solid')], ['none', tr('iconpacks.transparent')]])),
        field(tr('iconpacks.shape'), select('shape', [['rounded', tr('iconpacks.rounded')], ['square', tr('iconpacks.square')], ['circle', tr('iconpacks.circle')]]))),
      h('div', { class: 'colors' },
        h('label', {}, h('input', { type: 'color', value: o.color1, oninput: (e) => set('color1', e.target.value) }), tr('iconpacks.color1')),
        h('label', {}, h('input', { type: 'color', value: o.color2, oninput: (e) => set('color2', e.target.value) }), tr('iconpacks.color2'))),
      h('div', { class: 'row' },
        field(tr('iconpacks.symbol'), h('input', { value: o.emoji, maxlength: 16, placeholder: '🎮', oninput: (e) => set('emoji', e.target.value.trim()) })),
        field(tr('iconpacks.symbolSize'), h('input', { type: 'range', min: 20, max: 90, value: o.emojiSize, oninput: (e) => set('emojiSize', Number(e.target.value)) }))),
      h('div', { class: 'row' },
        field(tr('iconpacks.text'), h('input', { value: o.text, maxlength: 24, placeholder: tr('iconpacks.textPlaceholder'), oninput: (e) => set('text', e.target.value) })),
        field(tr('iconpacks.textPosition'), select('textPos', [['bottom', tr('iconpacks.bottom')], ['top', tr('iconpacks.top')], ['none', tr('iconpacks.noText')]]))),
      h('div', { class: 'row' },
        field(tr('iconpacks.textSize'), h('input', { type: 'range', min: 14, max: 64, value: o.textSize, oninput: (e) => set('textSize', Number(e.target.value)) })),
        h('div', { class: 'colors' },
          h('label', {}, h('input', { type: 'color', value: o.textColor, oninput: (e) => set('textColor', e.target.value) }), tr('iconpacks.textColor')),
          h('label', { class: 'switch-row compact' }, h('input', { type: 'checkbox', checked: o.outline, onchange: (e) => set('outline', e.target.checked) }), tr('iconpacks.outline')))),
      h('div', { class: 'row' },
        field(tr('iconpacks.saveTo'), h('select', { value: o.target, onchange: (e) => (o.target = e.target.value) },
          [h('option', { value: 'own' }, tr('iconpacks.ownImagesShort')), ...S.iconPacks.map((p) => h('option', { value: p.id }, p.name))])),
        h('button', { class: 'btn primary', style: 'align-self:end', onclick: save }, tr('iconpacks.saveIcon')))));
}

function iconPacksSection() {
  const title = [h('h1', {}, tr('common.iconPacks')), h('p', { class: 'hint-text' },
    tr('iconpacks.yourOwnImageCollections'))];
  const importInput = h('input', { type: 'file', hidden: true, accept: '.tdicons,.zip,application/zip', onchange: (e) => e.target.files[0] && importPack(e.target.files[0]) });
  return [
    ...title,
    h('div', { class: 'settings-card' },
      h('div', { class: 'pack-actions' },
        h('button', { class: 'btn primary small', onclick: () => { S.newPack = !S.newPack; render(); } }, tr('iconpacks.newPack')),
        h('label', { class: 'btn small' }, tr('iconpacks.importPackTdiconsZip'), importInput),
        h('button', { class: 'btn small', onclick: () => { S.showIconEditor = !S.showIconEditor; render(); } }, tr('iconpacks.createIcon'))),
      S.showIconEditor && iconEditor(),
      S.newPack && packForm(null, async (f) => {
        try {
          const r = await api('POST', '/api/iconpacks', f);
          S.iconPacks = r.packs;
          S.newPack = false;
          S.openPack = r.id;
          render();
        } catch (err) {
          toast(err.message, 'error');
        }
      })),
    ...(S.iconPacks.length ? S.iconPacks.map(packCard) : [h('p', { class: 'hint-text' }, tr('iconpacks.noIconPacksYet'))]),
  ];
}

/** Bildauswahl beim Button: eigene Bilder und alle Icon-Pakete, mit Suche. key: icon oder iconOn. */
export function iconPicker(b, key = 'icon', toggle = 'showIcons') {
  const sources = [['own', tr('iconpacks.ownImages', [S.icons.length]), S.icons.map((n) => ({ value: n, label: n }))],
    ...S.iconPacks.map((p) => [p.id, `${p.name} (${p.icons.length})`, p.icons.map((i) => ({ value: packIcon(p.id, i), label: i }))])];
  if (!sources.some(([, , list]) => list.length)) return null;
  if (!sources.some(([id]) => id === S.iconSource)) S.iconSource = sources.find(([, , l]) => l.length)?.[0] ?? 'own';
  const [, , list] = sources.find(([id]) => id === S.iconSource);
  const q = (S.iconSearch ?? '').toLowerCase();
  const shown = list.filter((i) => !q || i.label.toLowerCase().includes(q));
  const MAX = 240;
  return h('div', {},
    h('button', { class: 'btn small ghost', style: 'margin-top:8px', onclick: () => { S[toggle] = !S[toggle]; render(); } },
      tr('iconpacks.imagesIconPacks', [S[toggle] ? '▾' : '▸'])),
    S[toggle] && h('div', { class: 'picker' },
      h('div', { class: 'row' },
        h('select', { value: S.iconSource, onchange: (e) => { S.iconSource = e.target.value; render(); } }, sources.map(([id, label]) => h('option', { value: id }, label))),
        h('input', { type: 'search', placeholder: tr('common.search'), value: S.iconSearch ?? '', oninput: (e) => { S.iconSearch = e.target.value; render(); } })),
      shown.length
        ? h('div', { class: 'icon-grid' }, shown.slice(0, MAX).map((i) => h('button', { type: 'button', title: i.label, class: b[key]?.value === i.value ? 'active' : '',
          onclick: () => mutate(() => (b[key] = { type: 'image', value: i.value })) }, h('img', { src: iconUrl(i.value), alt: '', loading: 'lazy' }))))
        : h('p', { class: 'hint-text' }, tr('iconpacks.noImagesFound')),
      shown.length > MAX && h('p', { class: 'hint-text' }, tr('iconpacks.moreRefineSearch', [shown.length - MAX]))));
}
