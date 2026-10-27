// Discord: Steuerung des Discord-Programms am PC über dessen lokale Schnittstelle (RPC über Unix-Socket)
// und Nachrichten per Webhook. Für die Sprachsteuerung braucht jeder Nutzer eine eigene Discord-Anwendung.
use crate::t;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::core::log::Log;
use crate::core::store::{DiscordSettings, Store, new_id};

const API: &str = "https://discord.com/api/v10";
pub const SCOPES: [&str; 4] = ["rpc", "rpc.voice.read", "rpc.voice.write", "identify"];
pub const REDIRECT_URI: &str = "http://localhost";

const OP_HANDSHAKE: i32 = 0;
const OP_FRAME: i32 = 1;
const OP_CLOSE: i32 = 2;
const OP_PING: i32 = 3;
const OP_PONG: i32 = 4;

/** Unter Windows spricht Discord über benannte Pipes; Discord nimmt die erste freie von 0 bis 9. */
#[cfg(windows)]
pub fn socket_paths() -> Vec<PathBuf> {
    (0..10).map(|i| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{i}"))).collect()
}

type Reader = Box<dyn AsyncRead + Unpin + Send>;
type Writer = Box<dyn AsyncWrite + Unpin + Send>;

/** Verbindet mit dem ersten erreichbaren Socket (Linux) bzw. der ersten offenen Pipe (Windows). */
async fn connect(paths: Vec<PathBuf>) -> Result<Option<(Reader, Writer)>> {
    for path in paths {
        #[cfg(unix)]
        {
            if !path.exists() {
                continue;
            }
            let sock = tokio::net::UnixStream::connect(&path).await.map_err(|e| anyhow!("Discord: {e}"))?;
            let (r, w) = sock.into_split();
            return Ok(Some((Box::new(r), Box::new(w))));
        }
        #[cfg(windows)]
        if let Ok(pipe) = tokio::net::windows::named_pipe::ClientOptions::new().open(&path) {
            let (r, w) = tokio::io::split(pipe);
            return Ok(Some((Box::new(r), Box::new(w))));
        }
    }
    Ok(None)
}

/** Mögliche Orte des Sockets: normales Paket, Flatpak, Snap; Discord nimmt den ersten freien von 0 bis 9. */
#[cfg(not(windows))]
pub fn socket_paths() -> Vec<PathBuf> {
    let mut bases: Vec<String> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().filter_map(|k| std::env::var(k).ok()).filter(|v| !v.is_empty()).collect();
    bases.push("/tmp".into());
    let subdirs = ["", "app/com.discordapp.Discord", "app/com.discordapp.DiscordCanary", "snap.discord", "snap.discord-canary", ".flatpak/com.discordapp.Discord/xdg-run"];
    let mut out = Vec::new();
    for base in &bases {
        for sub in subdirs {
            for i in 0..10 {
                let p = PathBuf::from(base).join(sub).join(format!("discord-ipc-{i}"));
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn encode(op: i32, data: &Value) -> Vec<u8> {
    let body = data.to_string().into_bytes();
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&op.to_le_bytes());
    out.extend_from_slice(&(body.len() as i32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

struct Conn {
    tx: mpsc::UnboundedSender<Vec<u8>>,
    alive: Arc<AtomicBool>,
}

struct Inner {
    store: Arc<Mutex<Store>>,
    log: Arc<Log>,
    http: reqwest::Client,
    paths: Box<dyn Fn() -> Vec<PathBuf> + Send + Sync>,
    conn: tokio::sync::Mutex<Option<Conn>>,
    pending: Pending,
    authenticated: AtomicBool,
    authorizing: AtomicBool,
    last_failure: Mutex<Option<(Instant, String)>>,
    /// Zuletzt bekannte Spracheinstellungen und aktueller Sprachkanal (id, Name).
    voice: Mutex<Option<Value>>,
    channel: Mutex<Option<(String, String)>>,
    events: broadcast::Sender<(String, Value)>,
}

#[derive(Clone)]
pub struct DiscordClient(Arc<Inner>);

impl DiscordClient {
    pub fn new(store: Arc<Mutex<Store>>, log: Arc<Log>) -> Self {
        Self::with_paths(store, log, Box::new(socket_paths))
    }

    pub fn with_paths(store: Arc<Mutex<Store>>, log: Arc<Log>, paths: Box<dyn Fn() -> Vec<PathBuf> + Send + Sync>) -> Self {
        DiscordClient(Arc::new(Inner {
            store,
            log,
            http: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("HTTP-Client"),
            paths,
            conn: tokio::sync::Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            authenticated: AtomicBool::new(false),
            authorizing: AtomicBool::new(false),
            last_failure: Mutex::new(None),
            voice: Mutex::new(None),
            channel: Mutex::new(None),
            events: broadcast::channel(64).0,
        }))
    }

    fn auth(&self) -> DiscordSettings {
        self.0.store.lock().unwrap().settings().discord.clone()
    }

    fn save(&self, f: impl FnOnce(&mut DiscordSettings)) {
        self.0.store.lock().unwrap().set_discord_auth(f);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(String, Value)> {
        self.0.events.subscribe()
    }

    pub fn configured(&self) -> bool {
        let a = self.auth();
        !a.client_id.is_empty() && !a.access_token.is_empty()
    }

    pub fn connected(&self) -> bool {
        self.0.authenticated.load(Ordering::Relaxed)
    }

    pub fn voice(&self) -> Option<Value> {
        self.0.voice.lock().unwrap().clone()
    }

    pub fn channel(&self) -> Option<(String, String)> {
        self.0.channel.lock().unwrap().clone()
    }

    pub fn status(&self) -> Value {
        if self.0.authorizing.load(Ordering::Relaxed) {
            return json!({ "state": "waiting" });
        }
        let a = self.auth();
        if !a.access_token.is_empty() {
            return json!({ "state": "connected", "user": a.user, "running": self.connected() });
        }
        json!({ "state": if a.last_error.is_null() { "idle" } else { "error" }, "error": a.last_error })
    }

    // ---------- Verbindung zum Discord-Programm ----------

    /** Verbindet mit dem laufenden Discord und meldet sich an (nutzt die gespeicherte Anmeldung). */
    pub async fn connect(&self) -> Result<()> {
        let mut guard = self.0.conn.lock().await;
        if guard.as_ref().is_some_and(|c| c.alive.load(Ordering::Relaxed)) && self.connected() {
            return Ok(());
        }
        if let Some((at, err)) = self.0.last_failure.lock().unwrap().as_ref()
            && at.elapsed() < Duration::from_secs(10)
        {
            bail!("{err}");
        }
        let result: Result<()> = async {
            let conn = self.open().await?;
            let tx = conn.tx.clone();
            *guard = Some(conn);
            if !self.auth().access_token.is_empty() {
                self.authenticate(&tx).await?;
            }
            Ok(())
        }
        .await;
        if let Err(err) = &result {
            *self.0.last_failure.lock().unwrap() = Some((Instant::now(), err.to_string()));
            self.close_conn(&mut guard);
        }
        result
    }

    fn close_conn(&self, guard: &mut Option<Conn>) {
        if let Some(c) = guard.take() {
            c.alive.store(false, Ordering::Relaxed);
        }
        self.0.authenticated.store(false, Ordering::Relaxed);
        for (_, p) in self.0.pending.lock().unwrap().drain() {
            let _ = p.send(Err(t!("discord.disconnectedFromDiscord").into()));
        }
    }

    async fn open(&self) -> Result<Conn> {
        let client_id = self.auth().client_id;
        if client_id.is_empty() {
            bail!(t!("discord.pleaseEnterDiscordApplication"));
        }
        let Some((mut reader, mut writer)) = connect((self.0.paths)()).await? else { bail!(t!("discord.discordIsNotRunning")) };
        let alive = Arc::new(AtomicBool::new(true));
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let a = alive.clone();
        tokio::spawn(async move {
            while let Some(bytes) = rx.recv().await {
                if writer.write_all(&bytes).await.is_err() {
                    break;
                }
            }
            a.store(false, Ordering::Relaxed);
        });
        let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();
        let mut ready_tx = Some(ready_tx);
        let me = self.clone();
        let (a, ping_tx) = (alive.clone(), tx.clone());
        tokio::spawn(async move {
            loop {
                let mut head = [0u8; 8];
                if reader.read_exact(&mut head).await.is_err() {
                    break;
                }
                let op = i32::from_le_bytes(head[..4].try_into().expect("4 Bytes"));
                let len = i32::from_le_bytes(head[4..].try_into().expect("4 Bytes")).max(0) as usize;
                let mut body = vec![0u8; len];
                if reader.read_exact(&mut body).await.is_err() {
                    break;
                }
                let msg: Value = serde_json::from_slice(&body).unwrap_or_default();
                match op {
                    OP_PING => {
                        let _ = ping_tx.send(encode(OP_PONG, &msg));
                    }
                    OP_CLOSE => {
                        if let Some(r) = ready_tx.take() {
                            let reason = msg["message"].as_str().map_or_else(|| msg["code"].to_string(), String::from);
                            let _ = r.send(Err(t!("discord.discordRefusedIsClient", reason)));
                        }
                        break;
                    }
                    OP_FRAME => {
                        if msg["cmd"] == "DISPATCH" && msg["evt"] == "READY" {
                            if let Some(r) = ready_tx.take() {
                                let _ = r.send(Ok(()));
                            }
                        } else if msg["cmd"] == "DISPATCH" {
                            me.on_dispatch(msg["evt"].as_str().unwrap_or(""), &msg["data"]);
                        } else if let Some(p) = msg["nonce"].as_str().and_then(|n| me.0.pending.lock().unwrap().remove(n)) {
                            let _ = p.send(if msg["evt"] == "ERROR" {
                                Err(format!("Discord: {}", msg["data"]["message"].as_str().unwrap_or(t!("common.error"))))
                            } else {
                                Ok(if msg["data"].is_null() { json!({}) } else { msg["data"].clone() })
                            });
                        }
                    }
                    _ => {}
                }
            }
            a.store(false, Ordering::Relaxed);
            me.0.authenticated.store(false, Ordering::Relaxed);
            for (_, p) in me.0.pending.lock().unwrap().drain() {
                let _ = p.send(Err(t!("discord.disconnectedFromDiscord").into()));
            }
        });
        let _ = tx.send(encode(OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id })));
        match tokio::time::timeout(Duration::from_secs(5), ready_rx).await {
            Ok(Ok(Ok(()))) => Ok(Conn { tx, alive }),
            Ok(Ok(Err(msg))) => bail!("{msg}"),
            Ok(Err(_)) => bail!(t!("discord.discordClosedConnection")),
            Err(_) => bail!(t!("discord.discordIsNotResponding2")),
        }
    }

    /** Einen RPC-Befehl an Discord schicken. */
    async fn send(&self, tx: &mpsc::UnboundedSender<Vec<u8>>, cmd: &str, args: Value, evt: Option<&str>) -> Result<Value> {
        let nonce = new_id("n_");
        let (done, wait) = oneshot::channel();
        self.0.pending.lock().unwrap().insert(nonce.clone(), done);
        let mut frame = json!({ "cmd": cmd, "args": args, "nonce": nonce });
        if let Some(e) = evt {
            frame["evt"] = json!(e);
        }
        if tx.send(encode(OP_FRAME, &frame)).is_err() {
            self.0.pending.lock().unwrap().remove(&nonce);
            bail!(t!("discord.notConnectedToDiscord2"));
        }
        let timeout = Duration::from_secs(if cmd == "AUTHORIZE" { 120 } else { 8 });
        match tokio::time::timeout(timeout, wait).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(anyhow!(e)),
            Ok(Err(_)) => bail!(t!("discord.disconnectedFromDiscord")),
            Err(_) => {
                self.0.pending.lock().unwrap().remove(&nonce);
                bail!(t!("discord.discordIsNotResponding", cmd))
            }
        }
    }

    async fn sender(&self) -> Result<mpsc::UnboundedSender<Vec<u8>>> {
        self.0.conn.lock().await.as_ref().filter(|c| c.alive.load(Ordering::Relaxed)).map(|c| c.tx.clone()).ok_or_else(|| anyhow!(t!("discord.notConnectedToDiscord2")))
    }

    /** Verbinden (falls nötig) und Befehl ausführen. */
    pub async fn call(&self, cmd: &str, args: Value) -> Result<Value> {
        if self.auth().access_token.is_empty() {
            bail!(t!("discord.notConnectedToDiscord"));
        }
        self.connect().await?;
        let tx = self.sender().await?;
        self.send(&tx, cmd, args, None).await
    }

    // ---------- Anmeldung ----------

    /** Erstes Verbinden: Discord zeigt ein Fenster „TouchDeck möchte … – Autorisieren“. */
    pub async fn login(&self) -> Result<()> {
        let a = self.auth();
        if a.client_id.is_empty() || a.client_secret.is_empty() {
            bail!(t!("discord.pleaseEnterClientId"));
        }
        let mut guard = self.0.conn.lock().await;
        self.close_conn(&mut guard);
        *self.0.last_failure.lock().unwrap() = None;
        self.0.authorizing.store(true, Ordering::Relaxed);
        let result: Result<()> = async {
            let conn = self.open().await?;
            let tx = conn.tx.clone();
            *guard = Some(conn);
            // Discord zeigt jetzt im Programm die Freigabe-Abfrage.
            let d = self.send(&tx, "AUTHORIZE", json!({ "client_id": a.client_id, "scopes": SCOPES }), None).await?;
            let code = d["code"].as_str().unwrap_or("").to_string();
            let tokens = self.token(&[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", REDIRECT_URI)]).await?;
            self.save(|s| {
                (s.access_token, s.refresh_token, s.expires_at) = tokens;
                s.last_error = Value::Null;
            });
            self.authenticate(&tx).await?;
            self.0.log.info("Discord", format!("Verbunden als {}", self.auth().user));
            Ok(())
        }
        .await;
        self.0.authorizing.store(false, Ordering::Relaxed);
        if let Err(err) = &result {
            let msg = err.to_string();
            self.save(|s| s.last_error = json!(msg));
            self.close_conn(&mut guard);
        }
        result
    }

    async fn token(&self, form: &[(&str, &str)]) -> Result<(String, String, Value)> {
        let a = self.auth();
        let mut pairs = vec![("client_id", a.client_id.as_str()), ("client_secret", a.client_secret.as_str())];
        pairs.extend_from_slice(form);
        let body = pairs
            .iter()
            .map(|(k, v)| format!("{k}={}", percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)))
            .collect::<Vec<_>>()
            .join("&");
        let res = self.0.http.post(format!("{API}/oauth2/token")).header("Content-Type", "application/x-www-form-urlencoded").body(body).send().await?;
        let status = res.status();
        let d: Value = res.json().await.unwrap_or_default();
        if !status.is_success() {
            let why = d["error_description"].as_str().or(d["error"].as_str()).map_or_else(|| status.as_u16().to_string(), String::from);
            bail!("Discord-Anmeldung: {why}");
        }
        let expires = now_ms() + d["expires_in"].as_i64().unwrap_or(604_800) * 1000;
        Ok((d["access_token"].as_str().unwrap_or("").into(), d["refresh_token"].as_str().unwrap_or("").into(), json!(expires)))
    }

    async fn authenticate(&self, tx: &mpsc::UnboundedSender<Vec<u8>>) -> Result<()> {
        let a = self.auth();
        let expiring = a.expires_at.as_i64().is_some_and(|e| e != 0 && e - now_ms() < 3_600_000);
        if expiring && !a.refresh_token.is_empty() {
            let tokens = self.token(&[("grant_type", "refresh_token"), ("refresh_token", &a.refresh_token)]).await?;
            self.save(|s| (s.access_token, s.refresh_token, s.expires_at) = tokens);
        }
        match self.send(tx, "AUTHENTICATE", json!({ "access_token": self.auth().access_token }), None).await {
            Ok(d) => {
                self.0.authenticated.store(true, Ordering::Relaxed);
                let user = d["user"]["global_name"].as_str().filter(|s| !s.is_empty()).or(d["user"]["username"].as_str()).unwrap_or("").to_string();
                self.save(|s| {
                    s.user = user;
                    s.last_error = Value::Null;
                });
            }
            Err(err) => {
                self.save(|s| {
                    s.access_token.clear();
                    s.refresh_token.clear();
                    s.last_error = json!(t!("discord.discordLoginExpiredPlease"));
                });
                return Err(err);
            }
        }
        // Spracheinstellungen und Kanalwechsel abonnieren (für Live-Anzeigen und Events).
        let me = self.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let r: Result<()> = async {
                me.send(&tx, "SUBSCRIBE", json!({}), Some("VOICE_SETTINGS_UPDATE")).await?;
                me.send(&tx, "SUBSCRIBE", json!({}), Some("VOICE_CHANNEL_SELECT")).await?;
                *me.0.voice.lock().unwrap() = Some(me.send(&tx, "GET_VOICE_SETTINGS", json!({}), None).await?);
                let ch = me.send(&tx, "GET_SELECTED_VOICE_CHANNEL", json!({}), None).await?;
                *me.0.channel.lock().unwrap() = ch["id"].as_str().map(|id| (id.to_string(), ch["name"].as_str().unwrap_or("").to_string()));
                Ok(())
            }
            .await;
            if let Err(err) = r {
                me.0.log.warn("Discord", t!("discord.eventsNotAvailable", err));
            }
        });
        Ok(())
    }

    fn emit(&self, kind: &str, data: Value) {
        let _ = self.0.events.send((kind.into(), data));
    }

    fn on_dispatch(&self, evt: &str, data: &Value) {
        match evt {
            "VOICE_SETTINGS_UPDATE" => {
                let before = self.0.voice.lock().unwrap().replace(data.clone());
                if let Some(b) = before {
                    if b["mute"] != data["mute"] {
                        let m = data["mute"] == true;
                        self.emit("discord_voice", json!({ "state": if m { "muted" } else { "unmuted" }, "Status": if m { "stumm" } else { "an" } }));
                    }
                    if b["deaf"] != data["deaf"] {
                        let d = data["deaf"] == true;
                        self.emit("discord_voice", json!({ "state": if d { "deafened" } else { "undeafened" }, "Status": if d { "taub" } else { "hört" } }));
                    }
                }
            }
            "VOICE_CHANNEL_SELECT" => {
                let name = data["channel_name"].as_str().unwrap_or("").to_string();
                let now = data["channel_id"].as_str().map(|id| (id.to_string(), name.clone()));
                let before = std::mem::replace(&mut *self.0.channel.lock().unwrap(), now.clone());
                if let Some((id, _)) = &now {
                    // Den richtigen Kanalnamen nachladen.
                    let (me, id) = (self.clone(), id.clone());
                    tokio::spawn(async move {
                        if let Ok(tx) = me.sender().await
                            && let Ok(c) = me.send(&tx, "GET_CHANNEL", json!({ "channel_id": id }), None).await
                        {
                            *me.0.channel.lock().unwrap() = Some((c["id"].as_str().unwrap_or("").into(), c["name"].as_str().unwrap_or("").into()));
                        }
                    });
                }
                match (&before, &now) {
                    (None, Some((id, _))) => self.emit("discord_channel", json!({ "state": "joined", "channel": id, "Kanal": name })),
                    (Some((id, n)), None) => self.emit("discord_channel", json!({ "state": "left", "channel": id, "Kanal": n })),
                    (Some((a, _)), Some((b, _))) if a != b => self.emit("discord_channel", json!({ "state": "joined", "channel": b, "Kanal": name })),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub async fn disconnect(&self) {
        let mut guard = self.0.conn.lock().await;
        self.close_conn(&mut guard);
        self.save(|s| {
            s.access_token.clear();
            s.refresh_token.clear();
            s.expires_at = json!(0);
            s.user.clear();
            s.last_error = Value::Null;
        });
        self.0.log.info("Discord", t!("common.disconnected"));
    }

    // ---------- Hilfen für Aktionen ----------

    pub async fn voice_settings(&self) -> Result<Value> {
        let v = self.call("GET_VOICE_SETTINGS", json!({})).await?;
        *self.0.voice.lock().unwrap() = Some(v.clone());
        Ok(v)
    }

    pub async fn set_voice(&self, patch: Value) -> Result<Value> {
        let v = self.call("SET_VOICE_SETTINGS", patch).await?;
        *self.0.voice.lock().unwrap() = Some(v.clone());
        Ok(v)
    }

    /** Alle Sprachkanäle aller Server als „Server › Kanal“. */
    pub async fn voice_channels(&self) -> Result<Vec<Value>> {
        let guilds = self.call("GET_GUILDS", json!({})).await?;
        let mut out = Vec::new();
        for g in guilds["guilds"].as_array().into_iter().flatten() {
            let chans = self.call("GET_CHANNELS", json!({ "guild_id": g["id"] })).await?;
            for c in chans["channels"].as_array().into_iter().flatten() {
                if c["type"] == 2 || c["type"] == 13 {
                    out.push(json!({ "value": c["id"], "label": format!("{} › {}", g["name"].as_str().unwrap_or(""), c["name"].as_str().unwrap_or("")) }));
                }
            }
        }
        Ok(out)
    }
}

/** Nachricht über einen Discord-Webhook senden (ohne Anmeldung, nur mit der Webhook-Adresse). */
pub async fn send_webhook(url: &str, content: &str, username: &str) -> Result<()> {
    if !valid_webhook(url) {
        bail!(t!("discord.invalidWebhookAddressDiscord"));
    }
    let mut body = json!({ "content": content.chars().take(2000).collect::<String>() });
    if !username.is_empty() {
        body["username"] = json!(username.chars().take(80).collect::<String>());
    }
    let res = reqwest::Client::new()
        .post(url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    if !res.status().is_success() {
        bail!(t!("discord.discordWebhookError", res.status().as_u16()));
    }
    Ok(())
}

/** https://(sub.)discord(app).com/api/webhooks/<Zahl>/<Schlüssel> */
fn valid_webhook(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let Some((host, path)) = rest.split_once('/') else { return false };
    let host_ok = ["discord.com", "discordapp.com"].iter().any(|d| host == *d || host.strip_suffix(d).is_some_and(|sub| sub.ends_with('.') && sub[..sub.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '_')));
    let Some(path) = path.strip_prefix("api/webhooks/") else { return false };
    let Some((id, key)) = path.split_once('/') else { return false };
    host_ok && !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) && !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || "_-".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_adressen() {
        assert!(valid_webhook("https://discord.com/api/webhooks/123/abc-DEF_1"));
        assert!(valid_webhook("https://ptb.discord.com/api/webhooks/123/abc"));
        assert!(!valid_webhook("https://evil.com/api/webhooks/123/abc"));
        assert!(!valid_webhook("https://discord.com/api/webhooks/12a/abc"));
        assert!(!valid_webhook("http://discord.com/api/webhooks/123/abc"));
    }

    #[test]
    fn rahmen() {
        let f = encode(OP_FRAME, &json!({ "a": 1 }));
        assert_eq!(&f[..4], &1i32.to_le_bytes());
        assert_eq!(&f[4..8], &7i32.to_le_bytes());
    }

    #[cfg(unix)]
    async fn read(s: &mut tokio::net::UnixStream) -> Option<Value> {
        let mut head = [0u8; 8];
        s.read_exact(&mut head).await.ok()?;
        let len = i32::from_le_bytes(head[4..].try_into().unwrap()) as usize;
        let mut body = vec![0u8; len];
        s.read_exact(&mut body).await.ok()?;
        serde_json::from_slice(&body).ok()
    }

    /** Ersatz-Discord über einen Unix-Socket: Handshake, Anmeldung und Spracheinstellungen. */
    #[cfg(unix)]
    #[tokio::test]
    async fn spricht_mit_discord() {
        use tokio::net::UnixListener;
        let dir = std::env::temp_dir().join(new_id("td-discord-"));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("discord-ipc-0");
        let listener = UnixListener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            read(&mut s).await.unwrap();
            s.write_all(&encode(OP_FRAME, &json!({ "cmd": "DISPATCH", "evt": "READY", "data": {} }))).await.unwrap();
            while let Some(msg) = read(&mut s).await {
                let data = match msg["cmd"].as_str().unwrap_or("") {
                    "AUTHENTICATE" => json!({ "user": { "username": "tester" } }),
                    "GET_VOICE_SETTINGS" => json!({ "mute": true, "deaf": false }),
                    _ => json!({}),
                };
                s.write_all(&encode(OP_FRAME, &json!({ "cmd": msg["cmd"], "nonce": msg["nonce"], "data": data }))).await.unwrap();
            }
        });
        let data = std::env::temp_dir().join(new_id("td-data-"));
        let store = Arc::new(Mutex::new(Store::open(&data).unwrap()));
        store.lock().unwrap().set_discord_auth(|d| {
            d.client_id = "1".into();
            d.access_token = "t".into();
        });
        let log = Arc::new(Log::new(data.join("logs")));
        let sock2 = sock.clone();
        let discord = DiscordClient::with_paths(store.clone(), log, Box::new(move || vec![sock2.clone()]));
        let v = discord.voice_settings().await.unwrap();
        assert_eq!(v["mute"], true);
        assert_eq!(store.lock().unwrap().settings().discord.user, "tester");
        assert!(discord.connected());
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(data);
    }
}
