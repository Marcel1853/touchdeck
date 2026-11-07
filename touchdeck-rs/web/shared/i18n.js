// Übersetzung der Oberfläche. Im Code stehen Schlüssel wie 'deck.notConnected';
// die Texte liegen je Sprache in lang/<sprache>.json, nach Bereichen gruppiert.
// Fehlt ein Text in der gewählten Sprache, wird der englische genommen, dann der deutsche.
export const LANGUAGES = ['de', 'en', 'es', 'fr', 'pl', 'pt-br', 'ru', 'zh-cn'];

/** Namen der Sprachen in ihrer eigenen Sprache (für die Auswahl). */
export const LANGUAGE_NAMES = { de: 'Deutsch', en: 'English', es: 'Español', fr: 'Français', pl: 'Polski', 'pt-br': 'Português (Brasil)', ru: 'Русский', 'zh-cn': '简体中文' };

/** Sprachkürzel aus einer Browser-Sprache wie „pt-BR“ oder „fr-CA“; Unbekanntes wird Englisch. */
export function languageOf(locale) {
  const l = String(locale ?? '').toLowerCase().replace('_', '-');
  return LANGUAGES.find((x) => x === l) ?? LANGUAGES.find((x) => x.split('-')[0] === l.split('-')[0]) ?? 'en';
}
const dicts = {};
let lang = 'de';

async function load(l) {
  dicts[l] ??= await fetch(`/shared/lang/${l}.json`).then((r) => r.json());
}

/** Sprache setzen und ihre Texte laden (unbekannte Sprachen fallen auf Englisch zurück). */
export async function setLang(l) {
  lang = LANGUAGES.includes(l) ? l : 'en';
  document.documentElement.lang = lang;
  await Promise.all([load('de'), load('en'), load(lang)]);
}

export const getLang = () => lang;

const lookup = (dict, key) => key.split('.').reduce((o, k) => o?.[k], dict);

/**
 * Text zum Schlüssel holen. Platzhalter {0}, {1} … (Liste) oder {name} (Objekt) werden eingesetzt:
 * t('iconpacks.imagesAdded', [n, name])
 */
export function t(key, vars) {
  let s = lookup(dicts[lang], key) ?? lookup(dicts.en, key) ?? lookup(dicts.de, key) ?? key;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  return s;
}

/** Feste Texte im HTML übersetzen: data-i18n (Inhalt), data-i18n-title, data-i18n-placeholder, data-i18n-aria-label. */
export function translateDom(root = document) {
  for (const el of root.querySelectorAll('[data-i18n]')) el.textContent = t(el.dataset.i18n);
  for (const attr of ['title', 'placeholder', 'aria-label']) {
    for (const el of root.querySelectorAll(`[data-i18n-${attr}]`)) el.setAttribute(attr, t(el.getAttribute(`data-i18n-${attr}`)));
  }
}
