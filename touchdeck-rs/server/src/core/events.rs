// Events: „Wenn <Auslöser>, dann <Aktionen>“ – global (Bereich Events) und je Button („Bei Event“).
// Die Beschreibungen der Auslöser für den Editor stehen in catalog/triggers.json.
use crate::t;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use chrono::{Datelike, Local, Timelike};
use serde_json::{Map, Value, json};
use tokio::task::JoinHandle;

use crate::core::actions::{RunOpts, js_number, run_actions};
use crate::core::catalog;
use crate::core::store::{Action, Trigger, js_string};
use crate::integrations::steam;
use crate::server::App;
use crate::system::{linux, x11};

// Verkettung erkennen: ein Event, das ein Event auslöst, das ein Event auslöst …
tokio::task_local! {
    static CHAIN: u32;
}
const MAX_DEPTH: u32 = 8;

/** Wie tief die aktuelle Kette von Events ist (0 = direkt ausgelöst). */
pub fn current_depth() -> u32 {
    CHAIN.try_with(|d| *d).unwrap_or(0)
}

fn s(v: &Value) -> String {
    js_string(v)
}

fn compare(op: &str, actual: &Value, expected: &Value) -> bool {
    let (a, e) = (s(actual), s(expected));
    let num = |x: &str| js_number(Some(&json!(x)));
    match op {
        "eq" => a == e,
        "ne" => a != e,
        "gt" => num(&a) > num(&e),
        "lt" => num(&a) < num(&e),
        "contains" => a.contains(&e),
        _ => true, // „ändert sich“
    }
}

fn state_matches(want: &Value, actual: &Value) -> bool {
    let w = s(want);
    w.is_empty() || w == "any" || w == s(actual)
}

/** Leerer Parameter passt immer, sonst muss er gleich sein. */
fn empty_or_eq(p: &Value, d: &Value) -> bool {
    let w = s(p);
    w.is_empty() || w == s(d)
}

/** Passt ein Ereignis zu den Einstellungen eines Auslösers? */
pub fn trigger_matches(kind: &str, p: &Map<String, Value>, d: &Value) -> bool {
    let g = |k: &str| p.get(k).cloned().unwrap_or(Value::Null);
    let min = |k: &str| {
        let n = js_number(p.get(k));
        if n.is_nan() || n == 0.0 { 1.0 } else { n }
    };
    match kind {
        "value_change" => s(&g("name")) == s(&d["name"]) && compare(&s(&g("op")), &d["value"], &g("value")),
        "app_start" | "timer" | "time_of_day" | "file_change" | "obs_replay_saved" | "twitch_follow" | "twitch_ad" => true,
        "device" | "obs_stream" | "obs_record" | "obs_studio" | "obs_connection" | "twitch_stream" | "discord_voice" | "discord_channel" => state_matches(&g("state"), &d["state"]),
        "button_toggle" => empty_or_eq(&g("button"), &d["buttonId"]) && state_matches(&g("state"), &d["state"]),
        "page_view" => empty_or_eq(&g("page"), &d["pageId"]) && state_matches(&g("state"), &d["state"]),
        "timer_done" => empty_or_eq(&g("name"), &d["name"]),
        "hotkey" => x11::normalize_combo(&s(&g("keys"))) == s(&d["keys"]),
        "app_focus" => {
            let want = s(&g("app")).to_lowercase();
            want.is_empty() || s(&d["app"]).to_lowercase().contains(&want)
        }
        "audio_output" => empty_or_eq(&g("device"), &d["device"]),
        "audio_mute" => {
            let target = if s(&g("target")).is_empty() { "output".to_string() } else { s(&g("target")) };
            target == s(&d["target"]) && state_matches(&g("state"), &d["state"])
        }
        "steam_game" => empty_or_eq(&g("game"), &d["appid"]) && state_matches(&g("state"), &d["state"]),
        "obs_scene" | "obs_preview" => empty_or_eq(&g("scene"), &d["scene"]),
        "obs_mute" => empty_or_eq(&g("input"), &d["input"]) && state_matches(&g("state"), &d["state"]),
        "obs_source" => empty_or_eq(&g("item"), &d["item"]) && state_matches(&g("state"), &d["state"]),
        "obs_filter" => empty_or_eq(&g("filter"), &d["filter"]) && state_matches(&g("state"), &d["state"]),
        "obs_profile" => state_matches(&g("kind"), &d["kind"]),
        "twitch_sub" => {
            let kind = if d["kind"] == "gift_received" { json!("gift") } else { d["kind"].clone() };
            state_matches(&g("kind"), &kind)
        }
        "twitch_cheer" => d["bits"].as_f64().unwrap_or(0.0) >= min("min"),
        "twitch_raid" => d["viewers"].as_f64().unwrap_or(0.0) >= min("min"),
        "twitch_redeem" => {
            let want = s(&g("reward")).trim().to_lowercase();
            want.is_empty() || want == s(&d["reward"]).to_lowercase()
        }
        "twitch_chat" => {
            let msg = s(&d["text"]).trim().to_lowercase();
            let t = s(&g("text")).trim().to_lowercase();
            let mode = s(&g("mode"));
            let text_ok = mode == "any" || t.is_empty() || if mode == "contains" { msg.contains(&t) } else { msg == t || msg.starts_with(&format!("{t} ")) };
            let roles: &[&str] = match s(&g("who")).as_str() {
                "mod" => &["broadcaster", "moderator"],
                "sub" => &["broadcaster", "subscriber", "founder"],
                _ => &[],
            };
            text_ok && (roles.is_empty() || d["badges"].as_array().into_iter().flatten().any(|b| roles.contains(&s(b).as_str())))
        }
        // Plugin-Auslöser: jede ausgefüllte Einstellung muss zum gleichnamigen Feld des Ereignisses passen.
        k if k.starts_with("plugin:") => p.iter().all(|(key, want)| empty_or_eq(want, &d[key])),
        _ => false,
    }
}

fn known(kind: &str) -> bool {
    catalog::TRIGGER_TYPES.as_array().into_iter().flatten().any(|t| t["type"] == kind)
}

#[derive(Clone)]
struct Sub {
    key: String,
    name: String,
    trigger: Trigger,
    actions: Vec<Action>,
    cooldown: i64,
    button_id: Option<String>,
}

#[derive(Default)]
struct State {
    obs_connected: bool,
    running: Option<BTreeSet<String>>,
    sink: Option<String>,
    /// Zuletzt gesehene Stummschaltung von Ausgang und Eingang.
    muted: (Option<bool>, Option<bool>),
    /// Programm im Vordergrund (zuletzt gesehen), für „Programm im Vordergrund“.
    focused: Option<String>,
}

#[derive(Default)]
pub struct EventEngine {
    subs: Mutex<Vec<Sub>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    last_run: Mutex<HashMap<String, Instant>>,
    last_day: Mutex<HashMap<String, String>>,
    state: Mutex<State>,
    /** Stopp-Signal für den Thread mit den globalen Tastenkürzeln. */
    hotkeys: Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>,
}

impl EventEngine {
    /** Startet die dauerhaften Empfänger (OBS, Twitch, Discord), liest die Events ein und meldet „TouchDeck startet“. */
    pub fn start(app: &Arc<App>) {
        let a = app.clone();
        let mut obs = app.obs.subscribe();
        tokio::spawn(async move {
            loop {
                match obs.recv().await {
                    Ok((kind, data)) => {
                        if let Err(err) = on_obs(&a, &kind, &data).await {
                            a.log.warn("Events", format!("OBS-Ereignis {kind}: {err}"));
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
        for mut rx in [app.twitch.subscribe(), app.discord.subscribe()] {
            let a = app.clone();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok((kind, data)) => a.events.fire(&a, &kind, &data),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
        }
        app.events.reload(app);
        app.events.fire(app, "app_start", &json!({}));
    }

    /** Alle aktiven Events neu einlesen und Zeitgeber/Beobachter neu aufsetzen. */
    pub fn reload(&self, app: &Arc<App>) {
        self.stop();
        let subs = {
            let store = app.store.lock().unwrap();
            let c = &store.config;
            let mut subs: Vec<Sub> = c
                .events
                .iter()
                .filter(|e| e.enabled)
                .map(|e| Sub { key: e.id.clone(), name: e.name.clone(), trigger: e.trigger.clone(), actions: e.actions.clone(), cooldown: e.cooldown, button_id: None })
                .collect();
            // Logik der Seiten: „Seite geöffnet/verlassen“ ohne Seitenwahl meint diese Seite.
            for p in &c.pages {
                for e in p.events.iter().filter(|e| e.enabled) {
                    let mut trigger = e.trigger.clone();
                    if trigger.kind == "page_view" && trigger.params.get("page").and_then(Value::as_str).unwrap_or("").is_empty() {
                        trigger.params.insert("page".into(), Value::String(p.id.clone()));
                    }
                    subs.push(Sub { key: format!("{}:{}", p.id, e.id), name: t!("events.page", p.name), trigger, actions: e.actions.clone(), cooldown: 0, button_id: None });
                }
            }
            for b in c.pages.iter().flat_map(|p| &p.buttons) {
                for e in b.events.iter().filter(|e| e.enabled) {
                    subs.push(Sub {
                        key: format!("{}:{}", b.id, e.id),
                        name: t!("events.button", if b.title.is_empty() { &b.id } else { &b.title }),
                        trigger: e.trigger.clone(),
                        actions: e.actions.clone(),
                        cooldown: 0,
                        button_id: Some(b.id.clone()),
                    });
                }
            }
            subs
        };
        let has = |kind: &str| subs.iter().any(|s| s.trigger.kind == kind);
        let mut tasks = Vec::new();
        let every = |ms: u64, f: Box<dyn Fn() + Send + Sync>| {
            tokio::spawn(async move {
                let mut t = tokio::time::interval(Duration::from_millis(ms));
                t.tick().await;
                loop {
                    t.tick().await;
                    f();
                }
            })
        };

        for sub in subs.iter().filter(|s| s.trigger.kind == "timer") {
            let secs = js_number(sub.trigger.params.get("seconds"));
            let secs = if secs.is_nan() || secs == 0.0 { 60.0 } else { secs.max(1.0) };
            let (a, sub) = (app.clone(), sub.clone());
            tasks.push(every((secs * 1000.0) as u64, Box::new(move || a.events.run_sub(&a, &sub, &json!({})))));
        }
        if has("time_of_day") {
            let a = app.clone();
            tasks.push(every(15000, Box::new(move || a.events.check_time_of_day(&a))));
        }
        for sub in subs.iter().filter(|s| s.trigger.kind == "file_change") {
            let path = sub.trigger.params.get("path").map(js_string).unwrap_or_default();
            if path.is_empty() {
                continue;
            }
            let file = linux::expand_home(&path);
            let (a, sub) = (app.clone(), sub.clone());
            let mtime = |f: &std::path::Path| std::fs::metadata(f).and_then(|m| m.modified()).ok();
            let last: Mutex<Option<SystemTime>> = Mutex::new(mtime(&file));
            tasks.push(every(
                1500,
                Box::new(move || {
                    let now = mtime(&file);
                    let changed = std::mem::replace(&mut *last.lock().unwrap(), now) != now;
                    if changed {
                        a.events.run_sub(&a, &sub, &json!({ "Datei": file.to_string_lossy() }));
                    }
                }),
            ));
        }
        if subs.iter().any(|s| s.trigger.kind.starts_with("obs_")) {
            // OBS-Verbindung halten, damit Ereignisse ankommen; ohne OBS alle 10 s erneut versuchen.
            let a = app.clone();
            tasks.push(tokio::spawn(async move {
                let mut t = tokio::time::interval(Duration::from_secs(10));
                loop {
                    t.tick().await;
                    let ok = a.obs.connect().await.is_ok();
                    let was = std::mem::replace(&mut a.events.state.lock().unwrap().obs_connected, ok);
                    if ok && !was {
                        a.events.fire(&a, "obs_connection", &json!({ "state": "connected" }));
                    } else if !ok && was {
                        a.events.fire(&a, "obs_connection", &json!({ "state": "disconnected" }));
                    }
                }
            }));
        }
        if has("steam_game") {
            let a = app.clone();
            tasks.push(every(4000, Box::new(move || a.events.check_steam(&a))));
        }
        if subs.iter().any(|s| s.trigger.kind.starts_with("discord_")) && app.discord.configured() {
            // Verbindung zu Discord halten, damit Ereignisse ankommen (Discord kann auch später gestartet werden).
            let a = app.clone();
            tasks.push(tokio::spawn(async move {
                let mut t = tokio::time::interval(Duration::from_secs(10));
                loop {
                    t.tick().await;
                    let _ = a.discord.connect().await;
                }
            }));
        }
        if has("audio_output") {
            let a = app.clone();
            tasks.push(tokio::spawn(async move {
                let check = |a: Arc<App>| async move {
                    let Ok(devices) = linux::audio_devices(true).await else { return };
                    let sink = devices.into_iter().find(|d| d.is_default).map(|d| d.value);
                    let before = std::mem::replace(&mut a.events.state.lock().unwrap().sink, sink.clone());
                    if let (Some(b), Some(s)) = (before, sink)
                        && b != s
                    {
                        a.events.fire(&a, "audio_output", &json!({ "device": s, "Gerät": s }));
                    }
                };
                check(a.clone()).await;
                wait_audio(Duration::from_secs(4), || check(a.clone())).await;
            }));
        }
        if has("audio_mute") {
            let a = app.clone();
            tasks.push(tokio::spawn(async move {
                let check = |a: Arc<App>| async move {
                    for (target, dev) in [("output", linux::SINK), ("input", linux::SOURCE)] {
                        let Ok(v) = linux::get_volume(dev).await else { continue };
                        let before = {
                            let mut st = a.events.state.lock().unwrap();
                            let slot = if target == "output" { &mut st.muted.0 } else { &mut st.muted.1 };
                            slot.replace(v.muted)
                        };
                        if before.is_some_and(|b| b != v.muted) {
                            let state = if v.muted { "muted" } else { "unmuted" };
                            let label = if v.muted { "stumm" } else { "laut" };
                            a.events.fire(&a, "audio_mute", &json!({ "target": target, "state": state, "Status": label }));
                        }
                    }
                };
                check(a.clone()).await;
                wait_audio(Duration::from_secs(2), || check(a.clone())).await;
            }));
        }
        if has("app_focus") {
            // Fenster im Vordergrund jede Sekunde ansehen; nur bei einem Wechsel des Programms auslösen.
            let a = app.clone();
            tasks.push(tokio::spawn(async move {
                let mut t = tokio::time::interval(Duration::from_secs(1));
                loop {
                    t.tick().await;
                    let Ok(Some((class, title))) = x11::active_window().await else { continue };
                    let before = a.events.state.lock().unwrap().focused.replace(class.clone());
                    if before.is_some_and(|b| b != class) {
                        a.events.fire(&a, "app_focus", &json!({ "app": class, "Programm": class, "Titel": title }));
                    }
                }
            }));
        }
        let combos: Vec<String> = {
            let mut c: Vec<String> = subs.iter().filter(|s| s.trigger.kind == "hotkey").map(|s| x11::normalize_combo(&s.trigger.params.get("keys").map(js_string).unwrap_or_default())).filter(|c| !c.is_empty()).collect();
            c.sort();
            c.dedup();
            c
        };
        if !combos.is_empty() {
            // Globale Tastenkürzel in einem eigenen Thread (Xlib blockiert); stop() beendet ihn.
            let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            *self.hotkeys.lock().unwrap() = Some(flag.clone());
            let (a, rt) = (app.clone(), tokio::runtime::Handle::current());
            std::thread::spawn(move || {
                let _rt = rt.enter();
                // Der vorige Hörer gibt seine Tasten innerhalb von 100 ms frei; erst danach selbst belegen.
                std::thread::sleep(Duration::from_millis(150));
                let fire = |combo: &str| a.events.fire(&a, "hotkey", &json!({ "keys": combo, "Tasten": combo }));
                let busy = |combo: &str| a.log.warn("Events", t!("events.hotkeyBusy", combo));
                if let Err(err) = x11::listen_hotkeys(&combos, &flag, fire, busy) {
                    a.log.warn("Events", t!("events.hotkeysUnavailable", err));
                }
            });
        }
        let types: Vec<String> = subs.iter().map(|s| s.trigger.kind.clone()).collect();
        app.twitch.set_wanted(&types);
        app.log.debug("Events", t!("events.activeEvents", subs.len()));
        *self.subs.lock().unwrap() = subs;
        *self.tasks.lock().unwrap() = tasks;
    }

    /** Ein Ereignis ist eingetreten: alle passenden Events ausführen. */
    pub fn fire(&self, app: &Arc<App>, kind: &str, data: &Value) {
        self.fire_at(app, kind, data, current_depth());
    }

    /** Wie fire, aber mit bekannter Verkettungstiefe (Plugins laufen in eigenen Threads). */
    pub fn fire_at(&self, app: &Arc<App>, kind: &str, data: &Value, depth: u32) {
        let plugin = kind.starts_with("plugin:") && app.plugins.has_trigger(kind);
        if !plugin && !known(kind) {
            return;
        }
        // TouchDeck-Ereignisse auch an Plugins (td.on); eigene Plugin-Auslöser nicht, sonst drohen Schleifen.
        if !plugin {
            let mut d = data.clone();
            if let Some(o) = d.as_object_mut() {
                for (de, en) in crate::core::catalog::EVENT_ALIASES {
                    if let Some(v) = o.get(de).cloned() {
                        o.entry(en.to_string()).or_insert(v);
                    }
                }
            }
            app.plugins.broadcast_event(kind, &d);
        }
        let matching: Vec<Sub> = self.subs.lock().unwrap().iter().filter(|s| s.trigger.kind == kind && trigger_matches(kind, &s.trigger.params, data)).cloned().collect();
        for sub in matching {
            self.run_sub_at(app, &sub, data, depth);
        }
    }

    fn run_sub(&self, app: &Arc<App>, sub: &Sub, data: &Value) {
        self.run_sub_at(app, sub, data, current_depth());
    }

    fn run_sub_at(&self, app: &Arc<App>, sub: &Sub, data: &Value, depth: u32) {
        if depth >= MAX_DEPTH {
            app.log.warn("Events", t!("events.skippedEventsKeepTriggering", sub.name));
            return;
        }
        {
            let mut last = self.last_run.lock().unwrap();
            if sub.cooldown > 0 && last.get(&sub.key).is_some_and(|t| t.elapsed() < Duration::from_secs(sub.cooldown as u64)) {
                app.log.debug("Events", t!("events.isStillWaitingCooldown", sub.name));
                return;
            }
            last.insert(sub.key.clone(), Instant::now());
        }
        let mut event_data: Map<String, Value> = data.as_object().into_iter().flatten().map(|(k, v)| (format!("Event.{k}"), v.clone())).collect();
        // Englische Namen zusätzlich, damit {Event.User} genauso funktioniert wie {Event.Nutzer}.
        for (de, en) in catalog::EVENT_ALIASES {
            if let Some(v) = data.get(de) {
                event_data.entry(format!("Event.{en}")).or_insert_with(|| v.clone());
            }
        }
        let (a, sub) = (app.clone(), sub.clone());
        tokio::spawn(CHAIN.scope(depth + 1, async move {
            let opts = RunOpts { button_id: sub.button_id.clone(), event_data, flow_depth: 0 };
            let r = run_actions(&a, &sub.actions, opts).await;
            if r["ok"] == true {
                a.log.info("Events", t!("events.ran", sub.name, sub.trigger.kind));
            } else {
                a.log.error("Events", t!("events.format", sub.name, r["error"].as_str().unwrap_or("")));
            }
        }));
    }

    fn check_time_of_day(&self, app: &Arc<App>) {
        let now = Local::now();
        let hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
        let today = now.format("%Y-%m-%d").to_string();
        let weekend = now.weekday().number_from_monday() >= 6;
        let subs: Vec<Sub> = self.subs.lock().unwrap().iter().filter(|s| s.trigger.kind == "time_of_day").cloned().collect();
        for sub in subs {
            let p = &sub.trigger.params;
            // Mehrere Uhrzeiten mit Komma; „8:05“ gilt wie „08:05“.
            let time = p.get("time").map(js_string).unwrap_or_default();
            let hit = time.split([',', ';']).map(|t| t.trim()).any(|t| t == hhmm || format!("0{t}") == hhmm);
            let key = format!("{}@{hhmm}", sub.key);
            if !hit || self.last_day.lock().unwrap().get(&key) == Some(&today) {
                continue;
            }
            let days = p.get("days").map(js_string).unwrap_or_default();
            if (days == "weekdays" && weekend) || (days == "weekend" && !weekend) {
                continue;
            }
            if days == "pick" {
                let names = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
                let day = names[now.weekday().num_days_from_monday() as usize];
                if p.get(&format!("d_{day}")) != Some(&Value::Bool(true)) {
                    continue;
                }
            }
            let monthday = js_number(p.get("monthday"));
            if monthday >= 1.0 && monthday as u32 != now.day() {
                continue;
            }
            self.last_day.lock().unwrap().insert(key, today.clone());
            self.run_sub(app, &sub, &json!({ "Zeit": hhmm }));
        }
    }

    fn check_steam(&self, app: &Arc<App>) {
        let now = steam::running_games();
        let before = self.state.lock().unwrap().running.replace(now.clone());
        let Some(before) = before else { return };
        let games = steam::list_games();
        let name = |id: &str| games.iter().find(|(v, _)| v == id).map_or_else(|| id.to_string(), |(_, l)| l.clone());
        for id in now.difference(&before) {
            self.fire(app, "steam_game", &json!({ "appid": id, "state": "started", "Spiel": name(id) }));
        }
        for id in before.difference(&now) {
            self.fire(app, "steam_game", &json!({ "appid": id, "state": "stopped", "Spiel": name(id) }));
        }
    }

    /** Ein Wert hat sich geändert (vom Speicher gemeldet). */
    pub fn value_changed(&self, app: &Arc<App>, detail: &Value) {
        let Some(name) = detail["name"].as_str() else { return };
        let value = detail["value"].clone();
        let data = json!({ "name": name, "value": value, "Name": name, "Wert": if value.is_null() { json!("") } else { value.clone() }, "Alt": if detail["old"].is_null() { json!("") } else { detail["old"].clone() } });
        self.fire(app, "value_change", &data);
    }

    pub fn set_obs_connected(&self, on: bool) {
        self.state.lock().unwrap().obs_connected = on;
    }

    pub fn stop(&self) {
        if let Some(flag) = self.hotkeys.lock().unwrap().take() {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        for t in self.tasks.lock().unwrap().drain(..) {
            t.abort();
        }
    }
}

const OUTPUT_STATES: [(&str, &str); 4] = [
    ("OBS_WEBSOCKET_OUTPUT_STARTED", "started"),
    ("OBS_WEBSOCKET_OUTPUT_STOPPED", "stopped"),
    ("OBS_WEBSOCKET_OUTPUT_PAUSED", "paused"),
    ("OBS_WEBSOCKET_OUTPUT_RESUMED", "resumed"),
];

async fn on_obs(app: &Arc<App>, kind: &str, d: &Value) -> anyhow::Result<()> {
    let ev = &app.events;
    match kind {
        "CurrentProgramSceneChanged" => ev.fire(app, "obs_scene", &json!({ "scene": d["sceneName"], "Szene": d["sceneName"] })),
        "CurrentPreviewSceneChanged" => ev.fire(app, "obs_preview", &json!({ "scene": d["sceneName"], "Szene": d["sceneName"] })),
        "StreamStateChanged" | "RecordStateChanged" => {
            if let Some((_, state)) = OUTPUT_STATES.iter().find(|(k, _)| d["outputState"] == *k) {
                let trigger = if kind == "StreamStateChanged" { "obs_stream" } else { "obs_record" };
                ev.fire(app, trigger, &json!({ "state": state, "Status": state }));
            }
        }
        "ReplayBufferSaved" => ev.fire(app, "obs_replay_saved", &json!({ "Datei": d["savedReplayPath"] })),
        "InputMuteStateChanged" => {
            let state = if d["inputMuted"] == true { "muted" } else { "unmuted" };
            ev.fire(app, "obs_mute", &json!({ "input": d["inputName"], "state": state, "Quelle": d["inputName"] }));
        }
        "SceneItemEnableStateChanged" => {
            let r = app.obs.call("GetSceneItemSource", json!({ "sceneName": d["sceneName"], "sceneItemId": d["sceneItemId"] })).await?;
            let source = r["sourceName"].as_str().unwrap_or("");
            let scene = d["sceneName"].as_str().unwrap_or("");
            let state = if d["sceneItemEnabled"] == true { "shown" } else { "hidden" };
            ev.fire(app, "obs_source", &json!({ "item": format!("{scene}␟{source}"), "state": state, "Quelle": source, "Szene": scene }));
        }
        "SourceFilterEnableStateChanged" => {
            let (source, filter) = (d["sourceName"].as_str().unwrap_or(""), d["filterName"].as_str().unwrap_or(""));
            let state = if d["filterEnabled"] == true { "on" } else { "off" };
            ev.fire(app, "obs_filter", &json!({ "filter": format!("{source}␟{filter}"), "state": state, "Filter": filter, "Quelle": source }));
        }
        "StudioModeStateChanged" => ev.fire(app, "obs_studio", &json!({ "state": if d["studioModeEnabled"] == true { "on" } else { "off" } })),
        "CurrentProfileChanged" => ev.fire(app, "obs_profile", &json!({ "kind": "profile", "Name": d["profileName"] })),
        "CurrentSceneCollectionChanged" => ev.fire(app, "obs_profile", &json!({ "kind": "collection", "Name": d["sceneCollectionName"] })),
        "ExitStarted" => {
            ev.set_obs_connected(false);
            ev.fire(app, "obs_connection", &json!({ "state": "disconnected" }));
        }
        _ => {}
    }
    Ok(())
}

/**
 * Ruft `check` bei jeder Änderung am Ton auf (pactl subscribe). Ohne pactl – oder wenn es endet – wie
 * früher in festen Abständen (`every`); pactl wird jede Minute erneut versucht.
 */
async fn wait_audio<F: std::future::Future<Output = ()>>(every: Duration, check: impl Fn() -> F) {
    loop {
        if let Some(mut watch) = linux::audio_watch() {
            while watch.changed().await {
                check().await;
            }
        }
        let until = tokio::time::Instant::now() + Duration::from_secs(60);
        while tokio::time::Instant::now() < until {
            tokio::time::sleep(every).await;
            check().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn werte_vergleichen() {
        let d = json!({ "name": "Zähler", "value": "5" });
        assert!(trigger_matches("value_change", &p(json!({ "name": "Zähler", "op": "gt", "value": "3" })), &d));
        assert!(!trigger_matches("value_change", &p(json!({ "name": "Zähler", "op": "lt", "value": "3" })), &d));
        assert!(trigger_matches("value_change", &p(json!({ "name": "Zähler" })), &d));
        assert!(!trigger_matches("value_change", &p(json!({ "name": "Anders" })), &d));
    }

    #[test]
    fn zustaende_und_chat() {
        assert!(trigger_matches("obs_stream", &p(json!({ "state": "any" })), &json!({ "state": "started" })));
        assert!(!trigger_matches("obs_stream", &p(json!({ "state": "stopped" })), &json!({ "state": "started" })));
        let chat = json!({ "text": "!szene 2", "badges": ["moderator"] });
        assert!(trigger_matches("twitch_chat", &p(json!({ "mode": "command", "text": "!szene", "who": "mod" })), &chat));
        assert!(!trigger_matches("twitch_chat", &p(json!({ "mode": "command", "text": "!szene", "who": "sub" })), &chat));
        assert!(trigger_matches("twitch_sub", &p(json!({ "kind": "gift" })), &json!({ "kind": "gift_received" })));
        assert!(trigger_matches("twitch_cheer", &p(json!({ "min": 100 })), &json!({ "bits": 100 })));
        assert!(trigger_matches("audio_mute", &p(json!({ "target": "output", "state": "muted" })), &json!({ "target": "output", "state": "muted" })));
        assert!(!trigger_matches("audio_mute", &p(json!({ "target": "input", "state": "muted" })), &json!({ "target": "output", "state": "muted" })));
        assert!(trigger_matches("audio_mute", &p(json!({ "state": "any" })), &json!({ "target": "output", "state": "unmuted" })));
        assert!(!trigger_matches("twitch_cheer", &p(json!({ "min": 100 })), &json!({ "bits": 99 })));
    }

    #[test]
    fn alle_ausloeser_bekannt() {
        for t in catalog::TRIGGER_TYPES.as_array().unwrap() {
            let kind = t["type"].as_str().unwrap();
            let src = include_str!("events.rs");
            assert!(src.contains(&format!("\"{kind}\"")), "Auslöser {kind} fehlt");
        }
    }
}
