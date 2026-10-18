// Zeichnet Seiten und Buttons – gemeinsam für Editor und Bedienfeld, damit beide gleich aussehen.

export function iconUrl(name) {
  return `/icons/${encodeURIComponent(name)}`;
}

/** Ersetzt {Name} in Beschriftungen durch den Wert der Variable. */
export function fillTitle(title, variables) {
  if (!variables) return title;
  return title.replace(/\{([^{}]{1,40})\}/g, (m, name) => (name in variables ? String(variables[name]) : m));
}

/** Freie Seite? Dann stehen Buttons nicht im Raster, sondern mit Lage in Prozent auf der Fläche. */
export const isFree = (page) => page?.layout === 'free';

/**
 * Erzeugt das DOM-Element für einen Button. live = { text, on, value } vom Server, variables für {Name}-Platzhalter,
 * page = die Seite (für freie Lage und Seitenverhältnis).
 */
export function buttonElement(b, { on = false, live = null, variables = null, page = null } = {}) {
  if (typeof live?.on === 'boolean') on = live.on;
  // Eigenes Aussehen für „an“ (z. B. 🔇 „Stumm“ statt 🔊 „Ton an“).
  if (on) b = { ...b, icon: b.iconOn?.value ? b.iconOn : b.icon, title: b.titleOn || b.title };
  const el = document.createElement('div');
  const isSlider = b.mode === 'slider';
  const cover = b.iconFit === 'cover' && b.icon?.type === 'image' && b.icon.value;
  el.className = `td-btn pos-${b.titlePos}${on ? ' is-on' : ''}${b.mode === 'toggle' ? ' is-toggle' : ''}${cover ? ' is-cover' : ''}`;
  el.dataset.id = b.id;
  const free = isFree(page) && b.free;
  if (free) {
    el.style.left = `${b.free.x}%`;
    el.style.top = `${b.free.y}%`;
    el.style.width = `${b.free.w}%`;
    el.style.height = `${b.free.h}%`;
    // Innenabstand: 7 % der kürzeren Button-Seite (cqw/cqh beziehen sich auf die Fläche), damit flache
    // oder schmale Buttons nie über ihre Größe hinauswachsen.
    el.style.padding = `min(${(b.free.w * 0.07).toFixed(2)}cqw, ${(b.free.h * 0.07).toFixed(2)}cqh)`;
  } else {
    el.style.gridRow = `${b.row + 1} / span ${b.rowSpan}`;
    el.style.gridColumn = `${b.col + 1} / span ${b.colSpan}`;
  }
  // Deckkraft des Hintergrunds: unter 100 % scheint die Seite durch, bei 0 % nur Symbol und Text.
  const opacity = b.bgOpacity ?? 100;
  const bg = on ? b.colorOn : b.color;
  el.style.setProperty('--bg', opacity < 100 ? `color-mix(in srgb, ${bg} ${opacity}%, transparent)` : bg);
  if (opacity === 0) el.classList.add('is-transparent');
  el.style.setProperty('--fg', b.textColor);
  el.style.setProperty('--fs', b.fontSize);
  // Schrift: Familie (überall vorhandene Schriften), fett/kursiv, Umriss für Text auf Bildern.
  const FONTS = { serif: 'Georgia, serif', mono: 'ui-monospace, monospace', condensed: '"Roboto Condensed", "Arial Narrow", sans-serif-condensed, sans-serif', handwriting: '"Comic Neue", "Comic Sans MS", cursive' };
  if (FONTS[b.fontFamily]) el.style.setProperty('--ff', FONTS[b.fontFamily]);
  if (b.bold === false) el.style.setProperty('--fw', 400);
  if (b.italic) el.style.setProperty('--fst', 'italic');
  if (b.textOutline > 0) el.style.setProperty('--outline', `${b.textOutline}px`);

  if (cover) el.style.setProperty('--img', `url("${iconUrl(b.icon.value)}")`);
  // Textur: eigenes Hintergrundbild (unter Symbol und Text) und Overlay darüber.
  if (b.bgImage?.type === 'image' && b.bgImage.value && !cover) {
    el.classList.add('has-texture', `tex-${b.texFit ?? 'cover'}`);
    el.style.setProperty('--tex', `url("${iconUrl(b.bgImage.value)}")`);
    el.style.setProperty('--veil', `${b.texVeil ?? 35}%`);
  }
  // Symbolgröße in Prozent der normalen Größe.
  if ((b.iconSize ?? 100) !== 100) el.style.setProperty('--is', (b.iconSize ?? 100) / 100);


  const shape = isSlider ? b.slider?.shape ?? 'fill' : null;
  const isKnob = shape === 'knob' || shape === 'dial';
  if (isSlider) {
    // Regler: hoch, wenn der Button höher als breit ist, sonst quer. Alles hängt an --val (0–100).
    const vertical = free ? b.free.h * page.ratioH >= b.free.w * page.ratioW : b.rowSpan >= b.colSpan;
    const value = Number.isFinite(live?.value) ? live.value : 0;
    el.classList.add('is-slider', vertical ? 'vertical' : 'horizontal', `shape-${shape}`);
    el.dataset.vertical = vertical ? '1' : '';
    if (isKnob) el.dataset.knob = '1';
    el.style.setProperty('--val', value);
    el.style.setProperty('--thick', b.slider?.thickness ?? 10);
    if (b.slider?.color) el.style.setProperty('--sc', b.slider.color);
    if (shape === 'fill') {
      const fill = document.createElement('div');
      fill.className = 'td-fill';
      el.append(fill);
    } else if (shape === 'rail' || shape === 'fader') {
      const rail = document.createElement('div');
      rail.className = 'td-rail';
      rail.innerHTML = '<span class="td-rail-fill"></span><i class="td-rail-handle"></i>';
      el.append(rail);
    } else {
      // Drehregler: Ring über 270° (unten links bis unten rechts), wahlweise mit Zeiger.
      const knob = document.createElement('div');
      knob.className = 'td-knob';
      knob.innerHTML = `<svg viewBox="0 0 100 100"><circle class="track" cx="50" cy="50" r="40" pathLength="100"/>`
        + `<circle class="value" cx="50" cy="50" r="40" pathLength="100"/>${shape === 'dial' ? '<line class="pointer" x1="50" y1="50" x2="50" y2="22"/>' : ''}</svg>`;
      el.append(knob);
    }
    live = { text: live?.text ?? `${value} %` };
  }

  // Beim Drehregler steht der Wert in der Mitte, ein Symbol würde den Ring verdecken.
  // Webseite im Button: eingebettete Seite mit Zoom; Symbol entfällt.
  const isWeb = b.mode === 'web' && b.web?.url;
  if (isWeb) {
    const zoom = (b.web.zoom ?? 100) / 100;
    const frame = document.createElement('iframe');
    frame.className = 'td-web';
    frame.src = b.web.url;
    frame.loading = 'lazy';
    frame.referrerPolicy = 'no-referrer';
    // Keine Popups, keine neuen Fenster, kein Wegnavigieren des Bedienfelds – nur die Seite selbst.
    frame.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-forms');
    frame.style.width = `${100 / zoom}%`;
    frame.style.height = `${100 / zoom}%`;
    frame.style.transform = `scale(${zoom})`;
    if (!b.web.interactive) frame.style.pointerEvents = 'none';
    if (b.web.refresh > 0) frame.dataset.refresh = b.web.refresh;
    el.classList.add('is-web');
    el.append(frame);
  }
  const hasIcon = b.icon?.value && !cover && !isKnob && !isWeb;
  if (hasIcon) {
    const icon = document.createElement('div');
    icon.className = 'td-icon';
    if (b.icon.type === 'image') {
      const img = document.createElement('img');
      img.src = iconUrl(b.icon.value);
      img.alt = '';
      img.draggable = false;
      icon.append(img);
    } else {
      icon.textContent = b.icon.value;
    }
    el.append(icon);
  }
  // Fortschrittsbalken: Zahlenwert 0–100 der Live-Anzeige, unten im Button.
  if (!isSlider && b.live?.bar && Number.isFinite(live?.value)) {
    const bar = document.createElement('div');
    bar.className = 'td-bar';
    bar.style.setProperty('--val', Math.max(0, Math.min(100, live.value)));
    el.append(bar);
    // Unten Platz für den Balken lassen, damit Text ihn nicht überdeckt.
    el.classList.add('has-bar');
    if (free) el.style.paddingBottom = `calc(min(${(b.free.w * 0.07).toFixed(2)}cqw, ${(b.free.h * 0.07).toFixed(2)}cqh) + ${(b.free.h * 0.1).toFixed(2)}cqh)`;
  }
  if (live?.text) {
    const l = document.createElement('div');
    l.className = 'td-live';
    l.textContent = live.text;
    if (live.error) l.title = live.error;
    el.append(l);
  }
  if (b.title && b.titlePos !== 'hidden') {
    const t = document.createElement('div');
    t.className = 'td-title';
    t.textContent = fillTitle(b.title, variables);
    el.append(t);
  }
  if (b.overlay) {
    const ov = document.createElement('div');
    ov.className = `td-overlay ov-${b.overlay}`;
    el.append(ov);
  }
  if (!hasIcon && !cover && !isSlider && !isWeb && !live?.text && !(b.title && b.titlePos !== 'hidden') && !b.overlay && !el.classList.contains('has-texture')) el.classList.add('is-blank');
  return el;
}

const sum = (list) => list.reduce((a, n) => a + n, 0);

/**
 * Richtet ein Raster-Element für eine Seite ein. --cols/--rows bestimmen das Seitenverhältnis:
 * im Raster Anzahl (oder Summe der eigenen Größen), auf freien Seiten das Seitenverhältnis.
 */
export function setupGrid(grid, page) {
  const free = isFree(page);
  const colSizes = !free && page.colSizes?.length === page.cols ? page.colSizes : null;
  const rowSizes = !free && page.rowSizes?.length === page.rows ? page.rowSizes : null;
  grid.classList.toggle('is-free', free);
  grid.style.setProperty('--rows', free ? page.ratioH : rowSizes ? sum(rowSizes) : page.rows);
  grid.style.setProperty('--cols', free ? page.ratioW : colSizes ? sum(colSizes) : page.cols);
  grid.style.setProperty('--page-bg', page.background);
  grid.style.gridTemplateColumns = colSizes ? colSizes.map((n) => `minmax(0, ${n}fr)`).join(' ') : '';
  grid.style.gridTemplateRows = rowSizes ? rowSizes.map((n) => `minmax(0, ${n}fr)`).join(' ') : '';
}
