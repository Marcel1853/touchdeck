// Plugins: Erweiterungen in JavaScript, die abgeschottet in einer eigenen kleinen JS-Engine (QuickJS) laufen.
//
// Sicherheit:
// - Ein Plugin kann nur, was TouchDeck ihm anbietet (td.…): keine Dateien, keine Programme, kein freies Netz.
// - Was es darf, steht in plugin.json unter "permissions" und wird vor dem Installieren angezeigt:
//   "http": erlaubte Adressen (z. B. "api.example.com", "*.example.com", "lan" für das Heimnetz),
//   "values": TouchDeck-Werte lesen und setzen, "notify": Benachrichtigungen zeigen.
// - Installiert wird nur aus öffentlichen GitHub-Repos (jeder kann den Quelltext prüfen); zum Entwickeln
//   gibt es zusätzlich einen lokalen Testmodus.
// - Speicher (32 MB) und Rechenzeit (5 s je Aufruf) sind begrenzt.
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use rquickjs::{CatchResultExt, Context, Function, Runtime};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use crate::core::store::js_string;
use crate::server::App;

const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
const CALL_LIMIT_MS: i64 = 5000;
const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_TOTAL: u64 = 10 * 1024 * 1024;
const ALLOWED_EXT: [&str; 7] = ["js", "json", "md", "txt", "png", "svg", "jpg"];
pub const MARKET_URL: &str = "https://raw.githubusercontent.com/Marcel1853/touchdeck-plugins/main/plugins.json";

/** Vorgefertigte Hilfen, aus denen das Plugin `td` nutzt. Die __-Funktionen stellt TouchDeck bereit. */
const PRELUDE: &str = r#"
(() => {
  const actions = {};
  const timers = [];
  const log = (level) => (...args) => __log(level, args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' '));
  globalThis.console = { log: log('info'), info: log('info'), warn: log('warn'), error: log('error') };
  globalThis.td = Object.freeze({
    plugin: Object.freeze(JSON.parse(__plugin)),
    settings: Object.freeze(JSON.parse(__settings)),
    log: log('info'),
    warn: log('warn'),
    action(id, fn) { actions[String(id)] = fn; },
    every(ms, fn) { timers.push({ ms: Math.max(1000, Number(ms) || 1000), fn }); },
    fire(trigger, data = {}) { __fire(String(trigger), JSON.stringify(data)); },
    notify(title, body = '') { __notify(String(title), String(body)); },
    values: Object.freeze({
      get: (name) => __getValue(String(name)),
      set: (name, value) => __setValue(String(name), String(value ?? '')),
    }),
    http(req) {
      const r = typeof req === 'string' ? { url: req } : req;
      const res = JSON.parse(__http(JSON.stringify({ method: r.method ?? 'GET', url: r.url, headers: r.headers ?? {},
        body: r.json !== undefined ? JSON.stringify(r.json) : r.body ?? null, isJson: r.json !== undefined })));
      res.json = () => JSON.parse(res.body);
      return res;
    },
  });
  globalThis.__runAction = (id, params, info) => {
    const fn = actions[id];
    if (!fn) throw new Error(`Aktion „${id}“ ist im Plugin nicht registriert (td.action).`);
    const r = fn(JSON.parse(params), JSON.parse(info));
    return r === undefined || r === null ? null : String(r);
  };
  globalThis.__timerCount = () => timers.length;
  globalThis.__timerMs = (i) => timers[i].ms;
  globalThis.__runTimer = (i) => { timers[i].fn(); };
})();
"#;

// ---------- Beschreibung (plugin.json) ----------

fn valid_id(id: &str) -> bool {
    (1..=32).contains(&id.len()) && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !id.starts_with('-')
}

fn short_id(id: &str) -> bool {
    (1..=24).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn text(v: &Value, max: usize) -> String {
    v.as_str().unwrap_or("").trim().chars().take(max).collect()
}

/** Prüft plugin.json und bringt sie in eine feste Form. */
pub fn check_manifest(raw: &Value) -> Result<Value> {
    let id = text(&raw["id"], 32);
    if !valid_id(&id) {
        bail!("plugin.json: „id“ muss aus 1–32 Kleinbuchstaben, Ziffern oder Bindestrichen bestehen.");
    }
    let name = text(&raw["name"], 60);
    if name.is_empty() {
        bail!("plugin.json: „name“ fehlt.");
    }
    let main = text(&raw["main"], 80);
    let main = if main.is_empty() { "plugin.js".to_string() } else { main };
    if !main.ends_with(".js") || main.contains('/') || main.contains('\\') {
        bail!("plugin.json: „main“ muss eine .js-Datei im Hauptordner sein.");
    }
    let p = &raw["permissions"];
    let http: Vec<String> = p["http"].as_array().into_iter().flatten().filter_map(|h| h.as_str()).map(|h| h.trim().to_lowercase()).filter(|h| !h.is_empty()).take(20).collect();
    for h in &http {
        let host = h.strip_prefix("*.").unwrap_or(h);
        if h != "lan" && (host.is_empty() || !host.contains('.') || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')) {
            bail!("plugin.json: ungültige Adresse in permissions.http: „{h}“ (Beispiele: api.example.com, *.example.com, lan).");
        }
    }
    let items = |key: &str| -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for a in raw[key].as_array().into_iter().flatten().take(50) {
            let aid = text(&a["id"], 24);
            if !short_id(&aid) {
                bail!("plugin.json: jede Angabe in „{key}“ braucht eine „id“ (Buchstaben, Ziffern, _ oder -).");
            }
            let fields: Vec<Value> = a["fields"].as_array().into_iter().flatten().filter(|f| f["key"].is_string()).take(20).cloned().collect();
            out.push(json!({ "id": aid, "label": text(&a["label"], 80).chars().take(80).collect::<String>(), "icon": text(&a["icon"], 8), "fields": fields }));
        }
        Ok(out)
    };
    Ok(json!({
        "id": id,
        "name": name,
        "version": text(&raw["version"], 20),
        "author": text(&raw["author"], 60),
        "description": text(&raw["description"], 400),
        "main": main,
        "permissions": { "http": http, "values": p["values"] == true, "notify": p["notify"] == true },
        "settings": raw["settings"].as_array().into_iter().flatten().filter(|f| f["key"].is_string()).take(20).cloned().collect::<Vec<_>>(),
        "actions": items("actions")?,
        "triggers": items("triggers")?,
    }))
}

/** Lesbare Liste der Berechtigungen (für die Anzeige vor dem Installieren). */
pub fn describe_permissions(m: &Value) -> Vec<String> {
    let p = &m["permissions"];
    let mut out: Vec<String> = p["http"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h.as_str())
        .map(|h| if h == "lan" { "Geräte im Heimnetz ansprechen".to_string() } else { format!("Internet: {h}") })
        .collect();
    if p["values"] == true {
        out.push("TouchDeck-Werte lesen und ändern".into());
    }
    if p["notify"] == true {
        out.push("Benachrichtigungen anzeigen".into());
    }
    out
}

// ---------- Netzwerk-Erlaubnis ----------

fn is_lan(host: &str) -> bool {
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback(),
        Err(_) => host.ends_with(".local") || host == "localhost",
    }
}

/** Darf das Plugin diese Adresse aufrufen? */
pub fn http_allowed(allowed: &[String], url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url) else { return false };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = u.host_str().map(|h| h.trim_matches(['[', ']']).to_lowercase()) else { return false };
    allowed.iter().any(|a| {
        if a == "lan" {
            is_lan(&host)
        } else if let Some(base) = a.strip_prefix("*.") {
            host == base || host.ends_with(&format!(".{base}"))
        } else {
            host == *a
        }
    })
}

// ---------- Laufendes Plugin ----------

/** Nur die eigentliche Meldung, ohne JS-Stacktrace und „Error: “ davor. */
fn clean_error(e: impl std::fmt::Display) -> String {
    let text = e.to_string();
    let first = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Fehler im Plugin");
    first.strip_prefix("Error: ").unwrap_or(first).to_string()
}

enum Msg {
    Action { id: String, params: Value, info: Value, depth: u32, reply: oneshot::Sender<Result<Option<String>, String>> },
    Stop,
}

struct Running {
    tx: mpsc::Sender<Msg>,
    error: Arc<Mutex<Option<String>>>,
}

/** Was ein Plugin von TouchDeck nutzen kann (im eigenen Thread des Plugins). */
struct Host {
    id: String,
    app: Weak<App>,
    http: Vec<String>,
    values: bool,
    notify: bool,
    rt: tokio::runtime::Handle,
    depth: std::cell::Cell<u32>,
}

fn js_err(ctx: &rquickjs::Ctx<'_>, msg: impl AsRef<str>) -> rquickjs::Error {
    rquickjs::Exception::throw_message(ctx, msg.as_ref())
}

fn install_host(ctx: &rquickjs::Ctx<'_>, host: Rc<Host>, manifest: &Value, settings: &Value) -> rquickjs::Result<()> {
    let g = ctx.globals();
    g.set("__plugin", json!({ "id": manifest["id"], "name": manifest["name"], "version": manifest["version"] }).to_string())?;
    g.set("__settings", settings.to_string())?;

    let h = host.clone();
    g.set(
        "__log",
        Function::new(ctx.clone(), move |level: String, msg: String| {
            if let Some(app) = h.app.upgrade() {
                let source = format!("Plugin {}", h.id);
                let msg: String = msg.chars().take(1000).collect();
                match level.as_str() {
                    "warn" => app.log.warn(&source, msg),
                    "error" => app.log.error(&source, msg),
                    _ => app.log.info(&source, msg),
                }
            }
        })?,
    )?;

    let h = host.clone();
    g.set(
        "__http",
        Function::new(ctx.clone(), move |ctx: rquickjs::Ctx<'_>, req: String| -> rquickjs::Result<String> {
            let req: Value = serde_json::from_str(&req).unwrap_or_default();
            let url = req["url"].as_str().unwrap_or("").to_string();
            if !http_allowed(&h.http, &url) {
                return Err(js_err(&ctx, format!("Keine Erlaubnis für {url} (plugin.json → permissions.http).")));
            }
            let result = h.rt.block_on(async {
                let method = reqwest::Method::from_bytes(req["method"].as_str().unwrap_or("GET").to_uppercase().as_bytes())?;
                // Keine Weiterleitungen: sonst könnte ein erlaubter Server auf eine verbotene Adresse umleiten.
                let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).redirect(reqwest::redirect::Policy::none()).build()?;
                let mut r = client.request(method, &url);
                for (k, v) in req["headers"].as_object().into_iter().flatten() {
                    r = r.header(k.as_str(), js_string(v));
                }
                if req["isJson"] == true {
                    r = r.header("Content-Type", "application/json");
                }
                if let Some(body) = req["body"].as_str() {
                    r = r.body(body.to_string());
                }
                let res = r.send().await?;
                let status = res.status().as_u16();
                let bytes = res.bytes().await?;
                let body = String::from_utf8_lossy(&bytes[..bytes.len().min(1024 * 1024)]).into_owned();
                anyhow::Ok(json!({ "status": status, "ok": (200..300).contains(&status), "body": body }))
            });
            result.map(|v| v.to_string()).map_err(|e| js_err(&ctx, format!("HTTP: {e}")))
        })?,
    )?;

    let h = host.clone();
    g.set(
        "__getValue",
        Function::new(ctx.clone(), move |ctx: rquickjs::Ctx<'_>, name: String| -> rquickjs::Result<Option<String>> {
            if !h.values {
                return Err(js_err(&ctx, "Keine Erlaubnis für Werte (plugin.json → permissions.values)."));
            }
            let app = h.app.upgrade().ok_or_else(|| js_err(&ctx, "TouchDeck wird beendet."))?;
            Ok(app.store.lock().unwrap().config.variables.get(&name).map(js_string))
        })?,
    )?;

    let h = host.clone();
    g.set(
        "__setValue",
        Function::new(ctx.clone(), move |ctx: rquickjs::Ctx<'_>, name: String, value: String| -> rquickjs::Result<()> {
            if !h.values {
                return Err(js_err(&ctx, "Keine Erlaubnis für Werte (plugin.json → permissions.values)."));
            }
            let app = h.app.upgrade().ok_or_else(|| js_err(&ctx, "TouchDeck wird beendet."))?;
            let r = app.store.lock().unwrap().set_variable(&name, &json!(value));
            r.map_err(|e| js_err(&ctx, e.to_string()))
        })?,
    )?;

    let h = host.clone();
    g.set(
        "__fire",
        Function::new(ctx.clone(), move |trigger: String, data: String| {
            if let Some(app) = h.app.upgrade() {
                let data: Value = serde_json::from_str(&data).unwrap_or_else(|_| json!({}));
                app.events.fire_at(&app, &format!("plugin:{}:{trigger}", h.id), &data, h.depth.get());
            }
        })?,
    )?;

    let h = host;
    g.set(
        "__notify",
        Function::new(ctx.clone(), move |ctx: rquickjs::Ctx<'_>, title: String, body: String| -> rquickjs::Result<()> {
            if !h.notify {
                return Err(js_err(&ctx, "Keine Erlaubnis für Benachrichtigungen (plugin.json → permissions.notify)."));
            }
            let _ = std::process::Command::new("notify-send").args(["-a", "TouchDeck", &title, &body]).spawn();
            Ok(())
        })?,
    )?;
    Ok(())
}

/** Startet ein Plugin in einem eigenen Thread mit eigener JS-Engine. */
fn spawn_plugin(app: &Arc<App>, dir: PathBuf, manifest: Value, settings: Value) -> Result<Running> {
    let source = std::fs::read_to_string(dir.join(manifest["main"].as_str().unwrap_or("plugin.js")))
        .map_err(|_| anyhow!("Die Datei {} fehlt im Plugin.", manifest["main"]))?;
    let (tx, rx) = mpsc::channel::<Msg>();
    let error = Arc::new(Mutex::new(None));
    let err = error.clone();
    let rt = tokio::runtime::Handle::current();
    let weak = Arc::downgrade(app);
    let id = manifest["id"].as_str().unwrap_or("").to_string();
    std::thread::Builder::new().name(format!("plugin-{id}")).spawn(move || {
        // Im Laufzeit-Kontext von Tokio, damit Werte und Events wie gewohnt weitergereicht werden.
        let _guard = rt.enter();
        let report = |msg: String| {
            if let Some(app) = weak.upgrade() {
                app.log.error(&format!("Plugin {id}"), &msg);
            }
            *err.lock().unwrap() = Some(msg);
        };
        let p = &manifest["permissions"];
        let host = Rc::new(Host {
            id: id.clone(),
            app: weak.clone(),
            http: p["http"].as_array().into_iter().flatten().filter_map(|h| h.as_str().map(String::from)).collect(),
            values: p["values"] == true,
            notify: p["notify"] == true,
            rt: rt.clone(),
            depth: std::cell::Cell::new(0),
        });
        let Ok(runtime) = Runtime::new() else { return report("JS-Engine konnte nicht starten.".into()) };
        runtime.set_memory_limit(MEMORY_LIMIT);
        runtime.set_max_stack_size(512 * 1024);
        // Endlosschleifen abbrechen: jeder Aufruf hat höchstens 5 Sekunden.
        let deadline = Arc::new(AtomicI64::new(i64::MAX));
        let d = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || chrono::Utc::now().timestamp_millis() > d.load(Ordering::Relaxed))));
        let Ok(context) = Context::full(&runtime) else { return report("JS-Engine konnte nicht starten.".into()) };
        let start = |deadline: &AtomicI64| deadline.store(chrono::Utc::now().timestamp_millis() + CALL_LIMIT_MS, Ordering::Relaxed);

        // Laden: Hilfen, dann das Plugin selbst.
        start(&deadline);
        let loaded: Result<Vec<u64>, String> = context.with(|ctx| {
            install_host(&ctx, host.clone(), &manifest, &settings).map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(PRELUDE).catch(&ctx).map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(source).catch(&ctx).map_err(|e| format!("Fehler im Plugin: {}", clean_error(e)))?;
            let count: u32 = ctx.globals().get::<_, Function>("__timerCount").and_then(|f| f.call(())).unwrap_or(0);
            let ms: Function = ctx.globals().get("__timerMs").map_err(|e| e.to_string())?;
            Ok((0..count.min(20)).map(|i| ms.call::<_, f64>((i,)).unwrap_or(1000.0) as u64).collect())
        });
        deadline.store(i64::MAX, Ordering::Relaxed);
        let intervals = match loaded {
            Ok(t) => t,
            Err(e) => return report(e),
        };
        if let Some(app) = weak.upgrade() {
            app.log.info(&format!("Plugin {id}"), "gestartet");
        }
        let mut next: Vec<Instant> = intervals.iter().map(|ms| Instant::now() + Duration::from_millis(*ms)).collect();

        loop {
            let wait = next.iter().min().map_or(Duration::from_secs(3600), |t| t.saturating_duration_since(Instant::now()));
            match rx.recv_timeout(wait) {
                Ok(Msg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Msg::Action { id: action, params, info, depth, reply }) => {
                    host.depth.set(depth);
                    start(&deadline);
                    let r = context.with(|ctx| {
                        let f: Function = ctx.globals().get("__runAction").map_err(|e| e.to_string())?;
                        f.call::<_, Option<String>>((action, params.to_string(), info.to_string())).catch(&ctx).map_err(clean_error)
                    });
                    deadline.store(i64::MAX, Ordering::Relaxed);
                    let r = r.map_err(|e| if e.contains("interrupted") { "Das Plugin hat zu lange gerechnet (über 5 s) und wurde gestoppt.".into() } else { e });
                    let _ = reply.send(r);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let now = Instant::now();
                    for (i, t) in next.iter_mut().enumerate() {
                        if *t > now {
                            continue;
                        }
                        *t = now + Duration::from_millis(intervals[i]);
                        host.depth.set(0);
                        start(&deadline);
                        let r = context.with(|ctx| {
                            let f: Function = ctx.globals().get("__runTimer").map_err(|e| e.to_string())?;
                            f.call::<_, ()>((i as u32,)).catch(&ctx).map_err(clean_error)
                        });
                        deadline.store(i64::MAX, Ordering::Relaxed);
                        if let (Err(e), Some(app)) = (r, weak.upgrade()) {
                            app.log.warn(&format!("Plugin {id}"), format!("Zeitgeber: {e}"));
                        }
                    }
                }
            }
        }
    })?;
    Ok(Running { tx, error })
}

// ---------- Verwaltung ----------

/** Eintrag in installed.json. */
fn installed_file(dir: &Path) -> PathBuf {
    dir.join("installed.json")
}

pub struct Plugins {
    dir: PathBuf,
    running: Mutex<HashMap<String, Running>>,
    /// Beschreibungen aller installierten Plugins (für Katalog und Ausführung).
    manifests: Mutex<HashMap<String, Value>>,
    http: reqwest::Client,
}

/** owner/repo aus „owner/repo“ oder einer GitHub-Adresse. */
pub fn parse_repo(input: &str) -> Result<(String, String)> {
    let s = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let s = s.strip_prefix("github.com/").unwrap_or(s);
    let mut parts = s.split('/');
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else { bail!("Bitte das GitHub-Repo als „besitzer/name“ oder als github.com-Adresse angeben.") };
    let ok = |p: &str| !p.is_empty() && p.len() <= 100 && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if !ok(owner) || !ok(repo) {
        bail!("Ungültiger Repo-Name.");
    }
    Ok((owner.into(), repo.into()))
}

/** Dateien aus einem GitHub-ZIP: Hauptordner abschneiden, nur erlaubte Dateien, Größen begrenzt. */
fn unpack(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow!("Download ist kein gültiges ZIP."))?;
    let mut files = Vec::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let name = f.name()?.to_string();
        // Nur Dateien direkt im Hauptordner des Repos (keine Unterordner).
        let mut parts = name.splitn(2, '/');
        let (_, Some(rel)) = (parts.next(), parts.next()) else { continue };
        if rel.contains('/') || rel.starts_with('.') {
            continue;
        }
        let ext = Path::new(rel).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if !ALLOWED_EXT.contains(&ext.as_str()) || f.size() > MAX_FILE {
            continue;
        }
        total += f.size();
        if total > MAX_TOTAL {
            bail!("Das Plugin ist zu groß (mehr als 10 MB).");
        }
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        files.push((rel.to_string(), data));
    }
    Ok(files)
}

fn manifest_of(files: &[(String, Vec<u8>)]) -> Result<(Value, String)> {
    let raw = files.iter().find(|(n, _)| n == "plugin.json").ok_or_else(|| anyhow!("Im Repo fehlt die Datei plugin.json im Hauptordner."))?;
    let digest = hex::encode(Sha256::digest(&raw.1));
    let value: Value = serde_json::from_slice(&raw.1).map_err(|e| anyhow!("plugin.json ist kein gültiges JSON: {e}"))?;
    let m = check_manifest(&value)?;
    if !files.iter().any(|(n, _)| *n == m["main"]) {
        bail!("Die Datei {} fehlt im Repo.", m["main"]);
    }
    Ok((m, digest))
}

impl Plugins {
    pub fn new(data_dir: &Path) -> Self {
        let dir = data_dir.join("plugins");
        let _ = std::fs::create_dir_all(&dir);
        Plugins {
            dir,
            running: Mutex::new(HashMap::new()),
            manifests: Mutex::new(HashMap::new()),
            http: reqwest::Client::builder().user_agent("TouchDeck").timeout(Duration::from_secs(20)).build().expect("HTTP-Client"),
        }
    }

    fn read_installed(&self) -> Map<String, Value> {
        std::fs::read_to_string(installed_file(&self.dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn write_installed(&self, m: &Map<String, Value>) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let file = installed_file(&self.dir);
        std::fs::write(&file, serde_json::to_string_pretty(m)?)?;
        // Kann Zugangsdaten (Plugin-Einstellungen) enthalten.
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    /** Ordner eines Plugins: installiert unter plugins/<id>, im Testmodus der lokale Ordner. */
    fn plugin_dir(&self, id: &str, entry: &Value) -> PathBuf {
        match entry["source"].as_str().and_then(|s| s.strip_prefix("local:")) {
            Some(path) => PathBuf::from(path),
            None => self.dir.join(id),
        }
    }

    fn load_manifest(dir: &Path) -> Result<Value> {
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("plugin.json")).map_err(|_| anyhow!("plugin.json fehlt."))?)?;
        check_manifest(&raw)
    }

    /** Alle installierten Plugins einlesen und die eingeschalteten starten. */
    pub fn start_all(&self, app: &Arc<App>) {
        for (id, entry) in self.read_installed() {
            let dir = self.plugin_dir(&id, &entry);
            match Self::load_manifest(&dir) {
                Ok(m) => {
                    self.manifests.lock().unwrap().insert(id.clone(), m);
                    if entry["enabled"] != false {
                        self.start(app, &id);
                    }
                }
                Err(e) => app.log.error(&format!("Plugin {id}"), e.to_string()),
            }
        }
    }

    fn start(&self, app: &Arc<App>, id: &str) {
        self.stop(id);
        let entry = self.read_installed().get(id).cloned().unwrap_or_default();
        let Some(manifest) = self.manifests.lock().unwrap().get(id).cloned() else { return };
        match spawn_plugin(app, self.plugin_dir(id, &entry), manifest, entry["settings"].clone()) {
            Ok(r) => {
                self.running.lock().unwrap().insert(id.into(), r);
            }
            Err(e) => app.log.error(&format!("Plugin {id}"), e.to_string()),
        }
    }

    fn stop(&self, id: &str) {
        if let Some(r) = self.running.lock().unwrap().remove(id) {
            let _ = r.tx.send(Msg::Stop);
        }
    }

    pub fn stop_all(&self) {
        for (_, r) in self.running.lock().unwrap().drain() {
            let _ = r.tx.send(Msg::Stop);
        }
    }

    /** Für den Editor: installierte Plugins mit Beschreibung, Berechtigungen, Status und Einstellungen. */
    pub fn list(&self) -> Vec<Value> {
        let installed = self.read_installed();
        let manifests = self.manifests.lock().unwrap();
        let running = self.running.lock().unwrap();
        let mut out: Vec<Value> = installed
            .iter()
            .map(|(id, e)| {
                let m = manifests.get(id).cloned().unwrap_or_else(|| json!({ "id": id, "name": id }));
                let state = match running.get(id) {
                    Some(r) => r.error.lock().unwrap().clone().map_or_else(|| json!({ "state": "running" }), |e| json!({ "state": "error", "error": e })),
                    None if e["enabled"] == false => json!({ "state": "off" }),
                    None => json!({ "state": "error", "error": "Konnte nicht geladen werden (siehe Logs)." }),
                };
                json!({
                    "id": id, "manifest": m, "permissions": describe_permissions(&m), "enabled": e["enabled"] != false,
                    "source": e["source"], "ref": e["ref"], "url": e["url"], "settings": e["settings"], "status": state,
                })
            })
            .collect();
        out.sort_by_key(|p| p["manifest"]["name"].as_str().unwrap_or("").to_lowercase());
        out
    }

    /** Aktionen und Auslöser der eingeschalteten Plugins für den Editor-Katalog. */
    pub fn catalog(&self) -> (Vec<Value>, Vec<Value>) {
        let installed = self.read_installed();
        let manifests = self.manifests.lock().unwrap();
        let (mut actions, mut triggers) = (Vec::new(), Vec::new());
        for (id, m) in manifests.iter().filter(|(id, _)| installed.get(*id).is_some_and(|e| e["enabled"] != false)) {
            let name = m["name"].as_str().unwrap_or(id);
            for (list, out) in [(&m["actions"], &mut actions), (&m["triggers"], &mut triggers)] {
                for a in list.as_array().into_iter().flatten() {
                    out.push(json!({
                        "type": format!("plugin:{id}:{}", a["id"].as_str().unwrap_or("")),
                        "category": name, "group": "Plugins",
                        "label": a["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(a["id"].as_str().unwrap_or("")),
                        "icon": a["icon"].as_str().filter(|i| !i.is_empty()).unwrap_or("🧱"),
                        "fields": a["fields"],
                    }));
                }
            }
        }
        (actions, triggers)
    }

    pub fn label(&self, kind: &str) -> Option<String> {
        let (plugin, action) = kind.strip_prefix("plugin:")?.split_once(':')?;
        let manifests = self.manifests.lock().unwrap();
        let a = manifests.get(plugin)?["actions"].as_array()?.iter().find(|a| a["id"] == action)?.clone();
        Some(a["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(action).to_string())
    }

    pub fn has_trigger(&self, kind: &str) -> bool {
        let Some((plugin, trigger)) = kind.strip_prefix("plugin:").and_then(|k| k.split_once(':')) else { return false };
        self.manifests.lock().unwrap().get(plugin).is_some_and(|m| m["triggers"].as_array().is_some_and(|t| t.iter().any(|x| x["id"] == trigger)))
    }

    /** Eine Plugin-Aktion ausführen (Typ plugin:<id>:<aktion>). */
    pub async fn run_action(&self, kind: &str, params: &Map<String, Value>, info: Value, depth: u32) -> Result<Option<String>> {
        let (plugin, action) = kind.strip_prefix("plugin:").and_then(|k| k.split_once(':')).ok_or_else(|| anyhow!("Unbekannte Plugin-Aktion."))?;
        let (reply, wait) = oneshot::channel();
        {
            let running = self.running.lock().unwrap();
            let r = running.get(plugin).ok_or_else(|| anyhow!("Das Plugin „{plugin}“ ist nicht installiert oder ausgeschaltet."))?;
            if let Some(e) = r.error.lock().unwrap().as_ref() {
                bail!("Das Plugin „{plugin}“ läuft nicht: {e}");
            }
            r.tx.send(Msg::Action { id: action.into(), params: Value::Object(params.clone()), info, depth, reply }).map_err(|_| anyhow!("Das Plugin „{plugin}“ läuft nicht."))?;
        }
        match tokio::time::timeout(Duration::from_secs(30), wait).await {
            Ok(Ok(r)) => r.map_err(|e| anyhow!(e)),
            Ok(Err(_)) => bail!("Das Plugin „{plugin}“ wurde beendet."),
            Err(_) => bail!("Das Plugin „{plugin}“ antwortet nicht."),
        }
    }

    // ---------- Installieren ----------

    /** Prüft ein GitHub-Repo: öffentlich? plugin.json gültig? Liefert Beschreibung, Berechtigungen, Version und Prüfsumme. */
    pub async fn preview(&self, repo_input: &str) -> Result<Value> {
        let (owner, repo) = parse_repo(repo_input)?;
        let (files, git_ref, url) = self.download(&owner, &repo).await?;
        let (m, digest) = manifest_of(&files)?;
        Ok(json!({ "repo": format!("{owner}/{repo}"), "ref": git_ref, "url": url, "manifest": m, "permissions": describe_permissions(&m), "digest": digest }))
    }

    /** Öffentliches Repo herunterladen: neueste Veröffentlichung (Release), sonst der Hauptzweig. */
    async fn download(&self, owner: &str, repo: &str) -> Result<(Vec<(String, Vec<u8>)>, String, String)> {
        let api = format!("https://api.github.com/repos/{owner}/{repo}");
        let res = self.http.get(&api).header("Accept", "application/vnd.github+json").send().await?;
        if res.status().as_u16() == 404 {
            bail!("Das Repo {owner}/{repo} gibt es nicht oder es ist privat. Plugins müssen öffentlich auf GitHub liegen.");
        }
        let info: Value = res.error_for_status()?.json().await?;
        if info["private"] != false || info["visibility"].as_str().is_some_and(|v| v != "public") {
            bail!("Das Repo ist nicht öffentlich. Plugins müssen öffentlich auf GitHub liegen, damit jeder den Quelltext prüfen kann.");
        }
        let url = info["html_url"].as_str().unwrap_or("").to_string();
        let release = self.http.get(format!("{api}/releases/latest")).header("Accept", "application/vnd.github+json").send().await?;
        let (zip_ref, git_ref) = match release.status().is_success() {
            true => {
                let tag = release.json::<Value>().await?["tag_name"].as_str().unwrap_or("").to_string();
                (format!("refs/tags/{tag}"), tag)
            }
            false => {
                let branch = info["default_branch"].as_str().unwrap_or("main").to_string();
                (format!("refs/heads/{branch}"), branch)
            }
        };
        let zip = self.http.get(format!("https://codeload.github.com/{owner}/{repo}/zip/{zip_ref}")).send().await?.error_for_status()?.bytes().await?;
        if zip.len() as u64 > 50 * 1024 * 1024 {
            bail!("Das Repo ist zu groß für ein Plugin.");
        }
        Ok((unpack(&zip)?, git_ref, url))
    }

    /** Installieren oder aktualisieren; digest muss zur angezeigten Vorschau passen (nichts Unbemerktes). */
    pub async fn install(&self, app: &Arc<App>, repo_input: &str, digest: &str) -> Result<String> {
        let (owner, repo) = parse_repo(repo_input)?;
        let (files, git_ref, url) = self.download(&owner, &repo).await?;
        let (m, d) = manifest_of(&files)?;
        if d != digest {
            bail!("Das Plugin hat sich seit der Vorschau geändert. Bitte noch einmal prüfen.");
        }
        let id = m["id"].as_str().unwrap_or("").to_string();
        let mut installed = self.read_installed();
        if let Some(other) = installed.get(&id).and_then(|e| e["source"].as_str()).filter(|s| *s != format!("github:{owner}/{repo}"))
        {
            bail!("Ein anderes Plugin mit der ID „{id}“ ist schon installiert ({other}).");
        }
        self.stop(&id);
        let dir = self.dir.join(&id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        for (name, data) in &files {
            std::fs::write(dir.join(name), data)?;
        }
        let settings = installed.get(&id).map(|e| e["settings"].clone()).filter(Value::is_object).unwrap_or_else(|| json!({}));
        installed.insert(id.clone(), json!({ "source": format!("github:{owner}/{repo}"), "ref": git_ref, "url": url, "enabled": true, "settings": settings }));
        self.write_installed(&installed)?;
        self.manifests.lock().unwrap().insert(id.clone(), m);
        self.start(app, &id);
        app.log.info("Plugins", format!("{id} installiert ({owner}/{repo} @ {git_ref})"));
        Ok(id)
    }

    /** Testmodus: Plugin direkt aus einem lokalen Ordner laden (zum Entwickeln, bevor es veröffentlicht ist). */
    pub fn install_local(&self, app: &Arc<App>, path: &str) -> Result<String> {
        let dir = crate::system::linux::expand_home(path.trim());
        let dir = dir.canonicalize().map_err(|_| anyhow!("Den Ordner {} gibt es nicht.", dir.display()))?;
        let m = Self::load_manifest(&dir)?;
        let id = m["id"].as_str().unwrap_or("").to_string();
        let mut installed = self.read_installed();
        let settings = installed.get(&id).map(|e| e["settings"].clone()).filter(Value::is_object).unwrap_or_else(|| json!({}));
        installed.insert(id.clone(), json!({ "source": format!("local:{}", dir.display()), "ref": "lokal", "url": null, "enabled": true, "settings": settings }));
        self.write_installed(&installed)?;
        self.manifests.lock().unwrap().insert(id.clone(), m);
        self.start(app, &id);
        app.log.warn("Plugins", format!("{id} im Testmodus geladen aus {}", dir.display()));
        Ok(id)
    }

    /** Neu laden (z. B. nach Änderungen im Testmodus). */
    pub fn reload(&self, app: &Arc<App>, id: &str) -> Result<()> {
        let entry = self.read_installed().get(id).cloned().ok_or_else(|| anyhow!("Unbekanntes Plugin."))?;
        let m = Self::load_manifest(&self.plugin_dir(id, &entry))?;
        self.manifests.lock().unwrap().insert(id.into(), m);
        if entry["enabled"] != false {
            self.start(app, id);
        }
        Ok(())
    }

    pub fn set_enabled(&self, app: &Arc<App>, id: &str, on: bool) -> Result<()> {
        let mut installed = self.read_installed();
        let e = installed.get_mut(id).ok_or_else(|| anyhow!("Unbekanntes Plugin."))?;
        e["enabled"] = json!(on);
        self.write_installed(&installed)?;
        if on { self.start(app, id) } else { self.stop(id) }
        Ok(())
    }

    pub fn set_settings(&self, app: &Arc<App>, id: &str, settings: &Value) -> Result<()> {
        let mut installed = self.read_installed();
        let e = installed.get_mut(id).ok_or_else(|| anyhow!("Unbekanntes Plugin."))?;
        let keys: Vec<String> = self.manifests.lock().unwrap().get(id).and_then(|m| m["settings"].as_array().cloned()).unwrap_or_default().iter().filter_map(|f| f["key"].as_str().map(String::from)).collect();
        let clean: Map<String, Value> = settings.as_object().into_iter().flatten().filter(|(k, _)| keys.contains(k)).map(|(k, v)| (k.clone(), json!(js_string(v).chars().take(2000).collect::<String>()))).collect();
        e["settings"] = Value::Object(clean);
        let on = e["enabled"] != false;
        self.write_installed(&installed)?;
        if on {
            self.start(app, id);
        }
        Ok(())
    }

    pub fn remove(&self, app: &Arc<App>, id: &str) -> Result<()> {
        let mut installed = self.read_installed();
        let e = installed.remove(id).ok_or_else(|| anyhow!("Unbekanntes Plugin."))?;
        self.stop(id);
        self.write_installed(&installed)?;
        self.manifests.lock().unwrap().remove(id);
        if !e["source"].as_str().is_some_and(|s| s.starts_with("local:")) {
            let _ = std::fs::remove_dir_all(self.dir.join(id));
        }
        app.log.info("Plugins", format!("{id} entfernt"));
        Ok(())
    }

    /** Liste aus dem öffentlichen Marktplatz-Repo. */
    pub async fn market(&self) -> Result<Value> {
        let url = std::env::var("TOUCHDECK_MARKET").unwrap_or_else(|_| MARKET_URL.into());
        let res = self.http.get(&url).send().await.map_err(|e| anyhow!("Marktplatz nicht erreichbar: {e}"))?;
        if res.status().as_u16() == 404 {
            return Ok(json!({ "plugins": [], "note": "Der Marktplatz ist noch leer." }));
        }
        let v: Value = res.error_for_status()?.json().await.map_err(|_| anyhow!("Die Marktplatz-Liste ist beschädigt."))?;
        let plugins: Vec<Value> = v["plugins"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let (owner, repo) = parse_repo(p["repo"].as_str()?).ok()?;
                Some(json!({ "repo": format!("{owner}/{repo}"), "name": text(&p["name"], 60), "description": text(&p["description"], 300), "author": text(&p["author"], 60) }))
            })
            .collect();
        Ok(json!({ "plugins": plugins }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netz_erlaubnis() {
        let allowed = vec!["api.example.com".to_string(), "*.hue.com".to_string(), "lan".to_string()];
        assert!(http_allowed(&allowed, "https://api.example.com/x"));
        assert!(!http_allowed(&allowed, "https://api.example.com.evil.net/x"));
        assert!(http_allowed(&allowed, "https://bridge.hue.com/"));
        assert!(http_allowed(&allowed, "http://192.168.178.40/api"));
        assert!(!http_allowed(&allowed, "http://8.8.8.8/"));
        assert!(!http_allowed(&allowed, "file:///etc/passwd"));
        assert!(!http_allowed(&[], "https://api.example.com/"));
    }

    #[test]
    fn beschreibung_pruefen() {
        let m = check_manifest(&json!({ "id": "hue", "name": "Hue", "permissions": { "http": ["lan"], "values": true }, "actions": [{ "id": "toggle", "label": "Licht an/aus" }] })).unwrap();
        assert_eq!(m["main"], "plugin.js");
        assert_eq!(describe_permissions(&m), vec!["Geräte im Heimnetz ansprechen", "TouchDeck-Werte lesen und ändern"]);
        assert!(check_manifest(&json!({ "id": "Böse/../", "name": "x" })).is_err());
        assert!(check_manifest(&json!({ "id": "x", "name": "x", "main": "../a.js" })).is_err());
        assert!(check_manifest(&json!({ "id": "x", "name": "x", "permissions": { "http": ["*"] } })).is_err());
    }

    #[test]
    fn fehlertexte() {
        assert_eq!(clean_error("Error: Keine Erlaubnis\n    at get (eval_script:17:40)"), "Keine Erlaubnis");
        assert_eq!(clean_error("interrupted"), "interrupted");
    }

    #[test]
    fn github_zip_auspacken() {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in [
            ("td-x-1.0/plugin.json", r#"{ "id": "x", "name": "X" }"#),
            ("td-x-1.0/plugin.js", "td.log('hi')"),
            ("td-x-1.0/README.md", "# X"),
            ("td-x-1.0/tools/build.sh", "rm -rf /"),
            ("td-x-1.0/run.exe", "MZ"),
            ("td-x-1.0/.github/ci.yml", "x"),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let files = unpack(&bytes).unwrap();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["plugin.json", "plugin.js", "README.md"]);
        let (m, digest) = manifest_of(&files).unwrap();
        assert_eq!(m["id"], "x");
        assert_eq!(digest.len(), 64);
    }

    #[test]
    fn repo_namen() {
        assert_eq!(parse_repo("https://github.com/Marcel1853/td-hue").unwrap(), ("Marcel1853".into(), "td-hue".into()));
        assert_eq!(parse_repo("Marcel1853/td-hue.git").unwrap(), ("Marcel1853".into(), "td-hue".into()));
        assert!(parse_repo("nur-name").is_err());
        assert!(parse_repo("a/b c").is_err());
    }
}
