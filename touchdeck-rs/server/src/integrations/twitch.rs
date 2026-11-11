// Twitch: Anmeldung per Geräte-Code (jeder Nutzer mit eigener Client-ID), Helix-API und EventSub-Ereignisse.
use crate::t;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use futures_util::StreamExt;
use futures_util::future::{BoxFuture, FutureExt, Shared};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::core::log::Log;
use crate::core::store::{Store, TwitchSettings};

const ID: &str = "https://id.twitch.tv/oauth2";
const HELIX: &str = "https://api.twitch.tv/helix";
const EVENTSUB: &str = "wss://eventsub.wss.twitch.tv/ws";

pub const SCOPES: [&str; 22] = [
    "user:read:chat",
    "user:write:chat",
    "moderator:manage:announcements",
    "moderator:manage:chat_settings",
    "moderator:manage:shoutouts",
    "moderator:read:followers",
    "clips:edit",
    "channel:manage:broadcast",
    "channel:edit:commercial",
    "channel:manage:raids",
    "channel:manage:redemptions",
    "channel:read:redemptions",
    "channel:read:subscriptions",
    "bits:read",
    "channel:read:ads",
    "channel:manage:ads",
    "channel:manage:polls",
    "channel:manage:predictions",
    "channel:read:hype_train",
    "channel:read:goals",
    "moderator:manage:shield_mode",
    "moderator:manage:chat_messages",
];

/** EventSub-Typen je Auslöser (nur abonniert, wenn ein Event sie braucht): (Typ, Version, Bedingung). */
fn eventsub_types(trigger: &str, user: &str) -> Vec<(&'static str, &'static str, Value)> {
    let b = || json!({ "broadcaster_user_id": user });
    match trigger {
        "twitch_follow" => vec![("channel.follow", "2", json!({ "broadcaster_user_id": user, "moderator_user_id": user }))],
        "twitch_sub" => vec![("channel.subscribe", "1", b()), ("channel.subscription.gift", "1", b()), ("channel.subscription.message", "1", b())],
        "twitch_cheer" => vec![("channel.cheer", "1", b())],
        "twitch_raid" => vec![("channel.raid", "1", json!({ "to_broadcaster_user_id": user }))],
        "twitch_redeem" => vec![("channel.channel_points_custom_reward_redemption.add", "1", b())],
        "twitch_chat" => vec![("channel.chat.message", "1", json!({ "broadcaster_user_id": user, "user_id": user }))],
        "twitch_stream" => vec![("stream.online", "1", b()), ("stream.offline", "1", b())],
        "twitch_ad" => vec![("channel.ad_break.begin", "1", b())],
        "twitch_poll" => vec![("channel.poll.begin", "1", b()), ("channel.poll.end", "1", b())],
        "twitch_prediction" => vec![("channel.prediction.begin", "1", b()), ("channel.prediction.lock", "1", b()), ("channel.prediction.end", "1", b())],
        "twitch_hype" => vec![("channel.hype_train.begin", "2", b()), ("channel.hype_train.progress", "2", b()), ("channel.hype_train.end", "2", b())],
        _ => vec![],
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct Device {
    user_code: String,
    verification_uri: String,
    expires_at: i64,
}

type Refresh = Shared<BoxFuture<'static, Result<(), String>>>;

struct Inner {
    store: Arc<Mutex<Store>>,
    log: Arc<Log>,
    http: reqwest::Client,
    device: Mutex<Option<(u64, Device)>>,
    device_seq: Mutex<u64>,
    session_id: Mutex<Option<String>>,
    wanted: Mutex<BTreeSet<String>>,
    events_task: Mutex<Option<JoinHandle<()>>>,
    events: broadcast::Sender<(String, Value)>,
    cache: Mutex<HashMap<String, (Instant, Value)>>,
    /// Laufende Erneuerung (es darf nur eine geben).
    refreshing: Mutex<Option<Refresh>>,
    /// Letztes Ereignis je Auslöser (für Live-Anzeigen wie „letzter Sub“ und den Hype Train).
    last: Mutex<HashMap<String, Value>>,
    /// Berechtigungen, die der gespeicherten Anmeldung fehlen (neue Funktionen → neu anmelden).
    missing_scopes: Mutex<Vec<String>>,
}

#[derive(Clone)]
pub struct TwitchClient(Arc<Inner>);

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)))
        .collect::<Vec<_>>()
        .join("&")
}

impl TwitchClient {
    pub fn new(store: Arc<Mutex<Store>>, log: Arc<Log>) -> Self {
        TwitchClient(Arc::new(Inner {
            store,
            log,
            http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("HTTP-Client"),
            device: Mutex::new(None),
            device_seq: Mutex::new(0),
            session_id: Mutex::new(None),
            wanted: Mutex::new(BTreeSet::new()),
            events_task: Mutex::new(None),
            events: broadcast::channel(256).0,
            cache: Mutex::new(HashMap::new()),
            refreshing: Mutex::new(None),
            last: Mutex::new(HashMap::new()),
            missing_scopes: Mutex::new(Vec::new()),
        }))
    }

    fn auth(&self) -> TwitchSettings {
        self.0.store.lock().unwrap().settings().twitch.clone()
    }

    fn save(&self, f: impl FnOnce(&mut TwitchSettings)) {
        self.0.store.lock().unwrap().set_twitch_auth(f);
    }

    pub fn connected(&self) -> bool {
        !self.auth().access_token.is_empty()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(String, Value)> {
        self.0.events.subscribe()
    }

    pub fn status(&self) -> Value {
        if let Some((_, d)) = self.0.device.lock().unwrap().as_ref() {
            return json!({ "state": "waiting", "userCode": d.user_code, "verificationUri": d.verification_uri, "expiresAt": d.expires_at });
        }
        let a = self.auth();
        if !a.access_token.is_empty() {
            let missing = self.0.missing_scopes.lock().unwrap().clone();
            return json!({ "state": "connected", "login": a.login, "displayName": a.display_name, "events": self.0.session_id.lock().unwrap().is_some(), "missingScopes": missing });
        }
        json!({ "state": if a.last_error.is_null() { "idle" } else { "error" }, "error": a.last_error })
    }

    // ---------- Anmeldung ----------

    /** Startet die Anmeldung: liefert den Code, den der Nutzer auf twitch.tv/activate eingibt. */
    pub async fn start_device_login(&self) -> Result<Value> {
        let client_id = self.auth().client_id;
        if client_id.is_empty() {
            bail!(t!("twitch.pleaseEnterClientId"));
        }
        let scopes = SCOPES.join(" ");
        let res = self
            .0
            .http
            .post(format!("{ID}/device"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form(&[("client_id", &client_id), ("scopes", &scopes)]))
            .send()
            .await?;
        let ok = res.status().is_success();
        let status = res.status().as_u16();
        let d: Value = res.json().await.unwrap_or_default();
        if !ok {
            let msg = d["message"].as_str().map_or_else(|| status.to_string(), String::from);
            bail!(t!("twitch.twitchRefusedIsClient", msg));
        }
        let seq = {
            let mut s = self.0.device_seq.lock().unwrap();
            *s += 1;
            *s
        };
        let expires_at = now_ms() + d["expires_in"].as_i64().unwrap_or(1800) * 1000;
        *self.0.device.lock().unwrap() = Some((
            seq,
            Device {
                user_code: d["user_code"].as_str().unwrap_or("").into(),
                verification_uri: d["verification_uri"].as_str().unwrap_or("").into(),
                expires_at,
            },
        ));
        let me = self.clone();
        let code = d["device_code"].as_str().unwrap_or("").to_string();
        let interval = d["interval"].as_u64().unwrap_or(5).max(1);
        tokio::spawn(async move { me.poll_device(seq, client_id, code, interval, expires_at).await });
        Ok(self.status())
    }

    fn device_active(&self, seq: u64) -> bool {
        self.0.device.lock().unwrap().as_ref().is_some_and(|(s, _)| *s == seq)
    }

    async fn poll_device(&self, seq: u64, client_id: String, code: String, interval: u64, expires_at: i64) {
        let scopes = SCOPES.join(" ");
        while self.device_active(seq) && now_ms() < expires_at {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            if !self.device_active(seq) {
                return;
            }
            let body = form(&[
                ("client_id", &client_id),
                ("scopes", &scopes),
                ("device_code", &code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ]);
            let res = self.0.http.post(format!("{ID}/token")).header("Content-Type", "application/x-www-form-urlencoded").body(body).send().await;
            let res = match res {
                Ok(r) => r,
                Err(err) => {
                    self.0.log.warn("Twitch", format!("Anmeldung: {err}"));
                    continue;
                }
            };
            let ok = res.status().is_success();
            let d: Value = res.json().await.unwrap_or_default();
            if ok {
                *self.0.device.lock().unwrap() = None;
                self.store_tokens(&d);
                match self.validate().await {
                    Ok(me) => self.0.log.info("Twitch", format!("Verbunden als {}", me["login"].as_str().unwrap_or(""))),
                    Err(err) => self.0.log.warn("Twitch", err.to_string()),
                }
                self.sync_events();
                return;
            }
            if d["message"] != "authorization_pending" {
                *self.0.device.lock().unwrap() = None;
                let msg = d["message"].as_str().unwrap_or("").to_string();
                self.save(|t| t.last_error = json!(format!("Anmeldung abgebrochen: {msg}")));
                return;
            }
        }
        if self.device_active(seq) {
            *self.0.device.lock().unwrap() = None;
            self.save(|t| t.last_error = json!(t!("twitch.codeHasExpiredPlease")));
        }
    }

    pub fn cancel_login(&self) {
        *self.0.device.lock().unwrap() = None;
    }

    fn store_tokens(&self, d: &Value) {
        let (at, rt) = (d["access_token"].as_str().unwrap_or("").to_string(), d["refresh_token"].as_str().unwrap_or("").to_string());
        let expires = now_ms() + d["expires_in"].as_i64().unwrap_or(3600) * 1000;
        self.save(|t| {
            t.access_token = at;
            t.refresh_token = rt;
            t.expires_at = json!(expires);
            t.last_error = Value::Null;
        });
    }

    fn expiring(&self) -> bool {
        self.auth().expires_at.as_i64().is_some_and(|e| e != 0 && e - now_ms() < 60_000)
    }

    /** Prüft die Anmeldung; ist der Zugang abgelaufen (gilt nur ca. 4 h), wird er zuerst erneuert. */
    pub async fn validate(&self) -> Result<Value> {
        let check = || async {
            let token = self.auth().access_token;
            self.0.http.get(format!("{ID}/validate")).header("Authorization", format!("OAuth {token}")).send().await
        };
        if self.expiring() {
            self.refresh().await?;
        }
        let mut res = check().await?;
        if res.status().as_u16() == 401 {
            self.refresh().await?;
            res = check().await?;
        }
        if !res.status().is_success() {
            bail!(t!("twitch.twitchLoginInvalidPlease"));
        }
        let d: Value = res.json().await?;
        *self.0.missing_scopes.lock().unwrap() = missing_scopes(&d["scopes"]);
        let user_id = d["user_id"].as_str().unwrap_or("").to_string();
        let users = self.api("GET", &format!("/users?id={user_id}"), None).await?;
        let login = d["login"].as_str().unwrap_or("").to_string();
        let display = users["data"][0]["display_name"].as_str().map_or_else(|| login.clone(), String::from);
        self.save(|t| {
            t.user_id = user_id;
            t.login = login;
            t.display_name = display;
        });
        Ok(d)
    }

    /**
     * Erneuert den Zugang. Twitch lässt jeden Erneuerungsschlüssel öffentlicher Anwendungen nur einmal zu –
     * deshalb läuft immer nur eine Erneuerung, alle gleichzeitigen Aufrufe warten auf dasselbe Ergebnis.
     */
    async fn refresh(&self) -> Result<()> {
        let fut = {
            let mut slot = self.0.refreshing.lock().unwrap();
            match slot.as_ref() {
                Some(f) => f.clone(),
                None => {
                    let me = self.clone();
                    let f = async move {
                        let r = me.do_refresh().await.map_err(|e| e.to_string());
                        *me.0.refreshing.lock().unwrap() = None;
                        r
                    }
                    .boxed()
                    .shared();
                    *slot = Some(f.clone());
                    f
                }
            }
        };
        fut.await.map_err(|e| anyhow!(e))
    }

    async fn do_refresh(&self) -> Result<()> {
        let a = self.auth();
        if a.refresh_token.is_empty() {
            bail!(t!("twitch.notConnectedToTwitch2"));
        }
        let body = form(&[("client_id", &a.client_id), ("grant_type", "refresh_token"), ("refresh_token", &a.refresh_token)]);
        // Netz weg: Anmeldung behalten und später erneut versuchen.
        let res = self
            .0
            .http
            .post(format!("{ID}/token"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| anyhow!(t!("twitch.twitchNotReachable", e)))?;
        let status = res.status().as_u16();
        let d: Value = res.json().await.unwrap_or_default();
        if !(200..300).contains(&status) {
            self.0.log.warn("Twitch", t!("twitch.refreshRejected", status, d["message"].as_str().unwrap_or(t!("twitch.noReasonGiven"))));
            // Nur bei echter Ablehnung abmelden, nicht bei Serverproblemen von Twitch.
            if status == 400 || status == 401 {
                self.save(|t| {
                    t.access_token.clear();
                    t.refresh_token.clear();
                    t.last_error = json!(t!("twitch.twitchLoginExpiredPlease2"));
                });
                self.close_events();
            }
            bail!(t!("twitch.twitchLoginExpiredPlease"));
        }
        self.store_tokens(&d);
        self.0.log.debug("Twitch", "Zugang erneuert");
        Ok(())
    }

    pub fn disconnect(&self) {
        let a = self.auth();
        if !a.access_token.is_empty() {
            let http = self.0.http.clone();
            let body = form(&[("client_id", &a.client_id), ("token", &a.access_token)]);
            tokio::spawn(async move {
                let _ = http.post(format!("{ID}/revoke")).header("Content-Type", "application/x-www-form-urlencoded").body(body).send().await;
            });
        }
        self.close_events();
        self.save(|t| {
            for f in [&mut t.access_token, &mut t.refresh_token, &mut t.user_id, &mut t.login, &mut t.display_name] {
                f.clear();
            }
            t.expires_at = json!(0);
            t.last_error = Value::Null;
        });
        self.0.log.info("Twitch", t!("common.disconnected"));
    }

    // ---------- Helix-API ----------

    pub async fn api(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        self.api_inner(method, path, body, true).await
    }

    fn api_inner<'a>(&'a self, method: &'a str, path: &'a str, body: Option<Value>, retry: bool) -> BoxFuture<'a, Result<Value>> {
        async move {
            let a = self.auth();
            if a.access_token.is_empty() {
                bail!(t!("twitch.notConnectedToTwitch"));
            }
            if self.expiring() {
                self.refresh().await?;
            }
            let token = self.auth().access_token;
            let m = reqwest::Method::from_bytes(method.as_bytes())?;
            let mut req = self.0.http.request(m, format!("{HELIX}{path}")).header("Client-Id", &a.client_id).header("Authorization", format!("Bearer {token}"));
            if let Some(b) = &body {
                req = req.header("Content-Type", "application/json").body(b.to_string());
            }
            let res = req.send().await?;
            let status = res.status();
            if status.as_u16() == 401 && retry {
                self.refresh().await?;
                return self.api_inner(method, path, body, false).await;
            }
            if status.as_u16() == 204 {
                return Ok(json!({}));
            }
            let d: Value = res.json().await.unwrap_or_default();
            if !status.is_success() {
                bail!("Twitch: {}", d["message"].as_str().unwrap_or(status.canonical_reason().unwrap_or(t!("common.error"))));
            }
            Ok(d)
        }
        .boxed()
    }

    pub fn me(&self) -> Result<String> {
        let id = self.auth().user_id;
        if id.is_empty() { Err(anyhow!(t!("twitch.notConnectedToTwitch"))) } else { Ok(id) }
    }

    pub async fn user_id(&self, login: &str) -> Result<String> {
        let name = login.trim().trim_start_matches('@').to_lowercase();
        if name.is_empty() {
            bail!(t!("twitch.noTwitchNameGiven"));
        }
        let enc = percent_encoding::utf8_percent_encode(&name, percent_encoding::NON_ALPHANUMERIC);
        let d = self.api("GET", &format!("/users?login={enc}"), None).await?;
        d["data"][0]["id"].as_str().map(String::from).ok_or_else(|| anyhow!(t!("twitch.twitchUserNotFound", name)))
    }

    /** Kurz zwischengespeicherte Abfragen (Live-Anzeigen fragen sonst alle 2 s). */
    async fn cached(&self, key: &str, ttl: Duration, f: impl std::future::Future<Output = Result<Value>>) -> Result<Value> {
        if let Some((at, v)) = self.0.cache.lock().unwrap().get(key)
            && at.elapsed() < ttl
        {
            return Ok(v.clone());
        }
        let v = f.await?;
        self.0.cache.lock().unwrap().insert(key.into(), (Instant::now(), v.clone()));
        Ok(v)
    }

    /** GET mit kurzem Zwischenspeicher (für Live-Anzeigen). */
    pub async fn get_cached(&self, key: &str, secs: u64, path: &str) -> Result<Value> {
        self.cached(key, Duration::from_secs(secs), async { self.api("GET", path, None).await }).await
    }

    /** Zwischengespeicherte Abfrage verwerfen (nach einer Änderung, damit die Anzeige sofort stimmt). */
    pub fn forget(&self, key: &str) {
        self.0.cache.lock().unwrap().retain(|k, _| !k.starts_with(key));
    }

    /** Laufender Stream oder null. */
    pub async fn stream(&self) -> Result<Value> {
        let me = self.me()?;
        self.cached("stream", Duration::from_secs(30), async { Ok(self.api("GET", &format!("/streams?user_id={me}"), None).await?["data"][0].clone()) }).await
    }

    pub async fn followers(&self) -> Result<i64> {
        let me = self.me()?;
        let v = self
            .cached("followers", Duration::from_secs(60), async {
                Ok(self.api("GET", &format!("/channels/followers?broadcaster_id={me}&first=1"), None).await?["total"].clone())
            })
            .await?;
        Ok(v.as_i64().unwrap_or(0))
    }

    // ---------- EventSub ----------

    /** Abonniert genau die Ereignisse, die aktive Events brauchen. */
    pub fn set_wanted(&self, trigger_types: &[String]) {
        let next: BTreeSet<String> = trigger_types.iter().filter(|t| !eventsub_types(t, "").is_empty()).cloned().collect();
        let same = *self.0.wanted.lock().unwrap() == next;
        *self.0.wanted.lock().unwrap() = next;
        if !same {
            self.sync_events();
        }
    }

    pub fn sync_events(&self) {
        self.close_events();
        if self.0.wanted.lock().unwrap().is_empty() || !self.connected() {
            return;
        }
        let me = self.clone();
        *self.0.events_task.lock().unwrap() = Some(tokio::spawn(async move { me.run_events().await }));
    }

    fn close_events(&self) {
        if let Some(t) = self.0.events_task.lock().unwrap().take() {
            t.abort();
        }
        *self.0.session_id.lock().unwrap() = None;
    }

    /** Hält die EventSub-Verbindung; zieht bei „session_reconnect“ um und verbindet nach Abbruch neu. */
    async fn run_events(&self) {
        let mut url = EVENTSUB.to_string();
        loop {
            match self.event_session(&url).await {
                Ok(Some(next)) => url = next,
                Ok(None) | Err(_) => {
                    *self.0.session_id.lock().unwrap() = None;
                    // Verbindung verloren: nach 10 s neu verbinden.
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    url = EVENTSUB.to_string();
                }
            }
        }
    }

    /** Eine Sitzung; liefert die neue Adresse, wenn Twitch umzieht. */
    async fn event_session(&self, url: &str) -> Result<Option<String>> {
        let (ws, _) = tokio_tungstenite::connect_async(url).await?;
        let (_sink, mut stream) = ws.split();
        let reconnecting = url != EVENTSUB;
        while let Some(msg) = stream.next().await {
            let Message::Text(text) = msg? else { continue };
            let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
            match msg["metadata"]["message_type"].as_str() {
                Some("session_welcome") => {
                    let session = msg["payload"]["session"]["id"].as_str().unwrap_or("").to_string();
                    *self.0.session_id.lock().unwrap() = Some(session.clone());
                    // Nach einem Umzug bleiben die Abos bestehen.
                    if !reconnecting {
                        self.subscribe_all(&session).await;
                    }
                }
                Some("session_reconnect") => {
                    // Twitch zieht um: neue Adresse übernehmen.
                    return Ok(msg["payload"]["session"]["reconnect_url"].as_str().map(String::from));
                }
                Some("notification") => {
                    let kind = msg["payload"]["subscription"]["type"].as_str().unwrap_or("");
                    if let Some((trigger, data)) = translate(kind, &msg["payload"]["event"]) {
                        self.0.last.lock().unwrap().insert(trigger.into(), data.clone());
                        let _ = self.0.events.send((trigger.into(), data));
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }

    async fn subscribe_all(&self, session: &str) {
        let Ok(user) = self.me() else { return };
        let wanted: Vec<String> = self.0.wanted.lock().unwrap().iter().cloned().collect();
        let mut ok = 0;
        for trigger in wanted {
            for (kind, version, condition) in eventsub_types(&trigger, &user) {
                let body = json!({ "type": kind, "version": version, "condition": condition, "transport": { "method": "websocket", "session_id": session } });
                match self.api("POST", "/eventsub/subscriptions", Some(body)).await {
                    Ok(_) => ok += 1,
                    Err(err) => self.0.log.warn("Twitch", t!("twitch.eventNotAvailable", kind, err)),
                }
            }
        }
        self.0.log.info("Twitch", format!("Ereignisse aktiv ({ok} Abos)"));
    }

    /** Letztes Ereignis eines Auslösers seit dem Start (z. B. „twitch_sub“), sonst null. */
    pub fn last(&self, trigger: &str) -> Value {
        self.0.last.lock().unwrap().get(trigger).cloned().unwrap_or(Value::Null)
    }

    /** Ist eine Anzeige, die von Ereignissen lebt, für diesen Auslöser auf Empfang? (abonniert und verbunden) */
    pub fn listening(&self, trigger: &str) -> bool {
        self.0.wanted.lock().unwrap().contains(trigger) && self.0.session_id.lock().unwrap().is_some()
    }

    pub fn stop(&self) {
        self.cancel_login();
        self.close_events();
    }
}

/** Berechtigungen aus SCOPES, die eine Anmeldung (Antwort von /validate) nicht hat. */
fn missing_scopes(have: &Value) -> Vec<String> {
    let have: Vec<&str> = have.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    SCOPES.iter().filter(|s| !have.contains(s)).map(|s| s.to_string()).collect()
}

fn tier(t: &Value) -> Value {
    match t.as_str() {
        Some("1000") => json!("1"),
        Some("2000") => json!("2"),
        Some("3000") => json!("3"),
        _ => t.clone(),
    }
}

/** Twitch-Ereignis → Auslöser von TouchDeck mit deutschen Platzhaltern. */
pub fn translate(kind: &str, e: &Value) -> Option<(&'static str, Value)> {
    let user = |anon: bool| if anon { json!("Anonym") } else { e["user_name"].clone() };
    let anon = e["is_anonymous"] == true;
    Some(match kind {
        "channel.follow" => ("twitch_follow", json!({ "Nutzer": e["user_name"] })),
        "channel.subscribe" => (
            "twitch_sub",
            json!({ "kind": if e["is_gift"] == true { "gift_received" } else { "new" }, "Nutzer": e["user_name"], "Stufe": tier(&e["tier"]), "Anzahl": 1 }),
        ),
        "channel.subscription.gift" => ("twitch_sub", json!({ "kind": "gift", "Nutzer": user(anon), "Stufe": tier(&e["tier"]), "Anzahl": e["total"] })),
        "channel.subscription.message" => (
            "twitch_sub",
            json!({ "kind": "resub", "Nutzer": e["user_name"], "Stufe": tier(&e["tier"]), "Monate": e["cumulative_months"], "Nachricht": e["message"]["text"].as_str().unwrap_or("") }),
        ),
        "channel.cheer" => ("twitch_cheer", json!({ "bits": e["bits"], "Nutzer": user(anon), "Bits": e["bits"], "Nachricht": e["message"] })),
        "channel.raid" => ("twitch_raid", json!({ "viewers": e["viewers"], "Nutzer": e["from_broadcaster_user_name"], "Zuschauer": e["viewers"] })),
        "channel.channel_points_custom_reward_redemption.add" => (
            "twitch_redeem",
            json!({ "reward": e["reward"]["title"], "Nutzer": e["user_name"], "Belohnung": e["reward"]["title"], "Kosten": e["reward"]["cost"], "Eingabe": e["user_input"].as_str().unwrap_or("") }),
        ),
        "channel.chat.message" => {
            let badges: Vec<Value> = e["badges"].as_array().into_iter().flatten().map(|b| b["set_id"].clone()).collect();
            let text = e["message"]["text"].as_str().unwrap_or("");
            let mut words = text.split_whitespace();
            let command = words.next().unwrap_or("");
            let args: Vec<&str> = words.collect();
            let arg = |i: usize| args.get(i).copied().unwrap_or("");
            (
                "twitch_chat",
                json!({ "text": text, "badges": badges, "Nutzer": e["chatter_user_name"], "Nachricht": text,
                        "Befehl": command, "Argumente": args.join(" "), "Arg1": arg(0), "Arg2": arg(1), "Arg3": arg(2) }),
            )
        }
        "stream.online" => ("twitch_stream", json!({ "state": "online", "Status": "online" })),
        "stream.offline" => ("twitch_stream", json!({ "state": "offline", "Status": "offline" })),
        "channel.ad_break.begin" => ("twitch_ad", json!({ "Dauer": e["duration_seconds"] })),
        "channel.poll.begin" | "channel.poll.end" => {
            let choices = e["choices"].as_array().cloned().unwrap_or_default();
            let names: Vec<&str> = choices.iter().filter_map(|c| c["title"].as_str()).collect();
            let best = choices.iter().max_by_key(|c| c["votes"].as_i64().unwrap_or(0));
            let state = if kind == "channel.poll.begin" { "begin" } else { "end" };
            (
                "twitch_poll",
                json!({ "state": state, "Titel": e["title"], "Auswahl": names.join(" / "),
                        "Gewinner": best.map_or(json!(""), |c| c["title"].clone()), "Stimmen": best.map_or(json!(0), |c| c["votes"].clone()) }),
            )
        }
        "channel.prediction.begin" | "channel.prediction.lock" | "channel.prediction.end" => {
            let state = kind.rsplit('.').next().unwrap_or("");
            let winner = e["outcomes"].as_array().into_iter().flatten().find(|o| o["id"] == e["winning_outcome_id"]);
            (
                "twitch_prediction",
                json!({ "state": state, "Titel": e["title"], "Gewinner": winner.map_or(json!(""), |o| o["title"].clone()),
                        "Punkte": winner.map_or(json!(0), |o| o["channel_points"].clone()) }),
            )
        }
        "channel.hype_train.begin" | "channel.hype_train.progress" | "channel.hype_train.end" => {
            let state = kind.rsplit('.').next().unwrap_or("");
            ("twitch_hype", json!({ "state": state, "Level": e["level"], "Fortschritt": e["progress"], "Ziel": e["goal"], "Gesamt": e["total"] }))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ereignisse_uebersetzen() {
        let (t, d) = translate("channel.subscription.gift", &json!({ "is_anonymous": true, "tier": "2000", "total": 5 })).unwrap();
        assert_eq!(t, "twitch_sub");
        assert_eq!(d["Nutzer"], "Anonym");
        assert_eq!(d["Stufe"], "2");
        let (_, chat) = translate("channel.chat.message", &json!({ "message": { "text": "!szene" }, "badges": [{ "set_id": "moderator" }], "chatter_user_name": "x" })).unwrap();
        assert_eq!(chat["badges"], json!(["moderator"]));
        assert!(translate("unbekannt", &json!({})).is_none());
        let (_, cmd) = translate("channel.chat.message", &json!({ "message": { "text": "!szene  Spiel 2" }, "chatter_user_name": "x" })).unwrap();
        assert_eq!((cmd["Befehl"].as_str(), cmd["Argumente"].as_str(), cmd["Arg1"].as_str(), cmd["Arg3"].as_str()), (Some("!szene"), Some("Spiel 2"), Some("Spiel"), Some("")));
        let poll = json!({ "title": "Was?", "choices": [{ "title": "A", "votes": 3 }, { "title": "B", "votes": 7 }] });
        let (t, d) = translate("channel.poll.end", &poll).unwrap();
        assert_eq!((t, d["state"].as_str(), d["Gewinner"].as_str(), d["Stimmen"].as_i64()), ("twitch_poll", Some("end"), Some("B"), Some(7)));
        let pred = json!({ "title": "Sieg?", "winning_outcome_id": "2", "outcomes": [{ "id": "1", "title": "Ja" }, { "id": "2", "title": "Nein", "channel_points": 500 }] });
        let (_, d) = translate("channel.prediction.end", &pred).unwrap();
        assert_eq!((d["state"].as_str(), d["Gewinner"].as_str()), (Some("end"), Some("Nein")));
        let (t, d) = translate("channel.hype_train.progress", &json!({ "level": 2, "progress": 300, "goal": 1000 })).unwrap();
        assert_eq!((t, d["state"].as_str(), d["Level"].as_i64()), ("twitch_hype", Some("progress"), Some(2)));
    }

    #[test]
    fn fehlende_berechtigungen() {
        let all: Vec<&str> = SCOPES.to_vec();
        assert!(missing_scopes(&json!(all)).is_empty());
        let old = json!(["user:read:chat", "bits:read"]);
        assert!(missing_scopes(&old).contains(&"channel:manage:polls".to_string()));
    }

    #[test]
    fn formular() {
        assert_eq!(form(&[("a", "b c"), ("x", "ä")]), "a=b%20c&x=%C3%A4");
    }
}
