// Alle Aktionstypen: Ausführung auf dem PC. Die Beschreibungen für den Editor stehen in catalog/actions.json.
use crate::t;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use chrono::{Datelike, Local, Timelike};
use futures_util::future::{BoxFuture, FutureExt};
use serde_json::{Map, Value, json};

use crate::core::catalog;
use crate::core::store::{Action, Params, js_string};
use crate::integrations::{discord, steam};
use crate::server::App;
use crate::system::{linux, sort_key, x11};

// ---------- Platzhalter ----------

/** Ersetzt {Name} durch den Wert, {Event.X} durch Daten des auslösenden Events und {Zeit}/{Datum}. */
pub fn fill_variables(text: &str, variables: &Map<String, Value>, extra: &Map<String, Value>) -> String {
    let now = Local::now();
    let builtin = |name: &str| -> Option<String> {
        match name {
            _ if extra.contains_key(name) => Some(js_string(&extra[name])),
            "Zeit" | "Time" => Some(format!("{:02}:{:02}", now.hour(), now.minute())),
            "Datum" => Some(format!("{}.{}.{}", now.day(), now.month(), now.year())),
            "Date" => Some(format!("{}-{:02}-{:02}", now.year(), now.month(), now.day())),
            _ => None,
        }
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find(['{', '}']);
        match end {
            Some(e) if after.as_bytes()[e] == b'}' && (1..=40).contains(&after[..e].chars().count()) => {
                let name = &after[..e];
                match variables.get(name).map(js_string).or_else(|| builtin(name)) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[start..start + e + 2]),
                }
                rest = &after[e + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------- Parameter wie in JavaScript lesen ----------

/** Wie JavaScript Number(v): leerer Text = 0, Unlesbares = NaN. */
pub fn js_number(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() { 0.0 } else { t.parse().unwrap_or(f64::NAN) }
        }
        Some(_) => f64::NAN,
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

struct P<'a>(&'a Params);

impl P<'_> {
    /** Textwert (oder leer). */
    fn s(&self, k: &str) -> String {
        self.0.get(k).map(js_string).unwrap_or_default()
    }
    /** Wie p.k || fallback. */
    fn or(&self, k: &str, fallback: &str) -> String {
        if truthy(self.0.get(k)) { self.s(k) } else { fallback.to_string() }
    }
    fn n(&self, k: &str) -> f64 {
        js_number(self.0.get(k))
    }
    /** Wie Number(p.k) || fallback. */
    fn n_or(&self, k: &str, fallback: f64) -> f64 {
        let v = self.n(k);
        if v.is_nan() || v == 0.0 { fallback } else { v }
    }
    fn t(&self, k: &str) -> bool {
        truthy(self.0.get(k))
    }
}

// ---------- Ausführen ----------

/** Wer die Aktionen auslöst: Button, Event (mit Daten) und wie tief Vorlagen verschachtelt sind. */
#[derive(Clone, Default)]
pub struct RunOpts {
    pub button_id: Option<String>,
    pub event_data: Map<String, Value>,
    pub flow_depth: u32,
}

struct Ctx {
    opts: RunOpts,
    navigate: Option<String>,
    stop: bool,
}

fn label(app: &App, kind: &str) -> String {
    let lang = app.store.lock().unwrap().settings().language.clone();
    if let Some(l) = app.plugins.label(kind, &lang) {
        return l;
    }
    catalog::action_label(kind, &lang).unwrap_or_else(|| kind.to_string())
}

fn known(kind: &str) -> bool {
    kind.starts_with("plugin:") || catalog::ACTION_TYPES.as_array().into_iter().flatten().any(|a| a["type"] == kind)
}

/** Führt die Aktionen nacheinander aus und bricht beim ersten Fehler ab. Ergebnis: { ok, error?, messages, navigate }. */
pub fn run_actions<'a>(app: &'a Arc<App>, actions: &'a [Action], opts: RunOpts) -> BoxFuture<'a, Value> {
    async move {
        let mut ctx = Ctx { opts, navigate: None, stop: false };
        let mut messages: Vec<String> = Vec::new();
        let enabled: Vec<&Action> = actions.iter().filter(|a| a.enabled).collect();
        // „Zufällig“: von den nächsten Aktionen nur eine ausführen, die anderen überspringen.
        let mut skip = std::collections::HashSet::new();
        for (i, action) in enabled.iter().enumerate() {
            if skip.contains(&i) {
                continue;
            }
            if action.kind == "random" {
                let rest = enabled.len() - i - 1;
                let count = match js_number(action.params.get("count")) as usize { 0 => rest, n => n.min(rest) };
                if count > 1 {
                    let pick = random_below(count);
                    skip.extend((0..count).filter(|&k| k != pick).map(|k| i + 1 + k));
                }
                continue;
            }
            let result = if known(&action.kind) { run_action(app, action, &mut ctx).await } else { Err(anyhow!(t!("actions.unknownAction", action.kind))) };
            match result {
                Ok(Some(msg)) if !msg.is_empty() => messages.push(msg),
                Ok(_) => {}
                Err(err) => {
                    let label = if known(&action.kind) { label(app, &action.kind) } else { action.kind.clone() };
                    return json!({ "ok": false, "error": format!("{label}: {err:#}"), "messages": messages, "navigate": ctx.navigate });
                }
            }
            if ctx.stop {
                break;
            }
        }
        json!({ "ok": true, "messages": messages, "navigate": ctx.navigate })
    }
    .boxed()
}

/** Zufallszahl 0..n (für „Zufällig“; keine Kryptografie nötig). */
fn random_below(n: usize) -> usize {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    (h.finish() % n as u64) as usize
}

fn hex_color(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    if v.is_nan() { min } else { v.clamp(min, max) }
}

fn variables(app: &App) -> Map<String, Value> {
    app.store.lock().unwrap().config.variables.clone()
}

fn fill(app: &App, ctx: &Ctx, text: &str) -> String {
    fill_variables(text, &variables(app), &ctx.opts.event_data)
}

fn set_variable(app: &App, name: &str, value: &str) -> Result<()> {
    app.store.lock().unwrap().set_variable(name, &json!(value))
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn enc(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn resolve_button(app: &App, target: &str, ctx: &Ctx) -> Result<String> {
    let id = if target.is_empty() || target == "self" {
        ctx.opts.button_id.clone().ok_or_else(|| anyhow!(t!("actions.thisButtonOnlyWorks")))?
    } else {
        target.to_string()
    };
    if app.store.lock().unwrap().find_button(&id).is_none() {
        bail!(t!("actions.buttonNoLongerExists"));
    }
    Ok(id)
}

async fn run_action(app: &Arc<App>, action: &Action, ctx: &mut Ctx) -> Result<Option<String>> {
    if action.kind.starts_with("plugin:") {
        // Platzhalter wie {Wert} oder {Event.Nutzer} vor der Übergabe ans Plugin ersetzen.
        let vars = variables(app);
        let params: Map<String, Value> =
            action.params.iter().map(|(k, v)| (k.clone(), v.as_str().map_or_else(|| v.clone(), |t| json!(fill_variables(t, &vars, &ctx.opts.event_data))))).collect();
        let info = json!({ "buttonId": ctx.opts.button_id, "event": ctx.opts.event_data });
        return app.plugins.run_action(&action.kind, &params, info, crate::core::events::current_depth()).await;
    }
    let p = P(&action.params);
    let none = Ok(None);
    match action.kind.as_str() {
        "open_url" => {
            let url = p.s("url");
            let scheme_ok = url.split_once(':').is_some_and(|(s, _)| {
                let mut c = s.chars();
                c.next().is_some_and(|f| f.is_ascii_alphabetic()) && c.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
            });
            if !scheme_ok {
                bail!(t!("actions.pleaseEnterFullAddress"));
            }
            linux::open_with_default(&url)?;
            none
        }
        "open_path" => {
            let path = p.s("path");
            if path.is_empty() {
                bail!(t!("actions.noPathGiven"));
            }
            linux::open_with_default(&linux::expand_home(&path).to_string_lossy())?;
            none
        }
        "launch_app" => {
            let id = p.s("app");
            if id.is_empty() {
                bail!(t!("actions.noProgramSelected"));
            }
            linux::launch_app(&id)?;
            none
        }
        "run_command" => {
            let cmd = p.s("command");
            if cmd.trim().is_empty() {
                bail!(t!("common.noCommandGiven"));
            }
            let wait = p.t("wait");
            let out = linux::shell(&cmd, wait, Duration::from_secs(30)).await?;
            Ok((wait && !out.is_empty()).then(|| truncate(&out, 500)))
        }
        "hotkey" => {
            let keys = p.s("keys");
            if keys.is_empty() {
                bail!(t!("actions.noShortcutGiven"));
            }
            x11::send_hotkey(&keys).await?;
            none
        }
        "type_text" => {
            let text = p.s("text");
            if !text.is_empty() {
                x11::type_text(&text).await?;
            }
            none
        }
        "media" => Ok(Some(format!("Player: {}", linux::media(&p.or("op", "play-pause")).await?))),
        "volume" => {
            let v = linux::volume(&p.or("op", "up"), p.n("step"), p.n("value")).await?;
            Ok(Some(if v.muted { t!("common.muted").into() } else { t!("actions.volume", v.volume.map_or_else(|| "?".into(), |v| v.to_string())) }))
        }
        "mic_mute" => {
            let v = linux::mic(&p.or("op", "toggle")).await?;
            Ok(Some(if v.muted { t!("actions.microphoneMuted") } else { t!("actions.microphoneOn") }.into()))
        }
        "timer" => {
            let name = p.or("name", "Timer");
            let duration = (p.n_or("minutes", 0.0).max(0.0) * 60.0 + p.n_or("seconds", 0.0).max(0.0)) * 1000.0;
            let running = app.timers.control(&name, &p.or("op", "toggle"), duration as u64);
            app.timers_changed();
            Ok(Some(if running { t!("actions.timerRunning", name) } else { t!("actions.timerStopped", name) }))
        }
        "deck_show_page" => {
            let page = p.s("page");
            if app.store.lock().unwrap().page(&page).is_none() {
                bail!(t!("actions.targetPageNoLonger"));
            }
            app.show_page_on_devices(&page);
            none
        }
        "go_page" => {
            let page = p.s("page");
            if !["__back", "__next", "__prev"].contains(&page.as_str()) && app.store.lock().unwrap().page(&page).is_none() {
                bail!(t!("actions.targetPageNoLonger"));
            }
            ctx.navigate = Some(page);
            none
        }
        "delay" => {
            let ms = clamp(p.n_or("ms", 0.0), 0.0, 60000.0);
            tokio::time::sleep(Duration::from_millis(ms as u64)).await;
            none
        }
        "notify" => {
            linux::notify(&p.s("title"), &p.s("body")).await?;
            none
        }
        "http" => {
            let url = p.s("url");
            let lower = url.to_lowercase();
            if !(lower.starts_with("http://") || lower.starts_with("https://")) {
                bail!(t!("actions.addressMustStartWith"));
            }
            let method = p.or("method", "GET");
            let mut req = reqwest::Client::new().request(reqwest::Method::from_bytes(method.as_bytes())?, &url).timeout(Duration::from_secs(15));
            for line in p.s("headers").lines() {
                if let Some((k, v)) = line.split_once(':').filter(|(k, _)| !k.is_empty()) {
                    req = req.header(k.trim(), v.trim());
                }
            }
            let body = p.s("body");
            if method != "GET" && !body.is_empty() {
                req = req.body(body);
            }
            let res = req.send().await?;
            Ok(Some(format!("Antwort {} {}", res.status().as_u16(), res.status().canonical_reason().unwrap_or(""))))
        }
        "audio_output" => Ok(Some(format!("Ausgang: {}", linux::set_default_audio(true, &p.or("target", "next")).await?))),
        "audio_input" => Ok(Some(format!("Eingang: {}", linux::set_default_audio(false, &p.or("target", "next")).await?))),
        "screenshot" => {
            linux::screenshot(&p.s("mode"))?;
            none
        }
        "lock_screen" => {
            linux::lock_screen().await?;
            none
        }
        "power" => {
            linux::power(&p.or("op", "suspend")).await?;
            none
        }
        "night_light" => Ok(Some(if linux::night_light(&p.or("mode", "toggle")).await? { t!("actions.nightLightOn") } else { t!("actions.nightLightOff") }.into())),
        "dark_mode" => Ok(Some(if linux::dark_mode(&p.or("mode", "toggle")).await? { "Dunkles Design" } else { "Helles Design" }.into())),
        "wake_on_lan" => {
            linux::wake_on_lan(&p.s("mac")).await?;
            Ok(Some(t!("actions.wakePacketSent").into()))
        }

        // ---------- Steam ----------
        "steam_game" => {
            steam::run_game(&p.s("game"))?;
            none
        }
        "steam_open" => {
            steam::open_steam(&p.s("view"))?;
            none
        }

        // ---------- OBS ----------
        "obs_scene" | "obs_preview" => {
            let scene = p.s("scene");
            if scene.is_empty() {
                bail!(t!("actions.noSceneSelected"));
            }
            let req = if action.kind == "obs_scene" { "SetCurrentProgramScene" } else { "SetCurrentPreviewScene" };
            app.obs.call(req, json!({ "sceneName": scene })).await?;
            none
        }
        "obs_stream" => {
            let req = match p.or("op", "toggle").as_str() {
                "start" => "StartStream",
                "stop" => "StopStream",
                _ => "ToggleStream",
            };
            app.obs.call0(req).await?;
            none
        }
        "obs_record" => {
            let req = match p.or("op", "toggle").as_str() {
                "start" => "StartRecord",
                "stop" => "StopRecord",
                "pause" => "ToggleRecordPause",
                _ => "ToggleRecord",
            };
            app.obs.call0(req).await?;
            none
        }
        "obs_replay" => {
            let toggle = p.s("op") == "toggle";
            app.obs.call0(if toggle { "ToggleReplayBuffer" } else { "SaveReplayBuffer" }).await?;
            Ok((!toggle).then(|| t!("actions.clipSaved").into()))
        }
        "obs_mute" => {
            let input = p.s("input");
            if input.is_empty() {
                bail!(t!("actions.noAudioSourceSelected"));
            }
            match p.s("op").as_str() {
                op @ ("mute" | "unmute") => app.obs.call("SetInputMute", json!({ "inputName": input, "inputMuted": op == "mute" })).await?,
                _ => app.obs.call("ToggleInputMute", json!({ "inputName": input })).await?,
            };
            none
        }
        "obs_source" => Ok(Some(if app.obs.set_source_visible(&p.s("item"), &p.or("mode", "toggle")).await? { t!("actions.shown") } else { t!("actions.hidden") }.into())),
        "obs_misc" => {
            if p.s("op") == "studio" {
                let on = app.obs.call0("GetStudioModeEnabled").await?["studioModeEnabled"] == true;
                app.obs.call("SetStudioModeEnabled", json!({ "studioModeEnabled": !on })).await?;
            } else {
                app.obs.call0("ToggleVirtualCam").await?;
            }
            none
        }
        "obs_transition" => {
            if p.s("op") == "set" {
                let t = p.s("transition");
                if t.is_empty() {
                    bail!(t!("actions.noTransitionSelected"));
                }
                app.obs.call("SetCurrentSceneTransition", json!({ "transitionName": t })).await?;
                let d = p.n("duration");
                if d > 0.0 {
                    app.obs.call("SetCurrentSceneTransitionDuration", json!({ "transitionDuration": d })).await?;
                }
            } else {
                app.obs.call0("TriggerStudioModeTransition").await?;
            }
            none
        }
        "obs_volume" => {
            let input = p.s("input");
            if input.is_empty() {
                bail!(t!("actions.noAudioSourceSelected"));
            }
            if p.s("op") == "set" {
                let v = clamp(p.n("value"), 0.0, 100.0) / 100.0;
                app.obs.call("SetInputVolume", json!({ "inputName": input, "inputVolumeMul": v * v })).await?;
                return Ok(Some(format!("{} %", p.s("value"))));
            }
            let cur = app.obs.call("GetInputVolume", json!({ "inputName": input })).await?["inputVolumeDb"].as_f64();
            let step = p.n_or("step", 3.0) * if p.s("op") == "down" { -1.0 } else { 1.0 };
            // OBS erlaubt -100 bis +26 dB; -100 gilt als stumm.
            let db = (cur.filter(|c| c.is_finite()).map_or(-60.0, |c| c.max(-60.0)) + step).clamp(-100.0, 26.0);
            app.obs.call("SetInputVolume", json!({ "inputName": input, "inputVolumeDb": db })).await?;
            Ok(Some(format!("{db:.1} dB")))
        }
        "obs_media" => {
            let input = p.s("input");
            if input.is_empty() {
                bail!(t!("actions.noMediaSourceSelected"));
            }
            let action = match p.s("op").as_str() {
                "restart" => "RESTART".to_string(),
                "stop" => "STOP".into(),
                "next" => "NEXT".into(),
                "previous" => "PREVIOUS".into(),
                _ => {
                    let st = app.obs.call("GetMediaInputStatus", json!({ "inputName": input })).await?;
                    if st["mediaState"] == "OBS_MEDIA_STATE_PLAYING" { "PAUSE".into() } else { "PLAY".into() }
                }
            };
            app.obs.call("TriggerMediaInputAction", json!({ "inputName": input, "mediaAction": format!("OBS_WEBSOCKET_MEDIA_INPUT_ACTION_{action}") })).await?;
            none
        }
        "obs_filter" => Ok(Some(if app.obs.set_filter(&p.s("filter"), &p.or("mode", "toggle")).await? { t!("actions.filterOn") } else { t!("actions.filterOff") }.into())),
        "obs_text" => {
            let input = p.s("input");
            if input.is_empty() {
                bail!(t!("actions.noTextSourceSelected"));
            }
            let text = fill(app, ctx, &p.s("text"));
            app.obs.call("SetInputSettings", json!({ "inputName": input, "inputSettings": { "text": text } })).await?;
            none
        }
        "obs_browser" => {
            let input = p.s("input");
            if input.is_empty() {
                bail!(t!("actions.noBrowserSourceSelected"));
            }
            if p.s("op") == "url" {
                app.obs.call("SetInputSettings", json!({ "inputName": input, "inputSettings": { "url": p.s("url") } })).await?;
            } else {
                app.obs.call("PressInputPropertiesButton", json!({ "inputName": input, "propertyName": "refreshnocache" })).await?;
            }
            none
        }
        "obs_image" => {
            let (input, file) = (p.s("input"), p.s("file"));
            if input.is_empty() || file.is_empty() {
                bail!(t!("actions.specifyImageSourceAnd"));
            }
            app.obs.call("SetInputSettings", json!({ "inputName": input, "inputSettings": { "file": linux::expand_home(&file) } })).await?;
            none
        }
        "obs_profile" => {
            if p.s("kind") == "collection" {
                app.obs.call("SetCurrentSceneCollection", json!({ "sceneCollectionName": p.s("collection") })).await?;
            } else {
                app.obs.call("SetCurrentProfile", json!({ "profileName": p.s("profile") })).await?;
            }
            none
        }
        "obs_hotkey" => {
            let hk = p.s("hotkey");
            if hk.is_empty() {
                bail!(t!("actions.noFunctionSelected"));
            }
            app.obs.call("TriggerHotkeyByName", json!({ "hotkeyName": hk })).await?;
            none
        }
        "obs_screenshot" => {
            let source = p.s("source");
            if source.is_empty() {
                bail!(t!("common.noSourceSelected"));
            }
            let stamp = Local::now().format("%Y-%m-%d %H-%M-%S");
            let file = linux::expand_home(&p.or("folder", "~/Bilder")).join(format!("OBS {source} {stamp}.png"));
            app.obs.call("SaveSourceScreenshot", json!({ "sourceName": source, "imageFormat": "png", "imageFilePath": file })).await?;
            Ok(Some(format!("Gespeichert: {}", file.display())))
        }
        "obs_raw" => {
            let request = p.s("request");
            if request.is_empty() {
                bail!(t!("actions.noRequestGiven"));
            }
            let raw = p.s("data");
            let data: Value = if raw.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(&raw).map_err(|_| anyhow!(t!("actions.dataIsNotValid")))?
            };
            let r = app.obs.call(&request, data).await?;
            Ok(r.as_object().filter(|o| !o.is_empty()).map(|_| truncate(&r.to_string(), 300)))
        }

        // ---------- Maus & Fenster ----------
        "mouse_click" => {
            x11::mouse_click(&p.or("button", "left")).await?;
            none
        }
        "mouse_move" => {
            let num = |k| {
                let v = p.n(k);
                if v.is_nan() { 0 } else { v.round() as i32 }
            };
            x11::mouse_move(num("x"), num("y"), p.s("mode") == "rel").await?;
            none
        }
        "key_hold" => {
            let keys = p.s("keys");
            if keys.is_empty() {
                bail!(t!("actions.noKeyGiven"));
            }
            x11::hold_keys(&keys, p.n_or("ms", 1000.0).max(0.0) as u64).await?;
            none
        }
        "window_activate" => {
            let w = p.s("window");
            if w.is_empty() {
                bail!(t!("actions.noWindowSelected"));
            }
            Ok(Some(t!("actions.activated", x11::activate_window(&w).await?)))
        }
        "kill_app" => {
            linux::kill_process(&p.s("name"), p.t("force")).await?;
            none
        }

        // ---------- Soundboard & Dateien ----------
        "sound_play" => {
            let vol = if action.params.contains_key("volume") && !action.params["volume"].is_null() { p.n("volume") } else { 100.0 };
            linux::play_sound(&p.s("file"), vol, p.t("exclusive"))?;
            none
        }
        "sound_stop" => {
            linux::stop_sounds();
            none
        }
        "file_write" => {
            linux::write_file(&p.s("path"), &fill(app, ctx, &p.s("text")), &p.s("mode"))?;
            none
        }

        // ---------- Logik & Variablen ----------
        "var_set" => {
            let value = fill(app, ctx, &p.s("value"));
            set_variable(app, &p.s("name"), &value)?;
            none
        }
        "var_change" => {
            let name = p.s("name");
            let cur = js_number(variables(app).get(&name));
            let cur = if cur.is_nan() { 0.0 } else { cur };
            let n = p.n_or("amount", 0.0);
            let next = match p.s("op").as_str() {
                "reset" => n,
                "sub" => cur - n,
                _ => cur + n,
            };
            let text = js_string(&json!(next));
            set_variable(app, &name, &text)?;
            Ok(Some(format!("{name} = {text}")))
        }
        "var_toggle" => {
            let name = p.s("name");
            let cur = variables(app).get(&name).map(js_string).unwrap_or_default().to_lowercase();
            let on = !["an", "true", "1", "ja", "on"].contains(&cur.as_str());
            set_variable(app, &name, if on { "an" } else { "aus" })?;
            Ok(Some(format!("{name}: {}", if on { "an" } else { "aus" })))
        }
        "condition" => {
            let v = variables(app).get(&p.s("name")).map(js_string).unwrap_or_default();
            let w = p.s("value");
            let num = |s: &str| js_number(Some(&json!(s)));
            let ok = match p.or("op", "eq").as_str() {
                "ne" => v != w,
                "gt" => num(&v) > num(&w),
                "lt" => num(&v) < num(&w),
                "contains" => v.contains(&w),
                "empty" => v.is_empty(),
                _ => v == w,
            };
            // Nicht erfüllt: die restlichen Aktionen dieses Buttons überspringen.
            if !ok {
                ctx.stop = true;
            }
            none
        }

        // ---------- Twitch ----------
        "twitch_chat" => {
            let t = &app.twitch;
            let me = t.me()?;
            let msg = truncate(&fill(app, ctx, &p.s("message")), 500);
            let r = t.api("POST", "/chat/messages", Some(json!({ "broadcaster_id": me, "sender_id": me, "message": msg }))).await?;
            if r["data"][0]["is_sent"] == false {
                bail!("{}", r["data"][0]["drop_reason"]["message"].as_str().unwrap_or(t!("actions.messageWasNotSent")));
            }
            none
        }
        "twitch_announcement" => {
            let me = app.twitch.me()?;
            let msg = truncate(&fill(app, ctx, &p.s("message")), 500);
            let body = json!({ "message": msg, "color": p.or("color", "primary") });
            app.twitch.api("POST", &format!("/chat/announcements?broadcaster_id={me}&moderator_id={me}"), Some(body)).await?;
            none
        }
        "twitch_clip" => {
            let me = app.twitch.me()?;
            let r = app.twitch.api("POST", &format!("/clips?broadcaster_id={me}"), None).await?;
            match r["data"][0]["id"].as_str() {
                Some(id) => {
                    set_variable(app, "Twitch.Clip", &format!("https://clips.twitch.tv/{id}"))?;
                    Ok(Some(t!("actions.clipCreatedLinkIn").into()))
                }
                None => none,
            }
        }
        "twitch_marker" => {
            let me = app.twitch.me()?;
            let desc = truncate(&fill(app, ctx, &p.s("description")), 140);
            app.twitch.api("POST", "/streams/markers", Some(json!({ "user_id": me, "description": desc }))).await?;
            Ok(Some(t!("actions.markerSet").into()))
        }
        "twitch_channel" => {
            let me = app.twitch.me()?;
            let mut body = Map::new();
            if p.t("title") {
                body.insert("title".into(), json!(truncate(&fill(app, ctx, &p.s("title")), 140)));
            }
            if p.t("game") {
                let game = p.s("game");
                let g = app.twitch.api("GET", &format!("/games?name={}", enc(&game)), None).await?;
                let id = g["data"][0]["id"].as_str().ok_or_else(|| anyhow!(t!("actions.categoryNotFoundUse", game)))?;
                body.insert("game_id".into(), json!(id));
            }
            if body.is_empty() {
                bail!(t!("actions.neitherTitleNorCategory"));
            }
            app.twitch.api("PATCH", &format!("/channels?broadcaster_id={me}"), Some(Value::Object(body))).await?;
            none
        }
        "twitch_ad" => {
            let me = app.twitch.me()?;
            let r = app.twitch.api("POST", "/channels/commercial", Some(json!({ "broadcaster_id": me, "length": p.n_or("length", 60.0) }))).await?;
            Ok(Some(r["data"][0]["message"].as_str().filter(|m| !m.is_empty()).unwrap_or(t!("actions.adStarted")).to_string()))
        }
        "twitch_chat_mode" => {
            let me = app.twitch.me()?;
            let q = format!("broadcaster_id={me}&moderator_id={me}");
            let mode = p.or("mode", "emote_mode");
            let op = p.s("op");
            let on = if op.is_empty() || op == "toggle" {
                app.twitch.api("GET", &format!("/chat/settings?{q}"), None).await?["data"][0][&mode] != true
            } else {
                op == "on"
            };
            let mut body = Map::new();
            body.insert(mode, json!(on));
            app.twitch.api("PATCH", &format!("/chat/settings?{q}"), Some(Value::Object(body))).await?;
            Ok(Some(if on { t!("actions.modeOn") } else { t!("actions.modeOff") }.into()))
        }
        "twitch_shoutout" => {
            let me = app.twitch.me()?;
            let to = app.twitch.user_id(&fill(app, ctx, &p.s("user"))).await?;
            app.twitch.api("POST", &format!("/chat/shoutouts?from_broadcaster_id={me}&to_broadcaster_id={to}&moderator_id={me}"), None).await?;
            none
        }
        "twitch_raid" => {
            let me = app.twitch.me()?;
            if p.s("op") == "cancel" {
                app.twitch.api("DELETE", &format!("/raids?broadcaster_id={me}"), None).await?;
                return none;
            }
            let to = app.twitch.user_id(&fill(app, ctx, &p.s("user"))).await?;
            app.twitch.api("POST", &format!("/raids?from_broadcaster_id={me}&to_broadcaster_id={to}"), None).await?;
            Ok(Some(t!("actions.raidRunningStartsIn").into()))
        }
        "twitch_reward" => {
            let me = app.twitch.me()?;
            let reward = p.s("reward");
            if reward.is_empty() {
                bail!(t!("actions.noRewardSelected"));
            }
            let q = format!("broadcaster_id={me}&id={reward}");
            let cur = app.twitch.api("GET", &format!("/channel_points/custom_rewards?{q}&only_manageable_rewards=true"), None).await?["data"][0].clone();
            if cur.is_null() {
                bail!(t!("actions.rewardNotFound"));
            }
            let key = if p.s("op") == "pause" { "is_paused" } else { "is_enabled" };
            let mut body = Map::new();
            body.insert(key.into(), json!(cur[key] != true));
            app.twitch.api("PATCH", &format!("/channel_points/custom_rewards?{q}"), Some(Value::Object(body))).await?;
            none
        }
        "twitch_info" => {
            let me = app.twitch.me()?;
            let s = app.twitch.stream().await?;
            let ch = app.twitch.api("GET", &format!("/channels?broadcaster_id={me}"), None).await?["data"][0].clone();
            let followers = app.twitch.followers().await?;
            let set = |k: &str, v: &Value| set_variable(app, &format!("Twitch.{k}"), &js_string(v));
            set("Live", &json!(if s.is_null() { "aus" } else { "an" }))?;
            set("Zuschauer", &if s.is_null() { json!(0) } else { s["viewer_count"].clone() })?;
            set("Titel", &ch["title"])?;
            set("Kategorie", &ch["game_name"])?;
            set("Follower", &json!(followers))?;
            Ok(Some(t!("actions.updatedValuesTwitchLive").into()))
        }

        // ---------- Discord ----------
        "discord_mute" | "discord_deafen" => {
            let key = if action.kind == "discord_mute" { "mute" } else { "deaf" };
            let v = app.discord.voice_settings().await?;
            let on = match p.s("op").as_str() {
                "on" => true,
                "off" => false,
                _ => v[key] != true,
            };
            let mut patch = Map::new();
            patch.insert(key.into(), json!(on));
            app.discord.set_voice(Value::Object(patch)).await?;
            Ok(Some(
                match (key, on) {
                    ("mute", true) => t!("actions.discordMuted"),
                    ("mute", false) => t!("actions.discordMicrophoneOn"),
                    (_, true) => t!("actions.discordDeafened"),
                    _ => t!("actions.discordListeningAgain"),
                }
                .into(),
            ))
        }
        "discord_channel" => {
            let current = app.discord.call("GET_SELECTED_VOICE_CHANNEL", json!({})).await?["id"].as_str().map(String::from);
            let (op, channel) = (p.s("op"), p.s("channel"));
            let leave = op == "leave" || (op == "toggle" && current.as_deref() == Some(channel.as_str()));
            if leave {
                app.discord.call("SELECT_VOICE_CHANNEL", json!({ "channel_id": null })).await?;
                return Ok(Some(t!("actions.leftVoiceChannel").into()));
            }
            if channel.is_empty() {
                bail!(t!("actions.noVoiceChannelSelected"));
            }
            app.discord.call("SELECT_VOICE_CHANNEL", json!({ "channel_id": channel, "force": true })).await?;
            none
        }
        "discord_volume" => {
            let target = if p.s("target") == "input" { "input" } else { "output" };
            let max = if target == "input" { 100.0 } else { 200.0 };
            let v = app.discord.voice_settings().await?;
            let cur = v[target]["volume"].as_f64().unwrap_or(100.0);
            let step = p.n_or("step", 10.0);
            let next = match p.s("op").as_str() {
                "set" => p.n("value"),
                "down" => cur - step,
                _ => cur + step,
            };
            let next = clamp(next, 0.0, max).round();
            let mut patch = Map::new();
            patch.insert(target.into(), json!({ "volume": next }));
            app.discord.set_voice(Value::Object(patch)).await?;
            Ok(Some(format!("Discord {}: {next}", if target == "input" { t!("actions.input") } else { t!("actions.output") })))
        }
        "discord_mode" => {
            let v = app.discord.voice_settings().await?;
            let op = p.s("op");
            let kind = if op.is_empty() || op == "toggle" {
                if v["mode"]["type"] == "PUSH_TO_TALK" { "VOICE_ACTIVITY".to_string() } else { "PUSH_TO_TALK".into() }
            } else {
                op
            };
            app.discord.set_voice(json!({ "mode": { "type": kind } })).await?;
            Ok(Some(if kind == "PUSH_TO_TALK" { "Push-to-Talk" } else { t!("actions.voiceActivity") }.into()))
        }
        "discord_webhook" => {
            discord::send_webhook(&p.s("url"), &fill(app, ctx, &p.s("message")), &p.s("username")).await?;
            Ok(Some(t!("actions.messageSent").into()))
        }

        // ---------- Logik ----------
        "run_flow" => {
            let id = p.s("flow");
            let flow = app.store.lock().unwrap().config.flows.iter().find(|f| f.id == id).cloned();
            let flow = flow.ok_or_else(|| anyhow!(t!("actions.templateNoLongerExists")))?;
            if ctx.opts.flow_depth >= 8 {
                bail!(t!("actions.templatesCallEachOther"));
            }
            let times = clamp(p.n_or("times", 1.0), 1.0, 100.0) as u32;
            let mut messages: Vec<String> = Vec::new();
            for _ in 0..times {
                let opts = RunOpts { flow_depth: ctx.opts.flow_depth + 1, ..ctx.opts.clone() };
                let r = run_actions(app, &flow.actions, opts).await;
                if let Some(nav) = r["navigate"].as_str() {
                    ctx.navigate = Some(nav.into());
                }
                messages.extend(r["messages"].as_array().into_iter().flatten().filter_map(|m| m.as_str().map(String::from)));
                if r["ok"] != true {
                    bail!(t!("actions.template", flow.name, r["error"].as_str().unwrap_or("")));
                }
            }
            Ok((!messages.is_empty()).then(|| messages.join(" · ")))
        }
        "stop" => {
            ctx.stop = true;
            none
        }
        "comment" => none,

        // ---------- Visuelles ----------
        "change_visuals" => {
            let id = resolve_button(app, &p.s("target"), ctx)?;
            let mut patch = Map::new();
            if p.t("title") {
                patch.insert("title".into(), json!(fill(app, ctx, &p.s("title"))));
            }
            for key in ["color", "textColor"] {
                let c = p.s(key);
                if hex_color(&c) {
                    patch.insert(key.into(), json!(c));
                }
            }
            if p.t("icon") {
                patch.insert("icon".into(), json!({ "type": "emoji", "value": p.s("icon") }));
            }
            app.store.lock().unwrap().set_override(false, &id, Some(&patch));
            none
        }
        "restore_visuals" => {
            let id = resolve_button(app, &p.s("target"), ctx)?;
            app.store.lock().unwrap().set_override(false, &id, None);
            none
        }
        "page_background" => {
            let page = p.s("page");
            let mut store = app.store.lock().unwrap();
            if store.page(&page).is_none() {
                bail!(t!("actions.noPageSelected"));
            }
            let c = p.s("color");
            let patch = hex_color(&c).then(|| {
                let mut m = Map::new();
                m.insert("background".into(), json!(c));
                m
            });
            store.set_override(true, &page, patch.as_ref());
            none
        }
        "unsupported" => bail!(t!("actions.actionIsNotSupported", p.s("original"))),
        other => bail!(t!("actions.unknownAction", other)),
    }
}

// ---------- Auswahllisten ----------

fn opts(list: impl IntoIterator<Item = (String, String)>) -> Value {
    json!(list.into_iter().map(|(v, l)| json!({ "value": v, "label": l })).collect::<Vec<_>>())
}

fn obs_kind(kind: &str, pattern: &str) -> bool {
    match pattern {
        "media" => kind.contains("ffmpeg_source") || kind.contains("vlc_source"),
        "text" => kind.starts_with("text_"),
        "browser" => kind.starts_with("browser_source"),
        "image" => kind.starts_with("image_source") || kind.contains("slideshow"),
        _ => false,
    }
}

/** Auswahllisten, die erst zur Laufzeit feststehen (Spiele, Audiogeräte, OBS-Szenen …). None = unbekannte Liste. */
pub async fn option_source(app: &Arc<App>, name: &str) -> Option<Result<Value>> {
    let obs = &app.obs;
    let r: Result<Value> = match name {
        "steam-games" => Ok(steam::games_as_options()),
        "audio-sinks" | "audio-sources" => {
            let sinks = name == "audio-sinks";
            let first = json!({ "value": "next", "label": if sinks { t!("actions.alwaysNextOutput") } else { t!("actions.alwaysNextInput") } });
            linux::audio_devices(sinks).await.map(|d| {
                let mut list = vec![first];
                list.extend(d.into_iter().map(|d| json!(d)));
                json!(list)
            })
        }
        "audio-sinks-only" => linux::audio_devices(true).await.map(|d| json!(d)),
        "obs-scenes" => obs.scenes().await.map(|s| opts(s.into_iter().map(|v| (v.clone(), v)))),
        "obs-inputs" => obs.inputs().await.map(|s| opts(s.into_iter().map(|v| (v.clone(), v)))),
        "obs-scene-items" => obs.scene_items().await.map(Value::from),
        "obs-sources" => async {
            let mut all = obs.scenes().await?;
            all.extend(obs.inputs().await?);
            Ok(opts(all.into_iter().map(|v| (v.clone(), v))))
        }
        .await,
        "obs-media-inputs" => obs.inputs_of_kind(|k| obs_kind(k, "media")).await.map(Value::from),
        "obs-text-inputs" => obs.inputs_of_kind(|k| obs_kind(k, "text")).await.map(Value::from),
        "obs-browser-inputs" => obs.inputs_of_kind(|k| obs_kind(k, "browser")).await.map(Value::from),
        "obs-image-inputs" => obs.inputs_of_kind(|k| obs_kind(k, "image")).await.map(Value::from),
        "obs-filters" => obs.filters().await.map(Value::from),
        "obs-transitions" => obs.list_of("GetSceneTransitionList", "transitions", "transitionName").await.map(Value::from),
        "obs-profiles" => obs.list_of("GetProfileList", "profiles", "").await.map(Value::from),
        "obs-collections" => obs.list_of("GetSceneCollectionList", "sceneCollections", "").await.map(Value::from),
        "obs-hotkeys" => obs.list_of("GetHotkeyList", "hotkeys", "").await.map(Value::from),
        "windows" => x11::open_windows().await.map(opts),
        "flows" => Ok(opts(app.store.lock().unwrap().config.flows.iter().map(|f| (f.id.clone(), f.name.clone())))),
        "buttons" => {
            let store = app.store.lock().unwrap();
            let mut list = vec![("self".to_string(), t!("actions.thisButton").to_string())];
            for pg in &store.config.pages {
                for b in &pg.buttons {
                    let name = [&b.title, &b.icon.value].into_iter().find(|t| !t.is_empty()).cloned().unwrap_or_else(|| t!("actions.field", b.row + 1, b.col + 1));
                    list.push((b.id.clone(), format!("{} › {name}", pg.name)));
                }
            }
            Ok(opts(list))
        }
        "toggle-buttons" => {
            let store = app.store.lock().unwrap();
            Ok(opts(store.config.pages.iter().flat_map(|pg| {
                pg.buttons
                    .iter()
                    .filter(|b| b.mode == "toggle")
                    .map(|b| (b.id.clone(), format!("{} › {}", pg.name, if b.title.is_empty() { &b.id } else { &b.title })))
                    .collect::<Vec<_>>()
            })))
        }
        "discord-voice-channels" => app.discord.voice_channels().await.map(Value::from),
        "twitch-rewards" => async {
            let me = app.twitch.me()?;
            let r = app.twitch.api("GET", &format!("/channel_points/custom_rewards?broadcaster_id={me}&only_manageable_rewards=true"), None).await?;
            Ok(opts(r["data"].as_array().into_iter().flatten().map(|r| {
                (r["id"].as_str().unwrap_or("").to_string(), format!("{} ({})", r["title"].as_str().unwrap_or(""), r["cost"]))
            })))
        }
        .await,
        "variables" => {
            // Gesetzte Variablen plus Namen, die in Aktionen oder Anzeigen schon vorkommen.
            let store = app.store.lock().unwrap();
            let c = &store.config;
            let mut names: std::collections::BTreeSet<String> = c.variables.keys().cloned().collect();
            names.extend(c.values.keys().cloned());
            let mut lists: Vec<&Vec<Action>> = Vec::new();
            for b in c.pages.iter().flat_map(|p| &p.buttons) {
                lists.extend([&b.actions, &b.actions_off, &b.hold_actions, &b.release_actions]);
                lists.extend(b.events.iter().map(|e| &e.actions));
                if !b.live.variable.is_empty() {
                    names.insert(b.live.variable.clone());
                }
            }
            lists.extend(c.flows.iter().map(|f| &f.actions));
            lists.extend(c.events.iter().map(|e| &e.actions));
            for a in lists.into_iter().flatten() {
                if (a.kind.starts_with("var_") || a.kind.starts_with("condition"))
                    && let Some(n) = a.params.get("name").and_then(Value::as_str).filter(|n| !n.is_empty())
                {
                    names.insert(n.into());
                }
            }
            let mut names: Vec<String> = names.into_iter().collect();
            names.sort_by_key(|n| sort_key(n));
            Ok(opts(names.into_iter().map(|n| (n.clone(), n))))
        }
        _ => return None,
    };
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platzhalter() {
        let vars: Map<String, Value> = serde_json::from_value(json!({ "Zähler": "5", "x": 3 })).unwrap();
        let extra: Map<String, Value> = serde_json::from_value(json!({ "Event.Nutzer": "anna" })).unwrap();
        assert_eq!(fill_variables("{Zähler} und {x}", &vars, &extra), "5 und 3");
        assert_eq!(fill_variables("Hallo {Event.Nutzer}!", &vars, &extra), "Hallo anna!");
        assert_eq!(fill_variables("{unbekannt} {} {{x}}", &vars, &extra), "{unbekannt} {} {3}");
        assert_eq!(fill_variables("{Zeit}", &vars, &extra).len(), 5);
    }

    #[test]
    fn zahlen_wie_javascript() {
        assert_eq!(js_number(Some(&json!(""))), 0.0);
        assert_eq!(js_number(Some(&json!(" 7 "))), 7.0);
        assert!(js_number(Some(&json!("abc"))).is_nan());
        assert!(js_number(None).is_nan());
        assert!(truthy(Some(&json!("x"))));
        assert!(!truthy(Some(&json!(0))));
    }

    #[test]
    fn katalog_vollstaendig() {
        // Jede Aktion aus dem Katalog muss hier ausgeführt werden können.
        let src = include_str!("actions.rs");
        for a in catalog::ACTION_TYPES.as_array().unwrap() {
            let kind = a["type"].as_str().unwrap();
            assert!(src.contains(&format!("\"{kind}\"")), "Aktion {kind} fehlt");
        }
    }
}
