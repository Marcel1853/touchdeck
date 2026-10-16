// Icon-Pakete: eigene Bildersammlungen, die jeder selbst anlegt, füllt und als Datei (.tdicons) weitergeben kann.
// Ablage: <Datenordner>/icons/packs/<id>/pack.json plus die Bilder. Buttons verweisen auf „packs/<id>/<datei>“,
// damit Bedienfeld und Handy-App die Bilder wie alle anderen über /icons/… laden.
//
// pack.json (Format 2): name, author, link, license (Kurzname), description, version, color (passende
// Button-Farbe), cover (Titelbild), licenseText und icons: { "<datei>": { "name": …, "tags": […] } }.
use crate::t;
use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use crate::core::store::new_id;
use crate::system::sort_key;

const IMAGE_EXT: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
// SVG nur mit eigener Endung, damit Browser es als Bild (ohne Skripte, siehe CSP in /icons/) behandeln.
const SVG_EXT: &str = "svg";
const MAX_ICONS: usize = 3000;
const MAX_IMAGE: u64 = 5 * 1024 * 1024;
const MAX_TOTAL: u64 = 100 * 1024 * 1024;
const FORMAT: u64 = 2;
const MAX_TAGS: usize = 20;

pub struct IconPacks {
    dir: PathBuf,
}

fn text(v: &Value, max: usize) -> String {
    v.as_str().unwrap_or("").trim().chars().take(max).collect()
}

/** Paket-ID aus dem Namen: Kleinbuchstaben, Ziffern und Bindestriche plus Zufall gegen Doppelungen. */
fn make_id(name: &str) -> String {
    let mut slug = String::new();
    for c in sort_key(name).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(32).collect();
    new_id(&format!("{}-", if slug.is_empty() { "paket" } else { &slug }))
}

fn hex_color(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/** Schlagwörter: Liste oder kommagetrennter Text, kleingeschrieben, ohne Doppelte. */
fn tags(v: &Value) -> Vec<String> {
    let raw: Vec<String> = match v {
        Value::Array(l) => l.iter().filter_map(|t| t.as_str().map(String::from)).collect(),
        Value::String(s) => s.split([',', ';']).map(String::from).collect(),
        _ => Vec::new(),
    };
    let mut out: Vec<String> = Vec::new();
    for t in raw {
        let t: String = t.trim().to_lowercase().chars().take(40).collect();
        if !t.is_empty() && !out.contains(&t) && out.len() < MAX_TAGS {
            out.push(t);
        }
    }
    out
}

/** Name und Schlagwörter eines Bildes bereinigen. */
fn icon_info(v: &Value) -> Value {
    json!({ "name": text(&v["name"], 80), "tags": tags(&v["tags"]) })
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn is_image(name: &str) -> bool {
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    IMAGE_EXT.contains(&ext.as_str()) || ext == SVG_EXT
}

/** Sicherer Dateiname: nur der letzte Teil, keine Steuerzeichen, Bild-Endung Pflicht. */
fn clean_file_name(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next()?.trim();
    let base: String = base.chars().filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c)).take(120).collect();
    (!base.is_empty() && !base.starts_with('.') && is_image(&base)).then_some(base)
}

/** Freier Dateiname im Ordner (bild.png, bild-2.png, …). */
fn free_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("bild");
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("png");
    (2..).map(|i| format!("{stem}-{i}.{ext}")).find(|n| !dir.join(n).exists()).expect("unendlich viele Namen")
}

impl IconPacks {
    pub fn new(icon_dir: &Path) -> Self {
        let dir = icon_dir.join("packs");
        let _ = fs::create_dir_all(&dir);
        IconPacks { dir }
    }

    fn pack_dir(&self, id: &str) -> Result<PathBuf> {
        let d = self.dir.join(id);
        if !valid_id(id) || !d.join("pack.json").is_file() {
            bail!(t!("iconpacks.iconPackDoesNot"));
        }
        Ok(d)
    }

    fn read_meta(dir: &Path) -> Value {
        fs::read_to_string(dir.join("pack.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}))
    }

    fn icons_of(dir: &Path) -> Vec<String> {
        let mut icons: Vec<String> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.') && is_image(n))
            .collect();
        icons.sort_by_key(|n| sort_key(n));
        icons
    }

    /** Alle Pakete mit Beschreibung und Bildern (Dateinamen). */
    pub fn list(&self) -> Vec<Value> {
        let mut packs: Vec<Value> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join("pack.json").is_file())
            .filter_map(|e| {
                let id = e.file_name().into_string().ok().filter(|id| valid_id(id))?;
                let meta = Self::read_meta(&e.path());
                let icons = Self::icons_of(&e.path());
                // Infos nur für vorhandene Bilder; Titelbild nur, wenn es das Bild gibt.
                let info: serde_json::Map<String, Value> = icons.iter().filter_map(|i| meta["icons"].get(i).map(|v| (i.clone(), icon_info(v)))).collect();
                let cover = meta["cover"].as_str().filter(|c| icons.iter().any(|i| i == c)).or(icons.first().map(String::as_str)).unwrap_or("");
                Some(json!({
                    "id": id,
                    "name": meta["name"].as_str().filter(|n| !n.is_empty()).unwrap_or(&id),
                    "author": meta["author"].as_str().unwrap_or(""),
                    "link": meta["link"].as_str().unwrap_or(""),
                    "license": meta["license"].as_str().unwrap_or(""),
                    "description": meta["description"].as_str().unwrap_or(""),
                    "version": meta["version"].as_str().unwrap_or(""),
                    "color": meta["color"].as_str().filter(|c| hex_color(c)).unwrap_or(""),
                    "licenseText": meta["licenseText"].as_str().unwrap_or(""),
                    "cover": cover,
                    "icons": icons,
                    "info": info,
                }))
            })
            .collect();
        packs.sort_by_key(|p| sort_key(p["name"].as_str().unwrap_or("")));
        packs
    }

    fn write_meta(dir: &Path, meta: &Value) -> Result<()> {
        fs::write(dir.join("pack.json"), serde_json::to_string_pretty(meta)?)?;
        Ok(())
    }

    fn meta_from(body: &Value) -> Result<Value> {
        let name = text(&body["name"], 80);
        if name.is_empty() {
            bail!(t!("iconpacks.pleaseEnterNameFor"));
        }
        let link = text(&body["link"], 300);
        if !link.is_empty() && !(link.starts_with("https://") || link.starts_with("http://")) {
            bail!(t!("iconpacks.linkMustStartWith"));
        }
        let color = text(&body["color"], 7);
        Ok(json!({
            "format": FORMAT,
            "name": name,
            "author": text(&body["author"], 80),
            "link": link,
            "license": text(&body["license"], 200),
            "description": text(&body["description"], 500),
            "version": text(&body["version"], 20),
            "color": if hex_color(&color) { color } else { String::new() },
            "cover": clean_file_name(body["cover"].as_str().unwrap_or("")).unwrap_or_default(),
            "licenseText": body["licenseText"].as_str().unwrap_or("").chars().take(20_000).collect::<String>(),
            "icons": body["icons"].as_object().map(|o| o.iter().take(MAX_ICONS).filter_map(|(k, v)| clean_file_name(k).map(|k| (k, icon_info(v)))).collect::<serde_json::Map<_, _>>()).unwrap_or_default(),
        }))
    }

    pub fn create(&self, body: &Value) -> Result<String> {
        let meta = Self::meta_from(body)?;
        let id = make_id(meta["name"].as_str().unwrap_or(""));
        let dir = self.dir.join(&id);
        fs::create_dir_all(&dir)?;
        Self::write_meta(&dir, &meta)?;
        Ok(id)
    }

    /** Beschreibung ändern; Namen und Schlagwörter der Bilder bleiben erhalten. */
    pub fn update(&self, id: &str, body: &Value) -> Result<()> {
        let dir = self.pack_dir(id)?;
        let mut meta = Self::meta_from(body)?;
        meta["icons"] = Self::read_meta(&dir)["icons"].clone();
        if !meta["icons"].is_object() {
            meta["icons"] = json!({});
        }
        Self::write_meta(&dir, &meta)
    }

    /** Name und Schlagwörter eines Bildes setzen. */
    pub fn set_icon_info(&self, id: &str, file: &str, info: &Value) -> Result<()> {
        let dir = self.pack_dir(id)?;
        let name = clean_file_name(file).filter(|n| n == file && dir.join(n).is_file()).ok_or_else(|| anyhow!(t!("iconpacks.unknownImage")))?;
        let mut meta = Self::read_meta(&dir);
        if !meta["icons"].is_object() {
            meta["icons"] = json!({});
        }
        meta["icons"][name] = icon_info(info);
        Self::write_meta(&dir, &meta)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let dir = self.pack_dir(id)?;
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    /** Bilder hinzufügen: [{ name, data: "data:image/…;base64,…" }]; liefert die gespeicherten Dateinamen. */
    pub fn add_icons(&self, id: &str, files: &Value) -> Result<Vec<String>> {
        use base64::Engine;
        let dir = self.pack_dir(id)?;
        let existing = Self::icons_of(&dir).len();
        let mut saved = Vec::new();
        for (count, f) in (existing..).zip(files.as_array().into_iter().flatten()) {
            let name = clean_file_name(f["name"].as_str().unwrap_or("")).ok_or_else(|| anyhow!(t!("iconpacks.isNotImagePng", f["name"].as_str().unwrap_or("?"))))?;
            let data = f["data"].as_str().unwrap_or("");
            let b64 = data.split_once(";base64,").map(|(_, b)| b).ok_or_else(|| anyhow!(t!("iconpacks.isDamaged", name)))?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| anyhow!(t!("iconpacks.isDamaged", name)))?;
            if bytes.len() as u64 > MAX_IMAGE {
                bail!(t!("iconpacks.isLargerThan5", name));
            }
            if count >= MAX_ICONS {
                bail!(t!("iconpacks.packCanContainAt", MAX_ICONS));
            }
            let target = free_name(&dir, &name);
            fs::write(dir.join(&target), bytes)?;
            saved.push(target);
        }
        Ok(saved)
    }

    pub fn remove_icon(&self, id: &str, file: &str) -> Result<()> {
        let dir = self.pack_dir(id)?;
        let name = clean_file_name(file).filter(|n| n == file).ok_or_else(|| anyhow!(t!("iconpacks.unknownImage")))?;
        fs::remove_file(dir.join(&name))?;
        let mut meta = Self::read_meta(&dir);
        if let Some(icons) = meta["icons"].as_object_mut() {
            icons.remove(&name);
        }
        if meta["cover"] == json!(name) {
            meta["cover"] = json!("");
        }
        Self::write_meta(&dir, &meta)
    }

    /** Paket als ZIP (.tdicons): pack.json plus alle Bilder. Liefert (Dateiname, Inhalt). */
    pub fn export(&self, id: &str) -> Result<(String, Vec<u8>)> {
        let dir = self.pack_dir(id)?;
        let meta = Self::read_meta(&dir);
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("pack.json", opts)?;
        zip.write_all(serde_json::to_string_pretty(&meta)?.as_bytes())?;
        for icon in Self::icons_of(&dir) {
            // Bilder sind meist schon komprimiert.
            zip.start_file(&icon, zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored))?;
            zip.write_all(&fs::read(dir.join(&icon))?)?;
        }
        let bytes = zip.finish()?.into_inner();
        let file: String = meta["name"].as_str().unwrap_or(id).chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
        Ok((format!("{}.tdicons", file.trim()), bytes))
    }

    /**
     * Paket aus einer ZIP-Datei übernehmen: .tdicons oder ein ZIP mit Bildern (auch in Unterordnern).
     * Beschreibungen in anderen üblichen Aufbauten werden mit übernommen: eine info.txt mit „name=…“,
     * eine manifest.json (Name, Autor, Beschreibung, Version, Titelbild, Lizenzdatei) mit icons.json
     * (Name und Schlagwörter je Bild) und eine Lizenzdatei. Liefert die neue ID und die Anzahl der Bilder.
     */
    pub fn import(&self, bytes: &[u8], fallback_name: &str) -> Result<(String, usize)> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow!(t!("iconpacks.fileIsNotValid")))?;
        let mut texts: Vec<(String, String)> = Vec::new();
        let mut images: Vec<(String, Vec<u8>)> = Vec::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() || f.size() > MAX_IMAGE {
                continue;
            }
            let path = f.name()?.replace('\\', "/");
            if path.split('/').any(|p| p.starts_with('.') || p == "__MACOSX") {
                continue;
            }
            total += f.size();
            if total > MAX_TOTAL {
                bail!(t!("iconpacks.packIsTooLarge"));
            }
            let base = path.rsplit('/').next().unwrap_or("").to_lowercase();
            if ["pack.json", "manifest.json", "icons.json", "info.txt", "license.txt", "license.md", "license"].contains(&base.as_str()) {
                let mut t = String::new();
                if f.read_to_string(&mut t).is_ok() {
                    texts.push((path, t.trim_start_matches('\u{feff}').to_string()));
                }
            } else if clean_file_name(&path).is_some() {
                if images.len() >= MAX_ICONS {
                    bail!(t!("iconpacks.packContainsMoreThan", MAX_ICONS));
                }
                let mut data = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut data)?;
                images.push((path, data));
            }
        }
        if images.is_empty() {
            bail!(t!("iconpacks.fileContainsNoImages"));
        }
        let find = |name: &str| texts.iter().find(|(p, _)| p.rsplit('/').next().is_some_and(|b| b.eq_ignore_ascii_case(name))).map(|(p, t)| (p.as_str(), t.as_str()));
        let folder = |p: &str| p.rsplit_once('/').map_or(String::new(), |(d, _)| format!("{d}/"));

        // Beschreibung zusammentragen: eigenes Format zuerst, sonst die anderen Aufbauten.
        let mut meta = find("pack.json").and_then(|(_, t)| serde_json::from_str::<Value>(t).ok()).unwrap_or_else(|| json!({}));
        let mut info_by_path: HashMap<String, Value> = HashMap::new();
        let mut cover_path = String::new();
        if let Some((p, t)) = find("manifest.json")
            && let Ok(m) = serde_json::from_str::<Value>(t)
        {
            let dir = folder(p);
            let get = |k: &str| m.get(k).or_else(|| m.get(k.to_lowercase())).and_then(Value::as_str).unwrap_or("").to_string();
            for (ours, theirs) in [("name", "Name"), ("author", "Author"), ("link", "URL"), ("description", "Description"), ("version", "Version")] {
                if meta[ours].as_str().is_none_or(str::is_empty) && !get(theirs).is_empty() {
                    meta[ours] = json!(get(theirs));
                }
            }
            if !get("Icon").is_empty() {
                cover_path = format!("{dir}{}", get("Icon"));
            }
            if !get("License").is_empty()
                && let Some((_, lt)) = texts.iter().find(|(lp, _)| *lp == format!("{dir}{}", get("License")))
            {
                meta["licenseText"] = json!(lt);
            }
            // Name und Schlagwörter je Bild (Pfade relativ zum Ordner der Bilder).
            if let Some((ip, it)) = find("icons.json")
                && let Ok(Value::Array(list)) = serde_json::from_str::<Value>(it)
            {
                let base_dir = folder(ip);
                for e in list {
                    if let Some(rel) = e["path"].as_str() {
                        let hit = images.iter().find(|(p, _)| p.ends_with(&format!("/{rel}")) || p == &format!("{base_dir}{rel}") || p.ends_with(rel));
                        if let Some((p, _)) = hit {
                            info_by_path.insert(p.clone(), json!({ "name": e["name"], "tags": e["tags"] }));
                        }
                    }
                }
            }
        }
        if let Some((_, t)) = find("info.txt") {
            for line in t.lines() {
                let Some((k, v)) = line.split_once('=') else { continue };
                let (k, v) = (k.trim().to_lowercase(), v.trim().replace("##", "#"));
                match k.as_str() {
                    "name" | "author" | "link" if meta[&k].as_str().is_none_or(str::is_empty) => meta[&k] = json!(v),
                    "bgcolor" if hex_color(&v) && meta["color"].as_str().is_none_or(str::is_empty) => meta["color"] = json!(v),
                    _ => {}
                }
            }
        }
        if meta["licenseText"].as_str().is_none_or(str::is_empty)
            && let Some((_, lt)) = find("license.txt").or_else(|| find("license.md")).or_else(|| find("license"))
        {
            meta["licenseText"] = json!(lt);
        }
        if meta["name"].as_str().is_none_or(str::is_empty) {
            meta["name"] = json!(if fallback_name.is_empty() { t!("iconpacks.importedPack") } else { fallback_name });
        }
        let old_icons = meta["icons"].clone();
        let old_cover = meta["cover"].as_str().unwrap_or("").to_string();
        meta["icons"] = json!({});
        meta["cover"] = json!("");
        let id = self.create(&meta)?;
        let dir = self.dir.join(&id);
        let count = images.len();
        let mut final_meta = Self::read_meta(&dir);
        for (path, data) in images {
            let base = clean_file_name(&path).unwrap_or_default();
            let saved = free_name(&dir, &base);
            fs::write(dir.join(&saved), data)?;
            let info = info_by_path.get(&path).cloned().or_else(|| old_icons.get(&base).cloned());
            if let Some(info) = info {
                final_meta["icons"][&saved] = icon_info(&info);
            }
            if path == cover_path || (!old_cover.is_empty() && base == old_cover && final_meta["cover"] == "") {
                final_meta["cover"] = json!(saved);
            }
        }
        Self::write_meta(&dir, &final_meta)?;
        Ok((id, count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dateinamen() {
        assert_eq!(clean_file_name("ordner/Mikrofon an.png").as_deref(), Some("Mikrofon an.png"));
        assert_eq!(clean_file_name("..\\böse.svg").as_deref(), Some("böse.svg"));
        assert_eq!(clean_file_name("skript.html"), None);
        assert_eq!(clean_file_name(".versteckt.png"), None);
        assert!(make_id("Mein Paket!").starts_with("mein-paket-"));
        assert!(!valid_id("../x"));
    }

    #[test]
    fn anlegen_exportieren_importieren() {
        use base64::Engine;
        let dir = std::env::temp_dir().join(new_id("td-icons-"));
        let packs = IconPacks::new(&dir);
        let id = packs.create(&json!({ "name": "Streaming", "author": "Marcel", "license": "CC0" })).unwrap();
        let png = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode([137, 80, 78, 71]));
        let saved = packs.add_icons(&id, &json!([{ "name": "mic.png", "data": png }, { "name": "mic.png", "data": png }])).unwrap();
        assert_eq!(saved, vec!["mic.png", "mic-2.png"]);
        let (file, bytes) = packs.export(&id).unwrap();
        assert_eq!(file, "Streaming.tdicons");
        let (copy, n) = packs.import(&bytes, "").unwrap();
        assert_eq!(n, 2);
        let list = packs.list();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p["id"] == copy && p["license"] == "CC0"));
        packs.remove_icon(&id, "mic-2.png").unwrap();
        assert!(packs.remove_icon(&id, "../pack.json").is_err());
        packs.delete(&id).unwrap();
        assert_eq!(packs.list().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn andere_aufbauten_importieren() {
        let dir = std::env::temp_dir().join(new_id("td-icons2-"));
        let packs = IconPacks::new(&dir);
        // Bilder mit info.txt (name=…, bgcolor=…).
        let a = zip_of(&[("info.txt", b"name=Spiele\nauthor=Ich\nbgcolor=##112233\n"), ("pad.png", b"PNG"), ("unterordner/maus.png", b"PNG")]);
        let (id, n) = packs.import(&a, "").unwrap();
        assert_eq!(n, 2);
        let p = packs.list().into_iter().find(|p| p["id"] == id).unwrap();
        assert_eq!((p["name"].as_str(), p["author"].as_str(), p["color"].as_str()), (Some("Spiele"), Some("Ich"), Some("#112233")));
        // manifest.json + icons.json in einem Unterordner, Bilder in icons/, Lizenzdatei, Titelbild.
        let b = zip_of(&[
            ("x.paket/manifest.json", "\u{feff}{\"Name\":\"Logos\",\"Author\":\"A\",\"Description\":\"Bunte Logos\",\"Version\":\"2.0\",\"Icon\":\"icon.svg\",\"License\":\"license.txt\"}".as_bytes()),
            ("x.paket/icons.json", br#"[{"path":"mic.svg","name":"Mikrofon","tags":["Audio","Stimme"]}]"#),
            ("x.paket/license.txt", b"Frei nutzbar"),
            ("x.paket/icon.svg", b"<svg/>"),
            ("x.paket/icons/mic.svg", b"<svg/>"),
        ]);
        let (id2, n2) = packs.import(&b, "").unwrap();
        assert_eq!(n2, 2);
        let p = packs.list().into_iter().find(|p| p["id"] == id2).unwrap();
        assert_eq!(p["name"], "Logos");
        assert_eq!(p["description"], "Bunte Logos");
        assert_eq!(p["version"], "2.0");
        assert_eq!(p["licenseText"], "Frei nutzbar");
        assert_eq!(p["cover"], "icon.svg");
        assert_eq!(p["info"]["mic.svg"], json!({ "name": "Mikrofon", "tags": ["audio", "stimme"] }));
        // Infos überstehen Bearbeiten, Export und Import.
        packs.set_icon_info(&id2, "icon.svg", &json!({ "name": "Titel", "tags": "logo, Titel, logo" })).unwrap();
        packs.update(&id2, &json!({ "name": "Logos 2", "cover": "mic.svg", "color": "#abcdef" })).unwrap();
        let (_, bytes) = packs.export(&id2).unwrap();
        let (id3, _) = packs.import(&bytes, "").unwrap();
        let p = packs.list().into_iter().find(|p| p["id"] == id3).unwrap();
        assert_eq!((p["name"].as_str(), p["cover"].as_str(), p["color"].as_str()), (Some("Logos 2"), Some("mic.svg"), Some("#abcdef")));
        assert_eq!(p["info"]["icon.svg"]["tags"], json!(["logo", "titel"]));
        assert_eq!(p["info"]["mic.svg"]["name"], "Mikrofon");
        packs.remove_icon(&id3, "mic.svg").unwrap();
        let p = packs.list().into_iter().find(|p| p["id"] == id3).unwrap();
        assert!(p["info"].get("mic.svg").is_none());
        fs::remove_dir_all(dir).unwrap();
    }

}
