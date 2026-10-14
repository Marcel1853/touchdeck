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
