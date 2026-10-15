// Übersetzung der Oberfläche. Der deutsche Text im Code ist zugleich der Schlüssel;
// die Wörterbücher in lang/ liefern die anderen Sprachen. Fehlt ein Eintrag, bleibt es Deutsch.
import en from './lang/en.js';

const DICTS = { en };
let lang = 'de';

/** Sprache setzen: de, en (unbekannte Sprachen fallen auf Englisch zurück). */
export function setLang(l) {
  lang = l === 'de' || DICTS[l] ? l : 'en';
  document.documentElement.lang = lang;
}

export const getLang = () => lang;

/**
 * Text übersetzen. Platzhalter {0}, {1} … (Liste) oder {name} (Objekt) werden eingesetzt:
 * t('{0} Bilder hinzugefügt', [n])
 */
export function t(text, vars) {
  let s = lang === 'de' ? text : DICTS[lang]?.[text] ?? text;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  return s;
}

/** Feste Texte im HTML übersetzen: data-i18n (Inhalt), data-i18n-title, data-i18n-placeholder, data-i18n-aria-label. */
export function translateDom(root = document) {
  for (const el of root.querySelectorAll('[data-i18n]')) {
    el.dataset.i18nDe ??= el.textContent.trim();
    el.textContent = t(el.dataset.i18nDe);
  }
  for (const attr of ['title', 'placeholder', 'aria-label']) {
    for (const el of root.querySelectorAll(`[data-i18n-${attr}]`)) {
      const key = `i18nDe${attr.replace(/-./g, (x) => x[1].toUpperCase()).replace(/^./, (x) => x.toUpperCase())}`;
      el.dataset[key] ??= el.getAttribute(attr) ?? '';
      el.setAttribute(attr, t(el.dataset[key]));
    }
  }
}
