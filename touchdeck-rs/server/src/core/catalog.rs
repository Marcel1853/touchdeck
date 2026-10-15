// Beschreibungen für den Editor: welche Aktionen, Auslöser, Live-Anzeigen und Regler es gibt.
// Reine Daten (Felder, Beschriftungen, Auswahllisten), die der Editor zum Bauen der Formulare braucht.
use std::collections::HashMap;
use std::sync::LazyLock;

use serde_json::Value;

fn parse(raw: &str) -> Value {
    serde_json::from_str(raw).expect("Katalog ist gültiges JSON")
}

pub static ACTION_TYPES: LazyLock<Value> = LazyLock::new(|| parse(include_str!("../../catalog/actions.json")));
pub static TRIGGER_TYPES: LazyLock<Value> = LazyLock::new(|| parse(include_str!("../../catalog/triggers.json")));
static LIVE: LazyLock<Value> = LazyLock::new(|| parse(include_str!("../../catalog/live.json")));

/** Englische Texte: deutscher Text → englischer Text (Namen, Kategorien, Felder, Auswahlwerte). */
static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| serde_json::from_str(include_str!("../../catalog/en.json")).expect("Übersetzung ist gültiges JSON"));

/** Einen Katalog (Liste von Aktionen, Auslösern oder Auswahlwerten) in die gewünschte Sprache bringen. */
pub fn localized(list: &Value, lang: &str) -> Value {
    if lang == "de" {
        return list.clone();
    }
    let tr = |v: &mut Value| {
        if let Some(t) = v.as_str().and_then(|s| EN.get(s)) {
            *v = Value::String(t.clone());
        }
    };
    let mut out = list.clone();
    for item in out.as_array_mut().into_iter().flatten() {
        for key in ["label", "category", "group"] {
            tr(&mut item[key]);
        }
        for f in item["fields"].as_array_mut().into_iter().flatten() {
            tr(&mut f["label"]);
            tr(&mut f["placeholder"]);
            for o in f["options"].as_array_mut().into_iter().flatten() {
                tr(&mut o["label"]);
            }
        }
    }
    out
}

/** Englische Namen für Platzhalter in Event-Daten ({Event.User} = {Event.Nutzer} …). */
pub const EVENT_ALIASES: [(&str, &str); 23] = [
    ("Nutzer", "User"), ("Wert", "Value"), ("Alt", "Old"), ("Szene", "Scene"), ("Quelle", "Source"), ("Gerät", "Device"),
    ("Spiel", "Game"), ("Datei", "File"), ("Kanal", "Channel"), ("Nachricht", "Message"), ("Belohnung", "Reward"),
    ("Kosten", "Cost"), ("Eingabe", "Input"), ("Zuschauer", "Viewers"), ("Monate", "Months"), ("Stufe", "Tier"),
    ("Anzahl", "Count"), ("Dauer", "Duration"), ("Zeit", "Time"), ("Status", "Status"), ("Name", "Name"),
    ("Filter", "Filter"), ("Bits", "Bits"),
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
