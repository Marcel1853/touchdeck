// Profile: Seiten samt allem, was sie brauchen, als eigene Datei (.tdprofile) weitergeben.
// Aufbau: ZIP mit profile.json (Seiten, benutzte Vorlagen, Werte und Events) und images/ (benutzte Bilder).
// Beim Import bekommt alles neue IDs; Verweise untereinander werden mit umgestellt, Vorhandenes bleibt.
use crate::t;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};

use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};

use super::store::{Store, new_id};

pub const FORMAT: &str = "touchdeck-profile";
const FORMAT_VERSION: u64 = 1;
const MAX_SIZE: u64 = 100 * 1024 * 1024;
const IMAGE_EXT: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "svg"];

/** Alle Zeichenketten in einem JSON-Wert (zum Finden von Verweisen). */
fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(l) => l.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

/** Ersetzt überall Zeichenketten, die genau einer alten ID entsprechen. */
fn remap(v: &mut Value, ids: &HashMap<String, String>) {
    match v {
        Value::String(s) => {
            if let Some(n) = ids.get(s.as_str()) {
                *s = n.clone();
            }
        }
        Value::Array(l) => l.iter_mut().for_each(|x| remap(x, ids)),
        Value::Object(o) => o.values_mut().for_each(|x| remap(x, ids)),
        _ => {}
    }
}

/** Bildnamen, die Buttons der Seiten benutzen (eigene Bilder und Bilder aus Icon-Paketen). */
fn used_images(pages: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for b in pages.iter().flat_map(|p| p["buttons"].as_array().into_iter().flatten()) {
        for key in ["icon", "iconOn", "bgImage"] {
            if b[key]["type"] == "image"
                && let Some(name) = b[key]["value"].as_str().filter(|n| !n.is_empty())
                && !out.iter().any(|o| o == name)
            {
                out.push(name.to_string());
            }
        }
    }
    out
}

/** Sicherer Dateiname im Profil: nur Buchstaben, Ziffern, Punkt, Minus und Unterstrich. */
fn safe_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || ".-_".contains(c) { c } else { '_' }).collect()
}

fn image_ext(name: &str) -> Option<&str> {
    let ext = name.rsplit_once('.')?.1;
    IMAGE_EXT.iter().find(|e| e.eq_ignore_ascii_case(ext)).copied()
}

/**
 * Exportiert die Seiten (IDs; leer = alle) als .tdprofile. Liefert (Dateiname, Inhalt).
 */
pub fn export(store: &Store, page_ids: &[String], name: &str) -> Result<(String, Vec<u8>)> {
    let c = &store.config;
    let pages: Vec<Value> = c
        .pages
        .iter()
        .filter(|p| page_ids.is_empty() || page_ids.contains(&p.id))
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()?;
    if pages.is_empty() {
        bail!(t!("profiles.noPagesSelected"));
    }
    // Was die Seiten benutzen: Vorlagen (über ihre ID), Werte (über den Namen), Events (über IDs oder Werte).
    let mut texts = Vec::new();
    pages.iter().for_each(|p| strings(p, &mut texts));
    let used: HashSet<&str> = texts.iter().copied().collect();
    let mentions = |name: &str| texts.iter().any(|t| t.contains(name));
    let flows: Vec<Value> = c.flows.iter().filter(|f| used.contains(f.id.as_str())).map(serde_json::to_value).collect::<Result<_, _>>()?;
    let mut flow_texts = Vec::new();
    flows.iter().for_each(|f| strings(f, &mut flow_texts));
    let values: Map<String, Value> = c
        .values
        .iter()
        .filter(|(n, _)| mentions(n) || flow_texts.iter().any(|t| t.contains(n.as_str())))
        .map(|(n, m)| Ok((n.clone(), serde_json::to_value(m)?)))
        .collect::<Result<_, serde_json::Error>>()?;
    let page_and_button_ids: HashSet<String> = pages
        .iter()
        .flat_map(|p| std::iter::once(p["id"].as_str()).chain(p["buttons"].as_array().into_iter().flatten().map(|b| b["id"].as_str())))
        .flatten()
        .map(String::from)
        .collect();
    let events: Vec<Value> = c
        .events
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|e| {
            let mut t = Vec::new();
            strings(e, &mut t);
            t.iter().any(|s| page_and_button_ids.contains(*s) || values.keys().any(|v| s.contains(v.as_str())))
        })
        .collect();

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut images = Map::new();
    for (i, img) in used_images(&pages).into_iter().enumerate() {
        let Some(ext) = image_ext(&img) else { continue };
        if img.contains("..") {
            continue;
        }
        let Ok(bytes) = std::fs::read(store.icon_dir.join(&img)) else { continue };
        let base = safe_name(img.rsplit('/').next().unwrap_or(&img));
        let file = format!("images/{i:03}_{}", base.trim_end_matches(&format!(".{ext}")).to_string() + "." + ext);
        zip.start_file(&file, opts)?;
        zip.write_all(&bytes)?;
        images.insert(img, json!(file));
    }
    let profile = json!({
        "format": FORMAT,
        "formatVersion": FORMAT_VERSION,
        "app": env!("CARGO_PKG_VERSION"),
        "name": name,
        "pages": pages,
        "flows": flows,
        "events": events,
        "values": values,
        "images": images,
    });
    zip.start_file("profile.json", opts)?;
    zip.write_all(serde_json::to_string_pretty(&profile)?.as_bytes())?;
    let bytes = zip.finish()?.into_inner();
    let file = format!("{}.tdprofile", safe_name(if name.is_empty() { "TouchDeck" } else { name }));
    Ok((file, bytes))
}

/** Ergebnis eines Imports: neue Seiten-IDs und was dazukam. */
pub struct Imported {
    pub pages: Vec<String>,
    pub flows: usize,
    pub events: usize,
    pub values: usize,
    pub images: usize,
}

/** Liest ein .tdprofile und hängt seine Seiten an; nichts Vorhandenes wird überschrieben. */
pub fn import(store: &mut Store, bytes: &[u8]) -> Result<Imported> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow!(t!("profiles.notAProfile")))?;
    let mut profile: Option<Value> = None;
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if !f.is_file() {
            continue;
        }
        total += f.size();
        if total > MAX_SIZE {
            bail!(t!("profiles.tooLarge"));
        }
        let name = f.name()?.to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        if name == "profile.json" {
            profile = Some(serde_json::from_slice(&data).map_err(|_| anyhow!(t!("profiles.notAProfile")))?);
        } else if name.starts_with("images/") && image_ext(&name).is_some() {
            files.insert(name, data);
        }
    }
    let profile = profile.filter(|p| p["format"] == FORMAT).ok_or_else(|| anyhow!(t!("profiles.notAProfile")))?;
    let mut pages: Vec<Value> = profile["pages"].as_array().cloned().unwrap_or_default();
    if pages.is_empty() {
        bail!(t!("profiles.noPagesInFile"));
    }
    let mut flows: Vec<Value> = profile["flows"].as_array().cloned().unwrap_or_default();
    let mut events: Vec<Value> = profile["events"].as_array().cloned().unwrap_or_default();

    // Neue IDs für Seiten, Buttons, Vorlagen, Events und Aktionen.
    let mut ids: HashMap<String, String> = HashMap::new();
    let fresh = |old: &Value, prefix: &str, ids: &mut HashMap<String, String>| {
        if let Some(o) = old.as_str() {
            ids.entry(o.to_string()).or_insert_with(|| new_id(prefix));
        }
    };
    for p in &pages {
        fresh(&p["id"], "p_", &mut ids);
        for b in p["buttons"].as_array().into_iter().flatten() {
            fresh(&b["id"], "b_", &mut ids);
        }
    }
    flows.iter().for_each(|f| fresh(&f["id"], "f_", &mut ids));
    events.iter().for_each(|e| fresh(&e["id"], "ev_", &mut ids));

    // Bilder als eigene Bilder ablegen (Name mit Präfix, damit nichts überschrieben wird).
    let mut image_names: HashMap<String, String> = HashMap::new();
    for (orig, file) in profile["images"].as_object().into_iter().flatten() {
        let Some(data) = file.as_str().and_then(|f| files.get(f)) else { continue };
        let ext = image_ext(file.as_str().unwrap_or("")).unwrap_or("png");
        let base = safe_name(orig.rsplit('/').next().unwrap_or(orig));
        let stem = base.rsplit_once('.').map_or(base.as_str(), |(s, _)| s).to_string();
        let mut name = format!("profil_{stem}.{ext}");
        let mut n = 2;
        while store.icon_dir.join(&name).exists() {
            name = format!("profil_{stem}_{n}.{ext}");
            n += 1;
        }
        std::fs::write(store.icon_dir.join(&name), data)?;
        image_names.insert(orig.clone(), name);
    }

    for list in [&mut pages, &mut flows, &mut events] {
        for item in list.iter_mut() {
            remap(item, &ids);
        }
    }
    for p in &mut pages {
        p["template"] = json!("");
        for b in p["buttons"].as_array_mut().into_iter().flatten() {
            for key in ["icon", "iconOn", "bgImage"] {
                if let Some(new) = b[key]["value"].as_str().and_then(|v| image_names.get(v)) {
                    b[key]["value"] = json!(new);
                }
            }
        }
    }

    let c = &store.config;
    let mut all_pages = serde_json::to_value(&c.pages)?.as_array().cloned().unwrap_or_default();
    let new_page_ids: Vec<String> = pages.iter().filter_map(|p| p["id"].as_str().map(String::from)).collect();
    all_pages.extend(pages);
    let mut all_flows = serde_json::to_value(&c.flows)?.as_array().cloned().unwrap_or_default();
    let flow_count = flows.len();
    all_flows.extend(flows);
    // Globale Events nur, wenn es keines mit gleichem Namen gibt.
    let mut all_events = serde_json::to_value(&c.events)?.as_array().cloned().unwrap_or_default();
    let names: HashSet<String> = c.events.iter().map(|e| e.name.clone()).collect();
    let new_events: Vec<Value> = events.into_iter().filter(|e| !e["name"].as_str().is_some_and(|n| names.contains(n))).collect();
    let event_count = new_events.len();
    all_events.extend(new_events);
    let mut all_values = serde_json::to_value(&c.values)?;
    let mut value_count = 0;
    if let (Some(target), Some(src)) = (all_values.as_object_mut(), profile["values"].as_object()) {
        for (k, v) in src {
            if !target.contains_key(k) {
                target.insert(k.clone(), v.clone());
                value_count += 1;
            }
        }
    }
    store.set_project(&json!({ "pages": all_pages, "flows": all_flows, "events": all_events, "values": all_values }))?;
    Ok(Imported { pages: new_page_ids, flows: flow_count, events: event_count, values: value_count, images: image_names.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_und_import() {
        let dir = std::env::temp_dir().join(format!("td-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = Store::open(&dir).unwrap();
        std::fs::write(store.icon_dir.join("bild.png"), b"PNG").unwrap();
        let page_id = store.config.pages[0].id.clone();
        let mut pages = serde_json::to_value(&store.config.pages).unwrap();
        let b = &mut pages[0]["buttons"][0];
        b["icon"] = json!({ "type": "image", "value": "bild.png" });
        b["actions"] = json!([{ "type": "go_page", "enabled": true, "params": { "page": page_id } }]);
        store.set_project(&json!({ "pages": pages, "values": { "Zähler": { "default": "0" } } })).unwrap();

        let (file, bytes) = export(&store, std::slice::from_ref(&page_id), "Mein Profil").unwrap();
        assert!(file.ends_with(".tdprofile"));
        let before = store.config.pages.len();
        let r = import(&mut store, &bytes).unwrap();
        assert_eq!(store.config.pages.len(), before + 1);
        let new = store.config.pages.last().unwrap();
        assert_ne!(new.id, page_id, "neue ID");
        assert_eq!(new.buttons[0].actions[0].params["page"], json!(new.id), "Verweis auf die eigene Seite umgestellt");
        assert!(new.buttons[0].icon.value.starts_with("profil_"), "Bild als eigenes Bild abgelegt");
        assert!(store.icon_dir.join(&new.buttons[0].icon.value).exists());
        assert_eq!(r.images, 1);
        assert!(import(&mut store, b"kein zip").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
