// Live-Werte auf Buttons (Uhrzeit, CPU, Lautstärke, Stream-Status …) und Schieberegler.
use crate::t;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use chrono::{Datelike, Local, Timelike};
use serde_json::{Map, Value, json};
use tokio::sync::OnceCell;

use crate::core::i18n;
use crate::core::store::{Button, Page, js_string};
use crate::integrations::steam;
use crate::server::App;
use crate::system::linux::{self, Volume};

/** Liest pro Durchlauf jede Quelle nur einmal, auch wenn mehrere Buttons sie anzeigen. */
#[derive(Default)]
struct Snapshot {
    sink: OnceCell<Result<Volume, String>>,
    source: OnceCell<Result<Volume, String>>,
    obs_status: OnceCell<Result<(bool, bool, bool), String>>,
    obs_scene: OnceCell<Result<String, String>>,
    running: OnceCell<BTreeSet<String>>,
    cpu: OnceCell<Option<i64>>,
    ram: OnceCell<Option<i64>>,
    temp: OnceCell<Option<i64>>,
    track: OnceCell<Result<Option<linux::Track>, String>>,
    steam_updates: OnceCell<(Vec<String>, Vec<String>)>,
}

fn cloned<T: Clone>(r: &Result<T, String>) -> Result<T> {
    r.clone().map_err(|e| anyhow!(e))
}

impl Snapshot {
    async fn sink(&self) -> Result<Volume> {
        cloned(self.sink.get_or_init(|| async { linux::get_volume(linux::SINK).await.map_err(|e| e.to_string()) }).await)
    }
    async fn source(&self) -> Result<Volume> {
        cloned(self.source.get_or_init(|| async { linux::get_volume(linux::SOURCE).await.map_err(|e| e.to_string()) }).await)
    }
}

// Befehle für Live-Anzeigen höchstens alle 5 Sekunden ausführen.
static COMMAND_CACHE: LazyLock<Mutex<HashMap<String, (Instant, String)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

async fn command_output(cmd: &str) -> Result<String> {
    if let Some((at, text)) = COMMAND_CACHE.lock().unwrap().get(cmd)
        && at.elapsed() < Duration::from_secs(5)
    {
        return Ok(text.clone());
    }
    let out = linux::shell(cmd, true, Duration::from_secs(4)).await?;
    let text: String = out.lines().next().unwrap_or("").chars().take(60).collect();
    COMMAND_CACHE.lock().unwrap().insert(cmd.into(), (Instant::now(), text.clone()));
    Ok(text)
}

fn first_param(button: &Button, kind: &str, key: &str) -> Option<String> {
    button.actions.iter().find(|a| a.kind == kind).and_then(|a| a.params.get(key)).map(js_string).filter(|s| !s.is_empty())
}

fn truthy(v: &Value) -> bool {
    *v == Value::Bool(true) || ["true", "1", "an", "ja", "on"].contains(&js_string(v).to_lowercase().as_str())
}

const WEEKDAYS: [&str; 7] = ["Mo.", "Di.", "Mi.", "Do.", "Fr.", "Sa.", "So."];
const MONTHS: [&str; 12] = ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."];
const WEEKDAYS_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS_EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn text(t: impl Into<String>) -> Value {
    json!({ "text": t.into() })
}

async fn live_value(app: &Arc<App>, button: &Button, snap: &Snapshot, variables: &Map<String, Value>) -> Result<Option<Value>> {
    let live = &button.live;
    let now = Local::now();
    Ok(Some(match live.source.as_str() {
        "clock" => text(format!("{:02}:{:02}", now.hour(), now.minute())),
        "date" => {
            let (day, month) = (now.weekday().num_days_from_monday() as usize, now.month0() as usize);
            text(if i18n::is_english() { format!("{}, {} {}", WEEKDAYS_EN[day], MONTHS_EN[month], now.day()) } else { format!("{}, {}. {}", WEEKDAYS[day], now.day(), MONTHS[month]) })
        }
        "cpu" => {
            let v = *snap.cpu.get_or_init(|| async { linux::cpu_usage() }).await;
            json!({ "text": v.map_or_else(|| "…".into(), |v| format!("{v} %")), "value": v })
        }
        "ram" => {
            let v = snap.ram.get_or_init(|| async { linux::ram_percent() }).await.ok_or_else(|| anyhow!(t!("live.memoryUsageNotReadable")))?;
            json!({ "text": format!("{v} %"), "value": v })
        }
        "temp" => {
            let t = *snap.temp.get_or_init(|| async { linux::cpu_temp() }).await;
            json!({ "text": t.map_or_else(|| "–".into(), |t| format!("{t} °C")), "on": t.is_some_and(|t| t >= 80) })
        }
        "volume" => {
            let v = snap.sink().await?;
            let vol = v.volume.map_or_else(|| "?".into(), |x| x.to_string());
            json!({ "text": if v.muted { t!("common.muted").into() } else { format!("{vol} %") }, "on": v.muted, "value": v.volume })
        }
        "mic" => {
            let v = snap.source().await?;
            json!({ "text": if v.muted { t!("common.muted") } else { t!("live.on") }, "on": v.muted })
        }
        "track" => {
            let t = cloned(snap.track.get_or_init(|| async { linux::current_track().await.map_err(|e| e.to_string()) }).await)?;
            match t {
                Some(t) => {
                    let label = [t.title.as_str(), t.artist.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" – ");
                    json!({ "text": if label.is_empty() { t!("live.unknown").into() } else { label }, "on": t.playing })
                }
                None => text("Keine Wiedergabe"),
            }
        }
        "steam_updates" => {
            let (pending, running) = snap.steam_updates.get_or_init(|| async { steam::update_status() }).await;
            if !running.is_empty() {
                json!({ "text": t!("live.downloading", running.len()), "on": true })
            } else if pending.is_empty() {
                json!({ "text": "Aktuell", "on": false })
            } else {
                json!({ "text": format!("{} Update{}", pending.len(), if pending.len() > 1 { "s" } else { "" }), "on": true })
            }
        }
        "steam_running" => {
            let game = first_param(button, "steam_game", "game");
            let running = snap.running.get_or_init(|| async { steam::running_games() }).await;
            let on = game.is_some_and(|g| running.contains(&g));
            json!({ "text": if on { t!("live.running") } else { "" }, "on": on })
        }
        src @ ("obs_stream" | "obs_record") => {
            let s = snap.obs_status.get_or_init(|| async { app.obs.status().await.map_err(|e| e.to_string()) }).await;
            match s {
                Err(_) => text("OBS aus"),
                Ok((streaming, recording, paused)) => {
                    if src == "obs_stream" {
                        json!({ "text": if *streaming { "LIVE" } else { t!("live.offline") }, "on": streaming })
                    } else {
                        json!({ "text": if *paused { t!("live.paused") } else if *recording { "REC" } else { t!("live.off") }, "on": recording })
                    }
                }
            }
        }
        "obs_scene" => {
            let Some(want) = first_param(button, "obs_scene", "scene") else { return Ok(Some(json!({ "text": "", "on": false }))) };
            let cur = snap
                .obs_scene
                .get_or_init(|| async { app.obs.call0("GetCurrentProgramScene").await.map(|r| r["currentProgramSceneName"].as_str().unwrap_or("").to_string()).map_err(|e| e.to_string()) })
                .await;
            match cur {
                Err(_) => text(""),
                Ok(cur) => {
                    let on = *cur == want;
                    json!({ "text": if on { t!("live.active") } else { "" }, "on": on })
                }
            }
        }
        "command" => text(if live.command.is_empty() { String::new() } else { command_output(&live.command).await? }),
        src @ ("twitch_live" | "twitch_viewers" | "twitch_followers") => {
            if !app.twitch.connected() {
                return Ok(Some(text(t!("live.twitchOff"))));
            }
            if src == "twitch_followers" {
                return Ok(Some(text(app.twitch.followers().await?.to_string())));
            }
            let s = app.twitch.stream().await?;
            let live = !s.is_null();
            if src == "twitch_live" {
                json!({ "text": if live { "LIVE" } else { t!("live.offline") }, "on": live })
            } else {
                json!({ "text": if live { format!("{} 👁", s["viewer_count"]) } else { "–".into() }, "on": live })
            }
        }
        src @ ("discord_mute" | "discord_deaf" | "discord_channel") => {
            let d = &app.discord;
            if !d.configured() || d.connect().await.is_err() {
                return Ok(Some(text(t!("live.discordOff"))));
            }
            if src == "discord_channel" {
                let ch = d.channel();
                return Ok(Some(json!({ "text": ch.as_ref().map_or(t!("live.noChannel"), |(_, n)| n.as_str()), "on": ch.is_some() })));
            }
            let v = match d.voice() {
                Some(v) => v,
                None => d.voice_settings().await?,
            };
            if src == "discord_mute" {
                json!({ "text": if v["mute"] == true { t!("common.muted") } else { t!("live.on") }, "on": v["mute"] == true })
            } else {
                json!({ "text": if v["deaf"] == true { t!("live.deafened") } else { t!("live.listening") }, "on": v["deaf"] == true })
            }
        }
        "timer" => app.timers.live(&live.variable),
        "variable" => match variables.get(&live.variable) {
            None => json!({ "text": "–", "on": false }),
            Some(v) => json!({ "text": js_string(v), "on": truthy(v) }),
        },
        _ => return Ok(None),
    }))
}

async fn slider_value(app: &Arc<App>, button: &Button, snap: &Snapshot) -> Result<Option<Value>> {
    let s = &button.slider;
    Ok(match s.target.as_str() {
        "volume" => Some(json!({ "value": snap.sink().await?.volume })),
        "mic" => Some(json!({ "value": snap.source().await?.volume })),
        "obs_volume" if !s.input.is_empty() => Some(match app.obs.call("GetInputVolume", json!({ "inputName": s.input })).await {
            Ok(r) => json!({ "value": (r["inputVolumeMul"].as_f64().unwrap_or(0.0).sqrt() * 100.0).round() }),
            Err(_) => json!({ "value": null, "text": "OBS aus" }),
        }),
        _ => None,
    })
}

/** Berechnet die Live-Werte aller Buttons, die welche brauchen. */
/** Seitenanzeige: ●○○ für die Position der Seite in der Seitenliste (ab 9 Seiten „3 / 12“). */
fn page_indicator(index: usize, total: usize) -> Value {
    let text = if total > 8 { format!("{} / {total}", index + 1) } else { (0..total).map(|i| if i == index { '●' } else { '○' }).collect() };
    json!({ "text": text })
}

pub async fn collect_live(app: &Arc<App>, pages: &[Page], variables: &Map<String, Value>) -> Map<String, Value> {
    let order: Vec<String> = app.store.lock().unwrap().config.pages.iter().map(|p| p.id.clone()).collect();
    let mut values = Map::new();
    for p in pages {
        let index = order.iter().position(|id| *id == p.id).unwrap_or(0);
        for b in p.buttons.iter().filter(|b| b.mode != "slider" && b.live.source == "page_indicator") {
            values.insert(b.id.clone(), page_indicator(index, order.len()));
        }
    }
    let snap = Snapshot::default();
    let jobs = pages.iter().flat_map(|p| &p.buttons).filter(|b| b.mode == "slider" || !["none", "page_indicator"].contains(&b.live.source.as_str())).map(|b| {
        let snap = &snap;
        async move {
            let r = if b.mode == "slider" { slider_value(app, b, snap).await } else { live_value(app, b, snap, variables).await };
            (b.id.clone(), r)
        }
    });
    for (id, r) in futures_util::future::join_all(jobs).await {
        match r {
            Ok(Some(v)) => {
                values.insert(id, v);
            }
            Ok(None) => {}
            Err(err) => {
                values.insert(id, json!({ "text": "⚠", "error": err.to_string() }));
            }
        }
    }
    values
}

/** Setzt den Wert eines Schiebereglers (0–100). */
pub async fn apply_slider(app: &Arc<App>, button: &Button, value: f64) -> Result<()> {
    let v = value.round().clamp(0.0, 100.0) as i64;
    let s = &button.slider;
    match s.target.as_str() {
        "volume" => linux::set_volume_percent(linux::SINK, v).await,
        "mic" => linux::set_volume_percent(linux::SOURCE, v).await,
        "obs_volume" => {
            if s.input.is_empty() {
                bail!(t!("live.noObsAudioSource"));
            }
            // Quadratische Kurve: fühlt sich gleichmäßiger an als die lineare Verstärkung.
            let mul = (v as f64 / 100.0).powi(2);
            app.obs.call("SetInputVolume", json!({ "inputName": s.input, "inputVolumeMul": mul })).await.map(|_| ())
        }
        "command" => {
            if s.command.is_empty() {
                bail!(t!("common.noCommandGiven"));
            }
            // Auf das Ende warten, damit Befehle nacheinander laufen und der letzte Wert gewinnt.
            linux::shell(&s.command.replace("{value}", &v.to_string()), true, Duration::from_secs(10)).await.map(|_| ())
        }
        _ => bail!(t!("live.unknownTargetForSlider")),
    }
}
