// HTTP- und WebSocket-Server: Editor (nur lokal) und Bedienfeld fürs Handy (mit Kopplungsschlüssel und optional PIN).
use crate::t;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use axum::Router;
use axum::body::Body;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{HeaderMap, Method, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::discovery;
use super::http::{ApiResult, HttpError, error_response, json_response, safe_equal, serve_file, text_response};
use crate::core::actions::{RunOpts, option_source, run_actions};
use crate::core::backup::Backups;
use crate::core::catalog::{self, Section};
use crate::core::events::EventEngine;
use crate::core::iconpacks::IconPacks;
use crate::core::timers::Timers;
use crate::core::plugins::Plugins;
use crate::core::profiles;
use crate::core::live::{apply_slider, collect_live};
use crate::core::log::Log;
use crate::core::store::{Action, Button, Page, Store, action_list, new_id};
use crate::integrations::discord::DiscordClient;
use crate::integrations::obs::{self, ObsClient};
use crate::integrations::twitch::TwitchClient;
use crate::integrations::{steam, steam_pages};
use crate::system::linux;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/** Kennung dieses Programmstarts: ändert sie sich, laden offene Bedienfelder neu (neuer Build, gleiche Version). */
static BUILD: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis()).to_string());
const MAX_BODY: usize = 8 * 1024 * 1024;
// Icon-Pakete hochladen oder importieren darf nur der Editor auf diesem PC, dafür mit größerem Limit.
const MAX_UPLOAD: usize = 128 * 1024 * 1024;
const PAIR_TIMEOUT: Duration = Duration::from_secs(2 * 60);

/** Eine Kopplungsanfrage, die am PC bestätigt werden muss. */
#[derive(Clone, Debug)]
pub struct PairRequest {
    pub id: String,
    pub device: String,
    pub ip: String,
}

pub struct Options {
    pub port: u16,
    pub host: IpAddr,
    /// Ordner mit editor/, deck/ und shared/.
    pub public_dir: PathBuf,
    /// Die Android-App zum Herunterladen.
    pub apk: PathBuf,
    /// Wird aufgerufen, wenn ein Handy sich koppeln möchte (z. B. um das Fenster zu zeigen).
    pub on_pair_request: Option<Box<dyn Fn(PairRequest) + Send + Sync>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Who {
    /// Dieser PC.
    Local,
    /// Gekoppeltes Handy.
    Device,
    /// Schlüssel stimmt, PIN fehlt.
    Pin,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Editor,
    Device,
}

struct Client {
    role: Role,
    tx: mpsc::UnboundedSender<Message>,
    viewing: Option<String>,
    /// Zuletzt an dieses Gerät geschickte Live-Werte (für welche Seite), damit nur Änderungen rausgehen.
    sent_page: Option<String>,
    sent: serde_json::Map<String, Value>,
    /// Seite, die vor dem Wechsel in den Hintergrund angezeigt wurde (kein neues „Seite geöffnet“ beim Zurückkommen).
    paused: Option<String>,
    /// Für die Statusleiste: Adresse und Name, den das Gerät nach dem Verbinden meldet.
    ip: String,
    name: String,
}

struct Pairing {
    device: String,
    ip: String,
    created: Instant,
    state: &'static str,
}

/** Gedrückthalten: wiederholende Aufgabe und Aktionen fürs Loslassen. */
struct Hold {
    task: JoinHandle<()>,
    release: Vec<Action>,
    button_id: String,
    title: String,
}

/** Regler: immer nur den neuesten Wert anwenden, während ein Befehl noch läuft. */
#[derive(Default)]
struct SliderQueue {
    busy: bool,
    next: Option<f64>,
}

pub struct App {
    pub store: Arc<Mutex<Store>>,
    pub log: Arc<Log>,
    pub backups: Backups,
    pub obs: ObsClient,
    pub twitch: TwitchClient,
    pub discord: DiscordClient,
    pub events: EventEngine,
    pub iconpacks: IconPacks,
    pub timers: Arc<Timers>,
    pub plugins: Plugins,
    pub version: &'static str,
    pub port: u16,
    public_dir: PathBuf,
    apk: PathBuf,
    started: Instant,
    clients: Mutex<HashMap<u64, Client>>,
    next_client: AtomicU64,
    pairings: Mutex<HashMap<String, Pairing>>,
    holds: Mutex<HashMap<(u64, String), Hold>>,
    sliders: Mutex<HashMap<String, SliderQueue>>,
    live: Mutex<serde_json::Map<String, Value>>,
    /// Live-Anzeigen, die Plugins mit td.live.set(…) füllen ("plugin:<id>:<anzeige>" → Wert).
    pub plugin_lives: Mutex<serde_json::Map<String, Value>>,
    /// Verläufe für Live-Anzeigen mit Graph.
    pub graphs: crate::core::live::Graphs,
    /// Zustand der USB-Verbindung: { devices: [...], error }.
    pub usb: Mutex<Value>,
    live_busy: AtomicBool,
    /// PC gesperrt (Bildschirmsperre)? Nur bekannt, solange jemand es braucht (Auslöser oder „Bedienfeld mitsperren“).
    pc_locked: AtomicBool,
    live_again: AtomicBool,
    on_pair_request: Option<Box<dyn Fn(PairRequest) + Send + Sync>>,
}

fn with_type(mut v: Value, kind: &str) -> Value {
    v["type"] = json!(kind);
    v
}

impl App {
    fn deck_state(&self, store: &Store) -> Value {
        let c = &store.config;
        json!({
            "version": VERSION,
            "build": *BUILD,
            "revision": store.revision,
            "pages": c.pages,
            "toggles": c.toggles,
            "variables": c.variables,
            "overrides": c.overrides,
            "live": *self.live.lock().unwrap(),
            "startPage": c.settings.start_page,
            "deck": c.settings.deck,
            "language": c.settings.language,
            "pcLocked": self.pc_locked.load(Ordering::Relaxed),
        })
    }

    fn editor_state(&self, store: &Store) -> Value {
        let mut state = self.deck_state(store);
        // Aktionen und Auslöser der eingeschalteten Plugins kommen zum festen Katalog dazu.
        let lang = store.settings().language.as_str();
        let (plugin_actions, plugin_triggers) = self.plugins.catalog(lang);
        let with_plugins = |base: &Value, extra: Vec<Value>| {
            let mut list = base.as_array().cloned().unwrap_or_default();
            list.extend(extra);
            Value::Array(list)
        };
        let extra = json!({
            "flows": store.config.flows,
            "folders": crate::core::store::sanitize_folders(store.config.extra.get("folders").unwrap_or(&Value::Null)),
            "events": store.config.events,
            "values": store.config.values,
            "settings": store.public_settings(),
            "actionTypes": with_plugins(&catalog::localized(&catalog::ACTION_TYPES, Section::Actions, lang), plugin_actions),
            "triggerTypes": with_plugins(&catalog::localized(&catalog::TRIGGER_TYPES, Section::Triggers, lang), plugin_triggers),
            "liveSources": with_plugins(&catalog::localized(catalog::live_sources(), Section::LiveSources, lang), self.plugins.live_sources(lang)),
            "sliderTargets": catalog::localized(catalog::slider_targets(), Section::SliderTargets, lang),
            "pairingUrls": self.pairing_urls(store),
            "port": self.port,
        });
        if let (Some(s), Value::Object(e)) = (state.as_object_mut(), extra) {
            s.extend(e);
        }
        state
    }

    fn pairing_urls(&self, store: &Store) -> Vec<String> {
        let token = &store.settings().token;
        // Eigene Adresse aus den Einstellungen, sonst alle Adressen im Heimnetz.
        let host = &store.settings().public_host;
        let hosts = if host.is_empty() { linux::lan_addresses() } else { vec![host.clone()] };
        hosts.iter().map(|ip| format!("http://{ip}:{}/deck/?token={token}", self.port)).collect()
    }

    /** Live-Wert eines Plugins setzen und die Anzeigen gleich aktualisieren. */
    pub fn set_plugin_live(self: &Arc<Self>, key: &str, value: Value) {
        self.plugin_lives.lock().unwrap().insert(key.to_string(), value);
        // Nur Buttons mit diesem Plugin-Wert neu senden (ohne alle anderen Werte neu abzufragen).
        let key = key.to_string();
        refresh_quick(self, move |b| b.live.source == key);
    }

    /** Verbundene Geräte (Name, Adresse, angezeigte Seite) – für Statusleiste und Plugins. */
    pub fn device_list(&self) -> Vec<Value> {
        let store = self.store.lock().unwrap();
        let clients = self.clients.lock().unwrap();
        clients
            .values()
            .filter(|c| c.role == Role::Device)
            .map(|c| {
                let page = c.viewing.as_deref().and_then(|p| store.page(p)).map(|p| p.name.clone());
                json!({ "name": c.name, "ip": c.ip, "page": page })
            })
            .collect()
    }

    /** Ein Timer wurde gesteuert: Anzeige sofort aktualisieren und, solange einer läuft, jede Sekunde. */
    pub fn timers_changed(self: &Arc<Self>) {
        refresh_timers(self);
        self.timers.ensure_ticker(self.clone(), refresh_timers, |a, name| a.events.fire(a, "timer_done", &json!({ "name": name, "Name": name })));
    }

    /** Kurze Meldung in allen Bedienfeldern (z. B. aus Events, die kein Button ausgelöst hat). */
    pub fn toast_on_devices(&self, text: &str) {
        self.broadcast(Some(Role::Device), &json!({ "type": "toast", "text": text }));
    }

    /** Alle Bedienfelder (Handy, Tablet, Vorschau) auf eine Seite wechseln lassen. */
    pub fn show_page_on_devices(&self, page_id: &str) {
        self.broadcast(Some(Role::Device), &json!({ "type": "goto", "pageId": page_id }));
    }

    /** Nachricht an alle offenen Verbindungen einer Rolle (None = alle). */
    fn broadcast(&self, role: Option<Role>, msg: &Value) {
        let text = msg.to_string();
        for c in self.clients.lock().unwrap().values() {
            if role.is_none_or(|r| r == c.role) {
                let _ = c.tx.send(Message::Text(text.clone().into()));
            }
        }
    }

    fn device_count(&self) -> usize {
        self.clients.lock().unwrap().values().filter(|c| c.role == Role::Device).count()
    }

    /** Einmal am Tag automatisch sichern, wenn eingeschaltet. */
    fn auto_backup(&self) {
        let store = self.store.lock().unwrap();
        let b = &store.settings().backups;
        if !b.auto || !self.backups.auto_due() {
            return;
        }
        match self.backups.create(&store.config, "automatisch", b.keep) {
            Ok(name) => self.log.info("Backup", format!("Sicherung angelegt: {name}")),
            Err(err) => self.log.warn("Backup", err.to_string()),
        }
    }

    /** Wer fragt an? Lokal, gekoppeltes Gerät, Gerät ohne PIN oder niemand. */
    fn auth(&self, addr: &SocketAddr, query: &HashMap<String, String>, headers: &HeaderMap) -> Option<Who> {
        if is_loopback(addr) {
            return Some(Who::Local);
        }
        let store = self.store.lock().unwrap();
        let s = store.settings();
        if !s.lan_enabled {
            return None;
        }
        let param = |q: &str, h: &str| query.get(q).map(String::as_str).or_else(|| headers.get(h).and_then(|v| v.to_str().ok()));
        if !safe_equal(param("token", "x-token"), &s.token) {
            return None;
        }
        if !s.pin.is_empty() && !safe_equal(param("pin", "x-pin"), &s.pin) {
            return Some(Who::Pin);
        }
        Some(Who::Device)
    }
}

/** Aktionen ausführen und Fehler protokollieren. */
async fn run(app: &Arc<App>, actions: &[Action], button_id: Option<String>, source: &str) -> Value {
    run_with(app, actions, RunOpts { button_id, ..Default::default() }, source).await
}

async fn run_with(app: &Arc<App>, actions: &[Action], opts: RunOpts, source: &str) -> Value {
    let r = run_actions(app, actions, opts).await;
    if r["ok"] != true {
        let prefix = if source.is_empty() { String::new() } else { format!("{source}: ") };
        app.log.error(t!("server.action"), format!("{prefix}{}", r["error"].as_str().unwrap_or("")));
    }
    r
}

fn button_title(b: &crate::core::store::Button) -> String {
    [&b.title, &b.icon.value, &b.id].into_iter().find(|t| !t.is_empty()).cloned().unwrap_or_default()
}

// ---------- Buttons ----------

async fn press(app: &Arc<App>, page_id: &str, button_id: &str, double: bool) -> Value {
    let found = {
        let mut store = app.store.lock().unwrap();
        let found = store.page(page_id).and_then(|p| p.buttons.iter().find(|b| b.id == button_id).map(|b| (p.name.clone(), b.clone())));
        match found {
            None => None,
            // Abhängigkeit nicht erfüllt (z. B. erst „Scharf schalten“): nichts tun.
            Some((_, b)) if !store.requirement_met(&b) => {
                let need = store.config.pages.iter().flat_map(|p| &p.buttons).find(|x| x.id == b.requires.button).map(button_title);
                return json!({ "ok": false, "locked": true, "error": match need {
                    Some(name) => t!("server.firstSwitchOn", name),
                    None => t!("server.conditionNotMet").to_string(),
                } });
            }
            Some((page_name, b)) => {
                // Anzeigen sind nicht zum Drücken da.
                let mut actions = if b.mode == "display" { Vec::new() } else { b.actions.clone() };
                let mut toggled = None;
                if b.mode == "display" {
                } else if double && !b.double_actions.is_empty() {
                    actions = b.double_actions.clone();
                } else if b.mode == "toggle" || b.mode == "switch" {
                    let on = store.config.toggles.get(button_id) != Some(&Value::Bool(true));
                    if !on {
                        actions = b.actions_off.clone();
                    }
                    store.set_toggle(button_id, on);
                    toggled = Some(json!({ "buttonId": button_id, "state": if on { "on" } else { "off" } }));
                } else if b.mode == "states" && !b.states.is_empty() {
                    // Per Druck: zum nächsten Zustand und dessen Aktionen. Nach Wert: Aktionen des aktuellen Zustands.
                    let mut i = store.state_index(&b);
                    if b.state_by.is_empty() {
                        i = (i + 1) % b.states.len();
                        store.set_state(button_id, i);
                    }
                    actions = b.states[i].actions.clone();
                    let title = if b.states[i].title.is_empty() { (i + 1).to_string() } else { b.states[i].title.clone() };
                    toggled = Some(json!({ "buttonId": button_id, "state": (i + 1).to_string(), "Zustand": title }));
                }
                // „Danach wieder ausschalten“: der Button, von dem dieser abhing.
                if b.requires.reset && !b.requires.button.is_empty() {
                    store.set_toggle(&b.requires.button, false);
                }
                Some((format!("{page_name} › {}", button_title(&b)), actions, toggled))
            }
        }
    };
    let Some((label, actions, toggled)) = found else { return json!({ "ok": false, "error": t!("server.buttonNotFound") }) };
    if let Some(data) = toggled {
        app.events.fire(app, "button_toggle", &data);
    }
    let result = run(app, &actions, Some(button_id.into()), &label).await;
    if result["ok"] == true {
        app.log.info("Button", t!("server.pressed", label));
    }
    result
}

// Gedrückthalten: Aktionen beim langen Drücken, auf Wunsch wiederholt, und Aktionen beim Loslassen.
fn hold_start(app: &Arc<App>, client: u64, tx: &mpsc::UnboundedSender<Message>, page_id: &str, button_id: &str) {
    let button = app.store.lock().unwrap().page(page_id).and_then(|p| p.buttons.iter().find(|b| b.id == button_id).cloned());
    let Some(button) = button.filter(|b| !b.hold_actions.is_empty()) else { return };
    hold_end(app, client, button_id, false);
    let (a, tx) = (app.clone(), tx.clone());
    let b = button.clone();
    let task = tokio::spawn(async move {
        let source = format!("Halten: {}", if b.title.is_empty() { &b.id } else { &b.title });
        loop {
            let r = run(&a, &b.hold_actions, Some(b.id.clone()), &source).await;
            let mut out = json!({ "type": "result", "buttonId": b.id });
            if let (Some(o), Value::Object(r)) = (out.as_object_mut(), r) {
                o.extend(r);
            }
            let _ = tx.send(Message::Text(out.to_string().into()));
            if b.hold_repeat <= 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(b.hold_repeat.max(50) as u64)).await;
        }
    });
    let title = if button.title.is_empty() { button.id.clone() } else { button.title.clone() };
    app.holds.lock().unwrap().insert((client, button_id.into()), Hold { task, release: button.release_actions, button_id: button_id.into(), title });
}

fn hold_end(app: &Arc<App>, client: u64, button_id: &str, release: bool) {
    let Some(h) = app.holds.lock().unwrap().remove(&(client, button_id.to_string())) else { return };
    // Läuft die Halte-Aktion nur einmal, darf sie zu Ende laufen; Wiederholungen enden hier.
    if !h.task.is_finished() {
        h.task.abort();
    }
    if release && !h.release.is_empty() {
        let a = app.clone();
        tokio::spawn(async move {
            run(&a, &h.release, Some(h.button_id.clone()), &format!("Loslassen: {}", h.title)).await;
        });
    }
}

fn slide(app: &Arc<App>, page_id: &str, button_id: &str, value: &Value, part: u8, tx: &mpsc::UnboundedSender<Message>) {
    let button = app.store.lock().unwrap().page(page_id).and_then(|p| p.buttons.iter().find(|b| b.id == button_id && b.mode == "slider").cloned());
    let v = crate::core::actions::js_number(Some(value));
    let Some(mut button) = button.filter(|_| v.is_finite()) else { return };
    // Teil 2 = Drehknopf beim Regler „Fader + Drehknopf“: eigenes Ziel, eigene Warteschlange.
    let second = part == 2 && button.slider.shape == "combo";
    if second {
        button.slider = button.slider2.clone();
    }
    let key = if second { format!("{button_id}#2") } else { button_id.to_string() };
    {
        let mut queues = app.sliders.lock().unwrap();
        let q = queues.entry(key.clone()).or_default();
        q.next = Some(v);
        if q.busy {
            return;
        }
        q.busy = true;
    }
    let (a, tx) = (app.clone(), tx.clone());
    tokio::spawn(async move {
        loop {
            let next = a.sliders.lock().unwrap().get_mut(&key).and_then(|q| q.next.take());
            let Some(v) = next else { break };
            match apply_slider(&a, &button, v).await {
                Ok(()) => {
                    let mut live = a.live.lock().unwrap();
                    let entry = live.entry(button.id.clone()).or_insert_with(|| json!({}));
                    entry[if second { "value2" } else { "value" }] = json!(v.round() as i64);
                }
                Err(err) => {
                    let _ = tx.send(Message::Text(json!({ "type": "result", "buttonId": button.id, "ok": false, "error": err.to_string() }).to_string().into()));
                }
            }
        }
        if let Some(q) = a.sliders.lock().unwrap().get_mut(&key) {
            q.busy = false;
        }
    });
}

/** Live-Werte aktualisieren – nur für Seiten, die ein Bedienfeld gerade anzeigt.
 * Wechselt ein Handy die Seite, während noch eine Abfrage läuft, folgt direkt danach eine weitere. */
/**
 * Live-Wert eines Graphen als Unterschied: statt des ganzen Verlaufs nur die neuen Punkte je Reihe
 * (`seriesAppend: [{ values, len }]`). None, wenn das nicht geht (andere Reihen, Verlauf neu begonnen).
 */
fn graph_delta(old: &Value, new: &Value) -> Option<Value> {
    let (Some(a), Some(b)) = (old["series"].as_array(), new["series"].as_array()) else { return None };
    if a.len() != b.len() {
        return None;
    }
    let mut append = Vec::new();
    for (x, y) in a.iter().zip(b) {
        if x["name"] != y["name"] {
            return None;
        }
        let (Some(old_v), Some(new_v)) = (x["values"].as_array(), y["values"].as_array()) else { return None };
        // Kleinstes k, bei dem der alte Verlauf ab k der Anfang des neuen ist (vorne fallen alte Punkte weg).
        let k = (0..=old_v.len()).find(|&k| {
            let rest = &old_v[k..];
            rest.len() <= new_v.len() && rest == &new_v[..rest.len()]
        })?;
        let kept = old_v.len() - k;
        append.push(json!({ "values": &new_v[kept..], "len": new_v.len() }));
    }
    let mut out = new.clone();
    let o = out.as_object_mut()?;
    o.remove("series");
    o.insert("seriesAppend".into(), Value::Array(append));
    Some(out)
}

/** Timer-Anzeigen: jede Sekunde, solange einer läuft – nur die Timer-Buttons. */
fn refresh_timers(app: &Arc<App>) {
    refresh_quick(app, |b| b.live.source == "timer");
}

/** Jedes Gerät bekommt nur Werte seiner Seite: nach einem Seitenwechsel alle, sonst nur geänderte. */
fn send_live(a: &Arc<App>, pages: &[Page], live: &serde_json::Map<String, Value>) {
    for c in a.clients.lock().unwrap().values_mut() {
        let Some(page) = c.viewing.as_ref().and_then(|v| pages.iter().find(|p| &p.id == v)) else { continue };
        let full = c.sent_page.as_ref() != Some(&page.id);
        if full {
            c.sent.clear();
            c.sent_page = Some(page.id.clone());
        }
        let mut values = serde_json::Map::new();
        for b in &page.buttons {
            if let Some(v) = live.get(&b.id)
                && (full || c.sent.get(&b.id) != Some(v))
            {
                // Graphen: nur die neuen Punkte, das Gerät hängt sie an.
                let delta = (!full).then(|| c.sent.get(&b.id).and_then(|old| graph_delta(old, v))).flatten();
                values.insert(b.id.clone(), delta.unwrap_or_else(|| v.clone()));
                c.sent.insert(b.id.clone(), v.clone());
            }
        }
        if full || !values.is_empty() {
            let msg = json!({ "type": "live", "values": values, "full": full });
            let _ = c.tx.send(Message::Text(msg.to_string().into()));
        }
    }
}

/**
 * Schnelle Teil-Aktualisierung ohne Prozesse: nur Buttons angezeigter Seiten, für die `only` gilt und deren
 * Quelle sofort lesbar ist (Timer, Plugin-Werte, Werte). Die übrigen Werte bleiben, wie sie sind.
 */
fn refresh_quick(app: &Arc<App>, only: impl Fn(&Button) -> bool) {
    let viewed: std::collections::HashSet<String> = app.clients.lock().unwrap().values().filter_map(|c| c.viewing.clone()).collect();
    if viewed.is_empty() {
        return;
    }
    let (pages, variables) = {
        let store = app.store.lock().unwrap();
        let pages: Vec<Page> = store.config.pages.iter().filter(|p| viewed.contains(&p.id) && p.buttons.iter().any(&only)).cloned().collect();
        if pages.is_empty() {
            return;
        }
        (pages, store.config.variables.clone())
    };
    let mut fresh = serde_json::Map::new();
    for b in pages.iter().flat_map(|p| &p.buttons).filter(|b| b.mode != "slider" && only(b)) {
        if let Some(mut v) = crate::core::live::quick_value(app, b, &variables) {
            if !b.live.graph.is_empty()
                && let Some(series) = app.graphs.history(&b.id)
            {
                v["series"] = series;
            }
            fresh.insert(b.id.clone(), v);
        }
    }
    let live = {
        let mut live = app.live.lock().unwrap();
        live.extend(fresh);
        live.clone()
    };
    send_live(app, &pages, &live);
}

fn refresh_live(app: &Arc<App>) {
    let viewed: std::collections::HashSet<String> = app.clients.lock().unwrap().values().filter_map(|c| c.viewing.clone()).collect();
    if viewed.is_empty() {
        return;
    }
    if app.live_busy.swap(true, Ordering::SeqCst) {
        app.live_again.store(true, Ordering::SeqCst);
        return;
    }
    let a = app.clone();
    tokio::spawn(async move {
        loop {
            let viewed: std::collections::HashSet<String> = a.clients.lock().unwrap().values().filter_map(|c| c.viewing.clone()).collect();
            let (pages, variables) = {
                let store = a.store.lock().unwrap();
                (store.config.pages.iter().filter(|p| viewed.contains(&p.id)).cloned().collect::<Vec<_>>(), store.config.variables.clone())
            };
            let mut fresh = collect_live(&a, &pages, &variables).await;
            // Graph-Buttons bekommen ihren Verlauf mit.
            for b in pages.iter().flat_map(|p| &p.buttons).filter(|b| !b.live.graph.is_empty()) {
                if let (Some(entry), Some(series)) = (fresh.get_mut(&b.id), a.graphs.history(&b.id)) {
                    entry["series"] = series;
                }
            }
            // Nur Werte von Buttons angezeigter Seiten behalten (gelöschte und nicht mehr angezeigte fallen raus).
            let shown: std::collections::HashSet<&str> = pages.iter().flat_map(|p| &p.buttons).map(|b| b.id.as_str()).collect();
            let live = {
                let mut live = a.live.lock().unwrap();
                live.extend(fresh);
                live.retain(|k, _| shown.contains(k.as_str()));
                live.clone()
            };
            send_live(&a, &pages, &live);
            if !a.live_again.swap(false, Ordering::SeqCst) {
                break;
            }
        }
        a.live_busy.store(false, Ordering::SeqCst);
    });
}

// ---------- Koppeln per Bestätigung am PC ----------

// Die App fragt an, am PC wird bestätigt, erst dann bekommt die App den Schlüssel.
fn pair_request(app: &App, device: &Value, ip: String) -> ApiResult {
    let id = new_id("pr_");
    let name: String = device.as_str().unwrap_or(t!("server.phone")).chars().filter(|c| c.is_alphanumeric() || " ._()-".contains(*c)).take(60).collect();
    let name = if name.is_empty() { t!("server.phone").to_string() } else { name };
    {
        let mut pairings = app.pairings.lock().unwrap();
        pairings.retain(|_, p| p.created.elapsed() <= PAIR_TIMEOUT && p.ip != ip);
        if pairings.len() >= 5 {
            return Err(HttpError::new(429, t!("server.tooManyOpenRequests")));
        }
        pairings.insert(id.clone(), Pairing { device: name.clone(), ip: ip.clone(), created: Instant::now(), state: "pending" });
    }
    app.log.info(t!("server.device"), t!("server.wantsToConnect", name, ip));
    app.broadcast(Some(Role::Editor), &json!({ "type": "pair-request", "id": id, "device": name, "ip": ip }));
    if let Some(cb) = &app.on_pair_request {
        cb(PairRequest { id: id.clone(), device: name, ip });
    }
    Ok(json!({ "id": id, "expiresIn": PAIR_TIMEOUT.as_secs() }))
}

fn pair_status(app: &App, id: &str) -> Value {
    let mut pairings = app.pairings.lock().unwrap();
    let Some(p) = pairings.get(id).filter(|p| p.created.elapsed() <= PAIR_TIMEOUT) else { return json!({ "state": "expired" }) };
    if p.state == "approved" {
        pairings.remove(id);
        let store = app.store.lock().unwrap();
        return json!({ "state": "approved", "token": store.settings().token, "pin": !store.settings().pin.is_empty() });
    }
    json!({ "state": p.state })
}

fn is_loopback(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()),
    }
}

fn client_ip(addr: &SocketAddr) -> String {
    match addr.ip() {
        IpAddr::V6(ip) => ip.to_ipv4_mapped().map_or_else(|| ip.to_string(), |v4| v4.to_string()),
        ip => ip.to_string(),
    }
}

/** QR-Code als SVG (dunkel auf weiß, schmaler Rand). */
fn qr_svg(text: &str) -> Option<String> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let w = code.width();
    let size = w + 2;
    let mut d = String::new();
    for (i, c) in code.to_colors().iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let _ = write!(d, "M{} {}h1v1h-1z", i % w + 1, i / w + 1);
        }
    }
    Some(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}" shape-rendering="crispEdges"><path fill="#ffffff" d="M0 0h{size}v{size}H0z"/><path fill="#0d0f14" d="{d}"/></svg>"##
    ))
}

const ICON_TYPES: [(&str, &str); 5] = [("image/png", "png"), ("image/jpeg", "jpg"), ("image/gif", "gif"), ("image/webp", "webp"), ("image/svg+xml", "svg")];

// Diese Routen darf auch ein gekoppeltes Handy aufrufen, alles andere nur der lokale Editor.
const DEVICE_ROUTES: [&str; 3] = ["GET /api/state", "POST /api/press", "GET /api/buttons"];
// Zum Koppeln braucht ein Handy noch keinen Schlüssel (nur WLAN-Zugriff muss erlaubt sein).
const PAIR_ROUTES: [&str; 3] = ["GET /api/hello", "POST /api/pair/request", "GET /api/pair/status"];

const ROUTES: [&str; 62] = [
    "GET /api/buttons",
    "GET /api/plugins",
    "GET /api/plugins/config",
    "POST /api/plugins/advanced",
    "GET /api/plugins/market",
    "GET /api/plugins/readme",
    "GET /api/looks",
    "GET /api/usb",
    "POST /api/plugins/preview",
    "POST /api/plugins/install",
    "POST /api/plugins/dev",
    "POST /api/plugins/reload",
    "POST /api/plugins/enable",
    "POST /api/plugins/settings",
    "POST /api/plugins/remove",
    "GET /api/iconpacks",
    "POST /api/iconpacks",
    "POST /api/iconpacks/update",
    "POST /api/iconpacks/delete",
    "POST /api/iconpacks/add",
    "POST /api/iconpacks/remove-icon",
    "POST /api/iconpacks/import",
    "POST /api/iconpacks/icon",
    "POST /api/profiles/import",
    "POST /api/plugins/import-page",
    "POST /api/save-file",
    "POST /api/web-check",
    "GET /api/state",
    "PUT /api/pages",
    "PUT /api/settings",
    "POST /api/press",
    "POST /api/test-actions",
    "PUT /api/variables",
    "POST /api/variables/delete",
    "POST /api/overrides/reset",
    "GET /api/logs",
    "POST /api/logs/clear",
    "GET /api/backups",
    "POST /api/backups",
    "POST /api/backups/restore",
    "POST /api/backups/delete",
    "GET /api/info",
    "POST /api/steam/pages",
    "POST /api/obs/autoconfig",
    "POST /api/obs/test",
    "GET /api/hello",
    "POST /api/firewall/open",
    "POST /api/pair/request",
    "GET /api/pair/status",
    "POST /api/pair/answer",
    "GET /api/discord/status",
    "POST /api/discord/connect",
    "POST /api/discord/disconnect",
    "GET /api/twitch/status",
    "POST /api/twitch/connect",
    "POST /api/twitch/cancel",
    "POST /api/twitch/disconnect",
    "GET /api/apps",
    "GET /api/pairing",
    "GET /api/icons",
    "POST /api/icons",
    "GET /api/options/*",
];

/** Sprache der Oberfläche (de oder en). */
fn lang(app: &App) -> String {
    app.store.lock().unwrap().settings().language.clone()
}

fn str_of<'a>(body: &'a Value, key: &str) -> &'a str {
    body[key].as_str().unwrap_or("")
}

fn bad(e: impl std::fmt::Display) -> HttpError {
    HttpError::new(400, e.to_string())
}

/** Fehler, die der Nutzer beheben kann (OBS aus, Twitch abgelehnt …), ohne Eintrag als Serverfehler. */
fn user_error(e: anyhow::Error) -> HttpError {
    HttpError::new(500, format!("{e:#}"))
}

// ---------- HTTP-Routen ----------

async fn api(app: &Arc<App>, key: &str, path: &str, who: Who, body: Value, query: &HashMap<String, String>, ip: String) -> ApiResult {
    match key {
        "GET /api/state" => {
            let store = app.store.lock().unwrap();
            Ok(if who == Who::Local { app.editor_state(&store) } else { app.deck_state(&store) })
        }
        "PUT /api/pages" => {
            let mut store = app.store.lock().unwrap();
            if !body["revision"].is_null() && body["revision"].as_u64() != Some(store.revision) {
                return Err(HttpError::new(409, t!("server.pagesWereChangedIn")));
            }
            store.set_project(&body).map_err(bad)?;
            Ok(json!({ "ok": true, "revision": store.revision }))
        }
        "PUT /api/settings" => {
            let r = {
                let mut store = app.store.lock().unwrap();
                store.set_settings(&body);
                json!({ "ok": true, "settings": store.public_settings(), "pairingUrls": app.pairing_urls(&store) })
            };
            if body["obs"].is_object() {
                app.obs.reset().await;
            }
            Ok(r)
        }
        // Leichte Liste für Widget, Schnellzugriff und NFC der App: Seiten mit Buttons (ID, Titel, Emoji).
        "GET /api/buttons" => {
            let store = app.store.lock().unwrap();
            Ok(json!(store.config.pages.iter().map(|p| json!({
                "id": p.id,
                "name": p.name,
                "buttons": p.buttons.iter().filter(|b| !["display", "web", "slider"].contains(&b.mode.as_str())).map(|b| json!({
                    "id": b.id,
                    "title": b.title,
                    "emoji": if b.icon.kind == "emoji" { b.icon.value.clone() } else { String::new() },
                })).collect::<Vec<_>>(),
            })).filter(|p| p["buttons"].as_array().is_some_and(|b| !b.is_empty())).collect::<Vec<_>>()))
        }
        "POST /api/press" => Ok(press(app, str_of(&body, "pageId"), str_of(&body, "buttonId"), body["double"] == true).await),
        "POST /api/test-actions" => {
            let actions = action_list(&body["actions"]);
            // Test aus einer Vorlage: deren Eingaben mit den Standardwerten belegen ({Eingabe.Name}).
            let mut event_data = serde_json::Map::new();
            if let Some(flow) = body["flowId"].as_str().and_then(|id| app.store.lock().unwrap().config.flows.iter().find(|f| f.id == id).cloned()) {
                for input in &flow.inputs {
                    for prefix in ["Eingabe", "Input"] {
                        event_data.insert(format!("{prefix}.{}", input.name), json!(input.default));
                    }
                }
            }
            Ok(run_with(app, &actions, RunOpts { button_id: body["buttonId"].as_str().map(String::from), event_data, ..Default::default() }, "Test").await)
        }

        // Werte
        "PUT /api/variables" => {
            let mut store = app.store.lock().unwrap();
            store.set_variable(str_of(&body, "name"), &body["value"]).map_err(|e| HttpError::new(500, e.to_string()))?;
            Ok(json!({ "ok": true, "variables": store.config.variables }))
        }
        "POST /api/variables/delete" => {
            let mut store = app.store.lock().unwrap();
            store.delete_variable(str_of(&body, "name"));
            Ok(json!({ "ok": true, "variables": store.config.variables }))
        }
        "POST /api/overrides/reset" => {
            app.store.lock().unwrap().reset_overrides();
            Ok(json!({ "ok": true }))
        }

        // Logs
        "GET /api/logs" => Ok(serde_json::to_value(app.log.entries()).unwrap_or_default()),
        "POST /api/logs/clear" => {
            app.log.clear();
            Ok(json!({ "ok": true }))
        }

        // Backups
        "GET /api/backups" => Ok(json!(app.backups.list())),
        "POST /api/backups" => {
            let note = body["note"].as_str().filter(|n| !n.is_empty()).unwrap_or("manuell");
            let name = {
                let store = app.store.lock().unwrap();
                app.backups.create(&store.config, note, store.settings().backups.keep)?
            };
            app.log.info("Backup", format!("Sicherung angelegt: {name}"));
            Ok(json!({ "ok": true, "name": name, "list": app.backups.list() }))
        }
        "POST /api/backups/restore" => {
            let (cfg, from) = match (&body["config"], body["name"].as_str()) {
                (c, _) if !c.is_null() => (c.clone(), t!("server.uploadedFile").to_string()),
                (_, Some(name)) => (app.backups.read(name)?, name.to_string()),
                _ => return Err(HttpError::new(404, t!("common.unknownBackup"))),
            };
            if !cfg["pages"].is_array() {
                return Err(HttpError::new(400, t!("server.thisIsNotTouchdeck")));
            }
            {
                let mut store = app.store.lock().unwrap();
                let keep = store.settings().backups.keep;
                let name = app.backups.create(&store.config, "vor Wiederherstellung", keep)?;
                app.log.info("Backup", format!("Sicherung angelegt: {name}"));
                store.restore(&cfg).map_err(bad)?;
            }
            app.log.info("Backup", format!("Wiederhergestellt: {from}"));
            Ok(json!({ "ok": true, "list": app.backups.list() }))
        }
        "POST /api/backups/delete" => {
            app.backups.remove(str_of(&body, "name"))?;
            Ok(json!({ "ok": true, "list": app.backups.list() }))
        }

        // Information
        "GET /api/info" => {
            let devices = app.device_count();
            let device_list = app.device_list();
            let (twitch, discord) = (app.twitch.status(), app.discord.status());
            let obs_connected = app.obs.connected();
            let store = app.store.lock().unwrap();
            let c = &store.config;
            let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
            Ok(json!({
                "version": VERSION,
                "platform": format!("Linux {}", release.trim()),
                "host": linux::hostname(),
                "dataDir": store.data_dir,
                "uptime": app.started.elapsed().as_secs(),
                "lan": linux::lan_addresses(),
                "port": app.port,
                "devices": devices,
                "deviceList": device_list,
                "obsConnected": obs_connected,
                "twitch": twitch,
                "discord": discord,
                "counts": {
                    "pages": c.pages.len(),
                    "buttons": c.pages.iter().map(|p| p.buttons.len()).sum::<usize>(),
                    "flows": c.flows.len(),
                    "events": c.events.len(),
                    "values": c.variables.len(),
                },
            }))
        }

        // Integrationen
        "POST /api/steam/pages" => {
            let (created, updated) = refresh_steam_pages(app).await?;
            let store = app.store.lock().unwrap();
            Ok(json!({ "ok": true, "pages": store.config.pages, "revision": store.revision, "created": created, "updated": updated }))
        }
        "POST /api/obs/autoconfig" => {
            let local = obs::read_local_config().map_err(user_error)?;
            let r = {
                let mut store = app.store.lock().unwrap();
                store.set_settings(&json!({ "obs": { "host": "127.0.0.1", "port": local["port"], "password": local["password"] } }));
                json!({ "ok": true, "settings": store.public_settings(), "serverEnabled": local["serverEnabled"], "flatpak": local["flatpak"] })
            };
            app.obs.reset().await;
            Ok(r)
        }
        "POST /api/obs/test" => {
            // Mit den eingegebenen Daten testen, ohne sie vorher zu speichern.
            let mut s = app.store.lock().unwrap().settings().obs.clone();
            if let Some(h) = body["host"].as_str() {
                s.host = h.into();
            }
            if let Some(p) = body["port"].as_u64() {
                s.port = p as u16;
            }
            if let Some(p) = body["password"].as_str() {
                s.password = p.into();
            }
            let probe = ObsClient::with_settings(Box::new(move || s.clone()));
            let scenes = probe.scenes().await.map_err(user_error)?;
            let v = probe.call0("GetVersion").await.map_err(user_error)?;
            probe.reset().await;
            Ok(json!({ "ok": true, "version": v["obsVersion"], "scenes": scenes.len() }))
        }
        "GET /api/hello" => Ok(json!({ "app": "touchdeck", "name": linux::hostname(), "version": VERSION, "port": app.port, "language": lang(app) })),
        "POST /api/firewall/open" => {
            linux::open_firewall(app.port).await.map_err(user_error)?;
            app.store.lock().unwrap().set_settings(&json!({ "firewallOpened": true }));
            app.log.info("Server", t!("server.firewallOpenedForHome"));
            Ok(json!({ "ok": true }))
        }
        "POST /api/pair/request" => pair_request(app, &body["device"], ip),
        "GET /api/pair/status" => Ok(pair_status(app, query.get("id").map_or("", String::as_str))),
        "POST /api/pair/answer" => {
            let allow = body["allow"] == true;
            let device = {
                let mut pairings = app.pairings.lock().unwrap();
                let p = pairings.get_mut(str_of(&body, "id")).ok_or_else(|| HttpError::new(404, t!("server.requestHasExpired")))?;
                p.state = if allow { "approved" } else { "denied" };
                p.device.clone()
            };
            app.log.info(t!("server.device"), format!("{device}: {}", if allow { t!("server.allowed") } else { t!("server.denied") }));
            Ok(json!({ "ok": true }))
        }
        "GET /api/discord/status" => Ok(app.discord.status()),
        "POST /api/discord/connect" => {
            app.store.lock().unwrap().set_settings(&json!({ "discord": { "clientId": str_of(&body, "clientId"), "clientSecret": str_of(&body, "clientSecret") } }));
            app.discord.login().await.map_err(user_error)?;
            app.events.reload(app);
            Ok(app.discord.status())
        }
        "POST /api/discord/disconnect" => {
            app.discord.disconnect().await;
            Ok(app.discord.status())
        }
        "GET /api/twitch/status" => Ok(app.twitch.status()),
        "POST /api/twitch/connect" => {
            if let Some(id) = body["clientId"].as_str() {
                app.store.lock().unwrap().set_settings(&json!({ "twitch": { "clientId": id } }));
            }
            app.twitch.start_device_login().await.map_err(user_error)
        }
        "POST /api/twitch/cancel" => {
            app.twitch.cancel_login();
            Ok(app.twitch.status())
        }
        "POST /api/twitch/disconnect" => {
            app.twitch.disconnect();
            Ok(app.twitch.status())
        }
        "GET /api/apps" => Ok(json!(tokio::task::spawn_blocking(linux::list_apps).await.unwrap_or_default())),
        "GET /api/pairing" => {
            let (urls, lan_enabled, pin, firewall_opened) = {
                let store = app.store.lock().unwrap();
                let s = store.settings();
                (app.pairing_urls(&store), s.lan_enabled, !s.pin.is_empty(), s.firewall_opened == Some(true))
            };
            let lan = linux::lan_addresses();
            let qr = urls.first().and_then(|u| qr_svg(u));
            let apk_url = lan.first().filter(|_| app.apk.is_file()).map(|ip| format!("http://{ip}:{}/app/TouchDeck.apk", app.port));
            let apk_qr = apk_url.as_deref().and_then(qr_svg);
            let subnet = lan.first().map(|ip| format!("{}.0/24", ip.rsplit_once('.').map_or(ip.as_str(), |(net, _)| net)));
            let firewall = match subnet {
                Some(net) if !firewall_opened && linux::firewall_active().await => Some(format!(
                    "sudo ufw allow from {net} to any port {} proto tcp && sudo ufw allow from {net} to any port 8769 proto udp",
                    app.port
                )),
                _ => None,
            };
            Ok(json!({ "urls": urls, "qr": qr, "lanEnabled": lan_enabled, "pin": pin, "apkUrl": apk_url, "apkQr": apk_qr, "firewall": firewall }))
        }

        // Bilder
        "GET /api/icons" => {
            let store = app.store.lock().unwrap();
            let mut names: Vec<String> = std::fs::read_dir(&store.icon_dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| !n.starts_with('.'))
                .collect();
            names.sort();
            Ok(json!(names))
        }
        "POST /api/icons" => {
            let data = str_of(&body, "data");
            let parsed = data.strip_prefix("data:").and_then(|d| d.split_once(";base64,"));
            let ext = parsed.and_then(|(mime, _)| ICON_TYPES.iter().find(|(m, _)| *m == mime).map(|(_, e)| *e));
            let (Some((_, b64)), Some(ext)) = (parsed, ext) else { return Err(HttpError::new(400, t!("server.onlyPngJpgGif"))) };
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| HttpError::new(400, t!("server.imageIsDamaged")))?;
            let name = format!("{}.{ext}", new_id("i_"));
            let dir = app.store.lock().unwrap().icon_dir.clone();
            std::fs::write(dir.join(&name), bytes).map_err(|e| HttpError::from(anyhow::Error::from(e)))?;
            Ok(json!({ "name": name }))
        }
        // Plugins
        "GET /api/plugins" => Ok(json!(app.plugins.list(&lang(app)))),
        "GET /api/plugins/config" => Ok(json!({ "advanced": app.plugins.advanced(), "confirm": crate::core::plugins::advanced_confirm(&lang(app)) })),
        "POST /api/plugins/advanced" => {
            app.plugins.set_advanced(app, body["enabled"] == true, str_of(&body, "confirm")).map_err(bad)?;
            Ok(json!({ "ok": true, "advanced": app.plugins.advanced(), "plugins": app.plugins.list(&lang(app)) }))
        }
        "GET /api/usb" => Ok(app.usb.lock().unwrap().clone()),
        "GET /api/looks" => {
            // Designs aus Plugins und Icon-Paketen für den Reiter „Aussehen“.
            let mut looks = app.plugins.looks(&lang(app));
            for p in app.iconpacks.list() {
                for l in p["looks"].as_array().into_iter().flatten() {
                    let mut l = l.clone();
                    l["source"] = p["name"].clone();
                    l["key"] = json!(format!("pack:{}:{}", p["id"].as_str().unwrap_or(""), l["id"].as_str().unwrap_or("")));
                    looks.push(l);
                }
            }
            Ok(json!(looks))
        }
        "GET /api/plugins/readme" => app.plugins.readme(query.get("repo").map_or("", String::as_str), &lang(app)).await.map_err(user_error),
        "GET /api/plugins/market" => app.plugins.market(&lang(app), query.contains_key("fresh")).await.map_err(user_error),
        "POST /api/plugins/preview" => app.plugins.preview(str_of(&body, "repo"), &lang(app)).await.map_err(user_error),
        "POST /api/plugins/install" => {
            let id = app.plugins.install(app, str_of(&body, "repo"), str_of(&body, "digest"), body["acceptRisk"] == true).await.map_err(user_error)?;
            Ok(json!({ "ok": true, "id": id, "plugins": app.plugins.list(&lang(app)) }))
        }
        "POST /api/plugins/dev" => {
            let id = if body["data"].is_string() {
                // ZIP-Datei aus dem Editor (Testmodus): als data-URL hochgeladen.
                let data = str_of(&body, "data");
                let b64 = data.split_once(";base64,").map_or(data, |(_, b)| b);
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| HttpError::new(400, t!("server.fileIsDamaged")))?;
                app.plugins.install_zip(app, &bytes).map_err(bad)?
            } else {
                app.plugins.install_local(app, str_of(&body, "path")).map_err(bad)?
            };
            Ok(json!({ "ok": true, "id": id, "plugins": app.plugins.list(&lang(app)) }))
        }
        "POST /api/plugins/reload" => {
            app.plugins.reload(app, str_of(&body, "id")).map_err(bad)?;
            Ok(json!({ "ok": true, "plugins": app.plugins.list(&lang(app)) }))
        }
        "POST /api/plugins/enable" => {
            app.plugins.set_enabled(app, str_of(&body, "id"), body["enabled"] == true).map_err(bad)?;
            Ok(json!({ "ok": true, "plugins": app.plugins.list(&lang(app)) }))
        }
        "POST /api/plugins/settings" => {
            app.plugins.set_settings(app, str_of(&body, "id"), &body["settings"]).map_err(bad)?;
            Ok(json!({ "ok": true, "plugins": app.plugins.list(&lang(app)) }))
        }
        "POST /api/plugins/remove" => {
            app.plugins.remove(app, str_of(&body, "id")).map_err(bad)?;
            Ok(json!({ "ok": true, "plugins": app.plugins.list(&lang(app)) }))
        }

        // Icon-Pakete
        "GET /api/iconpacks" => Ok(json!(app.iconpacks.list())),
        "POST /api/iconpacks" => Ok(json!({ "ok": true, "id": app.iconpacks.create(&body).map_err(bad)?, "packs": app.iconpacks.list() })),
        "POST /api/iconpacks/update" => {
            app.iconpacks.update(str_of(&body, "id"), &body).map_err(bad)?;
            Ok(json!({ "ok": true, "packs": app.iconpacks.list() }))
        }
        "POST /api/iconpacks/delete" => {
            app.iconpacks.delete(str_of(&body, "id")).map_err(bad)?;
            Ok(json!({ "ok": true, "packs": app.iconpacks.list() }))
        }
        "POST /api/iconpacks/add" => {
            let saved = app.iconpacks.add_icons(str_of(&body, "id"), &body["files"]).map_err(bad)?;
            Ok(json!({ "ok": true, "saved": saved, "packs": app.iconpacks.list() }))
        }
        "POST /api/iconpacks/icon" => {
            app.iconpacks.set_icon_info(str_of(&body, "id"), str_of(&body, "file"), &body).map_err(bad)?;
            Ok(json!({ "ok": true, "packs": app.iconpacks.list() }))
        }
        "POST /api/iconpacks/remove-icon" => {
            app.iconpacks.remove_icon(str_of(&body, "id"), str_of(&body, "file")).map_err(bad)?;
            Ok(json!({ "ok": true, "packs": app.iconpacks.list() }))
        }
        "POST /api/iconpacks/import" => {
            let data = str_of(&body, "data");
            let b64 = data.split_once(";base64,").map_or(data, |(_, b)| b);
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| HttpError::new(400, t!("server.fileIsDamaged")))?;
            let fallback = str_of(&body, "fileName").trim_end_matches(".tdicons").trim_end_matches(".zip").to_string();
            let imported = app.iconpacks.import(&bytes, &fallback).map_err(bad)?;
            let count: usize = imported.iter().map(|(_, n)| n).sum();
            app.log.info("Icons", t!("server.iconPackImported", count));
            let id = imported.first().map(|(id, _)| id.clone()).unwrap_or_default();
            Ok(json!({ "ok": true, "id": id, "count": count, "packCount": imported.len(), "packs": app.iconpacks.list() }))
        }

        "POST /api/web-check" => {
            // Darf die Seite eingebettet werden? (X-Frame-Options / CSP frame-ancestors)
            let url = str_of(&body, "url");
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Ok(json!({ "ok": false, "reason": "url" }));
            }
            let res = reqwest::Client::new().get(url).timeout(Duration::from_secs(8)).send().await;
            let Ok(res) = res else { return Ok(json!({ "ok": false, "reason": "unreachable" })) };
            let header = |k: &str| res.headers().get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
            let xfo = header("x-frame-options");
            let csp = header("content-security-policy");
            let ancestors = csp.split(';').map(str::trim).find(|d| d.starts_with("frame-ancestors")).unwrap_or("");
            let blocked = xfo.contains("deny") || xfo.contains("sameorigin") || (!ancestors.is_empty() && !ancestors.contains('*'));
            Ok(json!({ "ok": !blocked, "reason": if blocked { "blocked" } else { "" } }))
        }
        "POST /api/save-file" => {
            // Exporte direkt in den Download-Ordner schreiben (im Programmfenster gibt es keinen Browser-Download).
            let (file, bytes) = match str_of(&body, "kind") {
                "profile" => {
                    let pages: Vec<String> = body["pages"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect();
                    profiles::export(&app.store.lock().unwrap(), &pages, str_of(&body, "name")).map_err(bad)?
                }
                "iconpack" => app.iconpacks.export(str_of(&body, "id")).map_err(bad)?,
                "backup" => {
                    let name = str_of(&body, "name").to_string();
                    let cfg = app.backups.read(&name)?;
                    (name, serde_json::to_vec_pretty(&cfg).unwrap_or_default())
                }
                _ => return Err(HttpError::new(400, t!("server.unknownKind"))),
            };
            let path = linux::save_download(&file, &bytes).map_err(bad)?;
            linux::show_in_file_manager(&path);
            Ok(json!({ "ok": true, "path": path.to_string_lossy() }))
        }
        "POST /api/profiles/import" | "POST /api/plugins/import-page" => {
            // Profil-Datei aus dem Editor oder eine fertige Seite, die ein Plugin mitbringt.
            let bytes = if key == "POST /api/plugins/import-page" {
                app.plugins.page_bytes(str_of(&body, "id"), str_of(&body, "file")).map_err(bad)?
            } else {
                let data = str_of(&body, "data");
                let b64 = data.split_once(";base64,").map_or(data, |(_, b)| b);
                base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|_| HttpError::new(400, t!("server.fileIsDamaged")))?
            };
            let mut store = app.store.lock().unwrap();
            let r = profiles::import(&mut store, &bytes).map_err(bad)?;
            app.log.info("Profile", t!("profiles.imported", r.pages.len(), r.flows, r.values, r.images));
            Ok(json!({ "ok": true, "pages": store.config.pages, "flows": store.config.flows, "events": store.config.events, "values": store.config.values,
                       "revision": store.revision, "created": r.pages, "counts": { "pages": r.pages.len(), "flows": r.flows, "events": r.events, "values": r.values, "images": r.images } }))
        }

        "GET /api/options/*" => {
            let name = path.trim_start_matches("/api/options/");
            match option_source(app, name).await {
                None => Err(HttpError::new(404, "Unbekannte Liste")),
                Some(Ok(v)) => Ok(v),
                Some(Err(e)) => Err(HttpError::new(502, format!("{e:#}"))),
            }
        }
        _ => Err(HttpError::new(404, "Unbekannter Aufruf")),
    }
}

async fn handle(State(app): State<Arc<App>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, req: Request) -> Response {
    let uri = req.uri().clone();
    let method = req.method().clone();
    let headers = req.headers().clone();
    let query: HashMap<String, String> = Query::try_from_uri(&uri).map(|Query(q)| q).unwrap_or_default();
    let who = app.auth(&addr, &query, &headers);
    let path = percent_encoding::percent_decode_str(uri.path()).decode_utf8_lossy().into_owned();

    if path.starts_with("/api/") {
        let key = format!("{method} {path}");
        let route = if method == Method::GET && path.starts_with("/api/options/") { "GET /api/options/*" } else { key.as_str() };
        if !ROUTES.contains(&route) {
            return error_response(404, "Unbekannter Aufruf");
        }
        let lan = app.store.lock().unwrap().settings().lan_enabled;
        let pairing = PAIR_ROUTES.contains(&route) && (is_loopback(&addr) || lan);
        if !pairing && who == Some(Who::Pin) {
            return json_response(401, &json!({ "error": "PIN erforderlich", "pin": true }));
        }
        if !pairing && (who.is_none() || (who == Some(Who::Device) && !DEVICE_ROUTES.contains(&route))) {
            return error_response(401, t!("server.notPaired"));
        }
        // Schutz gegen fremde Webseiten: Schreibzugriffe brauchen einen eigenen Header (erzwingt CORS-Prüfung).
        if method != Method::GET && headers.get("x-touchdeck").is_none_or(|v| v != "1") {
            return error_response(403, t!("server.missingHeader"));
        }
        let body = if method == Method::GET {
            json!({})
        } else {
            let limit = if who == Some(Who::Local) && (route.starts_with("POST /api/iconpacks") || route == "POST /api/profiles/import") { MAX_UPLOAD } else { MAX_BODY };
            match axum::body::to_bytes(req.into_body(), limit).await {
                Err(_) => return error_response(413, t!("server.requestTooLarge")),
                Ok(b) if b.is_empty() => json!({}),
                Ok(b) => match serde_json::from_slice(&b) {
                    Ok(v) => v,
                    Err(_) => return error_response(400, t!("server.invalidJson")),
                },
            }
        };
        return match api(&app, route, &path, who.unwrap_or(Who::Device), body, &query, client_ip(&addr)).await {
            Ok(v) => json_response(200, &v),
            Err(err) => {
                if err.internal {
                    app.log.error("Server", format!("{key}: {err}"));
                }
                error_response(err.status, &err.message)
            }
        };
    }

    if path == "/" {
        let target = if who == Some(Who::Local) { "/editor/".to_string() } else { format!("/deck/{}", uri.query().map(|q| format!("?{q}")).unwrap_or_default()) };
        return Response::builder().status(302).header(header::LOCATION, target).body(Body::empty()).expect("gültige Antwort");
    }
    let index = |p: &str| if p.ends_with('/') { format!("{p}index.html") } else { p.to_string() };
    if path.starts_with("/editor/") {
        if who != Some(Who::Local) {
            return text_response(403, t!("server.editorIsOnlyReachable"));
        }
        return serve_file(&app.public_dir, &index(&path), &[]).await;
    }
    if path.starts_with("/deck/") || path.starts_with("/shared/") {
        return serve_file(&app.public_dir, &index(&path), &[]).await;
    }
    if let Some(rel) = path.strip_prefix("/icons/") {
        let dir = app.store.lock().unwrap().icon_dir.clone();
        // SVGs dürfen hier keine Skripte ausführen.
        let csp = ("content-security-policy", "default-src 'none'; style-src 'unsafe-inline'; sandbox".to_string());
        // Bilder aus Plugins („plugins/<id>/<pfad>“), nur Bilddateien.
        if let Some((id, file)) = rel.strip_prefix("plugins/").and_then(|r| r.split_once('/')) {
            let image = [".png", ".svg", ".jpg", ".jpeg", ".webp", ".gif"].iter().any(|e| file.to_lowercase().ends_with(e));
            return match app.plugins.dir_of(id).filter(|_| image && crate::core::plugins::safe_rel_path(file)) {
                Some(pdir) => serve_file(&pdir, file, &[csp]).await,
                None => text_response(404, t!("server.notFound")),
            };
        }
        return serve_file(&dir, rel, &[csp]).await;
    }
    let lan = app.store.lock().unwrap().settings().lan_enabled;
    if path == "/app/TouchDeck.apk" && (who.is_some() || lan) {
        let dir = app.apk.parent().unwrap_or(&app.apk).to_path_buf();
        let file = app.apk.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut res = serve_file(&dir, &file, &[("content-disposition", "attachment; filename=\"TouchDeck.apk\"".into())]).await;
        if res.status() == 200 {
            res.headers_mut().insert(header::CONTENT_TYPE, "application/vnd.android.package-archive".parse().expect("gültiger Typ"));
        }
        return res;
    }
    if path == "/profiles/export" && who == Some(Who::Local) {
        let pages: Vec<String> = query.get("pages").map(|p| p.split(',').filter(|s| !s.is_empty()).map(String::from).collect()).unwrap_or_default();
        let name = query.get("name").cloned().unwrap_or_default();
        let result = profiles::export(&app.store.lock().unwrap(), &pages, &name);
        return match result {
            Ok((file, bytes)) => Response::builder()
                .header(header::CONTENT_TYPE, "application/zip")
                .header(header::CACHE_CONTROL, "no-store")
                .header(header::CONTENT_DISPOSITION, format!("attachment; filename*=UTF-8''{}", percent_encoding::utf8_percent_encode(&file, percent_encoding::NON_ALPHANUMERIC)))
                .body(Body::from(bytes))
                .expect("gültige Antwort"),
            Err(err) => error_response(400, &err.to_string()),
        };
    }
    if path == "/iconpacks/export" && who == Some(Who::Local) {
        return match app.iconpacks.export(query.get("id").map_or("", String::as_str)) {
            Ok((file, bytes)) => Response::builder()
                .header(header::CONTENT_TYPE, "application/zip")
                .header(header::CACHE_CONTROL, "no-store")
                .header(header::CONTENT_DISPOSITION, format!("attachment; filename*=UTF-8''{}", percent_encoding::utf8_percent_encode(&file, percent_encoding::NON_ALPHANUMERIC)))
                .body(Body::from(bytes))
                .expect("gültige Antwort"),
            Err(err) => error_response(404, &err.to_string()),
        };
    }
    if path == "/backups/download" && who == Some(Who::Local) {
        let name = query.get("name").cloned().unwrap_or_default();
        return match app.backups.read(&name) {
            Ok(cfg) => Response::builder()
                .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
                .header(header::CACHE_CONTROL, "no-store")
                .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""))
                .body(Body::from(serde_json::to_string_pretty(&cfg).unwrap_or_default()))
                .expect("gültige Antwort"),
            Err(err) => error_response(err.status, &err.message),
        };
    }
    text_response(404, t!("server.notFound"))
}

// ---------- WebSocket: Bedienfelder und Editor ----------

async fn ws_upgrade(State(app): State<Arc<App>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, uri: Uri, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let query: HashMap<String, String> = Query::try_from_uri(&uri).map(|Query(q)| q).unwrap_or_default();
    let who = app.auth(&addr, &query, &headers);
    if !matches!(who, Some(Who::Local | Who::Device)) {
        return text_response(401, t!("server.notPaired"));
    }
    // Der Editor meldet sich mit role=editor; alles andere ist ein Bedienfeld.
    let role = if who == Some(Who::Local) && query.get("role").is_some_and(|r| r == "editor") { Role::Editor } else { Role::Device };
    upgrade.max_message_size(64 * 1024).on_upgrade(move |socket| ws_connection(app, socket, role, client_ip(&addr))).into_response()
}

async fn ws_connection(app: Arc<App>, socket: WebSocket, role: Role, ip: String) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let id = app.next_client.fetch_add(1, Ordering::Relaxed);
    app.clients.lock().unwrap().insert(id, Client { role, tx: tx.clone(), viewing: None, paused: None, sent_page: None, sent: serde_json::Map::new(), ip: ip.clone(), name: String::new() });
    if role == Role::Device {
        app.log.info(t!("server.device"), t!("server.connected", ip));
        app.events.fire(&app, "device", &json!({ "state": "connected", "Gerät": ip }));
    }
    let state = with_type(app.deck_state(&app.store.lock().unwrap()), "state");
    let _ = tx.send(Message::Text(state.to_string().into()));

    // Schreiben in eigener Aufgabe, damit ein langsames Handy niemanden aufhält.
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let close = matches!(msg, Message::Close(_));
            if sink.send(msg).await.is_err() || close {
                break;
            }
        }
    });

    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    heartbeat.tick().await;
    let mut alive = true;
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if !alive { break; }
                alive = false;
                let _ = tx.send(Message::Ping(Default::default()));
            }
            msg = stream.next() => {
                let Some(Ok(msg)) = msg else { break };
                match msg {
                    Message::Pong(_) => alive = true,
                    Message::Text(text) => {
                        alive = true;
                        let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
                        on_message(&app, id, &tx, msg);
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }
    let last_page = app.clients.lock().unwrap().remove(&id).and_then(|c| c.viewing);
    if let Some(page) = last_page {
        fire_page_view(&app, &page, "leave");
    }
    let held: Vec<String> = app.holds.lock().unwrap().keys().filter(|(c, _)| *c == id).map(|(_, b)| b.clone()).collect();
    for b in held {
        hold_end(&app, id, &b, false);
    }
    writer.abort();
    if role == Role::Device {
        app.log.info(t!("server.device"), t!("server.disconnected", ip));
        app.events.fire(&app, "device", &json!({ "state": "disconnected", "Gerät": ip }));
    }
}

/** Auslöser „Seite geöffnet / verlassen“ (Handy oder Tablet zeigt eine Seite bzw. blättert weg). */
fn fire_page_view(app: &Arc<App>, page_id: &str, state: &str) {
    let name = app.store.lock().unwrap().page(page_id).map(|p| p.name.clone()).unwrap_or_default();
    app.events.fire(app, "page_view", &json!({ "pageId": page_id, "state": state, "Seite": name }));
}

fn on_message(app: &Arc<App>, id: u64, tx: &mpsc::UnboundedSender<Message>, msg: Value) {
    let (page, button) = (str_of(&msg, "pageId").to_string(), str_of(&msg, "buttonId").to_string());
    match msg["type"].as_str() {
        Some("press") => {
            let (app, tx) = (app.clone(), tx.clone());
            tokio::spawn(async move {
                let result = press(&app, &page, &button, msg["double"] == true).await;
                let mut out = json!({ "type": "result", "reqId": msg["reqId"], "buttonId": msg["buttonId"] });
                if let (Some(o), Value::Object(r)) = (out.as_object_mut(), result) {
                    o.extend(r);
                }
                let _ = tx.send(Message::Text(out.to_string().into()));
            });
        }
        Some("hold") => {
            if msg["phase"] == "start" {
                hold_start(app, id, tx, &page, &button);
            } else {
                hold_end(app, id, &button, true);
            }
        }
        Some("slide") => slide(app, &page, &button, &msg["value"], if msg["part"] == 2 { 2 } else { 1 }, tx),
        Some("hello") => {
            // Name des Geräts (Android-App: Gerätename, Browser: aus der Kennung abgeleitet).
            let name: String = msg["name"].as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(60).collect();
            if let Some(c) = app.clients.lock().unwrap().get_mut(&id) {
                c.name = name.trim().to_string();
            }
        }
        Some("hidden") => {
            // Bedienfeld im Hintergrund: keine Live-Werte mehr abfragen und schicken.
            if let Some(c) = app.clients.lock().unwrap().get_mut(&id) {
                c.paused = c.viewing.take();
                c.sent_page = None;
            }
        }
        Some("view") if msg["pageId"].is_string() => {
            let steam = app.store.lock().unwrap().page(&page).is_some_and(|p| p.template == "steam");
            let previous = app.clients.lock().unwrap().get_mut(&id).and_then(|c| {
                let paused = c.paused.take();
                c.viewing.replace(page.clone()).or(paused)
            });
            if previous.as_deref() != Some(page.as_str()) {
                if let Some(old) = previous {
                    fire_page_view(app, &old, "leave");
                }
                fire_page_view(app, &page, "open");
            }
            refresh_live(app);
            if steam {
                check_steam_games(app);
            }
        }
        _ => {}
    }
}

// ---------- Start ----------

/** Startet den Server; liefert die App, sobald der Port offen ist. */
/** Steam-Seiten aus den installierten Spielen aktualisieren (fehlende Cover vorher von Steam laden).
 *  Vorhandene Spiel-Buttons und die Seiten-IDs bleiben erhalten. Liefert (Seiten-IDs, ob es vorher schon Steam-Seiten gab). */
async fn refresh_steam_pages(app: &Arc<App>) -> Result<(Vec<String>, bool), HttpError> {
    let (icon_dir, existing) = {
        let store = app.store.lock().unwrap();
        (store.icon_dir.clone(), store.config.pages.iter().filter(|p| p.template == "steam").cloned().collect::<Vec<_>>())
    };
    let games = tokio::task::spawn_blocking(steam::list_games).await.map_err(|e| HttpError::new(500, e.to_string()))?;
    let loaded = steam_pages::download_missing_covers(&icon_dir, &games).await;
    if loaded > 0 {
        app.log.info("Steam", t!("steam.coversDownloaded", loaded));
    }
    let had_pages = !existing.is_empty();
    let pages = tokio::task::spawn_blocking(move || steam_pages::build_steam_pages(&games, &icon_dir, &existing))
        .await
        .map_err(|e| HttpError::new(500, e.to_string()))??;
    let created: Vec<String> = pages.iter().map(|p| p.id.clone()).collect();
    app.store.lock().unwrap().replace_template_pages("steam", pages).map_err(|e| HttpError::new(400, e.to_string()))?;
    Ok((created, had_pages))
}

/** Spiele auf den Steam-Seiten (aus den „Spiel starten“-Aktionen). */
fn steam_games_on_pages(app: &App) -> BTreeSet<String> {
    let store = app.store.lock().unwrap();
    store.config.pages.iter().filter(|p| p.template == "steam")
        .flat_map(|p| p.buttons.iter())
        .flat_map(|b| b.actions.iter())
        .filter(|a| a.kind == "steam_game")
        .filter_map(|a| a.params.get("game").and_then(Value::as_str).map(String::from))
        .collect()
}

/** Wann zuletzt nach neuen oder deinstallierten Steam-Spielen geschaut wurde. */
static STEAM_CHECKED: Mutex<Option<Instant>> = Mutex::new(None);

/** Steam-Seiten aktuell halten: beim Start und beim Öffnen einer Steam-Seite, höchstens alle zwei Minuten. */
fn check_steam_games(app: &Arc<App>) {
    {
        let mut last = STEAM_CHECKED.lock().unwrap();
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(120)) {
            return;
        }
        *last = Some(Instant::now());
    }
    let a = app.clone();
    tokio::spawn(async move {
        let shown = steam_games_on_pages(&a);
        if shown.is_empty() {
            return;
        }
        let Ok(games) = tokio::task::spawn_blocking(steam::list_games).await else { return };
        let installed: BTreeSet<String> = games.into_iter().map(|(id, _)| id).collect();
        // Leere Liste: Steam gerade nicht lesbar – dann lieber nichts ändern.
        if installed.is_empty() || installed == shown {
            return;
        }
        let (added, removed) = (installed.difference(&shown).count(), shown.difference(&installed).count());
        match refresh_steam_pages(&a).await {
            Ok(_) => a.log.info("Steam", t!("steam.pagesUpdatedAutomatically", added, removed)),
            Err(err) => a.log.warn("Steam", err.message),
        }
    });
}

pub async fn start(store: Store, opts: Options) -> Result<Arc<App>> {
    let log = Arc::new(Log::new(store.data_dir.join("logs")));
    log.set_level(&store.settings().logs.level);
    let backups = Backups::new(&store.data_dir);
    let iconpacks = IconPacks::new(&store.icon_dir);
    let plugins = Plugins::new(&store.data_dir);
    let store = Arc::new(Mutex::new(store));
    let app = Arc::new(App {
        obs: ObsClient::new(store.clone()),
        twitch: TwitchClient::new(store.clone(), log.clone()),
        discord: DiscordClient::new(store.clone(), log.clone()),
        events: EventEngine::default(),
        iconpacks,
        timers: Arc::new(Timers::default()),
        plugins,
        backups,
        store,
        log,
        version: VERSION,
        port: opts.port,
        public_dir: opts.public_dir,
        apk: opts.apk,
        started: Instant::now(),
        clients: Mutex::new(HashMap::new()),
        next_client: AtomicU64::new(1),
        pairings: Mutex::new(HashMap::new()),
        holds: Mutex::new(HashMap::new()),
        sliders: Mutex::new(HashMap::new()),
        live: Mutex::new(serde_json::Map::new()),
        plugin_lives: Mutex::new(serde_json::Map::new()),
        graphs: Default::default(),
        usb: Mutex::new(json!({ "devices": [] })),
        live_busy: AtomicBool::new(false),
        pc_locked: AtomicBool::new(false),
        live_again: AtomicBool::new(false),
        on_pair_request: opts.on_pair_request,
    });

    let listener = tokio::net::TcpListener::bind((opts.host, opts.port)).await?;
    let router = Router::new().route("/ws", get(ws_upgrade)).fallback(handle).with_state(app.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await;
    });
    app.log.info("Server", t!("server.touchdeckIsRunningOn", opts.port, VERSION));
    check_steam_games(&app);

    // Wert geändert: Events sofort auslösen (im selben Ablauf, damit Endlosketten erkannt werden).
    let weak = Arc::downgrade(&app);
    app.store.lock().unwrap().set_hook(Box::new(move |change| {
        if change.what == "variables"
            && let Some(app) = weak.upgrade()
        {
            app.events.value_changed(&app, &change.detail);
        }
    }));

    // Gesammelte Änderungen (Werte, Umschalter, Aussehen) 1,5 s nach der ersten Änderung schreiben.
    let save = app.store.lock().unwrap().save_signal();
    let a = app.clone();
    tokio::spawn(async move {
        loop {
            save.notified().await;
            tokio::time::sleep(Duration::from_millis(1500)).await;
            a.store.lock().unwrap().flush();
        }
    });
    // Beim Abmelden/Herunterfahren (SIGTERM, SIGHUP) und Strg+C: Ungeschriebenes noch speichern.
    let a = app.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut hup), Ok(mut int)) = (signal(SignalKind::terminate()), signal(SignalKind::hangup()), signal(SignalKind::interrupt())) else { return };
        tokio::select! {
            _ = term.recv() => {}
            _ = hup.recv() => {}
            _ = int.recv() => {}
        }
        a.store.lock().unwrap().flush();
        std::process::exit(0);
    });

    // Änderungen an alle Verbindungen weitergeben.
    let mut changes = app.store.lock().unwrap().subscribe();
    let a = app.clone();
    tokio::spawn(async move {
        loop {
            let (change, detail) = match changes.recv().await {
                Ok(c) => (c.what, c.detail),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => ("pages", Value::Null),
                Err(_) => break,
            };
            match change {
                "settings" => a.log.set_level(&a.store.lock().unwrap().settings().logs.level),
                "pages" => a.events.reload(&a),
                _ => {}
            }
            if change == "security" {
                // Neuer Schlüssel, PIN geändert oder WLAN aus: gekoppelte Geräte trennen, damit sie sich neu anmelden.
                for c in a.clients.lock().unwrap().values().filter(|c| c.role == Role::Device) {
                    let _ = c.tx.send(Message::Close(Some(CloseFrame { code: 4001, reason: t!("server.settingsChanged").into() })));
                }
                continue;
            }
            // Laufzeit-Änderungen klein verschicken; den ganzen Zustand (alle Seiten) nur, wenn sich Seiten oder
            // Einstellungen ändern – sonst bauen die Geräte bei jedem Zählerstand alles neu.
            let msg = match change {
                "variables" => match detail["name"].as_str() {
                    Some(name) => {
                        // Live-Anzeigen mit Quelle „Wert“ sofort nachziehen (statt beim nächsten 2-s-Durchlauf).
                        let n = name.to_string();
                        refresh_quick(&a, move |b| b.live.source == "variable" && b.live.variable == n);
                        json!({ "type": "vars", "values": { name: detail["value"] } })
                    }
                    None => with_type(a.deck_state(&a.store.lock().unwrap()), "state"),
                },
                "toggles" => json!({ "type": "toggles", "toggles": a.store.lock().unwrap().config.toggles }),
                "overrides" => json!({ "type": "overrides", "overrides": a.store.lock().unwrap().config.overrides }),
                _ => {
                    // Das Gerät übernimmt die Live-Werte aus dem ganzen Zustand: danach wieder einmal alles schicken.
                    for c in a.clients.lock().unwrap().values_mut() {
                        c.sent_page = None;
                    }
                    with_type(a.deck_state(&a.store.lock().unwrap()), "state")
                }
            };
            a.broadcast(None, &msg);
        }
    });

    // Neue Log-Einträge sofort an offene Editoren schicken.
    let mut entries = app.log.subscribe();
    let a = app.clone();
    tokio::spawn(async move {
        loop {
            match entries.recv().await {
                Ok(entry) => a.broadcast(Some(Role::Editor), &json!({ "type": "log", "entry": entry })),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    // Verläufe für Graphen: jede Sekunde prüfen, welche Graph-Buttons fällig sind (ohne Graphen: nichts zu tun).
    let a = app.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(1));
        loop {
            t.tick().await;
            // Ohne verbundenes Gerät (oder Editor) misst niemand mit; die Lücke zeigt der Graph später als Pause.
            if a.clients.lock().unwrap().is_empty() {
                continue;
            }
            a.graphs.sample(&a).await;
        }
    });

    // USB-Verbindung: alle 5 Sekunden angeschlossene Android-Geräte per „adb reverse“ verbinden.
    let a = app.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(5));
        loop {
            t.tick().await;
            if !a.store.lock().unwrap().settings().usb {
                continue;
            }
            let status = match linux::usb_reverse(a.port).await {
                Ok(devices) => json!({ "devices": devices }),
                Err(e) => json!({ "devices": [], "error": e.to_string() }),
            };
            *a.usb.lock().unwrap() = status;
        }
    });

    // Bildschirmsperre am PC beobachten – nur wenn ein Auslöser „PC gesperrt“ oder „Bedienfeld mitsperren“ es braucht.
    let a = app.clone();
    tokio::spawn(async move {
        let needed = |a: &Arc<App>| {
            let store = a.store.lock().unwrap();
            store.settings().deck.lock_with_pc || store.uses_trigger("pc_lock")
        };
        loop {
            if !needed(&a) {
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
            if let Some(mut watch) = linux::lock_watch() {
                while let Some(locked) = watch.next().await {
                    if a.pc_locked.swap(locked, Ordering::Relaxed) != locked {
                        a.broadcast(Some(Role::Device), &json!({ "type": "pclock", "locked": locked }));
                        a.events.fire(&a, "pc_lock", &json!({ "state": if locked { "locked" } else { "unlocked" } }));
                    }
                    if !needed(&a) {
                        break;
                    }
                }
            }
            // dbus-monitor fehlt oder ist beendet: später erneut versuchen.
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    // Live-Werte alle 2 Sekunden aktualisieren.
    let a = app.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(2));
        loop {
            t.tick().await;
            refresh_live(&a);
        }
    });

    // Einmal am Tag automatisch sichern (beim Start und danach stündlich prüfen).
    let a = app.clone();
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            timer.tick().await;
            a.auto_backup();
        }
    });

    app.plugins.start_all(&app);
    EventEngine::start(&app);
    if app.twitch.connected() {
        let a = app.clone();
        tokio::spawn(async move {
            match a.twitch.validate().await {
                Ok(_) => a.twitch.sync_events(),
                Err(err) => a.log.warn("Twitch", err.to_string()),
            }
        });
    }
    discovery::start(app.clone());
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_nur_neue_punkte() {
        let old = json!({ "text": "1", "series": [{ "name": "", "values": [1, 2, 3] }] });
        // Verlauf voll: vorne fällt ein Punkt weg, hinten kommt einer dazu.
        let new = json!({ "text": "4", "series": [{ "name": "", "values": [2, 3, 4] }] });
        let d = graph_delta(&old, &new).unwrap();
        assert_eq!(d["seriesAppend"], json!([{ "values": [4], "len": 3 }]));
        assert_eq!(d["text"], "4");
        assert!(d.get("series").is_none());
        // Verlauf wächst noch.
        let grow = json!({ "series": [{ "name": "", "values": [1, 2, 3, 5] }] });
        assert_eq!(graph_delta(&old, &grow).unwrap()["seriesAppend"], json!([{ "values": [5], "len": 4 }]));
        // Andere Reihen: ganz schicken.
        let other = json!({ "series": [{ "name": "B", "values": [1] }] });
        assert!(graph_delta(&old, &other).is_none());
    }
}
