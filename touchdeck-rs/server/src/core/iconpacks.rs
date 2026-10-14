// Icon-Pakete: eigene Bildersammlungen, die jeder selbst anlegt, füllt und als Datei (.tdicons) weitergeben kann.
// Ablage: <Datenordner>/icons/packs/<id>/pack.json plus die Bilder. Buttons verweisen auf „packs/<id>/<datei>“,
// damit Bedienfeld und Handy-App die Bilder wie alle anderen über /icons/… laden.
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
            bail!("Das Icon-Paket gibt es nicht (mehr).");
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
                Some(json!({
                    "id": id,
                    "name": meta["name"].as_str().filter(|n| !n.is_empty()).unwrap_or(&id),
                    "author": meta["author"].as_str().unwrap_or(""),
                    "link": meta["link"].as_str().unwrap_or(""),
                    "license": meta["license"].as_str().unwrap_or(""),
                    "icons": icons,
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
            bail!("Bitte einen Namen für das Icon-Paket angeben.");
        }
        let link = text(&body["link"], 300);
        if !link.is_empty() && !(link.starts_with("https://") || link.starts_with("http://")) {
            bail!("Der Link muss mit https:// beginnen.");
        }
        Ok(json!({ "name": name, "author": text(&body["author"], 80), "link": link, "license": text(&body["license"], 200) }))
    }

    pub fn create(&self, body: &Value) -> Result<String> {
        let meta = Self::meta_from(body)?;
        let id = make_id(meta["name"].as_str().unwrap_or(""));
        let dir = self.dir.join(&id);
        fs::create_dir_all(&dir)?;
        Self::write_meta(&dir, &meta)?;
        Ok(id)
    }

    pub fn update(&self, id: &str, body: &Value) -> Result<()> {
        let dir = self.pack_dir(id)?;
        Self::write_meta(&dir, &Self::meta_from(body)?)
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
            let name = clean_file_name(f["name"].as_str().unwrap_or("")).ok_or_else(|| anyhow!("„{}“ ist kein Bild (PNG, JPG, GIF, WebP oder SVG).", f["name"].as_str().unwrap_or("?")))?;
            let data = f["data"].as_str().unwrap_or("");
            let b64 = data.split_once(";base64,").map(|(_, b)| b).ok_or_else(|| anyhow!("„{name}“ ist beschädigt."))?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| anyhow!("„{name}“ ist beschädigt."))?;
            if bytes.len() as u64 > MAX_IMAGE {
                bail!("„{name}“ ist größer als 5 MB.");
            }
            if count >= MAX_ICONS {
                bail!("Ein Paket kann höchstens {MAX_ICONS} Bilder enthalten.");
            }
            let target = free_name(&dir, &name);
            fs::write(dir.join(&target), bytes)?;
            saved.push(target);
        }
        Ok(saved)
    }

    pub fn remove_icon(&self, id: &str, file: &str) -> Result<()> {
        let dir = self.pack_dir(id)?;
        let name = clean_file_name(file).filter(|n| n == file).ok_or_else(|| anyhow!("Unbekanntes Bild."))?;
        fs::remove_file(dir.join(name))?;
        Ok(())
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

    /** Paket aus einer ZIP-Datei übernehmen (.tdicons oder ein ZIP mit Bildern). Liefert die neue ID und die Anzahl der Bilder. */
    pub fn import(&self, bytes: &[u8], fallback_name: &str) -> Result<(String, usize)> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow!("Die Datei ist kein gültiges Icon-Paket (ZIP)."))?;
        let mut meta = json!({});
        let mut images: Vec<(String, Vec<u8>)> = Vec::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let raw_name = f.name()?.to_string();
            let base = raw_name.rsplit('/').next().unwrap_or("").to_string();
            if f.size() > MAX_IMAGE {
                continue;
            }
            total += f.size();
            if total > MAX_TOTAL {
                bail!("Das Paket ist zu groß (mehr als 100 MB).");
            }
            if base == "pack.json" {
                let mut t = String::new();
                f.read_to_string(&mut t)?;
                meta = serde_json::from_str(&t).unwrap_or_else(|_| json!({}));
            } else if let Some(name) = clean_file_name(&base) {
                if images.len() >= MAX_ICONS {
                    bail!("Das Paket enthält mehr als {MAX_ICONS} Bilder.");
                }
                let mut data = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut data)?;
                images.push((name, data));
            }
        }
        if images.is_empty() {
            bail!("In der Datei sind keine Bilder (PNG, JPG, GIF, WebP oder SVG).");
        }
        if meta["name"].as_str().is_none_or(str::is_empty) {
            meta["name"] = json!(if fallback_name.is_empty() { "Importiertes Paket" } else { fallback_name });
        }
        let id = self.create(&meta)?;
        let dir = self.dir.join(&id);
        let count = images.len();
        for (name, data) in images {
            fs::write(dir.join(free_name(&dir, &name)), data)?;
        }
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
}
