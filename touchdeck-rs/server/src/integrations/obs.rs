// Anbindung an OBS Studio über obs-websocket 5 (ab OBS 28 eingebaut, Werkzeuge → WebSocket-Servereinstellungen).
use crate::t;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::core::store::{ObsSettings, Store};
use crate::system::{os, sort_key};

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>;

struct Conn {
    tx: mpsc::UnboundedSender<Message>,
    alive: Arc<AtomicBool>,
}

pub struct ObsClient {
    settings: Box<dyn Fn() -> ObsSettings + Send + Sync>,
    conn: tokio::sync::Mutex<Option<Conn>>,
    pending: Pending,
    next_id: AtomicU64,
    /// Nach einem Fehlschlag 10 s warten, statt OBS bei jeder Live-Abfrage erneut anzufragen.
    last_failure: Mutex<Option<(Instant, String)>>,
    events: broadcast::Sender<(String, Value)>,
}

fn sha256_b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(s.as_bytes()))
}

impl ObsClient {
    pub fn new(store: Arc<Mutex<Store>>) -> Self {
        Self::with_settings(Box::new(move || store.lock().unwrap().settings().obs.clone()))
    }

    pub fn with_settings(settings: Box<dyn Fn() -> ObsSettings + Send + Sync>) -> Self {
        ObsClient {
            settings,
            conn: tokio::sync::Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            last_failure: Mutex::new(None),
            events: broadcast::channel(256).0,
        }
    }

    /** OBS-Ereignisse (Szenenwechsel, Stream, Aufnahme …) als (eventType, eventData). */
    pub fn subscribe(&self) -> broadcast::Receiver<(String, Value)> {
        self.events.subscribe()
    }

    pub fn connected(&self) -> bool {
        self.conn.try_lock().is_ok_and(|c| c.as_ref().is_some_and(|c| c.alive.load(Ordering::Relaxed)))
    }

    /** Alte Verbindung trennen und Fehlversuch vergessen (z. B. nach geänderten Einstellungen). */
    pub async fn reset(&self) {
        if let Some(c) = self.conn.lock().await.take() {
            let _ = c.tx.send(Message::Close(None));
        }
        *self.last_failure.lock().unwrap() = None;
    }

    /** Verbindung herstellen (oder bestehende wiederverwenden). */
    pub async fn connect(&self) -> Result<mpsc::UnboundedSender<Message>> {
        let mut guard = self.conn.lock().await;
        if let Some(c) = guard.as_ref().filter(|c| c.alive.load(Ordering::Relaxed)) {
            return Ok(c.tx.clone());
        }
        if let Some((at, err)) = self.last_failure.lock().unwrap().as_ref()
            && at.elapsed() < Duration::from_secs(10)
        {
            bail!("{err}");
        }
        match tokio::time::timeout(Duration::from_secs(4), self.open()).await.unwrap_or_else(|_| Err(anyhow!(t!("obs.obsIsNotResponding2")))) {
            Ok(conn) => {
                *self.last_failure.lock().unwrap() = None;
                let tx = conn.tx.clone();
                *guard = Some(conn);
                Ok(tx)
            }
            Err(err) => {
                *self.last_failure.lock().unwrap() = Some((Instant::now(), err.to_string()));
                *guard = None;
                Err(err)
            }
        }
    }

    async fn open(&self) -> Result<Conn> {
        let s = (self.settings)();
        let url = format!("ws://{}:{}", s.host, s.port);
        let (ws, _) = tokio_tungstenite::connect_async(&url).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("refused") || msg.contains("Connection refused") {
                anyhow!(t!("obs.obsIsNotReachable"))
            } else {
                anyhow!(t!("common.obsConnection", msg))
            }
        })?;
        let (mut sink, mut stream) = ws.split();

        // Begrüßung (op 0) → Anmeldung (op 1) → bestätigt (op 2).
        let hello = next_json(&mut stream).await?;
        if hello["op"] != 0 {
            bail!(t!("obs.unexpectedAnswer"));
        }
        // Alle Ereignisse außer den sehr häufigen (Pegel, aktive Quellen).
        let mut identify = json!({ "rpcVersion": 1, "eventSubscriptions": 0x7ff });
        let auth = &hello["d"]["authentication"];
        if auth.is_object() {
            if s.password.is_empty() {
                bail!(t!("obs.obsRequiresPasswordPlease"));
            }
            let secret = sha256_b64(&format!("{}{}", s.password, auth["salt"].as_str().unwrap_or("")));
            identify["authentication"] = json!(sha256_b64(&format!("{secret}{}", auth["challenge"].as_str().unwrap_or(""))));
        }
        sink.send(Message::Text(json!({ "op": 1, "d": identify }).to_string().into())).await?;
        let ready = next_json(&mut stream).await?;
        if ready["op"] != 2 {
            bail!(t!("obs.obsConnectionLoginFailed"));
        }

        let alive = Arc::new(AtomicBool::new(true));
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let close = matches!(msg, Message::Close(_));
                if sink.send(msg).await.is_err() || close {
                    break;
                }
            }
        });
        let (pending, events, a) = (self.pending.clone(), self.events.clone(), alive.clone());
        tokio::spawn(async move {
            while let Some(Ok(msg)) = stream.next().await {
                let Message::Text(text) = msg else { continue };
                let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
                let d = &msg["d"];
                match msg["op"].as_u64() {
                    Some(5) => {
                        let _ = events.send((d["eventType"].as_str().unwrap_or("").to_string(), d.get("eventData").cloned().unwrap_or(json!({}))));
                    }
                    Some(7) => {
                        let Some(p) = d["requestId"].as_str().and_then(|id| pending.lock().unwrap().remove(id)) else { continue };
                        let st = &d["requestStatus"];
                        let _ = p.send(if st["result"] == true {
                            Ok(d.get("responseData").cloned().unwrap_or(json!({})))
                        } else {
                            Err(format!("OBS: {}", st["comment"].as_str().map_or_else(|| t!("obs.error", st["code"]), String::from)))
                        });
                    }
                    _ => {}
                }
            }
            a.store(false, Ordering::Relaxed);
            // Offene Anfragen mit Fehler beenden.
            for (_, p) in pending.lock().unwrap().drain() {
                let _ = p.send(Err(t!("obs.disconnectedFromObs").into()));
            }
        });
        Ok(Conn { tx, alive })
    }

    pub async fn call(&self, request_type: &str, data: Value) -> Result<Value> {
        let tx = self.connect().await?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let (done, wait) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), done);
        let msg = json!({ "op": 6, "d": { "requestType": request_type, "requestId": id, "requestData": data } });
        if tx.send(Message::Text(msg.to_string().into())).is_err() {
            self.pending.lock().unwrap().remove(&id);
            bail!(t!("obs.disconnectedFromObs"));
        }
        match tokio::time::timeout(Duration::from_secs(5), wait).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(anyhow!(e)),
            Ok(Err(_)) => bail!(t!("obs.disconnectedFromObs")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                bail!(t!("obs.obsIsNotResponding", request_type))
            }
        }
    }

    pub async fn call0(&self, request_type: &str) -> Result<Value> {
        self.call(request_type, json!({})).await
    }

    pub async fn scenes(&self) -> Result<Vec<String>> {
        let r = self.call0("GetSceneList").await?;
        let mut names = names(&r["scenes"], "sceneName");
        names.reverse();
        Ok(names)
    }

    pub async fn inputs(&self) -> Result<Vec<String>> {
        let r = self.call0("GetInputList").await?;
        let mut names = names(&r["inputs"], "inputName");
        names.sort_by_key(|n| sort_key(n));
        Ok(names)
    }

    /** Alle Quellen aller Szenen als „Szene › Quelle“. */
    pub async fn scene_items(&self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for scene in self.scenes().await? {
            let r = self.call("GetSceneItemList", json!({ "sceneName": scene })).await?;
            for source in names(&r["sceneItems"], "sourceName") {
                out.push(json!({ "value": format!("{scene}␟{source}"), "label": format!("{scene} › {source}") }));
            }
        }
        Ok(out)
    }

    pub async fn set_source_visible(&self, target: &str, mode: &str) -> Result<bool> {
        let Some((scene, source)) = target.split_once('␟') else { bail!(t!("common.noSourceSelected")) };
        let id = self.call("GetSceneItemId", json!({ "sceneName": scene, "sourceName": source })).await?["sceneItemId"].clone();
        let enabled = if mode == "toggle" {
            self.call("GetSceneItemEnabled", json!({ "sceneName": scene, "sceneItemId": id })).await?["sceneItemEnabled"] != true
        } else {
            mode == "show"
        };
        self.call("SetSceneItemEnabled", json!({ "sceneName": scene, "sceneItemId": id, "sceneItemEnabled": enabled })).await?;
        Ok(enabled)
    }

    /** Eingänge, deren Art (inputKind) zu matches passt. */
    pub async fn inputs_of_kind(&self, matches: impl Fn(&str) -> bool) -> Result<Vec<Value>> {
        let r = self.call0("GetInputList").await?;
        Ok(r["inputs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|i| matches(i["inputKind"].as_str().unwrap_or("")))
            .filter_map(|i| i["inputName"].as_str())
            .map(|n| json!({ "value": n, "label": n }))
            .collect())
    }

    /** Alle Filter aller Szenen und Quellen als „Quelle › Filter“. */
    pub async fn filters(&self) -> Result<Vec<Value>> {
        let mut sources = self.scenes().await?;
        sources.extend(self.inputs().await?);
        let mut out = Vec::new();
        for source in sources {
            let r = self.call("GetSourceFilterList", json!({ "sourceName": source })).await?;
            for f in names(&r["filters"], "filterName") {
                out.push(json!({ "value": format!("{source}␟{f}"), "label": format!("{source} › {f}") }));
            }
        }
        Ok(out)
    }

    pub async fn set_filter(&self, target: &str, mode: &str) -> Result<bool> {
        let Some((source, filter)) = target.split_once('␟') else { bail!(t!("obs.noFilterSelected")) };
        let enabled = if mode == "toggle" {
            self.call("GetSourceFilter", json!({ "sourceName": source, "filterName": filter })).await?["filterEnabled"] != true
        } else {
            mode == "on"
        };
        self.call("SetSourceFilterEnabled", json!({ "sourceName": source, "filterName": filter, "filterEnabled": enabled })).await?;
        Ok(enabled)
    }

    pub async fn list_of(&self, request: &str, key: &str, field: &str) -> Result<Vec<Value>> {
        let r = self.call0(request).await?;
        Ok(r[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().or_else(|| x[field].as_str()))
            .map(|v| json!({ "value": v, "label": v }))
            .collect())
    }

    /** Stream und Aufnahme: (streamt, nimmt auf, pausiert). */
    pub async fn status(&self) -> Result<(bool, bool, bool)> {
        let (stream, record) = tokio::try_join!(self.call0("GetStreamStatus"), self.call0("GetRecordStatus"))?;
        Ok((stream["outputActive"] == true, record["outputActive"] == true, record["outputPaused"] == true))
    }
}

fn names(list: &Value, key: &str) -> Vec<String> {
    list.as_array().into_iter().flatten().filter_map(|x| x[key].as_str().map(String::from)).collect()
}

async fn next_json<S>(stream: &mut S) -> Result<Value>
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match stream.next().await {
            Some(Ok(Message::Text(t))) => return Ok(serde_json::from_str(&t)?),
            Some(Ok(Message::Close(frame))) => {
                if frame.is_some_and(|f| u16::from(f.code) == 4009) {
                    bail!(t!("obs.obsRejectsPasswordPlease"));
                }
                bail!(t!("obs.obsClosedConnection"));
            }
            Some(Ok(_)) => continue,
            Some(Err(e)) => bail!(t!("common.obsConnection", e)),
            None => bail!(t!("obs.obsClosedConnection")),
        }
    }
}

/** Port und Passwort aus der lokalen OBS-Konfiguration (normale Installation oder Flatpak). */
pub fn read_local_config() -> Result<Value> {
    let rel = "obs-studio/plugin_config/obs-websocket/config.json";
    let config_home = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| os::home().join(".config"), Into::into);
    let candidates = [os::home().join(".var/app/com.obsproject.Studio/config").join(rel), config_home.join(rel)];
    let file = candidates
        .iter()
        .find(|f| f.exists())
        .ok_or_else(|| anyhow!(t!("obs.noObsConfigurationFound")))?;
    let cfg: Value = serde_json::from_str(&std::fs::read_to_string(file)?)?;
    Ok(json!({
        "port": cfg["server_port"].as_u64().unwrap_or(4455),
        "password": if cfg["auth_required"] == true { cfg["server_password"].as_str().unwrap_or("") } else { "" },
        "serverEnabled": cfg["server_enabled"] == true,
        "flatpak": file.to_string_lossy().contains("/.var/app/"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anmeldung_wie_bisher() {
        // Erwarteter Wert mit der bisherigen JavaScript-Fassung berechnet.
        let secret = sha256_b64("supersecretpasswordlZFQ1+yGdNOxUD3RyuRsw4ZMmPx13JKyzu1FJzeyqco=");
        let auth = sha256_b64(&format!("{secret}+IxH4CnCiqpX1rM9scsNynZzbOe4KhDeYcTNS3PDaeY="));
        assert_eq!(auth, "Pqh8HzeGieLSUfulTi8UOzPDn598Olj62Gunup9LFkE=");
    }

    /** Kleiner Ersatz-OBS: prüft Anmeldung, Anfragen und Ereignisse. */
    #[tokio::test]
    async fn verbindet_und_fragt() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            ws.send(Message::Text(json!({ "op": 0, "d": { "rpcVersion": 1 } }).to_string().into())).await.unwrap();
            let _identify = ws.next().await;
            ws.send(Message::Text(json!({ "op": 2, "d": {} }).to_string().into())).await.unwrap();
            ws.send(Message::Text(json!({ "op": 5, "d": { "eventType": "StudioModeStateChanged", "eventData": { "studioModeEnabled": true } } }).to_string().into()))
                .await
                .unwrap();
            while let Some(Ok(Message::Text(t))) = ws.next().await {
                let req: Value = serde_json::from_str(&t).unwrap();
                let id = req["d"]["requestId"].clone();
                let reply = json!({ "op": 7, "d": { "requestId": id, "requestStatus": { "result": true, "code": 100 },
                    "responseData": { "scenes": [{ "sceneName": "B" }, { "sceneName": "A" }] } } });
                ws.send(Message::Text(reply.to_string().into())).await.unwrap();
            }
        });
        let obs = ObsClient::with_settings(Box::new(move || ObsSettings { host: "127.0.0.1".into(), port, password: String::new() }));
        let mut events = obs.subscribe();
        assert_eq!(obs.scenes().await.unwrap(), vec!["A", "B"]);
        let (kind, _) = tokio::time::timeout(Duration::from_secs(2), events.recv()).await.unwrap().unwrap();
        assert_eq!(kind, "StudioModeStateChanged");
    }

    #[tokio::test]
    async fn ohne_obs_klare_meldung() {
        let obs = ObsClient::with_settings(Box::new(|| ObsSettings { host: "127.0.0.1".into(), port: 1, password: String::new() }));
        let err = obs.scenes().await.unwrap_err().to_string();
        assert!(err.contains("OBS"), "{err}");
    }
}
