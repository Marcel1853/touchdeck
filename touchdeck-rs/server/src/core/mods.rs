// Mods für Spiele, die ein Plugin mitbringt (Feld „mods“ in plugin.json): Angaben prüfen, Zielordner je nach
// Betriebssystem finden (Steam-Bibliothek, Proton-Prefix, AppData …) und den Mod dort hineinlegen.
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use crate::core::plugins::{ltext, text, version_newer};
use crate::integrations::steam;
use crate::system::os;
use crate::t;

/** Größte erlaubte Mod-Datei (Download oder im Plugin). */
pub const MAX_SIZE: usize = 200 * 1024 * 1024;
const MAX_ENTRIES: usize = 5000;

/** Nur Downloads von GitHub (Releases oder Repo-Dateien). */
pub fn url_allowed(url: &str) -> bool {
    url.starts_with("https://github.com/") && !url.contains("..") && url.len() <= 400
}

/** Ein Pfadstück ohne Tricks: kein „..“, kein absoluter Pfad, keine Laufwerksbuchstaben. */
fn safe_parts(p: &str) -> bool {
    !p.is_empty() && p.split('/').all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains(['\\', ':']))
}

/** „besitzer/repo“ wie auf GitHub erlaubt. */
fn repo_ok(r: &str) -> bool {
    let ok = |p: &str| !p.is_empty() && p.len() <= 100 && p != "." && p != ".." && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    r.split_once('/').is_some_and(|(a, b)| ok(a) && ok(b))
}

/** Ist der Mod über den Steam Workshop abonniert und heruntergeladen? */
pub fn workshop_installed(m: &Value) -> Option<PathBuf> {
    steam::workshop_dir(m["workshop"]["steamApp"].as_str()?, m["workshop"]["item"].as_str()?)
}

/**
 * Factorio-Anmeldung aus player-data.json des Spiels („service-username“, „service-token“) – Factorio legt sie an,
 * sobald man sich im Spiel anmeldet. Nur auf ausdrücklichen Klick in den Einstellungen.
 */
pub fn factorio_login() -> Option<(String, String)> {
    let home = os::home();
    let mut files = vec![home.join(".factorio/player-data.json"), home.join(".var/app/com.valvesoftware.Steam/.factorio/player-data.json")];
    if cfg!(windows) {
        files.insert(0, os::config_dir().join("Factorio/player-data.json"));
    }
    files.into_iter().find_map(|f| {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
        let (u, t) = (v["service-username"].as_str()?, v["service-token"].as_str()?);
        (!u.is_empty() && !t.is_empty()).then(|| (u.to_string(), t.to_string()))
    })
}

/** Öffentliche Seite eines Mods im Factorio-Mod-Portal. */
pub fn portal_page(m: &Value) -> Option<String> {
    m["factorioPortal"].as_str().filter(|n| !n.is_empty()).map(|n| format!("https://mods.factorio.com/mod/{}", n.replace(' ', "%20")))
}

/** Öffentliche API des Factorio-Mod-Portals (Versionen, ohne Anmeldung). */
pub fn portal_api(m: &Value) -> Option<String> {
    m["factorioPortal"].as_str().filter(|n| !n.is_empty()).map(|n| format!("https://mods.factorio.com/api/mods/{}", n.replace(' ', "%20")))
}

/** Neueste Fassung aus der Antwort des Mod-Portals: { name, url (Pfad), version }. */
pub fn pick_portal(info: &Value) -> Option<Value> {
    let best = info["releases"].as_array()?.iter().filter(|r| r["version"].is_string()).max_by(|a, b| {
        let (va, vb) = (a["version"].as_str().unwrap_or(""), b["version"].as_str().unwrap_or(""));
        if version_newer(va, vb) { std::cmp::Ordering::Greater } else if version_newer(vb, va) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal }
    })?;
    let path = best["download_url"].as_str().filter(|p| p.starts_with("/download/") && !p.contains(".."))?;
    let name = best["file_name"].as_str().filter(|n| safe_name(n))?;
    Some(json!({ "name": name, "url": path, "version": best["version"], "portal": true }))
}

/** Adresse der GitHub-API für das neueste Release eines Mods. */
pub fn release_api(m: &Value) -> Option<String> {
    m["release"]["repo"].as_str().map(|r| format!("https://api.github.com/repos/{r}/releases/latest"))
}

/**
 * Passende Datei aus der Antwort der GitHub-API: { name, url, version }. Die Version kommt aus dem „*“
 * im Dateinamen, sonst aus dem Release-Tag (ohne „v“).
 */
pub fn pick_asset(m: &Value, release: &Value) -> Option<Value> {
    let pattern = m["release"]["asset"].as_str()?;
    let asset = release["assets"].as_array()?.iter().find(|a| a["name"].as_str().is_some_and(|n| matches(pattern, n)))?;
    let name = asset["name"].as_str()?;
    let url = asset["browser_download_url"].as_str().filter(|u| url_allowed(u))?;
    let from_name = pattern.split_once('*').map(|(a, b)| name[a.len()..name.len() - b.len()].to_string());
    let version = from_name.filter(|v| !v.is_empty()).unwrap_or_else(|| release["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string());
    Some(json!({ "name": name, "url": url, "version": version }))
}

fn safe_name(n: &str) -> bool {
    safe_parts(n) && !n.contains('/')
}

/** Ziel wie „{steamGame:294100}/Mods“ oder „{home}/.factorio/mods“ prüfen. */
fn safe_target(t: &str) -> bool {
    let Some(rest) = t.strip_prefix('{') else { return false };
    let Some((key, path)) = rest.split_once('}') else { return false };
    let key_ok = match key.split_once(':') {
        Some(("steamGame" | "proton", id)) => !id.is_empty() && id.len() <= 12 && id.chars().all(|c| c.is_ascii_digit()),
        Some(_) => false,
        None => ["home", "appdata", "localappdata", "documents"].contains(&key),
    };
    key_ok && (path.is_empty() || path.strip_prefix('/').is_some_and(safe_parts))
}

/** „mods“ aus plugin.json: nur bekannte Felder, Pfade geprüft. Einträge ohne Quelle oder Ziel fallen weg. */
pub fn sanitize(v: &Value) -> Vec<Value> {
    let list = |v: &Value, max: usize, ok: &dyn Fn(&str) -> bool| -> Vec<String> {
        v.as_array().into_iter().flatten().filter_map(Value::as_str).filter(|s| ok(s)).take(max).map(String::from).collect()
    };
    v.as_array()
        .into_iter()
        .flatten()
        .take(10)
        .filter_map(|m| {
            // Ein „*“ im Dateinamen steht für die Version: TouchDeck nimmt die neueste passende Datei im Plugin.
            let file = m["file"].as_str().filter(|f| safe_parts(f) && f.split('/').count() <= 4 && f.rsplit('/').skip(1).all(|d| !d.contains('*')) && f.matches('*').count() <= 1).unwrap_or("");
            let url = m["url"].as_str().filter(|u| url_allowed(u)).unwrap_or("");
            // Neuestes GitHub-Release: { "repo": "besitzer/repo", "asset": "Mod_*.zip" }.
            let release = Some(&m["release"]).filter(|r| repo_ok(r["repo"].as_str().unwrap_or("")) && r["asset"].as_str().is_some_and(|a| safe_name(a) && a.matches('*').count() <= 1));
            let targets = json!({
                "linux": list(&m["targets"]["linux"], 6, &safe_target),
                "windows": list(&m["targets"]["windows"], 6, &safe_target),
                "proton": list(&m["targets"]["proton"], 6, &safe_target),
            });
            let has_target = ["linux", "windows", "proton"].iter().any(|k| targets[k].as_array().is_some_and(|a| !a.is_empty()));
            let name = ltext(&m["name"], 60);
            let digits = |v: &Value| v.as_str().filter(|d| !d.is_empty() && d.len() <= 20 && d.chars().all(|c| c.is_ascii_digit())).map(String::from);
            // Steam Workshop: nur Link zum Abonnieren (Steam lädt und aktualisiert selbst) und Erkennung.
            let workshop = match (digits(&m["workshop"]["steamApp"]), digits(&m["workshop"]["item"])) {
                (Some(app), Some(item)) => json!({ "steamApp": app, "item": item }),
                _ => Value::Null,
            };
            // Factorio-Mod-Portal: Name des Mods; Download nur mit eigenem Token, sonst Link und Hinweis.
            let portal = m["factorioPortal"].as_str().filter(|n| !n.is_empty() && n.len() <= 100 && n.chars().all(|c| c.is_ascii_alphanumeric() || " -_".contains(c))).unwrap_or("");
            let installable = (!file.is_empty() || !url.is_empty() || release.is_some() || !portal.is_empty()) && has_target;
            if !(installable || workshop.is_object()) || name.as_str() == Some("") {
                return None;
            }
            let sha = m["sha256"].as_str().map(str::to_lowercase).filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()));
            Some(json!({
                "name": name,
                "version": text(&m["version"], 20),
                "file": file,
                "url": url,
                "release": release.map_or(Value::Null, |r| json!({ "repo": r["repo"], "asset": r["asset"] })),
                "workshop": workshop,
                "factorioPortal": portal,
                "sha256": sha,
                "unpack": m["unpack"] == true,
                "keep": list(&m["keep"], 20, &|k| safe_parts(k)),
                "remove": list(&m["remove"], 10, &|r| safe_name(r) && r.matches('*').count() <= 1),
                "conflicts": list(&m["conflicts"], 10, &|r| safe_name(r) && r.matches('*').count() <= 1),
                "targets": targets,
                "page": m["page"].as_str().filter(|u| u.starts_with("https://") && u.len() <= 400).unwrap_or(""),
                "hint": ltext(&m["hint"], 300),
            }))
        })
        .collect()
}

/** Platzhalter am Anfang eines Ziels auflösen; None, wenn es den Ordner hier nicht gibt. */
fn resolve(target: &str) -> Option<PathBuf> {
    let rest = target.strip_prefix('{')?;
    let (key, path) = rest.split_once('}')?;
    let base = match key.split_once(':') {
        Some(("steamGame", id)) => steam::game_dir(id)?,
        Some(("proton", id)) if !cfg!(windows) => steam::proton_user_dir(id)?,
        Some(_) => return None,
        None => match key {
            "home" => os::home(),
            "appdata" if cfg!(windows) => os::config_dir(),
            "localappdata" if cfg!(windows) => PathBuf::from(std::env::var_os("LOCALAPPDATA")?),
            "documents" => os::home().join("Documents"),
            _ => return None,
        },
    };
    let full = path.trim_start_matches('/').split('/').filter(|p| !p.is_empty()).fold(base.clone(), |p, c| p.join(c));
    // Der Mod-Ordner selbst darf fehlen (wird angelegt), sein Elternordner nicht: sonst ist das Spiel nicht da.
    let parent = full.parent().unwrap_or(&base);
    (base.is_dir() && (full.is_dir() || parent.is_dir())).then_some(full)
}

/** Mögliche Zielordner dieses Mods auf diesem PC (Linux: eigene Ziele und Proton-Ziele; Windows: Windows-Ziele). */
pub fn targets(m: &Value) -> Vec<PathBuf> {
    let keys: &[&str] = if cfg!(windows) { &["windows"] } else { &["linux", "proton"] };
    let mut out: Vec<PathBuf> = Vec::new();
    for k in keys {
        for t in m["targets"][k].as_array().into_iter().flatten().filter_map(Value::as_str) {
            if let Some(p) = resolve(t) {
                let same = out.iter().any(|o| o == &p || (o.canonicalize().ok().is_some() && o.canonicalize().ok() == p.canonicalize().ok()));
                if !same {
                    out.push(p);
                }
            }
        }
    }
    out
}

/** Quelle eines Mods auf diesem PC: Datei im Plugin oder Link. */
pub struct Source {
    /** Datei relativ zum Plugin-Ordner (leer, wenn der Mod per Link kommt). */
    pub file: String,
    /** Dateiname im Mod-Ordner. */
    pub name: String,
    pub version: String,
}

/**
 * Welche Datei ist gemeint? Bei „mod_*.zip“ die mit der höchsten Version im Plugin-Ordner; die Version kommt
 * dann aus dem Dateinamen (sofern plugin.json keine angibt). So muss beim Mod-Update nur die ZIP getauscht werden.
 */
pub fn source(m: &Value, plugin_dir: &Path) -> Option<Source> {
    let given = m["version"].as_str().unwrap_or("").to_string();
    let file = m["file"].as_str().unwrap_or("");
    if file.is_empty() {
        let url = m["url"].as_str().filter(|u| !u.is_empty())?;
        let name = url.rsplit('/').next().unwrap_or("").split(['?', '#']).next().unwrap_or("").to_string();
        return Some(Source { file: String::new(), name, version: given });
    }
    let (dir, pattern) = file.rsplit_once('/').map_or(("", file), |(d, p)| (d, p));
    let Some((pre, post)) = pattern.split_once('*') else {
        return Some(Source { file: file.into(), name: pattern.into(), version: given });
    };
    let mut best: Option<(String, String)> = None;
    for e in std::fs::read_dir(plugin_dir.join(dir)).ok()?.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if !matches(pattern, &n) {
            continue;
        }
        let v = n[pre.len()..n.len() - post.len()].to_string();
        if best.as_ref().is_none_or(|(_, bv)| version_newer(&v, bv)) {
            best = Some((n, v));
        }
    }
    let (name, v) = best?;
    let file = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
    Some(Source { file, name, version: if given.is_empty() { v } else { given } })
}

/** Muster für ältere Fassungen im Mod-Ordner: „remove“ aus plugin.json, sonst das Muster aus „file“. */
fn remove_patterns(m: &Value) -> Vec<String> {
    let given: Vec<String> = m["remove"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
    if !given.is_empty() {
        return given;
    }
    // Factorio benennt Mod-Dateien „<name>_<version>.zip“.
    if let Some(n) = m["factorioPortal"].as_str().filter(|n| !n.is_empty()) {
        return vec![format!("{n}_*.zip")];
    }
    m["file"].as_str().and_then(|f| f.rsplit('/').next()).filter(|p| p.contains('*')).map(|p| vec![p.to_string()]).unwrap_or_default()
}

/** Prüfsumme vergleichen, wenn das Plugin eine angibt. */
pub fn check_sha(m: &Value, bytes: &[u8]) -> Result<()> {
    use sha2::{Digest, Sha256};
    if let Some(want) = m["sha256"].as_str() {
        let got = hex::encode(Sha256::digest(bytes));
        if got != want {
            bail!(t!("plugins.modChecksumWrong"));
        }
    }
    Ok(())
}

/**
 * Andere Fassungen des Mods im Zielordner, die TouchDeck nicht anfasst (z. B. ein Entwicklungsordner):
 * Einträge nach „conflicts“, außer dem, was TouchDeck selbst dort hingelegt hat.
 */
pub fn conflicts(m: &Value, target: &Path, own: Option<&Path>, mine: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(target) else { return Vec::new() };
    let names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let removes = remove_patterns(m);
    let mut out = Vec::new();
    for pattern in m["conflicts"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        for n in names.iter().filter(|n| matches(pattern, n)) {
            let p = target.join(n);
            let removed_anyway = m["unpack"] != true && removes.iter().any(|r| matches(r, n)) && p.is_file();
            if Some(p.as_path()) != own && !(m["unpack"] != true && n == mine) && !removed_anyway && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/** Einfaches Muster mit höchstens einem „*“ (z. B. „touchdeck-bridge_*.zip“). */
fn matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        Some((a, b)) => name.len() >= a.len() + b.len() && name.starts_with(a) && name.ends_with(b),
        None => pattern == name,
    }
}

/**
 * Mod in den Zielordner legen. Entpackt: Dateien werden überschrieben, „keep“-Dateien (eigene Einstellungen)
 * bleiben, wenn es sie schon gibt. Als Datei: alte Fassungen nach „remove“ (nur Dateien) werden gelöscht.
 * Liefert den Pfad, an dem man die Installation wiedererkennt.
 */
pub fn install(m: &Value, bytes: &[u8], target: &Path, name: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(target)?;
    if m["unpack"] != true {
        if !safe_name(name) {
            bail!(t!("plugins.modNoFileName"));
        }
        for pattern in remove_patterns(m) {
            for e in std::fs::read_dir(target)?.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                // Nur echte Dateien, nie Ordner oder Verknüpfungen (z. B. ein Entwicklungsordner des Mods).
                if n != name && matches(&pattern, &n) && e.file_type().is_ok_and(|t| t.is_file()) {
                    std::fs::remove_file(e.path())?;
                }
            }
        }
        let path = target.join(name);
        std::fs::write(&path, bytes)?;
        return Ok(path);
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow!(t!("plugins.modZipInvalid")))?;
    if zip.len() > MAX_ENTRIES {
        bail!(t!("plugins.modZipInvalid"));
    }
    let keep: Vec<&str> = m["keep"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut first: Option<String> = None;
    let mut total = 0usize;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        // Nichts außerhalb des Zielordners: nur normale Pfadteile.
        let Some(rel) = f.enclosed_name() else { continue };
        if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
            continue;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if first.is_none() {
            first = rel_str.split('/').next().map(String::from);
        }
        let out = target.join(&rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        total += f.size() as usize;
        if total > MAX_SIZE {
            bail!(t!("plugins.modTooLarge"));
        }
        if keep.contains(&rel_str.as_str()) && out.exists() {
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        std::fs::write(&out, &data)?;
        if f.unix_mode().is_some_and(|m| m & 0o111 != 0) {
            os::set_mode(&out, 0o755)?;
        }
    }
    let first = first.ok_or_else(|| anyhow!(t!("plugins.modZipInvalid")))?;
    Ok(target.join(first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn angaben_pruefen() {
        let list = sanitize(&json!([
            { "name": "Bridge", "version": "0.1.0", "file": "Bridge.zip", "unpack": true, "keep": ["Bridge/settings.json", "../x"],
              "targets": { "linux": ["{home}/.factorio/mods", "/etc", "{home}/../x", "{steamGame:12a}/Mods"], "proton": ["{proton:1062090}/Documents/Mods"] } },
            { "name": "Ohne Ziel", "file": "a.zip", "targets": {} },
            { "name": "Fremder Link", "url": "https://example.com/a.zip", "targets": { "linux": ["{home}/x"] } },
        ]));
        assert_eq!(list.len(), 1, "ohne Ziel oder mit fremdem Link fällt weg");
        assert_eq!(list[0]["targets"]["linux"], json!(["{home}/.factorio/mods"]));
        assert_eq!(list[0]["targets"]["proton"], json!(["{proton:1062090}/Documents/Mods"]));
        assert_eq!(list[0]["keep"], json!(["Bridge/settings.json"]));
        assert!(matches("touchdeck-bridge_*.zip", "touchdeck-bridge_0.5.0.zip"));
        let r = sanitize(&json!([{ "name": "R", "release": { "repo": "a/b", "asset": "Mod_*.zip" }, "targets": { "windows": ["{appdata}/x"] } },
                                 { "name": "R2", "release": { "repo": "a/../b", "asset": "Mod.zip" }, "targets": { "windows": ["{appdata}/x"] } }]));
        assert_eq!(r.len(), 1, "Repo-Name mit „..“ fällt weg");
        let rel = json!({ "tag_name": "v1.4.0", "assets": [
            { "name": "Quellen.zip", "browser_download_url": "https://github.com/a/b/releases/download/v1.4.0/Quellen.zip" },
            { "name": "Mod_1.4.0.zip", "browser_download_url": "https://github.com/a/b/releases/download/v1.4.0/Mod_1.4.0.zip" }] });
        assert_eq!(pick_asset(&r[0], &rel).unwrap()["version"], "1.4.0");
        let portal = json!({ "releases": [
            { "version": "0.5.0", "download_url": "/download/touchdeck-bridge/a", "file_name": "touchdeck-bridge_0.5.0.zip" },
            { "version": "0.10.0", "download_url": "/download/touchdeck-bridge/c", "file_name": "touchdeck-bridge_0.10.0.zip" },
            { "version": "0.6.0", "download_url": "/download/touchdeck-bridge/b", "file_name": "touchdeck-bridge_0.6.0.zip" }] });
        assert_eq!(pick_portal(&portal).unwrap()["version"], "0.10.0");
        let ws = sanitize(&json!([{ "name": "W", "workshop": { "steamApp": "294100", "item": "3012345678" } }]));
        assert_eq!(ws.len(), 1, "nur Workshop, ohne Ziel: erlaubt (Steam installiert selbst)");
        let fixed = json!({ "release": { "repo": "a/b", "asset": "Quellen.zip" } });
        assert_eq!(pick_asset(&fixed, &rel).unwrap()["version"], "1.4.0", "ohne * aus dem Tag");
        assert!(!matches("touchdeck-bridge_*.zip", "touchdeck-bridge"));
    }

    #[test]
    fn entpacken_behaelt_einstellungen() {
        let dir = std::env::temp_dir().join(crate::core::store::new_id("td-mod-"));
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("Bridge/Code.dll", o).unwrap();
            z.write_all(b"neu").unwrap();
            z.start_file("Bridge/settings.json", o).unwrap();
            z.write_all(b"{}").unwrap();
            z.start_file("../boese.txt", o).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        let m = json!({ "unpack": true, "keep": ["Bridge/settings.json"] });
        std::fs::create_dir_all(dir.join("Mods/Bridge")).unwrap();
        std::fs::write(dir.join("Mods/Bridge/settings.json"), "{\"port\":1}").unwrap();
        let check = install(&m, buf.get_ref(), &dir.join("Mods"), "Bridge.zip").unwrap();
        assert_eq!(check, dir.join("Mods/Bridge"));
        assert_eq!(std::fs::read_to_string(dir.join("Mods/Bridge/Code.dll")).unwrap(), "neu");
        assert_eq!(std::fs::read_to_string(dir.join("Mods/Bridge/settings.json")).unwrap(), "{\"port\":1}", "eigene Einstellungen bleiben");
        assert!(!dir.join("boese.txt").exists());

        // Als Datei: alte Fassung weg, Ordner gleichen Namens bleibt.
        std::fs::write(dir.join("Mods/bridge_0.1.0.zip"), "alt").unwrap();
        std::fs::create_dir_all(dir.join("Mods/bridge_dev.zip")).unwrap();
        let m = json!({ "file": "bridge_*.zip" });
        install(&m, b"neu", &dir.join("Mods"), "bridge_0.2.0.zip").unwrap();
        assert!(!dir.join("Mods/bridge_0.1.0.zip").exists());
        assert!(dir.join("Mods/bridge_dev.zip").is_dir());
        assert!(dir.join("Mods/bridge_0.2.0.zip").is_file());

        // Entwicklungsordner gleichen Namens ist eine andere Fassung, die eigene ZIP nicht.
        std::fs::create_dir_all(dir.join("Mods/bridge")).unwrap();
        let m = json!({ "file": "bridge_*.zip", "conflicts": ["bridge", "bridge_*.zip"] });
        let mut found = conflicts(&m, &dir.join("Mods"), None, "bridge_0.3.0.zip");
        found.sort();

        // Neueste Datei im Plugin, Version aus dem Namen.
        let plugin = dir.join("plugin");
        std::fs::create_dir_all(&plugin).unwrap();
        for f in ["bridge_0.9.0.zip", "bridge_0.10.0.zip", "anderes.zip"] {
            std::fs::write(plugin.join(f), "x").unwrap();
        }
        let src = source(&m, &plugin).unwrap();
        assert_eq!((src.file.as_str(), src.version.as_str()), ("bridge_0.10.0.zip", "0.10.0"));
        // Alte ZIP-Dateien räumt die Installation selbst weg; Ordner (auch „bridge_dev.zip“) nie.
        assert_eq!(found, vec![dir.join("Mods/bridge"), dir.join("Mods/bridge_dev.zip")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
