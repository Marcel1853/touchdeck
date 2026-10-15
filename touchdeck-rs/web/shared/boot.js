// Start von Editor und Bedienfeld: erst die Sprache holen, dann die eigentliche Oberfläche laden.
// So sind auch Texte übersetzt, die beim Laden der Module schon feststehen.
// Die Texte selbst stehen in shared/lang/de.json und en.json.
import { setLang, translateDom } from '/shared/i18n.js';

const main = document.currentScript?.dataset.main ?? document.querySelector('script[data-main]').dataset.main;
try {
  const hello = await fetch('/api/hello', { cache: 'no-store' }).then((r) => r.json());
  await setLang(hello.language ?? 'de');
} catch {
  // Ohne Verbindung zunächst nach der Sprache des Browsers.
  await setLang(navigator.language?.startsWith('de') ? 'de' : 'en');
}
translateDom();
await import(main);
