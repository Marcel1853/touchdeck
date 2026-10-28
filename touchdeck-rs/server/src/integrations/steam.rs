// Steam: installierte Spiele aus allen Bibliotheken lesen, Coverbilder finden, Updates und laufende Spiele erkennen.
use crate::t;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::system::{os, sort_key};

// StateFlags im App-Manifest: Bit 2 = Update nötig, Bit 1024 = Update läuft.
const FLAG_UPDATE_REQUIRED: u64 = 2;
const FLAG_UPDATE_RUNNING: u64 = 1024;

#[cfg(not(windows))]
fn steam_root() -> Option<PathBuf> {
    let home = os::home();
    [".local/share/Steam", ".steam/steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam"]
        .iter()
        .map(|r| home.join(r))
        .find(|r| r.join("steamapps/libraryfolders.vdf").exists())
}

/** Unter Windows: Pfad aus der Registry (HKCU\Software\Valve\Steam), sonst der übliche Ordner. */
#[cfg(windows)]
fn steam_root() -> Option<PathBuf> {
    let from_reg = os::command("reg").args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"]).output().ok().and_then(|o| {
        let text = String::from_utf8_lossy(&o.stdout).into_owned();
        text.lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| PathBuf::from(v.trim().replace('/', "\\"))))
    });
    let fallback = std::env::var_os("ProgramFiles(x86)").map(|p| PathBuf::from(p).join("Steam"));
    [from_reg, fallback].into_iter().flatten().find(|r| r.join("steamapps/libraryfolders.vdf").exists())
}

/** Wert eines Schlüssels in einer VDF/ACF-Datei: "key"   "wert". */
fn vdf_values<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    let needle = format!("\"{key}\"");
    text.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix(&needle)?.trim_start();
            let rest = rest.strip_prefix('"')?;
            rest.split('"').next()
        })
        .collect()
}

fn library_paths() -> Vec<PathBuf> {
    let Some(root) = steam_root() else { return Vec::new() };
    let mut paths = vec![fs::canonicalize(&root).unwrap_or(root.clone())];
    if let Ok(vdf) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        for p in vdf_values(&vdf, "path") {
            let p = PathBuf::from(p.replace("\\\\", "\\"));
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
    }
    paths
}

/** Installationsordner eines Spiels (steamapps/common/<installdir>) aus seinem App-Manifest. */
pub fn game_dir(appid: &str) -> Option<PathBuf> {
    library_paths().into_iter().find_map(|lib| {
        let text = fs::read_to_string(lib.join("steamapps").join(format!("appmanifest_{appid}.acf"))).ok()?;
        let dir = lib.join("steamapps/common").join(vdf_values(&text, "installdir").first()?);
        dir.is_dir().then_some(dir)
    })
}

/** Benutzerordner im Proton-Prefix eines Windows-Spiels (…/compatdata/<appid>/pfx/drive_c/users/steamuser). */
pub fn proton_user_dir(appid: &str) -> Option<PathBuf> {
    library_paths().into_iter().map(|lib| lib.join("steamapps/compatdata").join(appid).join("pfx/drive_c/users/steamuser")).find(|d| d.is_dir())
}

struct SteamApp {
    appid: String,
    name: String,
    flags: u64,
    is_game: bool,
}

/** Werkzeuge, die Steam als „App“ führt, aber keine Spiele sind. */
fn is_tool(name: &str) -> bool {
    let n = name.to_lowercase();
    ["proton", "steam linux runtime", "steamworks common", "steam vr", "steamvr"].iter().any(|p| n.starts_with(p))
}

/** Alle installierten Apps inkl. Werkzeuge, mit Update-Status. */
fn all_apps() -> Vec<SteamApp> {
    let mut apps: HashMap<String, SteamApp> = HashMap::new();
    for lib in library_paths() {
        // Bibliothek auf nicht eingehängtem Laufwerk überspringen.
        let Ok(entries) = fs::read_dir(lib.join("steamapps")) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_manifest = name.strip_prefix("appmanifest_").and_then(|r| r.strip_suffix(".acf")).is_some_and(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()));
            if !is_manifest {
                continue;
            }
            let Ok(text) = fs::read_to_string(e.path()) else { continue };
            let (Some(appid), Some(name)) = (vdf_values(&text, "appid").first().copied(), vdf_values(&text, "name").first().copied()) else { continue };
            let flags = vdf_values(&text, "StateFlags").first().and_then(|f| f.parse().ok()).unwrap_or(0);
            apps.insert(appid.into(), SteamApp { appid: appid.into(), name: name.into(), flags, is_game: !is_tool(name) });
        }
    }
    apps.into_values().collect()
}

/** Installierte Spiele als Auswahlliste { value: appid, label: name }. */
pub fn list_games() -> Vec<(String, String)> {
    let mut games: Vec<(String, String)> = all_apps().into_iter().filter(|a| a.is_game).map(|a| (a.appid, a.name)).collect();
    games.sort_by_key(|(_, name)| sort_key(name));
    games
}

pub fn games_as_options() -> Value {
    json!(list_games().into_iter().map(|(v, l)| json!({ "value": v, "label": l })).collect::<Vec<_>>())
}

/** (Spiele mit ausstehendem Update, Spiele, die gerade aktualisiert werden). */
/** Liefert (Updates ausstehend, Updates laufen). 30 s zwischengespeichert: liest sonst alle Manifeste. */
pub fn update_status() -> (Vec<String>, Vec<String>) {
    type Cached = Option<(std::time::Instant, (Vec<String>, Vec<String>))>;
    static CACHE: std::sync::Mutex<Cached> = std::sync::Mutex::new(None);
    if let Some((at, v)) = CACHE.lock().unwrap().as_ref()
        && at.elapsed() < std::time::Duration::from_secs(30)
    {
        return v.clone();
    }
    let apps = all_apps();
    let names = |flag| apps.iter().filter(|a| a.flags & flag != 0).map(|a| a.name.clone()).collect();
    let v = (names(FLAG_UPDATE_REQUIRED), names(FLAG_UPDATE_RUNNING));
    *CACHE.lock().unwrap() = Some((std::time::Instant::now(), v.clone()));
    v
}

/** Coverbild aus dem Steam-Cache (neues Layout: Unterordner je Spiel, altes: appid_library_600x900.jpg). */
pub fn cover_image(appid: &str) -> Option<PathBuf> {
    let cache = steam_root()?.join("appcache/librarycache");
    let wanted = ["library_capsule.jpg", "library_600x900.jpg", "library_header.jpg", "header.jpg"];
    let dir = cache.join(appid);
    if dir.is_dir() {
        let mut found: HashMap<String, PathBuf> = HashMap::new();
        fn walk(d: &Path, wanted: &[&str], found: &mut HashMap<String, PathBuf>) {
            let Ok(entries) = fs::read_dir(d) else { return };
            for e in entries.flatten() {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    walk(&path, wanted, found);
                } else if wanted.contains(&name.as_str()) {
                    found.entry(name).or_insert(path);
                }
            }
        }
        walk(&dir, &wanted, &mut found);
        if let Some(p) = wanted.iter().find_map(|w| found.get(*w)) {
            return Some(p.clone());
        }
    }
    wanted.iter().map(|w| cache.join(format!("{appid}_{w}"))).find(|p| p.exists())
}

/** App-IDs der gerade laufenden Spiele (Steam startet sie über „reaper SteamLaunch AppId=…“). */
/** Schon angesehene Prozesse: PID → Steam-Spiel (oder keins). Die Kommandozeile ändert sich nicht. */
static SEEN: std::sync::Mutex<Option<HashMap<u32, Option<String>>>> = std::sync::Mutex::new(None);

pub fn running_games() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Ok(entries) = fs::read_dir("/proc") else { return ids };
    let mut guard = SEEN.lock().unwrap();
    let old = guard.take().unwrap_or_default();
    let mut seen = HashMap::with_capacity(old.len());
    for e in entries.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|p| p.parse::<u32>().ok()) else { continue };
        // Nur neue Prozesse lesen; bekannte aus dem letzten Durchlauf übernehmen.
        let game = match old.get(&pid) {
            Some(g) => g.clone(),
            None => {
                // Prozess bereits beendet oder fremder Benutzer: überspringen.
                let Ok(cmd) = fs::read(e.path().join("cmdline")) else { continue };
                let cmd = String::from_utf8_lossy(&cmd);
                cmd.split("SteamLaunch\0AppId=").nth(1).map(|rest| rest.chars().take_while(char::is_ascii_digit).collect::<String>()).filter(|id| !id.is_empty())
            }
        };
        if let Some(id) = &game {
            ids.insert(id.clone());
        }
        seen.insert(pid, game);
    }
    *guard = Some(seen);
    ids
}

pub fn run_game(appid: &str) -> Result<()> {
    if appid.is_empty() || !appid.chars().all(|c| c.is_ascii_digit()) {
        bail!(t!("steam.noGameSelected"));
    }
    os::open_with_default(&format!("steam://rungameid/{appid}"))
}

pub fn open_steam(view: &str) -> Result<()> {
    let url = match view {
        "downloads" => "steam://open/downloads",
        "bigpicture" => "steam://open/bigpicture",
        "friends" => "steam://open/friends",
        "store" => "steam://store",
        "screenshots" => "steam://open/screenshots",
        _ => "steam://open/games",
    };
    os::open_with_default(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdf_lesen() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"294100\"\n\t\"name\"\t\t\"RimWorld\"\n\t\"StateFlags\"\t\t\"6\"\n}";
        assert_eq!(vdf_values(acf, "appid"), vec!["294100"]);
        assert_eq!(vdf_values(acf, "name"), vec!["RimWorld"]);
        assert_eq!(vdf_values(acf, "StateFlags"), vec!["6"]);
        assert!(is_tool("Proton 9.0"));
        assert!(!is_tool("RimWorld"));
    }
}
