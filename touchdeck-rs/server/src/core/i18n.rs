// Meldungen des Servers auf Deutsch oder Englisch – je nach Einstellung „Sprache“.
// Im Code stehen Schlüssel wie "obs.noFilterSelected"; die Texte liegen je Sprache in lang/<sprache>.json.
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use serde_json::Value;

static ENGLISH: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(super::store::system_language() != "de"));
static DE: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(include_str!("../../lang/de.json")).expect("Sprachdatei ist gültiges JSON"));
static EN: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(include_str!("../../lang/en.json")).expect("Sprachdatei ist gültiges JSON"));

/** Legt die Sprache der Meldungen fest (de oder en). */
pub fn set_lang(lang: &str) {
    ENGLISH.store(lang != "de", Ordering::Relaxed);
}

/** Sind die Meldungen gerade englisch? */
pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

fn lookup(dict: &'static Value, key: &str) -> Option<&'static str> {
    key.split('.').try_fold(dict, |v, k| v.get(k))?.as_str()
}

/** Text zum Schlüssel in der eingestellten Sprache; fehlt er, gilt der deutsche, sonst der Schlüssel selbst. */
pub fn tr(key: &'static str) -> &'static str {
    let first = if is_english() { lookup(&EN, key) } else { None };
    first.or_else(|| lookup(&DE, key)).unwrap_or(key)
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
        let (mut de, mut en) = (Vec::new(), Vec::new());
        texts(&DE, "", &mut de);
        texts(&EN, "", &mut en);
        de.sort();
        en.sort();
        assert_eq!(de.iter().map(|x| &x.0).collect::<Vec<_>>(), en.iter().map(|x| &x.0).collect::<Vec<_>>(), "de.json und en.json haben verschiedene Schlüssel");
        for ((key, d), (_, e)) in de.iter().zip(&en) {
            assert!(!d.is_empty() && !e.is_empty(), "leerer Text: {key}");
            for i in 0..4 {
                let p = format!("{{{i}}}");
                assert_eq!(d.contains(&p), e.contains(&p), "Platzhalter {p} in {key}");
            }
        }
    }

    #[test]
    fn alle_schluessel_vorhanden() {
        // Jeder t!("…")-Schlüssel im Code muss in de.json stehen.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![dir];
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
                        assert!(lookup(&DE, key).is_some(), "{key} fehlt in lang/de.json ({})", p.display());
                    }
                }
            }
        }
    }
}
