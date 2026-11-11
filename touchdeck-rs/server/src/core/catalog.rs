// Beschreibungen für den Editor: welche Aktionen, Auslöser, Live-Anzeigen und Regler es gibt.
// Reine Daten (Felder, Beschriftungen, Auswahllisten), die der Editor zum Bauen der Formulare braucht.
use std::sync::LazyLock;

use serde_json::Value;

fn parse(raw: &str) -> Value {
    serde_json::from_str(raw).expect("Katalog ist gültiges JSON")
}

pub static ACTION_TYPES: LazyLock<Value> = LazyLock::new(|| parse(include_str!("../../catalog/actions.json")));
pub static TRIGGER_TYPES: LazyLock<Value> = LazyLock::new(|| parse(include_str!("../../catalog/triggers.json")));
static LIVE: LazyLock<Value> = LazyLock::new(|| parse(include_str!("../../catalog/live.json")));

/** Texte des Katalogs je Sprache (catalog/lang/<sprache>.json), nach Aktion, Feld und Auswahlwert geordnet; Reihenfolge wie i18n::LANGUAGES. */
static TEXTS: LazyLock<[Value; 8]> = LazyLock::new(|| {
    [
        parse(include_str!("../../catalog/lang/de.json")),
        parse(include_str!("../../catalog/lang/en.json")),
        parse(include_str!("../../catalog/lang/es.json")),
        parse(include_str!("../../catalog/lang/fr.json")),
        parse(include_str!("../../catalog/lang/pl.json")),
        parse(include_str!("../../catalog/lang/pt-br.json")),
        parse(include_str!("../../catalog/lang/ru.json")),
        parse(include_str!("../../catalog/lang/zh-cn.json")),
    ]
});

/** Sprachdateien in der Reihenfolge, in der ein Text gesucht wird: gewünschte Sprache, Englisch, Deutsch. */
fn dicts_for(lang: &str) -> Vec<&'static Value> {
    let texts = &*TEXTS;
    let i = super::i18n::LANGUAGES.iter().position(|l| *l == lang).unwrap_or(1);
    let mut order = vec![i, 1, 0];
    order.dedup();
    order.into_iter().map(|n| &texts[n]).collect()
}

/** Welche Liste übersetzt wird: bestimmt den Bereich in der Sprachdatei. */
#[derive(Clone, Copy)]
pub enum Section {
    Actions,
    Triggers,
    LiveSources,
    SliderTargets,
}

/** Einen Katalog mit den Texten der gewünschten Sprache füllen (fehlt ein Text, gilt der englische, dann der deutsche). */
pub fn localized(list: &Value, section: Section, lang: &str) -> Value {
    let dicts = dicts_for(lang);
    let text = |path: &[&str]| -> Option<Value> { dicts.iter().find_map(|d| path.iter().try_fold(*d, |v, k| v.get(*k)).filter(|v| v.is_string()).cloned()) };
    let mut out = list.clone();
    for item in out.as_array_mut().into_iter().flatten() {
        let (sect, id) = match section {
            Section::Actions => ("actions", item["type"].as_str().unwrap_or_default().to_string()),
            Section::Triggers => ("triggers", item["type"].as_str().unwrap_or_default().to_string()),
            Section::LiveSources => ("liveSources", item["value"].as_str().unwrap_or_default().to_string()),
            Section::SliderTargets => ("sliderTargets", item["value"].as_str().unwrap_or_default().to_string()),
        };
        if matches!(section, Section::LiveSources | Section::SliderTargets) {
            if let Some(t) = text(&[sect, &id]) {
                item["label"] = t;
            }
            continue;
        }
        if let Some(t) = text(&[sect, &id, "label"]) {
            item["label"] = t;
        }
        for (key, area) in [("category", "categories"), ("group", "groups")] {
            if let Some(t) = item[key].as_str().and_then(|c| text(&[area, c])) {
                item[key] = t;
            }
        }
        for f in item["fields"].as_array_mut().into_iter().flatten() {
            let key = f["key"].as_str().unwrap_or_default().to_string();
            for attr in ["label", "placeholder"] {
                if let Some(t) = text(&[sect, &id, "fields", &key, attr]) {
                    f[attr] = t;
                }
            }
            for o in f["options"].as_array_mut().into_iter().flatten() {
                let value = match &o["value"] {
                    Value::String(s) => s.clone(),
                    v => v.to_string(),
                };
                if let Some(t) = text(&[sect, &id, "fields", &key, "options", &value]) {
                    o["label"] = t;
                }
            }
        }
    }
    out
}

/** Name einer eingebauten Aktion in der gewünschten Sprache. */
pub fn action_label(kind: &str, lang: &str) -> Option<String> {
    dicts_for(lang).into_iter().find_map(|d| d["actions"][kind]["label"].as_str()).map(String::from)
}

/** Englische Namen für Platzhalter in Event-Daten ({Event.User} = {Event.Nutzer} …). */
pub const EVENT_ALIASES: [(&str, &str); 36] = [
    ("Nutzer", "User"), ("Wert", "Value"), ("Alt", "Old"), ("Szene", "Scene"), ("Quelle", "Source"), ("Gerät", "Device"),
    ("Spiel", "Game"), ("Datei", "File"), ("Kanal", "Channel"), ("Nachricht", "Message"), ("Belohnung", "Reward"),
    ("Kosten", "Cost"), ("Eingabe", "Input"), ("Zuschauer", "Viewers"), ("Monate", "Months"), ("Stufe", "Tier"),
    ("Anzahl", "Count"), ("Dauer", "Duration"), ("Zeit", "Time"), ("Status", "Status"), ("Name", "Name"),
    ("Filter", "Filter"), ("Bits", "Bits"), ("Seite", "Page"), ("Programm", "Program"), ("Titel", "Title"), ("Tasten", "Keys"),
    ("Befehl", "Command"), ("Argumente", "Args"), ("Auswahl", "Choices"), ("Gewinner", "Winner"), ("Stimmen", "Votes"),
    ("Punkte", "Points"), ("Fortschritt", "Progress"), ("Ziel", "Goal"), ("Gesamt", "Total"),
];

pub fn live_sources() -> &'static Value {
    &LIVE["liveSources"]
}

pub fn slider_targets() -> &'static Value {
    &LIVE["sliderTargets"]
}

fn ids(list: &'static Value) -> Vec<&'static str> {
    list.as_array().into_iter().flatten().filter_map(|o| o["value"].as_str()).collect()
}

pub fn live_source_ids() -> Vec<&'static str> {
    ids(live_sources())
}

pub fn slider_target_ids() -> Vec<&'static str> {
    ids(slider_targets())
}

#[cfg(test)]
mod tests {
    use super::*;

    /** Alle Schlüssel als Pfade, z. B. actions.open_url.fields.url.label. */
    fn paths(v: &Value, prefix: &str, out: &mut Vec<String>) {
        match v.as_object() {
            Some(o) => o.iter().for_each(|(k, v)| paths(v, &format!("{prefix}{k}."), out)),
            None => out.push(prefix.trim_end_matches('.').to_string()),
        }
    }

    #[test]
    fn sprachdateien_vollstaendig() {
        let mut de = Vec::new();
        paths(&TEXTS[0], "", &mut de);
        de.sort();
        for (n, lang) in super::super::i18n::LANGUAGES.iter().enumerate().skip(1) {
            let mut other = Vec::new();
            paths(&TEXTS[n], "", &mut other);
            other.sort();
            assert_eq!(de, other, "de.json und {lang}.json haben verschiedene Schlüssel");
        }
        for (list, section) in [(&*ACTION_TYPES, Section::Actions), (&*TRIGGER_TYPES, Section::Triggers)] {
            for lang in super::super::i18n::LANGUAGES {
                for item in localized(list, section, lang).as_array().unwrap() {
                    assert!(item["label"].is_string(), "Name fehlt: {}", item["type"]);
                    for f in item["fields"].as_array().unwrap() {
                        assert!(f["label"].is_string(), "Feld ohne Text: {} {}", item["type"], f["key"]);
                        for o in f["options"].as_array().into_iter().flatten() {
                            assert!(o["label"].is_string(), "Auswahl ohne Text: {} {} {}", item["type"], f["key"], o["value"]);
                        }
                    }
                }
            }
        }
        assert_eq!(action_label("open_url", "en").as_deref(), Some("Open website"));
    }
}
