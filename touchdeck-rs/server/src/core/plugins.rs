// Plugins: Erweiterungen in JavaScript, die abgeschottet in einer eigenen kleinen JS-Engine (QuickJS) laufen.
//
// Sicherheit:
// - Ein Plugin kann nur, was TouchDeck ihm anbietet (td.…): keine Dateien, keine Programme, kein freies Netz.
// - Was es darf, steht in plugin.json unter "permissions" und wird vor dem Installieren angezeigt:
//   "http": erlaubte Adressen (z. B. "api.example.com", "*.example.com", "lan" für das Heimnetz),
//   "values": TouchDeck-Werte lesen und setzen, "notify": Benachrichtigungen zeigen,
//   "events": auf TouchDeck-Ereignisse reagieren (td.on), "deck": Bedienfeld steuern (Seiten, Aussehen,
//   Timer), "status": verbundene Geräte und Integrationen lesen.
// - Eigene Live-Anzeigen stehen unter "lives" und werden mit td.live.set(…) gefüllt.
// - Installiert wird nur aus öffentlichen GitHub-Repos (jeder kann den Quelltext prüfen); zum Entwickeln
//   gibt es zusätzlich einen lokalen Testmodus.
// - Speicher (32 MB) und Rechenzeit (5 s je Aufruf) sind begrenzt.
//
// Daneben gibt es erweiterte Plugins (native_plugins.rs): eigene Programme mit vollen Rechten. Die laufen
// nur, wenn der Nutzer den erweiterten Modus bewusst mit doppelter Bestätigung eingeschaltet hat.
use crate::t;
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

use crate::core::native_plugins::{NativeCmd, spawn_native};
use crate::core::store::js_string;
use crate::server::App;

const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
const CALL_LIMIT_MS: i64 = 5000;
const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_TOTAL: u64 = 10 * 1024 * 1024;
// Erweiterte Plugins bringen eigene Programme und Bibliotheken mit.
const NATIVE_MAX_FILE: u64 = 30 * 1024 * 1024;
const NATIVE_MAX_TOTAL: u64 = 80 * 1024 * 1024;
const MAX_FILES: usize = 3000;
/** Diesen Satz muss der Nutzer zum Einschalten des erweiterten Modus eintippen (je Sprache). */
pub const ADVANCED_CONFIRM: &str = "AUF EIGENE GEFAHR";
pub const ADVANCED_CONFIRM_EN: &str = "AT MY OWN RISK";

pub fn advanced_confirm(lang: &str) -> &'static str {
    if lang == "de" { ADVANCED_CONFIRM } else { ADVANCED_CONFIRM_EN }
}
const ALLOWED_EXT: [&str; 11] = ["js", "json", "md", "txt", "png", "svg", "jpg", "webp", "gif", "tdprofile", "tdicons"];
pub const MARKET_URL: &str = "https://raw.githubusercontent.com/Marcel1853/touchdeck-plugins/main/plugins.json";

/** Vorgefertigte Hilfen, aus denen das Plugin `td` nutzt. Die __-Funktionen stellt TouchDeck bereit. */
const PRELUDE: &str = r#"
(() => {
  const actions = {};
  const timers = [];
  const listeners = {};
  const call = (op, args = {}) => JSON.parse(__td(op, JSON.stringify(args)));
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
    // Auf TouchDeck-Ereignisse reagieren, z. B. td.on('page_view', (data) => …); '*' = alle.
    on(kind, fn) { (listeners[String(kind)] ??= []).push(fn); },
    // Eigene Live-Anzeige (in plugin.json unter "lives"): Text, Zahl 0–100 (Balken) und „an“.
    live: Object.freeze({
      set: (id, v) => call('live.set', { id: String(id), value: v !== null && typeof v === 'object' ? v : { text: String(v ?? '') } }),
    }),
    deck: Object.freeze({
      pages: () => call('deck.pages'),
      showPage: (page) => call('deck.showPage', { page: String(page) }),
      setLook: (button, look = {}) => call('deck.setLook', { button: String(button), look }),
      resetLook: (button) => call('deck.resetLook', { button: String(button) }),
      timer: (name, op = 'toggle', seconds = 0) => call('deck.timer', { name: String(name), op: String(op), seconds: Number(seconds) || 0 }),
    }),
    // Karte auf dem Bildschirmschoner: { title, text, items: [{ label, text }], color }; clear() entfernt sie.
    saver: Object.freeze({
      set: (card) => call('saver.set', { card: card !== null && typeof card === 'object' ? card : { text: String(card ?? '') } }),
      clear: () => call('saver.clear', {}),
    }),
    status: () => call('status'),
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
    // Objekt = Daten (z. B. { players: 7, memory: 4210 }); TouchDeck zeigt und verteilt sie.
    if (r && typeof r === 'object') return '\u0001' + JSON.stringify({ data: r });
    return r === undefined || r === null ? null : String(r);
  };
  globalThis.__onEvent = (kind, data) => {
    for (const fn of [...(listeners[kind] ?? []), ...(listeners['*'] ?? [])]) fn(JSON.parse(data), kind);
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

/** Sprachkürzel wie „de“, „en“ oder „pt-br“. */
fn is_language_code(k: &str) -> bool {
    let b = k.as_bytes();
    match b.len() {
        2 => b.iter().all(u8::is_ascii_lowercase),
        5 => b[2] == b'-' && b[..2].iter().chain(&b[3..]).all(u8::is_ascii_lowercase),
        _ => false,
    }
}

/** Ist das eine Übersetzung wie { "de": "Hallo", "en": "Hello" }? */
fn is_translation(v: &Value) -> bool {
    v.as_object().is_some_and(|o| {
        !o.is_empty() && o.iter().all(|(k, v)| is_language_code(k) && v.is_string())
    })
}

/** Text oder Übersetzung übernehmen (gekürzt). */
fn ltext(v: &Value, max: usize) -> Value {
    if is_translation(v) {
        let map: Map<String, Value> = v.as_object().into_iter().flatten().map(|(k, t)| (k.clone(), json!(text(t, max)))).collect();
        return Value::Object(map);
    }
    json!(text(v, max))
}

/** Übersetzungen in einer Beschreibung auflösen: { "de": …, "en": … } → Text in der gewünschten Sprache
 * (sonst Englisch, sonst Deutsch, sonst die erste). */
pub fn localize(v: &Value, lang: &str) -> Value {
    match v {
        _ if is_translation(v) => {
            let o = v.as_object().expect("Übersetzung ist ein Objekt");
            [lang, "en", "de"].iter().find_map(|l| o.get(*l)).or_else(|| o.values().next()).cloned().unwrap_or_default()
        }
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), localize(x, lang))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(|x| localize(x, lang)).collect()),
        other => other.clone(),
    }
}

/** Prüft plugin.json und bringt sie in eine feste Form. */
pub fn check_manifest(raw: &Value) -> Result<Value> {
    let id = text(&raw["id"], 32);
    if !valid_id(&id) {
        bail!(t!("plugins.pluginJsonIdMust"));
    }
    let name = ltext(&raw["name"], 60);
    if localize(&name, "en").as_str().unwrap_or("").is_empty() {
        bail!(t!("plugins.pluginJsonNameIs"));
    }
    let main = text(&raw["main"], 80);
    let main = if main.is_empty() { "plugin.js".to_string() } else { main };
    if !main.ends_with(".js") || main.contains('/') || main.contains('\\') {
        bail!(t!("plugins.pluginJsonMainMust"));
    }
    let p = &raw["permissions"];
    let http: Vec<String> = p["http"].as_array().into_iter().flatten().filter_map(|h| h.as_str()).map(|h| h.trim().to_lowercase()).filter(|h| !h.is_empty()).take(20).collect();
    for h in &http {
        let host = h.strip_prefix("*.").unwrap_or(h);
        if h != "lan" && (host.is_empty() || !host.contains('.') || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')) {
            bail!(t!("plugins.pluginJsonInvalidAddress", h));
        }
    }
    let native: Vec<String> = raw["native"]["command"].as_array().into_iter().flatten().filter_map(|c| c.as_str()).map(String::from).take(20).collect();
    if raw["native"].is_object() && native.first().is_none_or(|c| c.trim().is_empty()) {
        bail!(t!("plugins.pluginJsonNativeCommand"));
    }
    let items = |key: &str| -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for a in raw[key].as_array().into_iter().flatten().take(50) {
            let aid = text(&a["id"], 24);
            if !short_id(&aid) {
                bail!(t!("plugins.pluginJsonEveryEntry", key));
            }
            let fields: Vec<Value> = a["fields"].as_array().into_iter().flatten().filter(|f| f["key"].is_string()).take(20).cloned().collect();
            let mut item = json!({ "id": aid, "label": ltext(&a["label"], 80), "icon": text(&a["icon"], 8), "fields": fields });
            // Aktionen mit Rückgabe als Daten: welche Schlüssel, wie heißen sie, in welcher Einheit.
            if key == "actions" {
                item["returns"] = Value::Array(
                    a["returns"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| {
                            let k = text(&r["key"], 40);
                            (!k.is_empty()).then(|| json!({ "key": k, "label": ltext(&r["label"], 40), "unit": one_unit(&r["unit"]) }))
                        })
                        .take(20)
                        .collect(),
                );
            }
            out.push(item);
        }
        Ok(out)
    };
    Ok(json!({
        "id": id,
        "name": name,
        "version": text(&raw["version"], 20),
        "author": text(&raw["author"], 60),
        "description": ltext(&raw["description"], 400),
        "license": text(&raw["license"], 100),
        "main": main,
        "permissions": { "http": http, "values": p["values"] == true, "notify": p["notify"] == true,
                         "events": p["events"] == true, "deck": p["deck"] == true, "status": p["status"] == true },
        "native": if native.is_empty() { Value::Null } else { json!({ "command": native }) },
        "settings": raw["settings"].as_array().into_iter().flatten().filter(|f| f["key"].is_string()).take(20).cloned().collect::<Vec<_>>(),
        "actions": items("actions")?,
        "triggers": items("triggers")?,
        "lives": items("lives")?,
        // Designs für Buttons und Regler, Bilder relativ zum Plugin-Ordner.
        "looks": sanitize_looks(&raw["looks"]),
        // Icon-Pakete, die das Plugin mitbringt und voraussetzt (.tdicons im Plugin-Ordner).
        "iconPacks": raw["iconPacks"].as_array().into_iter().flatten().filter_map(Value::as_str)
            .filter(|p| p.to_lowercase().ends_with(".tdicons") && safe_rel_path(p)).take(10).collect::<Vec<_>>(),
    }))
}

/** Einheiten für Rückgabe-Daten (Anzeige im Button). */
const UNITS: [&str; 9] = ["number", "bytes", "kb", "mb", "gb", "seconds", "ms", "percent", "text"];

fn one_unit(v: &Value) -> String {
    v.as_str().filter(|u| UNITS.contains(u)).unwrap_or("").to_string()
}

/** Wert lesbar machen: 4210 MB → „4,1 GB“, 50432 s → „14 h“, 0.42 % bleibt. */
pub fn format_unit(v: &Value, unit: &str) -> String {
    let Some(n) = v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().replace(',', ".").parse().ok())) else { return js_string(v) };
    let dec = |x: f64, d: usize| {
        let s = format!("{x:.d$}");
        if crate::core::i18n::is_english() { s } else { s.replace('.', ",") }
    };
    let size = |bytes: f64| {
        let units = ["B", "KB", "MB", "GB", "TB"];
        let mut x = bytes;
        let mut i = 0;
        while x >= 1024.0 && i < units.len() - 1 {
            x /= 1024.0;
            i += 1;
        }
        format!("{} {}", dec(x, if x < 10.0 && i > 0 { 1 } else { 0 }), units[i])
    };
    let time = |secs: f64| {
        let s = secs.round() as i64;
        match s {
            s if s < 60 => format!("{s} s"),
            s if s < 3600 => format!("{} min", s / 60),
            s if s < 86_400 => format!("{} h", s / 3600),
            s => format!("{} d {} h", s / 86_400, (s % 86_400) / 3600),
        }
    };
    match unit {
        "bytes" => size(n),
        "kb" => size(n * 1024.0),
        "mb" => size(n * 1024.0 * 1024.0),
        "gb" => size(n * 1024.0 * 1024.0 * 1024.0),
        "seconds" => time(n),
        "ms" => time(n / 1000.0),
        "percent" => format!("{} %", dec(n, if n.fract() == 0.0 { 0 } else { 1 })),
        _ if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", n as i64),
        _ => dec(n, 2),
    }
}

/** Relativer Pfad im Plugin: keine „..“, keine versteckten Teile, höchstens 4 Ebenen. */
pub fn safe_rel_path(p: &str) -> bool {
    !p.is_empty() && p.split('/').count() <= 4 && p.split('/').all(|c| !c.is_empty() && !c.starts_with('.') && !c.contains('\\'))
}

/**
 * Designs (aus Plugins oder Icon-Paketen): [{ id, name, style: {…}, slider: {…} }].
 * Nur bekannte Felder; endgültig geprüft wird beim Speichern des Buttons.
 */
pub fn sanitize_looks(v: &Value) -> Value {
    const STYLE: [&str; 17] = ["color", "colorOn", "textColor", "bgOpacity", "fontSize", "fontFamily", "bold", "italic", "textOutline", "titlePos", "iconSize",
        "iconFit", "overlay", "texFit", "texVeil", "bgImage", "icon"];
    const SLIDER: [&str; 3] = ["shape", "color", "thickness"];
    let pick = |o: &Value, keys: &[&str]| -> Value {
        Value::Object(o.as_object().map(|m| m.iter().filter(|(k, _)| keys.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default())
    };
    Value::Array(
        v.as_array()
            .into_iter()
            .flatten()
            .take(50)
            .filter_map(|l| {
                let id = text(&l["id"], 40);
                (!id.is_empty()).then(|| json!({ "id": id, "name": ltext(&l["name"], 60), "style": pick(&l["style"], &STYLE), "slider": pick(&l["slider"], &SLIDER) }))
            })
            .collect(),
    )
}

/** Lesbare Liste der Berechtigungen (für die Anzeige vor dem Installieren). */
pub fn describe_permissions(m: &Value) -> Vec<String> {
    if is_native(m) {
        return vec![t!("plugins.fullRightsOwnProgram").into()];
    }
    let p = &m["permissions"];
    let mut out: Vec<String> = p["http"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h.as_str())
        .map(|h| if h == "lan" { t!("plugins.talkToDevicesOn").to_string() } else { format!("Internet: {h}") })
        .collect();
    if p["values"] == true {
        out.push(t!("plugins.readAndChangeTouchdeck").into());
    }
    if p["notify"] == true {
        out.push(t!("plugins.showNotifications").into());
    }
    for (key, text) in [("events", t!("plugins.permEvents")), ("deck", t!("plugins.permDeck")), ("status", t!("plugins.permStatus"))] {
        if p[key] == true {
            out.push(text.into());
        }
    }
    out
}

pub fn is_native(m: &Value) -> bool {
    m["native"].is_object()
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
    let first = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or(t!("plugins.errorInPlugin"));
    first.strip_prefix("Error: ").unwrap_or(first).to_string()
}

enum Msg {
    Action { id: String, params: Value, info: Value, depth: u32, reply: oneshot::Sender<Result<Option<String>, String>> },
    /** Ein TouchDeck-Ereignis für td.on(…). */
    Event { kind: String, data: Value },
    Stop,
}

enum Channel {
    Js(mpsc::Sender<Msg>),
    Native(tokio::sync::mpsc::UnboundedSender<NativeCmd>),
}

struct Running {
    tx: Channel,
    error: Arc<Mutex<Option<String>>>,
    /// Bekommt TouchDeck-Ereignisse (Berechtigung „events“; erweiterte Plugins immer).
    events: bool,
}

impl Running {
    fn stop(&self) {
        match &self.tx {
            Channel::Js(tx) => {
                let _ = tx.send(Msg::Stop);
            }
            Channel::Native(tx) => {
                let _ = tx.send(NativeCmd::Stop);
            }
        }
    }
}

/** Was ein Plugin von TouchDeck nutzen kann (im eigenen Thread des Plugins). */
struct Host {
    id: String,
    app: Weak<App>,
    http: Vec<String>,
    values: bool,
    notify: bool,
    /// Weitere Berechtigungen und erlaubte Live-Anzeigen (für __td).
    perms: Value,
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
                return Err(js_err(&ctx, t!("plugins.noPermissionForPlugin", url)));
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
                return Err(js_err(&ctx, t!("plugins.noPermissionForValues")));
            }
            let app = h.app.upgrade().ok_or_else(|| js_err(&ctx, t!("plugins.touchdeckIsShuttingDown")))?;
            Ok(app.store.lock().unwrap().config.variables.get(&name).map(js_string))
        })?,
    )?;

    let h = host.clone();
    g.set(
        "__setValue",
        Function::new(ctx.clone(), move |ctx: rquickjs::Ctx<'_>, name: String, value: String| -> rquickjs::Result<()> {
            if !h.values {
                return Err(js_err(&ctx, t!("plugins.noPermissionForValues")));
            }
            let app = h.app.upgrade().ok_or_else(|| js_err(&ctx, t!("plugins.touchdeckIsShuttingDown")))?;
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

    let h = host.clone();
    g.set(
        "__td",
        Function::new(ctx.clone(), move |ctx: rquickjs::Ctx<'_>, op: String, args: String| -> rquickjs::Result<String> {
            let args: Value = serde_json::from_str(&args).unwrap_or_else(|_| json!({}));
            // Berechtigung je Bereich prüfen; Live-Anzeigen nur, wenn sie in plugin.json stehen.
            let allowed = match op.split('.').next().unwrap_or("") {
                "live" => h.perms["lives"].as_array().is_some_and(|l| l.iter().any(|x| x.as_str() == args["id"].as_str())),
                // Bildschirmschoner gehört zum Bedienfeld: Berechtigung „deck“.
                "saver" => h.perms["deck"] == true,
                area => h.perms[area] == true,
            };
            if !allowed {
                let msg = if op.starts_with("live.") { t!("plugins.unknownLive", js_string(&args["id"])) } else { t!("plugins.noPermissionFor", op.split('.').next().unwrap_or("")) };
                return Err(js_err(&ctx, msg));
            }
            let app = h.app.upgrade().ok_or_else(|| js_err(&ctx, t!("plugins.touchdeckIsShuttingDown")))?;
            host_call(&app, &h.id, &op, &args).map(|v| v.to_string()).map_err(|e| js_err(&ctx, e.to_string()))
        })?,
    )?;

    let h = host;
    g.set(
        "__notify",
        Function::new(ctx.clone(), move |ctx: rquickjs::Ctx<'_>, title: String, body: String| -> rquickjs::Result<()> {
            if !h.notify {
                return Err(js_err(&ctx, t!("plugins.noPermissionForNotifications")));
            }
            let _ = crate::system::linux::command("notify-send").args(["-a", "TouchDeck", &title, &body]).spawn();
            Ok(())
        })?,
    )?;
    Ok(())
}

/**
 * Befehle der Plugin-Schnittstelle (für normale Plugins über td.…, für erweiterte über „call“).
 * Berechtigungen prüft der Aufrufer; hier wird nur ausgeführt.
 */
pub fn host_call(app: &Arc<App>, plugin: &str, op: &str, args: &Value) -> Result<Value> {
    let hex = |v: &Value| v.as_str().filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())).map(String::from);
    match op {
        "live.set" => {
            let id = text(&args["id"], 24);
            let v = &args["value"];
            let raw = v["value"].as_f64().filter(|n| n.is_finite());
            let value = raw.map(|n| n.clamp(0.0, 100.0));
            // „number“: echte Zahl für Graphen (z. B. 35.2 für MW), sonst der Wert selbst.
            let number = v["number"].as_f64().filter(|n| n.is_finite()).or(raw);
            let mut live = json!({ "text": text(&v["text"], 200), "value": value, "number": number });
            // Mehrere Datenreihen für Graphen: { "Dampf": 30.5, "Solar": 12 } (höchstens 12).
            if let Some(map) = v["series"].as_object() {
                let series: serde_json::Map<String, Value> =
                    map.iter().filter_map(|(k, x)| x.as_f64().filter(|n| n.is_finite()).map(|n| (k.chars().take(40).collect(), json!(n)))).take(12).collect();
                live["series"] = Value::Object(series);
            }
            if let Some(on) = v["on"].as_bool() {
                live["on"] = json!(on);
            }
            app.set_plugin_live(&format!("plugin:{plugin}:{id}"), live);
            Ok(json!(true))
        }
        "deck.pages" => {
            let store = app.store.lock().unwrap();
            Ok(json!(store.config.pages.iter().map(|p| json!({
                "id": p.id, "name": p.name,
                "buttons": p.buttons.iter().map(|b| json!({ "id": b.id, "title": b.title })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>()))
        }
        "deck.showPage" => {
            let want = js_string(&args["page"]);
            let id = {
                let store = app.store.lock().unwrap();
                store.config.pages.iter().find(|p| p.id == want || p.name.eq_ignore_ascii_case(&want)).map(|p| p.id.clone())
            };
            let id = id.ok_or_else(|| anyhow!(t!("actions.targetPageNoLonger")))?;
            app.show_page_on_devices(&id);
            Ok(json!(id))
        }
        "deck.setLook" | "deck.resetLook" => {
            let button = js_string(&args["button"]);
            let mut store = app.store.lock().unwrap();
            if !store.config.pages.iter().any(|p| p.buttons.iter().any(|b| b.id == button)) {
                bail!(t!("actions.buttonNoLongerExists"));
            }
            if op == "deck.resetLook" {
                store.set_override(false, &button, None);
                return Ok(json!(true));
            }
            let l = &args["look"];
            let mut patch = Map::new();
            if l["title"].is_string() {
                patch.insert("title".into(), json!(text(&l["title"], 200)));
            }
            for key in ["color", "textColor"] {
                if let Some(c) = hex(&l[key]) {
                    patch.insert(key.into(), json!(c));
                }
            }
            if l["icon"].is_string() {
                patch.insert("icon".into(), json!({ "type": "emoji", "value": text(&l["icon"], 16) }));
            }
            store.set_override(false, &button, Some(&patch));
            Ok(json!(true))
        }
        "deck.timer" => {
            let name = text(&args["name"], 60);
            let ms = (args["seconds"].as_f64().unwrap_or(0.0).clamp(0.0, 86_400.0) * 1000.0) as u64;
            let running = app.timers.control(if name.is_empty() { "Timer" } else { &name }, args["op"].as_str().unwrap_or("toggle"), ms);
            app.timers_changed();
            Ok(json!({ "running": running }))
        }
        "saver.set" => {
            let c = &args["card"];
            let hex = |v: &Value| v.as_str().filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())).unwrap_or("").to_string();
            let items: Vec<Value> = c["items"]
                .as_array()
                .into_iter()
                .flatten()
                .take(6)
                .map(|i| json!({ "label": text(&i["label"], 40), "text": text(&i["text"], 60) }))
                .collect();
            let card = json!({ "plugin": plugin, "title": text(&c["title"], 60), "text": text(&c["text"], 200), "items": items, "color": hex(&c["color"]) });
            app.set_plugin_saver(plugin, Some(card));
            Ok(json!(true))
        }
        "saver.clear" => {
            app.set_plugin_saver(plugin, None);
            Ok(json!(true))
        }
        "status" => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "devices": app.device_list(),
            "obs": app.obs.connected(),
            "twitch": app.twitch.status()["state"],
            "discord": app.discord.status()["state"],
        })),
        _ => bail!(t!("plugins.unknownCall", op)),
    }
}

/** Startet ein Plugin in einem eigenen Thread mit eigener JS-Engine. */
fn spawn_plugin(app: &Arc<App>, dir: PathBuf, manifest: Value, settings: Value) -> Result<Running> {
    let source = std::fs::read_to_string(dir.join(manifest["main"].as_str().unwrap_or("plugin.js")))
        .map_err(|_| anyhow!(t!("plugins.fileIsMissingFrom", manifest["main"])))?;
    let (tx, rx) = mpsc::channel::<Msg>();
    let error = Arc::new(Mutex::new(None));
    let err = error.clone();
    let rt = tokio::runtime::Handle::current();
    let weak = Arc::downgrade(app);
    let id = manifest["id"].as_str().unwrap_or("").to_string();
    let events = manifest["permissions"]["events"] == true;
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
            perms: json!({ "events": p["events"], "deck": p["deck"], "status": p["status"],
                           "lives": manifest["lives"].as_array().into_iter().flatten().filter_map(|l| l["id"].as_str()).collect::<Vec<_>>() }),
            rt: rt.clone(),
            depth: std::cell::Cell::new(0),
        });
        let Ok(runtime) = Runtime::new() else { return report(t!("plugins.jsEngineCouldNot").into()) };
        runtime.set_memory_limit(MEMORY_LIMIT);
        runtime.set_max_stack_size(512 * 1024);
        // Endlosschleifen abbrechen: jeder Aufruf hat höchstens 5 Sekunden.
        let deadline = Arc::new(AtomicI64::new(i64::MAX));
        let d = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || chrono::Utc::now().timestamp_millis() > d.load(Ordering::Relaxed))));
        let Ok(context) = Context::full(&runtime) else { return report(t!("plugins.jsEngineCouldNot").into()) };
        let start = |deadline: &AtomicI64| deadline.store(chrono::Utc::now().timestamp_millis() + CALL_LIMIT_MS, Ordering::Relaxed);

        // Laden: Hilfen, dann das Plugin selbst.
        start(&deadline);
        let loaded: Result<Vec<u64>, String> = context.with(|ctx| {
            install_host(&ctx, host.clone(), &manifest, &settings).map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(PRELUDE).catch(&ctx).map_err(|e| e.to_string())?;
            ctx.eval::<(), _>(source).catch(&ctx).map_err(|e| t!("plugins.errorInPlugin2", clean_error(e)))?;
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
            app.log.info(&format!("Plugin {id}"), t!("plugins.started"));
        }
        let mut next: Vec<Instant> = intervals.iter().map(|ms| Instant::now() + Duration::from_millis(*ms)).collect();

        loop {
            let wait = next.iter().min().map_or(Duration::from_secs(3600), |t| t.saturating_duration_since(Instant::now()));
            match rx.recv_timeout(wait) {
                Ok(Msg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Msg::Event { kind, data }) => {
                    host.depth.set(0);
                    start(&deadline);
                    let r = context.with(|ctx| {
                        let f: Function = ctx.globals().get("__onEvent").map_err(|e| e.to_string())?;
                        f.call::<_, ()>((kind.clone(), data.to_string())).catch(&ctx).map_err(clean_error)
                    });
                    deadline.store(i64::MAX, Ordering::Relaxed);
                    if let (Err(e), Some(app)) = (r, weak.upgrade()) {
                        app.log.warn(&format!("Plugin {id}"), t!("plugins.eventHandlerError", kind, e));
                    }
                }
                Ok(Msg::Action { id: action, params, info, depth, reply }) => {
                    host.depth.set(depth);
                    start(&deadline);
                    let r = context.with(|ctx| {
                        let f: Function = ctx.globals().get("__runAction").map_err(|e| e.to_string())?;
                        f.call::<_, Option<String>>((action, params.to_string(), info.to_string())).catch(&ctx).map_err(clean_error)
                    });
                    deadline.store(i64::MAX, Ordering::Relaxed);
                    let r = r.map_err(|e| if e.contains("interrupted") { t!("plugins.pluginTookTooLong").into() } else { e });
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
                            app.log.warn(&format!("Plugin {id}"), t!("plugins.timerError", e));
                        }
                    }
                }
            }
        }
    })?;
    Ok(Running { tx: Channel::Js(tx), error, events })
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
    /// Gefundene Updates: Plugin-ID → { current, latest, ref }.
    updates: Mutex<Map<String, Value>>,
    /// Schon gemeldete Updates („id@version“), damit jedes nur einmal benachrichtigt.
    notified: Mutex<std::collections::HashSet<String>>,
}

/** Versionen wie „1.2.10“ vergleichen (Zahlen je Teil, Fehlendes = 0). */
pub fn version_newer(latest: &str, current: &str) -> bool {
    let parts = |v: &str| v.trim_start_matches('v').split(['.', '-']).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/** Prüfsumme einer Datei (für „hat sich die fertige Seite geändert?“). */
fn file_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/** Ein Plugin auf GitHub: Repo und optional ein Unterordner darin (mehrere Plugins in einem Repo). */
#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    pub owner: String,
    pub repo: String,
    pub path: String,
}

impl std::fmt::Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() { write!(f, "{}/{}", self.owner, self.repo) } else { write!(f, "{}/{}/{}", self.owner, self.repo, self.path) }
    }
}

/**
 * Repo aus „owner/repo“, „owner/repo/unter/ordner“ oder einer GitHub-Adresse
 * (auch „github.com/owner/repo/tree/main/unter/ordner“).
 */
pub fn parse_repo(input: &str) -> Result<Repo> {
    let s = input.trim().trim_end_matches('/').trim_end_matches(".git");
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let s = s.strip_prefix("github.com/").unwrap_or(s);
    let mut parts: Vec<&str> = s.split('/').collect();
    if parts.len() < 2 {
        bail!(t!("plugins.pleaseEnterGithubRepo"));
    }
    // „…/tree/<zweig>/ordner“ aus einer kopierten GitHub-Adresse: Zweig überspringen.
    if parts.len() >= 4 && parts[2] == "tree" {
        parts.drain(2..4);
    }
    let ok = |p: &str| !p.is_empty() && p.len() <= 100 && !p.starts_with('.') && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if !parts.iter().all(|p| ok(p)) || !ok(parts[0].trim_start_matches('.')) || parts.len() > 8 {
        bail!(t!("plugins.invalidRepoName"));
    }
    Ok(Repo { owner: parts[0].into(), repo: parts[1].into(), path: parts[2..].join("/") })
}

/** Eine Datei aus dem Plugin: Pfad im Plugin, Inhalt, ausführbar? */
pub type PluginFile = (String, Vec<u8>, bool);

/** Alle Dateien aus einem GitHub-ZIP: Hauptordner abschneiden, keine versteckten Ordner, Größen begrenzt. */
fn unpack(bytes: &[u8]) -> Result<Vec<PluginFile>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| anyhow!(t!("plugins.downloadIsNotValid")))?;
    // GitHub-ZIPs haben einen Hauptordner; selbst gepackte ZIPs oft nicht (plugin.json liegt dann ganz oben).
    let flat = zip.file_names().any(|n| n.is_ok_and(|n| n == "plugin.json"));
    let mut files = Vec::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let name = f.name()?.to_string();
        let rel = if flat {
            name.as_str()
        } else {
            let mut parts = name.splitn(2, '/');
            let (_, Some(rel)) = (parts.next(), parts.next()) else { continue };
            rel
        };
        let safe = rel.split('/').all(|c| !c.is_empty() && !c.starts_with('.') && c != ".." && !c.contains('\\'));
        if !safe || f.size() > NATIVE_MAX_FILE {
            continue;
        }
        total += f.size();
        if total > NATIVE_MAX_TOTAL || files.len() >= MAX_FILES {
            bail!(t!("plugins.pluginIsTooLarge2"));
        }
        let exec = f.unix_mode().is_some_and(|m| m & 0o111 != 0);
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        files.push((rel.to_string(), data, exec));
    }
    Ok(files)
}

/** Beschreibung prüfen und die Dateien auswählen, die installiert werden. Liefert auch eine Prüfsumme über alle davon. */
fn manifest_of(files: Vec<PluginFile>) -> Result<(Value, String, Vec<PluginFile>)> {
    let raw = files.iter().find(|(n, _, _)| n == "plugin.json").ok_or_else(|| anyhow!(t!("plugins.repoHasNoPlugin")))?;
    let value: Value = serde_json::from_slice(&raw.1).map_err(|e| anyhow!(t!("plugins.pluginJsonIsNot", e)))?;
    let m = check_manifest(&value)?;
    let mut chosen: Vec<PluginFile> = if is_native(&m) {
        files
    } else {
        // Sichere Plugins: nur Dateien im Hauptordner mit erlaubter Endung.
        let mut total = 0;
        let mut out = Vec::new();
        for f in files {
            let ext = Path::new(&f.0).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            // Unterordner nur für fertige Seiten (.tdprofile), alles andere im Hauptordner.
            let nested_ok = ["tdprofile", "tdicons", "png", "svg", "jpg", "webp", "gif"].contains(&ext.as_str()) && f.0.matches('/').count() <= 3;
            if (f.0.contains('/') && !nested_ok) || !ALLOWED_EXT.contains(&ext.as_str()) || f.1.len() as u64 > MAX_FILE {
                continue;
            }
            total += f.1.len() as u64;
            if total > MAX_TOTAL {
                bail!(t!("plugins.pluginIsTooLarge"));
            }
            out.push((f.0, f.1, false));
        }
        if !out.iter().any(|(n, _, _)| *n == m["main"]) {
            bail!(t!("plugins.fileIsMissingFrom2", m["main"]));
        }
        out
    };
    chosen.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hash = Sha256::new();
    for (name, data, exec) in &chosen {
        hash.update(name.as_bytes());
        hash.update([0, u8::from(*exec)]);
        hash.update((data.len() as u64).to_le_bytes());
        hash.update(data);
    }
    Ok((m, hex::encode(hash.finalize()), chosen))
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
            updates: Mutex::new(Map::new()),
            notified: Mutex::new(std::collections::HashSet::new()),
        }
    }

    /** Ist der erweiterte Modus (Plugins mit vollen Rechten) eingeschaltet? */
    pub fn advanced(&self) -> bool {
        std::fs::read_to_string(self.dir.join("config.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).is_some_and(|v| v["advanced"] == true)
    }

    /** Erweiterten Modus ein- oder ausschalten; einschalten nur mit dem Bestätigungssatz. */
    pub fn set_advanced(&self, app: &Arc<App>, on: bool, confirm: &str) -> Result<()> {
        if on && ![ADVANCED_CONFIRM, ADVANCED_CONFIRM_EN].contains(&confirm.trim()) {
            bail!(t!("plugins.toTurnItOn", ADVANCED_CONFIRM));
        }
        std::fs::write(self.dir.join("config.json"), serde_json::to_string_pretty(&json!({ "advanced": on }))?)?;
        let natives: Vec<String> = self.manifests.lock().unwrap().iter().filter(|(_, m)| is_native(m)).map(|(id, _)| id.clone()).collect();
        let installed = self.read_installed();
        for id in natives {
            if on && installed.get(&id).is_some_and(|e| e["enabled"] != false) {
                self.start(app, &id);
            } else if !on {
                self.stop(app, &id);
            }
        }
        if on {
            app.log.warn("Plugins", t!("plugins.advancedModeTurnedOn"));
        } else {
            app.log.info("Plugins", "Erweiterter Modus ausgeschaltet.");
        }
        Ok(())
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
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("plugin.json")).map_err(|_| anyhow!(t!("plugins.pluginJsonMissing")))?)?;
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
        self.stop(app, id);
        let entry = self.read_installed().get(id).cloned().unwrap_or_default();
        let Some(manifest) = self.manifests.lock().unwrap().get(id).cloned() else { return };
        if is_native(&manifest) {
            if !self.advanced() {
                return;
            }
            let error = Arc::new(Mutex::new(None));
            let tx = spawn_native(app, self.plugin_dir(id, &entry), manifest, entry["settings"].clone(), error.clone());
            self.running.lock().unwrap().insert(id.into(), Running { tx: Channel::Native(tx), error, events: true });
            return;
        }
        match spawn_plugin(app, self.plugin_dir(id, &entry), manifest, entry["settings"].clone()) {
            Ok(r) => {
                self.running.lock().unwrap().insert(id.into(), r);
            }
            Err(e) => app.log.error(&format!("Plugin {id}"), e.to_string()),
        }
    }

    fn stop(&self, app: &Arc<App>, id: &str) {
        if let Some(r) = self.running.lock().unwrap().remove(id) {
            r.stop();
        }
        // Karte auf dem Bildschirmschoner gehört zum laufenden Plugin.
        app.set_plugin_saver(id, None);
    }

    pub fn stop_all(&self) {
        for (_, r) in self.running.lock().unwrap().drain() {
            r.stop();
        }
    }

    /** Für den Editor: installierte Plugins mit Beschreibung, Berechtigungen, Status und Einstellungen. */
    pub fn list(&self, lang: &str) -> Vec<Value> {
        let advanced = self.advanced();
        let installed = self.read_installed();
        let manifests = self.manifests.lock().unwrap();
        let running = self.running.lock().unwrap();
        let mut out: Vec<Value> = installed
            .iter()
            .map(|(id, e)| {
                let m = localize(&manifests.get(id).cloned().unwrap_or_else(|| json!({ "id": id, "name": id })), lang);
                let blocked = is_native(&m) && !advanced;
                let state = match running.get(id) {
                    _ if blocked => json!({ "state": "blocked", "error": t!("plugins.blockedNeedsAdvanced") }),
                    Some(r) => r.error.lock().unwrap().clone().map_or_else(|| json!({ "state": "running" }), |e| json!({ "state": "error", "error": e })),
                    None if e["enabled"] == false => json!({ "state": "off" }),
                    None => json!({ "state": "error", "error": t!("plugins.couldNotBeLoaded") }),
                };
                json!({
                    "id": id, "manifest": m, "permissions": describe_permissions(&m), "enabled": e["enabled"] != false, "native": is_native(&m),
                    "source": e["source"], "ref": e["ref"], "url": e["url"], "settings": e["settings"], "status": state,
                    "pages": Self::page_files(&self.plugin_dir(id, e)),
                    "update": self.updates.lock().unwrap().get(id).cloned().unwrap_or(Value::Null),
                    // Fertige Seiten, die sich mit dem letzten Update geändert haben (Frage „ersetzen?“).
                    "pagesChanged": e["pagesChanged"].clone(),
                })
            })
            .collect();
        out.sort_by_key(|p| p["manifest"]["name"].as_str().unwrap_or("").to_lowercase());
        out
    }

    /**
     * Icon-Pakete, die ein Plugin voraussetzt, importieren – nur wenn es noch kein Paket mit diesem Namen gibt.
     * Liefert die Namen der neu importierten Pakete.
     */
    pub fn ensure_icon_packs(&self, app: &Arc<App>, id: &str) -> Vec<String> {
        let Some(entry) = self.read_installed().get(id).cloned() else { return Vec::new() };
        let dir = self.plugin_dir(id, &entry);
        let manifest = self.manifests.lock().unwrap().get(id).cloned().unwrap_or_default();
        let mut added = Vec::new();
        for rel in manifest["iconPacks"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            let Ok(bytes) = std::fs::read(dir.join(rel)) else { continue };
            let name = pack_name(&bytes).unwrap_or_default();
            let existing = app.iconpacks.list().into_iter().find(|p| !name.is_empty() && p["name"] == name.as_str());
            if let Some(pack) = existing {
                // Neuere Fassung im Plugin (version in pack.json): an Ort und Stelle ersetzen, die ID bleibt.
                let (old, new) = (pack["version"].as_str().unwrap_or("").to_string(), pack_version(&bytes).unwrap_or_default());
                if !new.is_empty() && version_newer(&new, &old) {
                    match app.iconpacks.replace(pack["id"].as_str().unwrap_or(""), &bytes) {
                        Ok(()) => app.log.info("Plugins", t!("plugins.iconPackUpdated", id, &name, &new)),
                        Err(e) => app.log.warn("Plugins", t!("plugins.iconPackFailed", id, rel, e)),
                    }
                }
                continue;
            }
            let stem = rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".tdicons");
            match app.iconpacks.import(&bytes, stem) {
                Ok(r) => {
                    app.log.info("Plugins", t!("plugins.iconPackImported", id, if name.is_empty() { stem } else { &name }));
                    added.extend(r.into_iter().map(|(pid, _)| pid));
                }
                Err(e) => app.log.warn("Plugins", t!("plugins.iconPackFailed", id, rel, e)),
            }
        }
        added
    }

    /** Ordner eines installierten Plugins (für seine Bilder). */
    pub fn dir_of(&self, id: &str) -> Option<PathBuf> {
        self.read_installed().get(id).map(|e| self.plugin_dir(id, e))
    }

    /** Designs aller eingeschalteten Plugins; Bilder als „plugins/<id>/<pfad>“. */
    pub fn looks(&self, lang: &str) -> Vec<Value> {
        let installed = self.read_installed();
        let manifests = self.manifests.lock().unwrap();
        let mut out = Vec::new();
        for (id, e) in installed.iter().filter(|(_, e)| e["enabled"] != false) {
            let Some(m) = manifests.get(id) else { continue };
            let m = localize(m, lang);
            for l in m["looks"].as_array().into_iter().flatten() {
                let mut l = l.clone();
                for k in ["bgImage", "icon"] {
                    if let Some(f) = l["style"][k].as_str().filter(|f| safe_rel_path(f) && !f.starts_with("packs/")) {
                        l["style"][k] = json!(format!("plugins/{id}/{f}"));
                    }
                }
                l["source"] = json!(m["name"]);
                l["key"] = json!(format!("plugin:{id}:{}", l["id"].as_str().unwrap_or("")));
                out.push(l);
            }
            let _ = e;
        }
        out
    }

    /** Fertige Seiten eines Plugins: alle .tdprofile-Dateien im Hauptordner (Name ohne Endung, Dateiname). */
    fn page_files(dir: &Path) -> Vec<Value> {
        // Auch in Unterordnern (bis 3 Ebenen tief), z. B. pages/strom.tdprofile.
        fn walk(dir: &Path, rel: &str, depth: u32, out: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let Ok(name) = e.file_name().into_string() else { continue };
                if name.starts_with('.') || name == "node_modules" {
                    continue;
                }
                let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
                match e.file_type() {
                    Ok(t) if t.is_dir() && depth < 3 => walk(&e.path(), &path, depth + 1, out),
                    Ok(t) if t.is_file() && name.to_lowercase().ends_with(".tdprofile") => out.push(path),
                    _ => {}
                }
            }
        }
        let mut files = Vec::new();
        walk(dir, "", 0, &mut files);
        files.sort();
        files.truncate(50);
        files
            .into_iter()
            .map(|f| {
                // Sammelordner wie „pages/“ nicht im Namen zeigen.
                let parts: Vec<&str> = f.trim_end_matches(".tdprofile").split('/').collect();
                let skip = usize::from(parts.len() > 1 && ["pages", "seiten", "profiles", "profile"].contains(&parts[0].to_lowercase().as_str()));
                let name = parts[skip..].iter().map(|p| p.replace(['-', '_'], " ")).collect::<Vec<_>>().join(" › ");
                json!({ "file": f, "name": name })
            })
            .collect()
    }

    /** Inhalt einer Seiten-Datei eines Plugins (nur Dateien aus page_files). */
    pub fn page_bytes(&self, id: &str, file: &str) -> Result<Vec<u8>> {
        let entry = self.read_installed().get(id).cloned().ok_or_else(|| anyhow!(t!("plugins.unknownPlugin")))?;
        let dir = self.plugin_dir(id, &entry);
        if !Self::page_files(&dir).iter().any(|p| p["file"] == file) {
            bail!(t!("plugins.pageNotFound"));
        }
        Ok(std::fs::read(dir.join(file))?)
    }

    /** Zahl der Plugins mit Update oder mit geänderten fertigen Seiten (Hinweis am Plugins-Knopf). */
    pub fn update_count(&self) -> usize {
        let changed = self.read_installed().values().filter(|e| e["pagesChanged"].as_array().is_some_and(|l| !l.is_empty())).count();
        self.updates.lock().unwrap().len() + changed
    }

    /** Merken, welche Seiten aus welcher fertigen Seite eines Plugins importiert wurden (für Updates). */
    pub fn record_page_import(&self, id: &str, file: &str, bytes: &[u8], page_ids: &[String]) {
        let mut installed = self.read_installed();
        let Some(e) = installed.get_mut(id) else { return };
        if !e["pages"].is_object() {
            e["pages"] = json!({});
        }
        e["pages"][file] = json!({ "hash": file_hash(bytes), "pageIds": page_ids });
        if let Some(list) = e["pagesChanged"].as_array_mut() {
            list.retain(|f| f != file);
        }
        let _ = self.write_installed(&installed);
    }

    /** Nach einem Update: welche importierten fertigen Seiten haben sich geändert? Wird beim Plugin vermerkt. */
    fn mark_changed_pages(&self, id: &str) -> Vec<String> {
        let mut installed = self.read_installed();
        let Some(e) = installed.get(id).cloned() else { return Vec::new() };
        let dir = self.plugin_dir(id, &e);
        let changed: Vec<String> = e["pages"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(file, rec)| std::fs::read(dir.join(file.as_str())).is_ok_and(|b| rec["hash"].as_str() != Some(file_hash(&b).as_str())))
            .map(|(file, _)| file.clone())
            .collect();
        if let Some(entry) = installed.get_mut(id) {
            entry["pagesChanged"] = json!(changed);
        }
        let _ = self.write_installed(&installed);
        changed
    }

    /** Seiten, die zuletzt aus einer fertigen Seite eines Plugins importiert wurden. */
    pub fn imported_pages(&self, id: &str, file: &str) -> Vec<String> {
        self.read_installed().get(id).and_then(|e| e["pages"][file]["pageIds"].as_array().cloned()).unwrap_or_default().iter().filter_map(|v| v.as_str().map(String::from)).collect()
    }

    /** Frage „Seiten ersetzen?“ ist beantwortet (oder wird nicht gestellt). */
    pub fn clear_changed_pages(&self, id: &str) {
        let mut installed = self.read_installed();
        if let Some(e) = installed.get_mut(id) {
            e["pagesChanged"] = json!([]);
        }
        let _ = self.write_installed(&installed);
    }

    // ---------- Updates ----------

    /** plugin.json des neuesten Stands (Release, sonst Hauptzweig) – nur diese eine Datei, ohne alles zu laden. */
    async fn latest_manifest(&self, r: &Repo) -> Result<(Value, String)> {
        let api = format!("https://api.github.com/repos/{}/{}", r.owner, r.repo);
        let release = self.http.get(format!("{api}/releases/latest")).header("Accept", "application/vnd.github+json").send().await?;
        let git_ref = if release.status().is_success() {
            release.json::<Value>().await?["tag_name"].as_str().unwrap_or("").to_string()
        } else {
            let info: Value = self.http.get(&api).header("Accept", "application/vnd.github+json").send().await?.error_for_status()?.json().await?;
            info["default_branch"].as_str().unwrap_or("main").to_string()
        };
        let path = if r.path.is_empty() { "plugin.json".to_string() } else { format!("{}/plugin.json", r.path) };
        let raw = self.http.get(format!("https://raw.githubusercontent.com/{}/{}/{git_ref}/{path}", r.owner, r.repo)).send().await?.error_for_status()?.bytes().await?;
        let v: Value = serde_json::from_slice(&raw)?;
        Ok((check_manifest(&v)?, git_ref))
    }

    /**
     * Alle über GitHub/Marktplatz installierten Plugins auf neuere Versionen prüfen.
     * Liefert die Updates, die bisher noch nicht gemeldet wurden (für Benachrichtigungen).
     */
    pub async fn check_updates(&self) -> Vec<Value> {
        let installed = self.read_installed();
        let mut fresh = Vec::new();
        for (id, e) in installed {
            let Some(repo) = e["source"].as_str().and_then(|s| s.strip_prefix("github:")) else { continue };
            let Ok(r) = parse_repo(repo) else { continue };
            let current = self.manifests.lock().unwrap().get(&id).and_then(|m| m["version"].as_str().map(String::from)).unwrap_or_default();
            match self.latest_manifest(&r).await {
                Ok((m, git_ref)) => {
                    let latest = m["version"].as_str().unwrap_or("").to_string();
                    if !latest.is_empty() && version_newer(&latest, &current) {
                        let info = json!({ "id": id, "name": m["name"], "current": current, "latest": latest, "ref": git_ref, "native": is_native(&m) });
                        self.updates.lock().unwrap().insert(id.clone(), info.clone());
                        if self.notified.lock().unwrap().insert(format!("{id}@{latest}")) {
                            fresh.push(info);
                        }
                    } else {
                        self.updates.lock().unwrap().shift_remove(&id);
                    }
                }
                Err(_) => continue,
            }
        }
        fresh
    }

    /** Plugin auf den neuesten Stand bringen (Einstellungen bleiben). Liefert geänderte fertige Seiten. */
    pub async fn update(&self, app: &Arc<App>, id: &str) -> Result<Vec<String>> {
        let entry = self.read_installed().get(id).cloned().ok_or_else(|| anyhow!(t!("plugins.unknownPlugin")))?;
        let repo = entry["source"].as_str().and_then(|s| s.strip_prefix("github:")).ok_or_else(|| anyhow!(t!("plugins.updateOnlyGithub")))?;
        let r = parse_repo(repo)?;
        let (files, git_ref, url) = self.download(&r).await?;
        let (m, _, files) = manifest_of(files)?;
        if m["id"] != id {
            bail!(t!("plugins.updateOtherId", id));
        }
        if is_native(&m) && !self.advanced() {
            bail!(t!("plugins.thisPluginHasFull"));
        }
        // Wer ein erweitertes Plugin installiert hat, hat den vollen Rechten schon zugestimmt.
        self.write_plugin(app, &r, &m, files, &git_ref, &url)?;
        self.updates.lock().unwrap().shift_remove(id);
        app.log.info("Plugins", t!("plugins.updated", id, m["version"].as_str().unwrap_or("?")));
        Ok(self.mark_changed_pages(id))
    }

    /** Ein TouchDeck-Ereignis an alle Plugins weitergeben, die darauf hören dürfen. */
    pub fn broadcast_event(&self, kind: &str, data: &Value) {
        for r in self.running.lock().unwrap().values().filter(|r| r.events) {
            match &r.tx {
                Channel::Js(tx) => {
                    let _ = tx.send(Msg::Event { kind: kind.to_string(), data: data.clone() });
                }
                Channel::Native(tx) => {
                    let _ = tx.send(NativeCmd::Event { kind: kind.to_string(), data: data.clone() });
                }
            }
        }
    }

    /**
     * Daten einer Aktion als Anzeige: Liste aus Bezeichnung und lesbarem Wert, Reihenfolge und Namen aus
     * „returns“ im Manifest (sonst wie geliefert).
     */
    pub fn result_display(&self, kind: &str, data: &Map<String, Value>, lang: &str) -> Value {
        let (plugin, action) = kind.strip_prefix("plugin:").and_then(|k| k.split_once(':')).unwrap_or(("", ""));
        let returns: Vec<Value> = self
            .manifests
            .lock()
            .unwrap()
            .get(plugin)
            .map(|m| localize(m, lang))
            .and_then(|m| m["actions"].as_array().and_then(|l| l.iter().find(|a| a["id"] == action)).and_then(|a| a["returns"].as_array().cloned()))
            .unwrap_or_default();
        let mut items = Vec::new();
        for r in &returns {
            let k = r["key"].as_str().unwrap_or("");
            if let Some(v) = data.get(k) {
                let label = r["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(k);
                items.push(json!({ "label": label, "text": format_unit(v, r["unit"].as_str().unwrap_or("")) }));
            }
        }
        if returns.is_empty() {
            items = data.iter().filter(|(_, v)| !v.is_object() && !v.is_array()).map(|(k, v)| json!({ "label": k, "text": format_unit(v, "") })).collect();
        }
        let first = items.first().map(|i| i["text"].clone()).unwrap_or(json!(""));
        json!({ "text": first, "items": items.into_iter().take(8).collect::<Vec<_>>() })
    }

    /** Live-Anzeigen der Plugins für den Editor: { value: "plugin:<id>:<anzeige>", label }. */
    pub fn live_sources(&self, lang: &str) -> Vec<Value> {
        let advanced = self.advanced();
        let installed = self.read_installed();
        let manifests = self.manifests.lock().unwrap();
        let mut out = Vec::new();
        for (id, m) in manifests.iter().filter(|(id, m)| installed.get(*id).is_some_and(|e| e["enabled"] != false) && (advanced || !is_native(m))) {
            let m = localize(m, lang);
            let name = m["name"].as_str().unwrap_or(id).to_string();
            for l in m["lives"].as_array().into_iter().flatten() {
                let lid = l["id"].as_str().unwrap_or("");
                let label = l["label"].as_str().filter(|x| !x.is_empty()).unwrap_or(lid);
                out.push(json!({ "value": format!("plugin:{id}:{lid}"), "label": format!("{name} › {label}") }));
            }
            // Aktionen, die Daten zurückgeben: ihr letztes Ergebnis als Anzeige.
            for a in m["actions"].as_array().into_iter().flatten().filter(|a| a["returns"].as_array().is_some_and(|r| !r.is_empty())) {
                let aid = a["id"].as_str().unwrap_or("");
                let label = a["label"].as_str().filter(|x| !x.is_empty()).unwrap_or(aid);
                out.push(json!({ "value": format!("plugin-result:{id}:{aid}"), "label": format!("{name} › {} ({})", label, t!("plugins.result")) }));
            }
        }
        out
    }

    pub fn catalog(&self, lang: &str) -> (Vec<Value>, Vec<Value>) {
        let advanced = self.advanced();
        let installed = self.read_installed();
        let manifests = self.manifests.lock().unwrap();
        let (mut actions, mut triggers) = (Vec::new(), Vec::new());
        let usable = |id: &String, m: &Value| installed.get(id).is_some_and(|e| e["enabled"] != false) && (advanced || !is_native(m));
        for (id, m) in manifests.iter().filter(|(id, m)| usable(id, m)) {
            let m = &localize(m, lang);
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

    pub fn label(&self, kind: &str, lang: &str) -> Option<String> {
        let (plugin, action) = kind.strip_prefix("plugin:")?.split_once(':')?;
        let manifests = self.manifests.lock().unwrap();
        let a = localize(manifests.get(plugin)?["actions"].as_array()?.iter().find(|a| a["id"] == action)?, lang);
        Some(a["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(action).to_string())
    }

    pub fn has_trigger(&self, kind: &str) -> bool {
        let Some((plugin, trigger)) = kind.strip_prefix("plugin:").and_then(|k| k.split_once(':')) else { return false };
        self.manifests.lock().unwrap().get(plugin).is_some_and(|m| m["triggers"].as_array().is_some_and(|t| t.iter().any(|x| x["id"] == trigger)))
    }

    /** Eine Plugin-Aktion ausführen (Typ plugin:<id>:<aktion>). */
    /** Plugin-Aktion ausführen: Meldung und (falls das Plugin welche liefert) Daten. */
    pub async fn run_action(&self, kind: &str, params: &Map<String, Value>, info: Value, depth: u32) -> Result<(Option<String>, Option<Map<String, Value>>)> {
        let raw = self.run_action_raw(kind, params, info, depth).await?;
        // Daten kommen mit einem Kennzeichen vorneweg: "\u{1}{"message":…,"data":{…}}".
        if let Some(json) = raw.as_deref().and_then(|r| r.strip_prefix('\u{1}')) {
            let v: Value = serde_json::from_str(json).unwrap_or_default();
            let message = v["message"].as_str().filter(|m| !m.is_empty()).map(String::from);
            let data = v["data"].as_object().map(|d| d.iter().take(30).map(|(k, x)| (k.chars().take(40).collect(), x.clone())).collect());
            return Ok((message, data));
        }
        Ok((raw, None))
    }

    async fn run_action_raw(&self, kind: &str, params: &Map<String, Value>, info: Value, depth: u32) -> Result<Option<String>> {
        let (plugin, action) = kind.strip_prefix("plugin:").and_then(|k| k.split_once(':')).ok_or_else(|| anyhow!(t!("plugins.unknownPluginAction")))?;
        let (reply, wait) = oneshot::channel();
        {
            let running = self.running.lock().unwrap();
            let r = running.get(plugin).ok_or_else(|| {
                if self.manifests.lock().unwrap().get(plugin).is_some_and(is_native) && !self.advanced() {
                    anyhow!(t!("plugins.pluginNeedsAdvancedMode", plugin))
                } else {
                    anyhow!(t!("plugins.pluginIsNotInstalled", plugin))
                }
            })?;
            if let Some(e) = r.error.lock().unwrap().as_ref() {
                bail!(t!("plugins.pluginIsNotRunning3", plugin, e));
            }
            let sent = match &r.tx {
                Channel::Js(tx) => tx.send(Msg::Action { id: action.into(), params: Value::Object(params.clone()), info, depth, reply }).is_ok(),
                Channel::Native(tx) => tx.send(NativeCmd::Action { id: action.into(), params: Value::Object(params.clone()), info, depth, reply }).is_ok(),
            };
            if !sent {
                bail!(t!("plugins.pluginIsNotRunning2", plugin));
            }
        }
        match tokio::time::timeout(Duration::from_secs(30), wait).await {
            Ok(Ok(r)) => r.map_err(|e| anyhow!(e)),
            Ok(Err(_)) => bail!(t!("plugins.pluginHasExited", plugin)),
            Err(_) => bail!(t!("plugins.pluginIsNotResponding", plugin)),
        }
    }

    // ---------- Installieren ----------

    /** Prüft ein GitHub-Repo: öffentlich? plugin.json gültig? Liefert Beschreibung, Berechtigungen, Version und Prüfsumme. */
    pub async fn preview(&self, repo_input: &str, lang: &str) -> Result<Value> {
        let r = parse_repo(repo_input)?;
        let (files, git_ref, url) = self.download(&r).await?;
        let (m, digest, chosen) = manifest_of(files)?;
        let native = is_native(&m);
        // Steht das Plugin im Marktplatz? Sonst zeigt der Editor „auf eigene Gefahr“.
        let full = r.to_string().to_lowercase();
        let listed = self.market(lang, false).await.is_ok_and(|v| v["plugins"].as_array().into_iter().flatten().any(|p| p["repo"].as_str().is_some_and(|x| x.to_lowercase() == full)));
        Ok(json!({ "listed": listed, "repo": r.to_string(), "ref": git_ref, "url": url, "manifest": localize(&m, lang), "permissions": describe_permissions(&m),
            "digest": digest, "native": native, "advanced": self.advanced(), "files": chosen.len() }))
    }

    /** Öffentliches Repo herunterladen: neueste Veröffentlichung (Release), sonst der Hauptzweig. */
    async fn download(&self, r: &Repo) -> Result<(Vec<PluginFile>, String, String)> {
        let (owner, repo) = (&r.owner, &r.repo);
        let api = format!("https://api.github.com/repos/{owner}/{repo}");
        let res = self.http.get(&api).header("Accept", "application/vnd.github+json").send().await?;
        if res.status().as_u16() == 404 {
            bail!(t!("plugins.repoDoesNotExist", owner, repo));
        }
        let info: Value = res.error_for_status()?.json().await?;
        if info["private"] != false || info["visibility"].as_str().is_some_and(|v| v != "public") {
            bail!(t!("plugins.repoIsNotPublic"));
        }
        let mut url = info["html_url"].as_str().unwrap_or("").to_string();
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
        if zip.len() as u64 > NATIVE_MAX_TOTAL {
            bail!(t!("plugins.repoIsTooLarge"));
        }
        let mut files = unpack(&zip)?;
        if !r.path.is_empty() {
            // Nur der Unterordner des Plugins, mit ihm als Hauptordner.
            let prefix = format!("{}/", r.path);
            files = files.into_iter().filter_map(|(n, d, x)| n.strip_prefix(&prefix).map(|rest| (rest.to_string(), d, x))).collect();
            if files.is_empty() {
                bail!(t!("plugins.folderNotInRepo", r.path));
            }
            url = format!("{url}/tree/{git_ref}/{}", r.path);
        }
        Ok((files, git_ref, url))
    }

    /** Installieren oder aktualisieren; digest muss zur angezeigten Vorschau passen (nichts Unbemerktes). */
    pub async fn install(&self, app: &Arc<App>, repo_input: &str, digest: &str, accept_risk: bool) -> Result<String> {
        let r = parse_repo(repo_input)?;
        let (files, git_ref, url) = self.download(&r).await?;
        let (m, d, files) = manifest_of(files)?;
        if d != digest {
            bail!(t!("plugins.pluginHasChangedSince"));
        }
        if is_native(&m) && !self.advanced() {
            bail!(t!("plugins.thisPluginHasFull"));
        }
        if is_native(&m) && !accept_risk {
            bail!(t!("plugins.pleaseConfirmThatYou"));
        }
        let id = m["id"].as_str().unwrap_or("").to_string();
        // Ein anderes Plugin mit derselben ID nicht überschreiben – die eigene Ordner-Fassung (Testmodus) schon.
        if let Some(other) = self.read_installed().get(&id).and_then(|e| e["source"].as_str()).filter(|s| *s != format!("github:{r}") && !s.starts_with("local:"))
        {
            bail!(t!("plugins.anotherPluginWithId", id, other));
        }
        self.write_plugin(app, &r, &m, files, &git_ref, &url)?;
        app.log.info("Plugins", t!("plugins.installedFrom", id, format!("{r} @ {git_ref}")));
        Ok(id)
    }

    /** Dateien eines Plugins aus GitHub schreiben, eintragen, starten (Installation und Update). */
    fn write_plugin(&self, app: &Arc<App>, r: &Repo, m: &Value, files: Vec<PluginFile>, git_ref: &str, url: &str) -> Result<()> {
        let id = m["id"].as_str().unwrap_or("").to_string();
        let mut installed = self.read_installed();
        self.stop(app, &id);
        let dir = self.dir.join(&id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        for (name, data, exec) in &files {
            use std::os::unix::fs::PermissionsExt;
            let target = dir.join(name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, data)?;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(if *exec { 0o755 } else { 0o644 }))?;
        }
        let old = installed.get(&id).cloned().unwrap_or_default();
        let settings = Some(old["settings"].clone()).filter(Value::is_object).unwrap_or_else(|| json!({}));
        // Einstellungen und das Wissen über importierte Seiten bleiben bei Updates erhalten.
        let enabled = old["enabled"] != false;
        installed.insert(id.clone(), json!({ "source": format!("github:{r}"), "ref": git_ref, "url": url, "enabled": enabled, "settings": settings,
                                             "pages": old["pages"].clone(), "pagesChanged": [] }));
        self.write_installed(&installed)?;
        self.manifests.lock().unwrap().insert(id.clone(), m.clone());
        if enabled {
            self.start(app, &id);
        }
        self.ensure_icon_packs(app, &id);
        Ok(())
    }

    /** Testmodus: Plugin direkt aus einem lokalen Ordner laden (zum Entwickeln, bevor es veröffentlicht ist). */
    pub fn install_local(&self, app: &Arc<App>, path: &str) -> Result<String> {
        let dir = crate::system::linux::expand_home(path.trim());
        if dir.is_file() && dir.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            let bytes = std::fs::read(&dir)?;
            return self.install_zip(app, &bytes);
        }
        let dir = dir.canonicalize().map_err(|_| anyhow!(t!("plugins.folderDoesNotExist", dir.display())))?;
        let m = Self::load_manifest(&dir)?;
        if is_native(&m) && !self.advanced() {
            bail!(t!("plugins.thisPluginHasFull"));
        }
        let id = m["id"].as_str().unwrap_or("").to_string();
        let mut installed = self.read_installed();
        let settings = installed.get(&id).map(|e| e["settings"].clone()).filter(Value::is_object).unwrap_or_else(|| json!({}));
        installed.insert(id.clone(), json!({ "source": format!("local:{}", dir.display()), "ref": "lokal", "url": null, "enabled": true, "settings": settings }));
        self.write_installed(&installed)?;
        self.manifests.lock().unwrap().insert(id.clone(), m);
        self.start(app, &id);
        self.ensure_icon_packs(app, &id);
        app.log.warn("Plugins", t!("plugins.devLoaded", id, dir.display()));
        Ok(id)
    }

    /** Testmodus mit einer ZIP-Datei: auspacken nach plugins/_zip/<id> und von dort laden. */
    pub fn install_zip(&self, app: &Arc<App>, bytes: &[u8]) -> Result<String> {
        let files = unpack(bytes)?;
        let raw = files.iter().find(|(n, _, _)| n == "plugin.json").ok_or_else(|| anyhow!(t!("plugins.zipHasNoPlugin")))?;
        let m = check_manifest(&serde_json::from_slice(&raw.1).map_err(|e| anyhow!(t!("plugins.pluginJsonIsNot", e)))?)?;
        let id = m["id"].as_str().unwrap_or("").to_string();
        self.stop(app, &id);
        let dir = self.dir.join("_zip").join(&id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        for (name, data, exec) in &files {
            use std::os::unix::fs::PermissionsExt;
            let target = dir.join(name);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, data)?;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(if *exec { 0o755 } else { 0o644 }))?;
        }
        self.install_local(app, &dir.to_string_lossy())
    }

    /** Neu laden (z. B. nach Änderungen im Testmodus). */
    pub fn reload(&self, app: &Arc<App>, id: &str) -> Result<()> {
        let entry = self.read_installed().get(id).cloned().ok_or_else(|| anyhow!(t!("plugins.unknownPlugin")))?;
        let m = Self::load_manifest(&self.plugin_dir(id, &entry))?;
        self.manifests.lock().unwrap().insert(id.into(), m);
        if entry["enabled"] != false {
            self.start(app, id);
        }
        Ok(())
    }

    pub fn set_enabled(&self, app: &Arc<App>, id: &str, on: bool) -> Result<()> {
        let mut installed = self.read_installed();
        let e = installed.get_mut(id).ok_or_else(|| anyhow!(t!("plugins.unknownPlugin")))?;
        e["enabled"] = json!(on);
        self.write_installed(&installed)?;
        if on { self.start(app, id) } else { self.stop(app, id) }
        Ok(())
    }

    pub fn set_settings(&self, app: &Arc<App>, id: &str, settings: &Value) -> Result<()> {
        let mut installed = self.read_installed();
        let e = installed.get_mut(id).ok_or_else(|| anyhow!(t!("plugins.unknownPlugin")))?;
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
        let e = installed.remove(id).ok_or_else(|| anyhow!(t!("plugins.unknownPlugin")))?;
        self.stop(app, id);
        self.write_installed(&installed)?;
        self.manifests.lock().unwrap().remove(id);
        if !e["source"].as_str().is_some_and(|s| s.starts_with("local:")) {
            let _ = std::fs::remove_dir_all(self.dir.join(id));
        }
        app.log.info("Plugins", format!("{id} entfernt"));
        Ok(())
    }

    /** Liste aus dem öffentlichen Marktplatz-Repo. */
    /**
     * README eines Plugins für die Beschreibung im Marktplatz: README.<sprache>.md, sonst README.md
     * (aus dem Hauptzweig). base = Adresse des Ordners, damit relative Bilder gefunden werden.
     */
    pub async fn readme(&self, repo_input: &str, lang: &str) -> Result<Value> {
        let r = parse_repo(repo_input)?;
        let dir = if r.path.is_empty() { String::new() } else { format!("{}/", r.path) };
        let base = format!("https://raw.githubusercontent.com/{}/{}/HEAD/{dir}", r.owner, r.repo);
        let page = format!("https://github.com/{}/{}/tree/HEAD/{dir}", r.owner, r.repo);
        let names = [format!("README.{lang}.md"), "README.md".to_string(), "readme.md".to_string()];
        for name in names {
            let res = self.http.get(format!("{base}{name}")).timeout(std::time::Duration::from_secs(10)).send().await?;
            if !res.status().is_success() {
                continue;
            }
            let bytes = res.bytes().await?;
            let text = String::from_utf8_lossy(&bytes[..bytes.len().min(300_000)]).into_owned();
            return Ok(json!({ "text": text, "base": base, "page": page, "file": name }));
        }
        bail!(t!("plugins.noReadme"))
    }

    /** Neuester Commit des Marktplatz-Repos (ohne Zwischenspeicher); None bei Fehler. */
    async fn latest_market_commit(&self) -> Option<String> {
        let res = self.http.get("https://api.github.com/repos/Marcel1853/touchdeck-plugins/commits/main").header("Accept", "application/vnd.github.sha").send().await.ok()?;
        let sha = res.error_for_status().ok()?.text().await.ok()?;
        let sha = sha.trim();
        (sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit())).then(|| sha.to_string())
    }

    /** Marktplatz-Liste. `fresh`: am neuesten Commit vorbei am GitHub-Zwischenspeicher (hält die Datei sonst bis zu 5 Minuten). */
    pub async fn market(&self, lang: &str, fresh: bool) -> Result<Value> {
        let lang = lang.to_string();
        let mut url = std::env::var("TOUCHDECK_MARKET").unwrap_or_else(|_| MARKET_URL.into());
        if fresh
            && url == MARKET_URL
            && let Some(sha) = self.latest_market_commit().await
        {
            url = MARKET_URL.replace("/main/", &format!("/{sha}/"));
        }
        let res = self.http.get(&url).send().await.map_err(|e| anyhow!(t!("plugins.marketplaceNotReachable", e)))?;
        if res.status().as_u16() == 404 {
            return Ok(json!({ "plugins": [], "note": t!("plugins.marketplaceIsStillEmpty") }));
        }
        let v: Value = res.error_for_status()?.json().await.map_err(|_| anyhow!(t!("plugins.marketplaceListIsDamaged")))?;
        let plugins: Vec<Value> = v["plugins"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                // „path“ (Unterordner im Repo) darf auch getrennt angegeben sein.
                let full = match p["path"].as_str().map(|x| x.trim_matches('/')).filter(|x| !x.is_empty()) {
                    Some(path) => format!("{}/{path}", p["repo"].as_str()?),
                    None => p["repo"].as_str()?.to_string(),
                };
                let r = parse_repo(&full).ok()?;
                Some(json!({
                    "repo": r.to_string(),
                    "name": localize(&ltext(&p["name"], 60), &lang),
                    "description": localize(&ltext(&p["description"], 300), &lang),
                    "author": text(&p["author"], 60),
                    "license": text(&p["license"], 100),
                }))
            })
            .collect();
        Ok(json!({ "plugins": plugins }))
    }
}

/** Name eines Icon-Pakets aus seiner .tdicons-Datei (pack.json), ohne es zu importieren. */
/** Version aus der pack.json eines Icon-Pakets. */
fn pack_version(bytes: &[u8]) -> Option<String> {
    pack_field(bytes, "version")
}

fn pack_name(bytes: &[u8]) -> Option<String> {
    pack_field(bytes, "name")
}

fn pack_field(bytes: &[u8], key: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).ok()?;
        if f.name().ok()?.rsplit('/').next()? == "pack.json" {
            let mut t = String::new();
            f.read_to_string(&mut t).ok()?;
            return serde_json::from_str::<Value>(&t).ok()?[key].as_str().map(String::from);
        }
    }
    None
}

#[cfg(test)]
mod tests {

    #[test]
    fn versionen_vergleichen() {
        assert!(version_newer("0.7.1", "0.7.0"));
        assert!(version_newer("1.10.0", "1.9.9"));
        assert!(version_newer("v2.0", "1.99.99"));
        assert!(!version_newer("0.7.0", "0.7.0"));
        assert!(!version_newer("0.7", "0.7.0"));
        assert!(!version_newer("0.6.9", "0.7.0"));
    }


    #[test]
    fn einheiten_lesbar() {
        assert_eq!(format_unit(&json!(4210), "mb"), "4,1 GB");
        assert_eq!(format_unit(&json!(50432), "seconds"), "14 h");
        assert_eq!(format_unit(&json!(7), ""), "7");
        assert_eq!(format_unit(&json!(42.5), "percent"), "42,5 %");
        assert_eq!(format_unit(&json!(900), "bytes"), "900 B");
        assert_eq!(format_unit(&json!("läuft"), "text"), "läuft");
    }

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
        assert!(check_manifest(&json!({ "id": "x", "name": "x", "native": { "command": [] } })).is_err());
        let n = check_manifest(&json!({ "id": "x", "name": "x", "native": { "command": ["python3", "p.py"] } })).unwrap();
        assert!(is_native(&n));
        assert!(describe_permissions(&n)[0].starts_with("VOLLE RECHTE"));
    }

    #[test]
    fn uebersetzungen() {
        let m = check_manifest(&json!({ "id": "x", "name": { "de": "Licht", "en": "Light" }, "license": "MIT",
            "actions": [{ "id": "an", "label": { "de": "Einschalten", "en": "Turn on" }, "fields": [{ "key": "z", "label": { "de": "Zimmer", "en": "Room" } }] }] })).unwrap();
        let en = localize(&m, "en");
        assert_eq!(en["name"], "Light");
        assert_eq!(en["actions"][0]["label"], "Turn on");
        assert_eq!(en["actions"][0]["fields"][0]["label"], "Room");
        assert_eq!(localize(&m, "fr")["name"], "Light", "fehlende Sprache: Englisch");
        assert_eq!(m["license"], "MIT");
        // Gewöhnliche Objekte (Auswahlwerte) bleiben unverändert.
        assert_eq!(localize(&json!({ "value": "a", "label": "A" }), "en"), json!({ "value": "a", "label": "A" }));
        assert!(check_manifest(&json!({ "id": "x", "name": { "en": "" } })).is_err());
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
        assert_eq!(files.len(), 5, "versteckte Ordner fallen weg");
        let (m, digest, chosen) = manifest_of(files.clone()).unwrap();
        let names: Vec<&str> = chosen.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(names, vec!["README.md", "plugin.js", "plugin.json"], "sichere Plugins: nur erlaubte Dateien im Hauptordner");
        assert_eq!(m["id"], "x");
        assert_eq!(digest.len(), 64);
        // Erweiterte Plugins behalten alle Dateien, und die Prüfsumme ändert sich mit jeder Datei.
        let mut native = files;
        native[0].1 = br#"{ "id": "x", "name": "X", "native": { "command": ["sh", "tools/build.sh"] } }"#.to_vec();
        let (_, d1, all) = manifest_of(native.clone()).unwrap();
        assert_eq!(all.len(), 5);
        native[3].1 = b"echo anders".to_vec();
        let (_, d2, _) = manifest_of(native).unwrap();
        assert_ne!(d1, d2);
    }

    #[test]
    fn repo_namen() {
        let r = |o: &str, n: &str, p: &str| Repo { owner: o.into(), repo: n.into(), path: p.into() };
        assert_eq!(parse_repo("https://github.com/Marcel1853/td-hue").unwrap(), r("Marcel1853", "td-hue", ""));
        assert_eq!(parse_repo("Marcel1853/td-hue.git").unwrap(), r("Marcel1853", "td-hue", ""));
        assert_eq!(parse_repo("Marcel1853/touchdeck-plugins/plugins/clipboard-slots").unwrap(), r("Marcel1853", "touchdeck-plugins", "plugins/clipboard-slots"));
        assert_eq!(parse_repo("https://github.com/Marcel1853/touchdeck-plugins/tree/main/plugins/x").unwrap(), r("Marcel1853", "touchdeck-plugins", "plugins/x"));
        assert!(parse_repo("a/b/../c").is_err());
        assert!(parse_repo("a/b/.git").is_err());
        assert!(parse_repo("nur-name").is_err());
        assert!(parse_repo("a/b c").is_err());
    }
}
