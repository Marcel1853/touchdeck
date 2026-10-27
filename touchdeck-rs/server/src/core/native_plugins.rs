// Erweiterte Plugins: eigene Programme (Python, Node, …) mit vollen Rechten – nur im erweiterten Modus,
// den der Nutzer bewusst und auf eigene Gefahr einschaltet.
//
// TouchDeck startet das Programm selbst (Befehl aus plugin.json, Arbeitsordner = Plugin-Ordner) und
// spricht mit ihm über JSON-Zeilen auf stdin/stdout. Es gibt keinen offenen Port, über den sich ein
// anderes Programm als Plugin ausgeben könnte. Ausgaben auf stderr landen im Protokoll.
//
// TouchDeck → Plugin:
//   {"type":"start","plugin":{…},"settings":{…},"touchdeck":"0.7.0"}
//   {"type":"action","requestId":"…","id":"<aktion>","params":{…},"info":{"buttonId":…,"event":{…}}}
//   {"type":"value","requestId":"…","value":"…"|null}        (Antwort auf getValue)
//   {"type":"event","kind":"<auslöser>","data":{…}}               (TouchDeck-Ereignis, z. B. page_view)
//   {"type":"reply","requestId":"…","ok":true,"result":…} bzw. {"ok":false,"error":"…"}   (Antwort auf call)
//   {"type":"stop"}
// Plugin → TouchDeck:
//   {"type":"result","requestId":"…","ok":true,"message":"…"} bzw. {"ok":false,"error":"…"}
//   {"type":"setValue","name":"…","value":"…"}   {"type":"getValue","requestId":"…","name":"…"}
//   {"type":"fire","trigger":"<id>","data":{…}}   {"type":"notify","title":"…","body":"…"}
//   {"type":"log","level":"info|warn|error","message":"…"}
//   {"type":"call","requestId":"…","op":"live.set|deck.pages|deck.showPage|deck.setLook|deck.resetLook|deck.timer|status","args":{…}}
use crate::t;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

use crate::core::store::{js_string, new_id};
use crate::server::App;

pub enum NativeCmd {
    Action { id: String, params: Value, info: Value, depth: u32, reply: oneshot::Sender<Result<Option<String>, String>> },
    /** Ein TouchDeck-Ereignis (erweiterte Plugins bekommen alle). */
    Event { kind: String, data: Value },
    Stop,
}

type Pending = HashMap<String, oneshot::Sender<Result<Option<String>, String>>>;

fn log(app: &Weak<App>, id: &str, level: &str, msg: &str) {
    if let Some(app) = app.upgrade() {
        let source = format!("Plugin {id}");
        let msg: String = msg.chars().take(2000).collect();
        match level {
            "warn" => app.log.warn(&source, msg),
            "error" => app.log.error(&source, msg),
            "debug" => app.log.debug(&source, msg),
            _ => app.log.info(&source, msg),
        }
    }
}

/** Startet das Plugin-Programm und hält es am Laufen (Neustart nach Absturz, höchstens 3-mal pro Minute). */
pub fn spawn_native(app: &Arc<App>, dir: PathBuf, manifest: Value, settings: Value, error: Arc<Mutex<Option<String>>>) -> mpsc::UnboundedSender<NativeCmd> {
    let (tx, mut rx) = mpsc::unbounded_channel::<NativeCmd>();
    let weak = Arc::downgrade(app);
    let id = manifest["id"].as_str().unwrap_or("").to_string();
    let command: Vec<String> = manifest["native"]["command"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)).collect();
    let version = app.version;
    tokio::spawn(async move {
        let mut restarts: Vec<Instant> = Vec::new();
        loop {
            match run_once(&weak, &id, &dir, &command, &manifest, &settings, version, &mut rx).await {
                Ok(Stopped::ByTouchDeck) => return,
                Ok(Stopped::Exited(code)) => log(&weak, &id, "warn", &t!("plugins.programExitedCode", code)),
                Err(e) => log(&weak, &id, "error", &e.to_string()),
            }
            restarts.retain(|t| t.elapsed() < Duration::from_secs(60));
            if restarts.len() >= 3 {
                let msg = t!("plugins.pluginProgramCrashedSeveral").to_string();
                log(&weak, &id, "error", &msg);
                *error.lock().unwrap() = Some(msg);
                // Anfragen weiterhin beantworten, damit niemand hängen bleibt.
                while let Some(cmd) = rx.recv().await {
                    match cmd {
                        NativeCmd::Action { reply, .. } => {
                            let _ = reply.send(Err(t!("plugins.pluginIsNotRunning").into()));
                        }
                        NativeCmd::Event { .. } => {}
                        NativeCmd::Stop => return,
                    }
                }
                return;
            }
            restarts.push(Instant::now());
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
    tx
}

enum Stopped {
    ByTouchDeck,
    Exited(i32),
}

#[allow(clippy::too_many_arguments)]
async fn run_once(
    app: &Weak<App>,
    id: &str,
    dir: &PathBuf,
    command: &[String],
    manifest: &Value,
    settings: &Value,
    version: &str,
    rx: &mut mpsc::UnboundedReceiver<NativeCmd>,
) -> Result<Stopped> {
    let (program, args) = command.split_first().ok_or_else(|| anyhow::anyhow!("plugin.json: native.command fehlt."))?;
    // „./programm“ liegt im Plugin-Ordner, alles andere (python3, node …) wird im Suchpfad gesucht.
    let program = match program.strip_prefix("./") {
        Some(rel) => dir.join(rel).to_string_lossy().into_owned(),
        None => program.clone(),
    };
    let mut child = Command::from(crate::system::os::command(&program))
        .args(args)
        .current_dir(dir)
        .env("TOUCHDECK_PLUGIN_DIR", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!(t!("plugins.cannotStartPluginProgram", program, e)))?;
    let mut stdin = child.stdin.take().expect("stdin angefordert");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout angefordert")).lines();
    let mut stderr = BufReader::new(child.stderr.take().expect("stderr angefordert")).lines();
    let send = |v: Value| format!("{v}\n");

    let start = json!({ "type": "start", "plugin": { "id": manifest["id"], "name": manifest["name"], "version": manifest["version"] }, "settings": settings, "touchdeck": version });
    stdin.write_all(send(start).as_bytes()).await?;
    log(app, id, "info", t!("plugins.started"));

    let mut pending: Pending = HashMap::new();
    // Verkettungstiefe der zuletzt ausgeführten Aktion (für Auslöser, die das Plugin daraufhin meldet).
    let mut last_depth: (u32, Instant) = (0, Instant::now());
    let mut stderr_open = true;
    let result = loop {
        tokio::select! {
            line = stdout.next_line() => {
                let Ok(Some(line)) = line else { break child.wait().await.map(|s| Stopped::Exited(s.code().unwrap_or(-1))).map_err(Into::into) };
                let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                    log(app, id, "debug", &line);
                    continue;
                };
                let Some(app_arc) = app.upgrade() else { break Ok(Stopped::ByTouchDeck) };
                match msg["type"].as_str().unwrap_or("") {
                    "result" => {
                        if let Some(reply) = msg["requestId"].as_str().and_then(|r| pending.remove(r)) {
                            let _ = reply.send(if msg["ok"] == false {
                                Err(msg["error"].as_str().unwrap_or(t!("plugins.errorInPlugin")).to_string())
                            } else {
                                // Mit Daten: wie bei JS-Plugins mit Kennzeichen vorneweg weiterreichen.
                                if msg["data"].is_object() {
                                    Ok(Some(format!("\u{1}{}", json!({ "message": msg["message"], "data": msg["data"] }))))
                                } else {
                                    Ok(msg["message"].as_str().filter(|m| !m.is_empty()).map(String::from))
                                }
                            });
                        }
                    }
                    "setValue" => {
                        let r = app_arc.store.lock().unwrap().set_variable(msg["name"].as_str().unwrap_or(""), &json!(js_string(&msg["value"])));
                        if let Err(e) = r {
                            log(app, id, "warn", &format!("setValue: {e}"));
                        }
                    }
                    "getValue" => {
                        let value = app_arc.store.lock().unwrap().config.variables.get(msg["name"].as_str().unwrap_or("")).map(js_string);
                        stdin.write_all(send(json!({ "type": "value", "requestId": msg["requestId"], "value": value })).as_bytes()).await?;
                    }
                    "fire" => {
                        let depth = if last_depth.1.elapsed() < Duration::from_secs(5) { last_depth.0 } else { 0 };
                        let data = if msg["data"].is_object() { msg["data"].clone() } else { json!({}) };
                        app_arc.events.fire_at(&app_arc, &format!("plugin:{id}:{}", msg["trigger"].as_str().unwrap_or("")), &data, depth);
                    }
                    "notify" => {
                        let (t, b) = (msg["title"].as_str().unwrap_or("TouchDeck").to_string(), msg["body"].as_str().unwrap_or("").to_string());
                        let _ = crate::system::os::notify(&t, &b).await;
                    }
                    "log" => log(app, id, msg["level"].as_str().unwrap_or("info"), msg["message"].as_str().unwrap_or("")),
                    "call" => {
                        // Erweiterte Plugins haben volle Rechte; Live-Anzeigen müssen trotzdem in plugin.json stehen.
                        let op = msg["op"].as_str().unwrap_or("");
                        let args = if msg["args"].is_object() { msg["args"].clone() } else { json!({}) };
                        let known_live = !op.starts_with("live.") || manifest["lives"].as_array().is_some_and(|l| l.iter().any(|x| x["id"] == args["id"]));
                        let r = if known_live { crate::core::plugins::host_call(&app_arc, id, op, &args).map_err(|e| e.to_string()) } else { Err(t!("plugins.unknownLive", js_string(&args["id"]))) };
                        let reply = match r {
                            Ok(v) => json!({ "type": "reply", "requestId": msg["requestId"], "ok": true, "result": v }),
                            Err(e) => json!({ "type": "reply", "requestId": msg["requestId"], "ok": false, "error": e }),
                        };
                        stdin.write_all(send(reply).as_bytes()).await?;
                    }
                    _ => {}
                }
            }
            line = stderr.next_line(), if stderr_open => match line {
                Ok(Some(line)) => log(app, id, "warn", &line),
                // stderr geschlossen: nicht weiter abfragen, sonst liefe die Schleife leer.
                _ => stderr_open = false,
            },
            cmd = rx.recv() => match cmd {
                None | Some(NativeCmd::Stop) => {
                    let _ = stdin.write_all(send(json!({ "type": "stop" })).as_bytes()).await;
                    // Kurz Zeit zum Aufräumen geben, dann beenden.
                    if tokio::time::timeout(Duration::from_secs(2), child.wait()).await.is_err() {
                        let _ = child.kill().await;
                    }
                    break Ok(Stopped::ByTouchDeck);
                }
                Some(NativeCmd::Event { kind, data }) => {
                    let _ = stdin.write_all(send(json!({ "type": "event", "kind": kind, "data": data })).as_bytes()).await;
                }
                Some(NativeCmd::Action { id: action, params, info, depth, reply }) => {
                    let request = new_id("r_");
                    last_depth = (depth, Instant::now());
                    let msg = json!({ "type": "action", "requestId": request, "id": action, "params": params, "info": info });
                    if stdin.write_all(send(msg).as_bytes()).await.is_err() {
                        let _ = reply.send(Err(t!("plugins.pluginProgramIsNot").into()));
                    } else {
                        pending.insert(request, reply);
                    }
                }
            },
        }
    };
    for (_, reply) in pending.drain() {
        let _ = reply.send(Err(t!("plugins.pluginProgramHasExited").into()));
    }
    result
}
