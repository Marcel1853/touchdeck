// Meldungen des Servers in der eingestellten Sprache (Einstellung „Sprache“).
// Im Code stehen Schlüssel wie "obs.noFilterSelected"; die Texte liegen je Sprache in lang/<sprache>.json.
use std::fmt::Display;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

use serde_json::Value;

/** Sprachen von TouchDeck (Kürzel wie in den Plugin-Übersetzungen). Deutsch ist die Grundlage, Englisch der Rückfall. */
pub const LANGUAGES: [&str; 8] = ["de", "en", "es", "fr", "pl", "pt-br", "ru", "zh-cn"];

/** Sprachen, die Dezimalzahlen mit Komma schreiben (3,5 statt 3.5). */
const DECIMAL_COMMA: [&str; 6] = ["de", "es", "fr", "pl", "pt-br", "ru"];

fn parse(raw: &str) -> Value {
    serde_json::from_str(raw).expect("Sprachdatei ist gültiges JSON")
}

/** Texte je Sprache, in der Reihenfolge von LANGUAGES. */
static DICTS: LazyLock<[Value; 8]> = LazyLock::new(|| {
    [
        parse(include_str!("../../lang/de.json")),
        parse(include_str!("../../lang/en.json")),
        parse(include_str!("../../lang/es.json")),
        parse(include_str!("../../lang/fr.json")),
        parse(include_str!("../../lang/pl.json")),
        parse(include_str!("../../lang/pt-br.json")),
        parse(include_str!("../../lang/ru.json")),
        parse(include_str!("../../lang/zh-cn.json")),
    ]
});

// Tests prüfen deutsche Texte – dort immer Deutsch, unabhängig von der Sprache des Rechners.
static CURRENT: LazyLock<AtomicUsize> = LazyLock::new(|| AtomicUsize::new(if cfg!(test) { 0 } else { index(&super::store::system_language()) }));

/** Platz einer Sprache in LANGUAGES; Unbekanntes gilt als Englisch. */
fn index(lang: &str) -> usize {
    LANGUAGES.iter().position(|l| *l == lang).unwrap_or(1)
}

/** Legt die Sprache der Meldungen fest (eines der Kürzel aus LANGUAGES). */
pub fn set_lang(lang: &str) {
    CURRENT.store(index(lang), Ordering::Relaxed);
}

/** Kürzel der eingestellten Sprache. */
pub fn lang() -> &'static str {
    LANGUAGES[CURRENT.load(Ordering::Relaxed)]
}

/** Schreibt die eingestellte Sprache Dezimalzahlen mit Komma? */
pub fn decimal_comma() -> bool {
    DECIMAL_COMMA.contains(&lang())
}

fn lookup(dict: &'static Value, key: &str) -> Option<&'static str> {
    key.split('.').try_fold(dict, |v, k| v.get(k))?.as_str()
}

/** Text zum Schlüssel in der eingestellten Sprache; fehlt er, gilt der englische, dann der deutsche, sonst der Schlüssel. */
pub fn tr(key: &'static str) -> &'static str {
    let dicts = &*DICTS;
    let i = CURRENT.load(Ordering::Relaxed);
    [i, 1, 0].iter().find_map(|&n| lookup(&dicts[n], key)).unwrap_or(key)
}

/** Setzt {0}, {1} … in den Text ein. */
pub fn fill(text: &str, args: &[&dyn Display]) -> String {
    let mut out = text.to_string();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &a.to_string());
    }
    out
}

/// `t!("obs.noFilterSelected")` holt den Text, `t!("twitch.twitchUserNotFound", name)` setzt dazu Werte für {0}, {1} … ein.
#[macro_export]
macro_rules! t {
    ($s:literal) => { $crate::core::i18n::tr($s) };
    ($s:literal, $($a:expr),+ $(,)?) => { $crate::core::i18n::fill($crate::core::i18n::tr($s), &[$(&$a as &dyn std::fmt::Display),+]) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(v: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
        match v {
            Value::Object(o) => o.iter().for_each(|(k, v)| texts(v, &format!("{prefix}{k}."), out)),
            v => out.push((prefix.trim_end_matches('.').to_string(), v.as_str().unwrap_or_default().to_string())),
        }
    }

    #[test]
    fn sprachdateien_passen_zusammen() {
        assert_eq!(fill("{0} und {1}", &[&"a", &2]), "a und 2");
        let mut de = Vec::new();
        texts(&DICTS[0], "", &mut de);
        de.sort();
        for (n, lang) in LANGUAGES.iter().enumerate().skip(1) {
            let mut other = Vec::new();
            texts(&DICTS[n], "", &mut other);
            other.sort();
            assert_eq!(de.iter().map(|x| &x.0).collect::<Vec<_>>(), other.iter().map(|x| &x.0).collect::<Vec<_>>(), "de.json und {lang}.json haben verschiedene Schlüssel");
            for ((key, d), (_, e)) in de.iter().zip(&other) {
                assert!(!d.is_empty() && !e.is_empty(), "leerer Text: {key} ({lang})");
                for i in 0..4 {
                    let p = format!("{{{i}}}");
                    assert_eq!(d.contains(&p), e.contains(&p), "Platzhalter {p} in {key} ({lang})");
                }
            }
        }
    }

    #[test]
    fn alle_schluessel_vorhanden() {
        // Jeder t!("…")-Schlüssel im Code muss in de.json stehen.
        // Server und Desktop-App (die App nutzt dieselben Sprachdateien).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut stack = vec![root.join("src"), root.join("../app/src")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                for (i, _) in src.match_indices("t!(\"") {
                    if src[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                        continue; // z. B. format!(
                    }
                    let key = src[i + 4..].split('"').next().unwrap();
                    if key.contains('.') && !key.contains(' ') {
                        assert!(lookup(&DICTS[0], key).is_some(), "{key} fehlt in lang/de.json ({})", p.display());
                    }
                }
            }
        }
    }
}
