// Anbindung an den Linux-Desktop: Programme, Medien (MPRIS), Lautstärke (PipeWire), Systemwerte, Firewall.
// Tastatur, Maus und Fenster stehen in x11.rs.
use crate::t;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use tokio::process::Command;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

pub fn expand_home(p: &str) -> PathBuf {
    if p == "~" {
        home()
    } else if let Some(rest) = p.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(p)
    }
}

pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname").map(|s| s.trim().to_string()).unwrap_or_else(|_| "TouchDeck".into())
}

/** IPv4-Adressen im Heimnetz (ohne localhost). */
pub fn lan_addresses() -> Vec<String> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) => Some(ip.to_string()),
            std::net::IpAddr::V6(_) => None,
        })
        .collect()
}

/**
 * Umgebung für gestartete Programme. Die AppImage setzt PYTHONHOME, LD_LIBRARY_PATH usw. auf ihre
 * eigenen Ordner; Kindprozesse (Befehle, Programme, Plugins) sollen aber die normale Umgebung sehen.
 */
static HOST_ENV: LazyLock<Vec<(String, Option<String>)>> = LazyLock::new(|| {
    let Ok(appdir) = std::env::var("APPDIR") else { return Vec::new() };
    let mut out: Vec<(String, Option<String>)> = [
        "PYTHONHOME", "PYTHONPATH", "PYTHONDONTWRITEBYTECODE", "PERLLIB", "LD_LIBRARY_PATH", "QT_PLUGIN_PATH", "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0", "GDK_PIXBUF_MODULE_FILE", "GSETTINGS_SCHEMA_DIR", "GIO_MODULE_DIR", "APPDIR", "APPIMAGE", "ARGV0", "OWD",
    ]
    .iter()
    .map(|k| (k.to_string(), None))
    .collect();
    for key in ["PATH", "XDG_DATA_DIRS"] {
        if let Ok(v) = std::env::var(key) {
            let kept: Vec<&str> = v.split(':').filter(|p| !p.is_empty() && !p.starts_with(&appdir)).collect();
            out.push((key.into(), (!kept.is_empty()).then(|| kept.join(":"))));
        }
    }
    out
});

/** Neuer Prozess mit bereinigter Umgebung (siehe HOST_ENV); für tokio: `Command::from(command(..))`. */
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut c = std::process::Command::new(program);
    for (k, v) in HOST_ENV.iter() {
        match v {
            Some(v) => c.env(k, v),
            None => c.env_remove(k),
        };
    }
    c
}

/**
 * Meldet Änderungen am Ton (Lautstärke, stumm, Standardgerät) über ein dauerhaftes `pactl subscribe`, statt
 * alle paar Sekunden nachzufragen. None, wenn pactl fehlt.
 */
pub struct AudioWatch {
    lines: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    _child: tokio::process::Child,
}

pub fn audio_watch() -> Option<AudioWatch> {
    use tokio::io::AsyncBufReadExt;
    let mut cmd = tokio::process::Command::from(command("pactl"));
    // Englische Ausgabe erzwingen, sonst heißt es „Ereignis »ändern« auf Quelle“.
    cmd.arg("subscribe").env("LC_ALL", "C").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).kill_on_drop(true);
    // Endet TouchDeck (auch hart, z. B. beim Neustart), soll pactl nicht verwaist weiterlaufen.
    // SAFETY: prctl ist zwischen fork und exec erlaubt (async-signal-safe).
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let mut child = cmd.spawn().ok()?;
    let out = child.stdout.take()?;
    Some(AudioWatch { lines: tokio::io::BufReader::new(out).lines(), _child: child })
}

impl AudioWatch {
    /**
     * Wartet auf das nächste Ereignis an Sink, Source oder Server (Standardgerät) und sammelt kurz alle
     * folgenden ein (ein Lautstärke-Wechsel kommt oft als mehrere Zeilen). false, wenn pactl beendet ist.
     */
    pub async fn changed(&mut self) -> bool {
        loop {
            match self.lines.next_line().await {
                Ok(Some(line)) if [" on sink #", " on source #", " on server"].iter().any(|k| line.contains(k)) => break,
                Ok(Some(_)) => continue,
                _ => return false,
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        while let Ok(Ok(Some(_))) = tokio::time::timeout(std::time::Duration::ZERO, self.lines.next_line()).await {}
        true
    }
}

fn not_installed(cmd: &str, err: std::io::Error) -> anyhow::Error {
    if err.kind() == std::io::ErrorKind::NotFound { anyhow!(t!("system.programIsNotInstalled", cmd)) } else { anyhow!("{cmd}: {err}") }
}

/** Führt ein Programm aus und liefert die Ausgabe; Fehlertext ist die Fehlerausgabe des Programms. */
pub async fn run_with(cmd: &str, args: &[&str], timeout: Duration, input: Option<&str>) -> Result<String> {
    let mut c = Command::from(command(cmd));
    c.args(args).kill_on_drop(true).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = c.spawn().map_err(|e| not_installed(cmd, e))?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(text.as_bytes()).await?;
    }
    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(out) => out?,
        Err(_) => bail!(t!("system.timedOut", cmd)),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let code = out.status.code().map_or_else(|| "Signal".into(), |c| c.to_string());
        bail!("{}", if err.is_empty() { t!("system.exitedWithCode", cmd, code) } else { err });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn run(cmd: &str, args: &[&str]) -> Result<String> {
    run_with(cmd, args, DEFAULT_TIMEOUT, None).await
}

/** Startet ein Programm im Hintergrund, ohne auf das Ende zu warten. */
pub fn spawn_detached(cmd: &str, args: &[&str], cwd: Option<&Path>) -> Result<()> {
    let mut c = command(cmd);
    c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = cwd {
        c.current_dir(dir);
    }
    // Eigene Prozessgruppe: das Programm läuft weiter, auch wenn TouchDeck beendet wird.
    use std::os::unix::process::CommandExt;
    c.process_group(0);
    let mut child = c.spawn().map_err(|e| not_installed(cmd, e))?;
    // Beendete Kinder einsammeln, damit keine Zombies bleiben.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

pub fn open_with_default(target: &str) -> Result<()> {
    spawn_detached("xdg-open", &[target], None)
}

/** Download-Ordner des Nutzers (xdg-user-dirs), sonst ~/Downloads, sonst der Persönliche Ordner. */
pub fn download_dir() -> PathBuf {
    let home = home();
    let from_config = std::fs::read_to_string(home.join(".config/user-dirs.dirs")).ok().and_then(|t| {
        t.lines().find_map(|l| l.strip_prefix("XDG_DOWNLOAD_DIR=")).map(|v| PathBuf::from(v.trim_matches('"').replace("$HOME", &home.to_string_lossy())))
    });
    [from_config, Some(home.join("Downloads"))].into_iter().flatten().find(|d| d.is_dir()).unwrap_or(home)
}

/** Datei im Download-Ordner speichern; gibt es den Namen schon, wird „ (2)“ usw. angehängt. */
pub fn save_download(file: &str, bytes: &[u8]) -> Result<PathBuf> {
    let dir = download_dir();
    let name: String = file.chars().filter(|c| !['/', '\\', '\0'].contains(c)).collect();
    let name = if name.trim().is_empty() || name.starts_with('.') { "TouchDeck-Export".to_string() } else { name };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.clone(), String::new()),
    };
    let mut path = dir.join(&name);
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/** Datei im Dateimanager zeigen (markiert, wenn er das kann), sonst den Ordner öffnen. */
pub fn show_in_file_manager(path: &Path) {
    let uri = format!("file://{}", path.to_string_lossy());
    let shown = command("gdbus")
        .args(["call", "--session", "--dest", "org.freedesktop.FileManager1", "--object-path", "/org/freedesktop/FileManager1", "--method", "org.freedesktop.FileManager1.ShowItems", &format!("['{uri}']"), ""])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !shown && let Some(dir) = path.parent() {
        let _ = open_with_default(&dir.to_string_lossy());
    }
}

/**
 * USB-Verbindung: für jedes per USB angeschlossene Android-Gerät (USB-Debugging an) den Port per
 * „adb reverse“ auf den PC umleiten – die App verbindet sich dann mit 127.0.0.1. Liefert die Geräte.
 */
pub async fn usb_reverse(port: u16) -> Result<Vec<String>> {
    let list = run("adb", &["devices"]).await?;
    let devices: Vec<String> = list.lines().skip(1).filter_map(|l| l.split_once('\t')).filter(|(_, st)| st.trim() == "device").map(|(id, _)| id.trim().to_string()).collect();
    let spec = format!("tcp:{port}");
    for d in &devices {
        // Nur einrichten, wenn die Umleitung noch fehlt (adb reverse --list zeigt bestehende).
        let current = run("adb", &["-s", d, "reverse", "--list"]).await.unwrap_or_default();
        if !current.lines().any(|l| l.contains(&format!("{spec} {spec}"))) {
            run("adb", &["-s", d, "reverse", &spec, &spec]).await?;
        }
    }
    Ok(devices)
}

/** Befehl in der Shell ausführen; wait = auf Ende warten und Ausgabe liefern. */
pub async fn shell(command: &str, wait: bool, timeout: Duration) -> Result<String> {
    let home = home();
    if !wait {
        spawn_detached("sh", &["-c", command], Some(&home))?;
        return Ok(String::new());
    }
    let child = Command::from(self::command("sh"))
        .args(["-c", command])
        .current_dir(&home)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .output();
    let out = match tokio::time::timeout(timeout, child).await {
        Ok(out) => out?,
        Err(_) => bail!(t!("system.commandTakesTooLong")),
    };
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if !out.status.success() {
        bail!("{}", if text.is_empty() { t!("system.commandExitedWithCode", out.status.code().unwrap_or(-1)) } else { text });
    }
    Ok(text)
}

// ---------- Medien über MPRIS (D-Bus) ----------

fn quoted(out: &str, prefix: &str) -> Vec<String> {
    out.split('\'').filter(|s| s.starts_with(prefix)).map(String::from).collect()
}

async fn mpris_players() -> Result<Vec<String>> {
    let out = run(
        "gdbus",
        &["call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.ListNames"],
    )
    .await?;
    Ok(quoted(&out, "org.mpris.MediaPlayer2."))
}

async fn player_prop(name: &str, prop: &str) -> Result<String> {
    run_with(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            name,
            "--object-path",
            "/org/mpris/MediaPlayer2",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.mpris.MediaPlayer2.Player",
            prop,
        ],
        Duration::from_secs(2),
        None,
    )
    .await
}

async fn playback_status(name: &str) -> String {
    match player_prop(name, "PlaybackStatus").await {
        Ok(out) => out.split('\'').nth(1).unwrap_or("Unknown").to_string(),
        Err(_) => "Unknown".into(),
    }
}

/** Spielender Player, sonst pausierter (optional sonst der erste). */
async fn pick_player(fallback_first: bool) -> Result<Option<(String, String)>> {
    let players = mpris_players().await?;
    let states = futures_util::future::join_all(players.iter().map(|p| playback_status(p))).await;
    let pick = ["Playing", "Paused"]
        .iter()
        .find_map(|want| states.iter().position(|s| s == want))
        .or(if fallback_first && !players.is_empty() { Some(0) } else { None });
    Ok(pick.map(|i| (players[i].clone(), states[i].clone())))
}

pub async fn media(op: &str) -> Result<String> {
    let method = match op {
        "play-pause" => "PlayPause",
        "next" => "Next",
        "previous" => "Previous",
        "stop" => "Stop",
        "play" => "Play",
        "pause" => "Pause",
        _ => bail!("Unbekannte Medienaktion: {op}"),
    };
    let Some((player, _)) = pick_player(true).await? else { bail!(t!("system.noMediaPlayerFound")) };
    run(
        "gdbus",
        &["call", "--session", "--dest", &player, "--object-path", "/org/mpris/MediaPlayer2", "--method", &format!("org.mpris.MediaPlayer2.Player.{method}")],
    )
    .await?;
    let short = player.trim_start_matches("org.mpris.MediaPlayer2.");
    let short = match short.rsplit_once(".instance") {
        Some((base, n)) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => base,
        _ => short,
    };
    Ok(short.to_string())
}

/** Liest einen Text aus gdbus-Ausgabe wie 'xesam:title': <'Titel'> (mit \' im Text). */
fn gvariant_string(out: &str, key: &str, array: bool) -> Option<String> {
    let marker = if array { format!("'{key}': <['") } else { format!("'{key}': <'") };
    let start = out.find(&marker)? + marker.len();
    let mut s = String::new();
    let mut chars = out[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => s.push(chars.next()?),
            '\'' => return Some(s),
            c => s.push(c),
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

/** Aktueller Titel des gerade spielenden Players. */
pub async fn current_track() -> Result<Option<Track>> {
    let Some((player, state)) = pick_player(false).await? else { return Ok(None) };
    let out = player_prop(&player, "Metadata").await?;
    Ok(Some(Track {
        title: gvariant_string(&out, "xesam:title", false).unwrap_or_default(),
        artist: gvariant_string(&out, "xesam:artist", true).unwrap_or_default(),
        playing: state == "Playing",
    }))
}

// ---------- Lautstärke über PipeWire (wpctl) ----------

pub const SINK: &str = "@DEFAULT_AUDIO_SINK@";
pub const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Volume {
    pub volume: Option<i64>,
    pub muted: bool,
}

pub async fn get_volume(target: &str) -> Result<Volume> {
    let out = run("wpctl", &["get-volume", target]).await?;
    let volume = out
        .split("Volume:")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| (v * 100.0).round() as i64);
    Ok(Volume { volume, muted: out.contains("[MUTED]") })
}

pub async fn volume(op: &str, step: f64, value: f64) -> Result<Volume> {
    let s = if step.is_finite() && step != 0.0 { step.clamp(1.0, 50.0) } else { 5.0 };
    match op {
        "up" => run("wpctl", &["set-volume", "-l", "1.0", SINK, &format!("{s}%+")]).await?,
        "down" => run("wpctl", &["set-volume", SINK, &format!("{s}%-")]).await?,
        "set" => run("wpctl", &["set-volume", SINK, &format!("{}%", if value.is_finite() { value.clamp(0.0, 100.0) } else { 0.0 })]).await?,
        "mute" => run("wpctl", &["set-mute", SINK, "toggle"]).await?,
        _ => bail!(t!("system.unknownVolumeAction", op)),
    };
    get_volume(SINK).await
}

pub async fn mic(op: &str) -> Result<Volume> {
    let arg = match op {
        "mute" => "1",
        "unmute" => "0",
        _ => "toggle",
    };
    run("wpctl", &["set-mute", SOURCE, arg]).await?;
    get_volume(SOURCE).await
}

pub async fn set_volume_percent(target: &str, v: i64) -> Result<()> {
    run("wpctl", &["set-volume", target, &format!("{v}%")]).await.map(|_| ())
}

// ---------- Audiogeräte ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub value: String,
    pub label: String,
    pub is_default: bool,
}

/** Liest Ausgänge (sinks) oder Eingänge (sources) aus `wpctl status`. Gemerkt wird der Name, die Nummern ändern sich. */
pub fn parse_audio_devices(status: &str, sinks: bool) -> Vec<AudioDevice> {
    let audio = status.split("\nVideo\n").next().unwrap_or(status);
    let header = if sinks { "Sinks:" } else { "Sources:" };
    let Some(after) = audio.split_once(header).map(|(_, r)| r) else { return Vec::new() };
    let block = after.split(['├', '└']).next().unwrap_or("");
    let mut out = Vec::new();
    for line in block.lines() {
        let Some(rest) = line.trim_start().strip_prefix('│') else { continue };
        let rest = rest.trim_start();
        let (is_default, rest) = match rest.strip_prefix('*') {
            Some(r) => (true, r.trim_start()),
            None => (false, rest),
        };
        let Some((num, name)) = rest.split_once(". ") else { continue };
        if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let name = name.split(" [vol:").next().unwrap_or(name).trim();
        if name.is_empty() {
            continue;
        }
        out.push(AudioDevice { id: num.into(), value: name.into(), label: name.into(), is_default });
    }
    out
}

pub async fn audio_devices(sinks: bool) -> Result<Vec<AudioDevice>> {
    Ok(parse_audio_devices(&run("wpctl", &["status"]).await?, sinks))
}

pub async fn set_default_audio(sinks: bool, target: &str) -> Result<String> {
    let devices = audio_devices(sinks).await?;
    if devices.is_empty() {
        bail!(t!("system.noAudioDevicesFound"));
    }
    let next = if target == "next" {
        let i = devices.iter().position(|d| d.is_default).map_or(0, |i| i + 1);
        &devices[i % devices.len()]
    } else {
        devices.iter().find(|d| d.value == target).ok_or_else(|| anyhow!(t!("system.selectedAudioDeviceIs")))?
    };
    run("wpctl", &["set-default", &next.id]).await?;
    Ok(next.label.clone())
}

// ---------- Installierte Programme (.desktop-Dateien) ----------

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub comment: String,
}

fn app_dirs() -> Vec<PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME").map_or_else(|| home().join(".local/share"), PathBuf::from);
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut dirs = vec![
        data_home.join("applications"),
        data_home.join("flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ];
    dirs.extend(data_dirs.split(':').map(|d| Path::new(d).join("applications")));
    dirs
}

fn parse_desktop_file(text: &str, file_name: &str) -> Option<AppInfo> {
    // Nur Einträge aus dem Abschnitt [Desktop Entry] (nicht aus [Desktop Action …]).
    let mut values = std::collections::HashMap::new();
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry && let Some((k, v)) = line.split_once('=') {
            values.entry(k.trim().to_string()).or_insert_with(|| v.trim().to_string());
        }
    }
    let get = |key: &str| values.get(key).filter(|v| !v.is_empty()).cloned();
    if get("Type").as_deref() != Some("Application") || get("NoDisplay").as_deref() == Some("true") || get("Hidden").as_deref() == Some("true") {
        return None;
    }
    let lang = std::env::var("LANG").unwrap_or_else(|_| "de".into());
    let lang = lang.split(['_', '.']).next().unwrap_or("de").to_string();
    Some(AppInfo {
        id: file_name.trim_end_matches(".desktop").into(),
        name: get(&format!("Name[{lang}]")).or_else(|| get("Name")).unwrap_or_else(|| file_name.into()),
        icon: get("Icon").unwrap_or_default(),
        comment: get(&format!("Comment[{lang}]")).or_else(|| get("Comment")).unwrap_or_default(),
    })
}

type AppCache = Option<(Instant, Vec<AppInfo>)>;
static APP_CACHE: LazyLock<Mutex<AppCache>> = LazyLock::new(|| Mutex::new(None));

pub fn list_apps() -> Vec<AppInfo> {
    let mut cache = APP_CACHE.lock().unwrap();
    if let Some((at, apps)) = cache.as_ref()
        && at.elapsed() < Duration::from_secs(60)
    {
        return apps.clone();
    }
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in app_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let Ok(f) = e.file_name().into_string() else { continue };
            if !f.ends_with(".desktop") || seen.contains(&f) {
                continue;
            }
            // Unlesbare Dateien überspringen.
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            if let Some(info) = parse_desktop_file(&text, &f) {
                seen.insert(f);
                apps.push(info);
            }
        }
    }
    apps.sort_by_key(|a| crate::system::sort_key(&a.name));
    *cache = Some((Instant::now(), apps.clone()));
    apps
}

pub fn launch_app(id: &str) -> Result<()> {
    spawn_detached("gtk-launch", &[id], None)
}

pub async fn notify(title: &str, body: &str) -> Result<()> {
    let title = if title.is_empty() { "TouchDeck" } else { title };
    run("notify-send", &["-a", "TouchDeck", title, body]).await.map(|_| ())
}

// ---------- Firewall ----------

/** Ist die Firewall ufw aktiv? (Dann müssen die Ports für das Handy freigegeben werden.) */
pub async fn firewall_active() -> bool {
    run_with("systemctl", &["is-active", "ufw"], Duration::from_secs(2), None).await.is_ok_and(|s| s.trim() == "active")
}

/** Öffnet die Firewall fürs Heimnetz; pkexec fragt dabei grafisch nach dem Passwort. */
pub async fn open_firewall(port: u16) -> Result<()> {
    let rules: Vec<String> = ["192.168.0.0/16", "10.0.0.0/8", "172.16.0.0/12"]
        .iter()
        .flat_map(|net| {
            [
                format!("ufw allow from {net} to any port {port} proto tcp comment TouchDeck"),
                format!("ufw allow from {net} to any port 8769 proto udp comment TouchDeck"),
            ]
        })
        .collect();
    match run_with("pkexec", &["sh", "-c", &rules.join(" && ")], Duration::from_secs(120), None).await {
        Ok(_) => Ok(()),
        Err(err) => {
            let msg = err.to_string().to_lowercase();
            if ["dismissed", "not authorized", "126", "127"].iter().any(|m| msg.contains(m)) {
                bail!(t!("system.cancelledFirewallWasNot"))
            }
            Err(err)
        }
    }
}

// ---------- Systemwerte ----------

/** Letzte Messung (idle, total, Zeitpunkt, Ergebnis): mehrere Abfragen kurz hintereinander (Live-Anzeige und
 *  Graph) bekommen dasselbe Ergebnis, statt sich gegenseitig die Messspanne wegzunehmen. */
type CpuSample = (u64, u64, Instant, Option<i64>);
static LAST_CPU: Mutex<Option<CpuSample>> = Mutex::new(None);

/** CPU-Auslastung seit dem letzten Aufruf (braucht zwei Messpunkte). */
pub fn cpu_usage() -> Option<i64> {
    let mut last = LAST_CPU.lock().unwrap();
    if let Some((_, _, t, r)) = *last
        && t.elapsed() < Duration::from_millis(900)
        && r.is_some()
    {
        return r;
    }
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let parts: Vec<u64> = stat.lines().next()?.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
    let idle = parts.get(3)? + parts.get(4).unwrap_or(&0);
    let total: u64 = parts.iter().sum();
    let result = last.and_then(|(pidle, ptotal, _, _)| {
        (total != ptotal).then(|| (100.0 * (1.0 - (idle - pidle) as f64 / (total - ptotal) as f64)).round() as i64)
    });
    *last = Some((idle, total, Instant::now(), result));
    result
}

pub fn ram_percent() -> Option<i64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let get = |key: &str| info.lines().find_map(|l| l.strip_prefix(key)?.split_whitespace().next()?.parse::<f64>().ok());
    let (total, avail) = (get("MemTotal:")?, get("MemAvailable:")?);
    Some((100.0 * (1.0 - avail / total)).round() as i64)
}

/** CPU-Temperatur aus hwmon (k10temp, coretemp, zenpower …). */
pub fn cpu_temp() -> Option<i64> {
    for e in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let Ok(name) = std::fs::read_to_string(e.path().join("name")) else { continue };
        if !["k10temp", "coretemp", "zenpower", "cpu_thermal", "acpitz"].contains(&name.trim()) {
            continue;
        }
        if let Some(t) = std::fs::read_to_string(e.path().join("temp1_input")).ok().and_then(|t| t.trim().parse::<f64>().ok()) {
            return Some((t / 1000.0).round() as i64);
        }
    }
    None
}

/** Grafikkarten-Temperatur aus hwmon (amdgpu, radeon, nouveau). */
pub fn gpu_temp() -> Option<i64> {
    for e in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let Ok(name) = std::fs::read_to_string(e.path().join("name")) else { continue };
        if !["amdgpu", "radeon", "nouveau"].contains(&name.trim()) {
            continue;
        }
        if let Some(t) = std::fs::read_to_string(e.path().join("temp1_input")).ok().and_then(|t| t.trim().parse::<f64>().ok()) {
            return Some((t / 1000.0).round() as i64);
        }
    }
    None
}

/** Auslastung der Grafikkarte in Prozent (amdgpu/i915 über sysfs). */
pub fn gpu_load() -> Option<i64> {
    for e in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        if let Some(v) = std::fs::read_to_string(e.path().join("device/gpu_busy_percent")).ok().and_then(|t| t.trim().parse::<i64>().ok()) {
            return Some(v.clamp(0, 100));
        }
    }
    None
}

/** Belegung eines Laufwerks in Prozent (Standard: „/“). */
pub fn disk_percent(path: &str) -> Option<i64> {
    let c = std::ffi::CString::new(if path.is_empty() { "/" } else { path }).ok()?;
    // SAFETY: statvfs füllt die Struktur, c ist ein gültiger Pfad.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut st) != 0 || st.f_blocks == 0 {
            return None;
        }
        let used = (st.f_blocks - st.f_bfree) as f64;
        Some((100.0 * used / (used + st.f_bavail as f64)).round() as i64)
    }
}

/** Bytes empfangen/gesendet, Zeitpunkt und Ergebnis der letzten Messung. */
type NetSample = (u64, u64, std::time::Instant, Option<(f64, f64)>);
static LAST_NET: Mutex<Option<NetSample>> = Mutex::new(None);

/** Netzwerk: (Empfang, Senden) in Bytes pro Sekunde seit der letzten Abfrage. */
pub fn net_rate() -> Option<(f64, f64)> {
    // Kurz hintereinander (Live-Anzeige und Graph): letztes Ergebnis wiederverwenden.
    if let Some((_, _, t, Some(r))) = *LAST_NET.lock().unwrap()
        && t.elapsed() < std::time::Duration::from_millis(900)
    {
        return Some(r);
    }
    let dev = std::fs::read_to_string("/proc/net/dev").ok()?;
    let (mut rx, mut tx) = (0u64, 0u64);
    for line in dev.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else { continue };
        if name.trim() == "lo" {
            continue;
        }
        let v: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        rx += v.first().copied().unwrap_or(0);
        tx += v.get(8).copied().unwrap_or(0);
    }
    let now = std::time::Instant::now();
    let mut last = LAST_NET.lock().unwrap();
    let result = last.and_then(|(prx, ptx, pt, _)| {
        let secs = now.duration_since(pt).as_secs_f64();
        (secs >= 0.2).then(|| (rx.saturating_sub(prx) as f64 / secs, tx.saturating_sub(ptx) as f64 / secs))
    });
    *last = Some((rx, tx, now, result));
    result
}

/** Laufzeit des PCs in Sekunden. */
pub fn uptime_secs() -> Option<u64> {
    std::fs::read_to_string("/proc/uptime").ok()?.split_whitespace().next()?.parse::<f64>().ok().map(|v| v as u64)
}

/** Bildschirm sofort ausschalten (DPMS, X11); eine Bewegung der Maus weckt ihn wieder. */
pub async fn screen_off() -> Result<()> {
    run("xset", &["dpms", "force", "off"]).await.map(|_| ())
}

/** Helligkeit aller Bildschirme: über DDC (ddcutil, echte Helligkeit), sonst per xrandr (abgedunkelt). */
pub async fn screen_brightness(percent: u32) -> Result<&'static str> {
    let p = percent.clamp(10, 100);
    let has_ddc = std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d.join("ddcutil").is_file()));
    if has_ddc && run("ddcutil", &["setvcp", "10", &p.to_string()]).await.is_ok() {
        return Ok("ddc");
    }
    let list = run("xrandr", &["--listactivemonitors"]).await?;
    let outputs: Vec<&str> = list.lines().skip(1).filter_map(|l| l.split_whitespace().last()).collect();
    if outputs.is_empty() {
        bail!(t!("system.noMonitorFound"));
    }
    for o in outputs {
        run("xrandr", &["--output", o, "--brightness", &format!("{:.2}", p as f64 / 100.0)]).await?;
    }
    Ok("xrandr")
}

// ---------- Desktop & System ----------

pub fn screenshot(mode: &str) -> Result<()> {
    let flags: &[&str] = match mode {
        "full" => &[],
        "area" => &["-a"],
        "window" => &["-w"],
        _ => &["-i"],
    };
    spawn_detached("gnome-screenshot", flags, None)
}

pub async fn lock_screen() -> Result<()> {
    if run("cinnamon-screensaver-command", &["-l"]).await.is_err() {
        run("loginctl", &["lock-session"]).await?;
    }
    Ok(())
}

pub async fn power(op: &str) -> Result<()> {
    match op {
        "suspend" | "reboot" | "poweroff" => run("systemctl", &[op]).await.map(|_| ()),
        "logout" => spawn_detached("cinnamon-session-quit", &["--logout"], None),
        _ => bail!(t!("system.unknownPowerAction", op)),
    }
}

async fn gsettings_bool(schema: &str, key: &str, mode: &str) -> Result<bool> {
    let cur = run("gsettings", &["get", schema, key]).await?.trim() == "true";
    let next = if mode == "toggle" { !cur } else { mode == "on" };
    run("gsettings", &["set", schema, key, &next.to_string()]).await?;
    Ok(next)
}

pub async fn night_light(mode: &str) -> Result<bool> {
    gsettings_bool("org.cinnamon.settings-daemon.plugins.color", "night-light-enabled", mode).await
}

/** Hell/Dunkel umschalten: Farbschema plus passendes Mint-Y-Thema. */
pub async fn dark_mode(mode: &str) -> Result<bool> {
    let theme = run("gsettings", &["get", "org.cinnamon.desktop.interface", "gtk-theme"]).await?.trim().replace('\'', "");
    let is_dark = theme.to_lowercase().contains("-dark");
    let dark = if mode == "toggle" { !is_dark } else { mode == "on" };
    let next_theme = if dark {
        if is_dark {
            theme.clone()
        } else if theme == "Mint-Y" {
            "Mint-Y-Dark".into()
        } else if let Some(rest) = theme.strip_prefix("Mint-Y-") {
            format!("Mint-Y-Dark-{rest}")
        } else {
            theme.clone()
        }
    } else {
        match theme.to_lowercase().find("-dark") {
            Some(i) => format!("{}{}", &theme[..i], &theme[i + 5..]),
            None => theme.clone(),
        }
    };
    let _ = run("gsettings", &["set", "org.x.apps.portal", "color-scheme", if dark { "prefer-dark" } else { "prefer-light" }]).await;
    if next_theme != theme {
        run("gsettings", &["set", "org.cinnamon.desktop.interface", "gtk-theme", &next_theme]).await?;
        let _ = run("gsettings", &["set", "org.cinnamon.theme", "name", &next_theme]).await;
    }
    Ok(dark)
}

/** Wake-on-LAN: weckt einen Rechner im Netz über sein „Magic Packet“. */
pub async fn wake_on_lan(mac: &str) -> Result<()> {
    let hex: String = mac.chars().filter(char::is_ascii_hexdigit).collect();
    if hex.len() != 12 {
        bail!(t!("system.invalidMacAddressExample"));
    }
    let mac = hex::decode(hex)?;
    let mut packet = vec![0xffu8; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&mac);
    }
    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
    sock.set_broadcast(true)?;
    sock.send_to(&packet, "255.255.255.255:9").await?;
    Ok(())
}

/** Beendet ein Programm über seinen Prozessnamen (wie pkill). */
pub async fn kill_process(name: &str, force: bool) -> Result<()> {
    let valid = (1..=64).contains(&name.chars().count()) && name.chars().all(|c| c.is_alphanumeric() || " _.+-".contains(c));
    if !valid {
        bail!(t!("system.invalidProgramName"));
    }
    run("pkill", &[if force { "-KILL" } else { "-TERM" }, "-x", name]).await.map_err(|_| anyhow!(t!("system.noRunningProgramFound", name)))?;
    Ok(())
}

// ---------- Soundboard ----------

static SOUNDS: LazyLock<Mutex<HashSet<u32>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/** Spielt eine Audiodatei über PipeWire ab; mehrere Sounds können gleichzeitig laufen. */
pub fn play_sound(file: &str, volume: f64, exclusive: bool) -> Result<()> {
    let f = expand_home(file);
    if file.is_empty() || !f.exists() {
        bail!(t!("system.audioFileNotFound", file));
    }
    if exclusive {
        stop_sounds();
    }
    let vol = if volume.is_finite() { volume.clamp(0.0, 100.0) } else { 0.0 } / 100.0;
    let mut child = command("pw-play")
        .arg("--volume")
        .arg(vol.to_string())
        .arg(&f)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| not_installed("pw-play", e))?;
    let pid = child.id();
    SOUNDS.lock().unwrap().insert(pid);
    std::thread::spawn(move || {
        let _ = child.wait();
        SOUNDS.lock().unwrap().remove(&pid);
    });
    Ok(())
}

pub fn stop_sounds() {
    for pid in SOUNDS.lock().unwrap().drain() {
        // SAFETY: kill() mit einer Prozessnummer, die wir selbst gestartet haben, hat keine Speicherwirkungen.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }
}

// ---------- Dateien ----------

pub fn write_file(file: &str, text: &str, mode: &str) -> Result<()> {
    if file.is_empty() {
        bail!(t!("system.noFilePathGiven"));
    }
    let f = expand_home(file);
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if mode == "append" {
        use std::io::Write;
        let mut h = std::fs::OpenOptions::new().create(true).append(true).open(&f)?;
        writeln!(h, "{text}")?;
    } else {
        std::fs::write(&f, text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "PipeWire 'pipewire-0' [1.0.5]
Audio
 ├─ Devices:
 │      46. Navi 31 HDMI/DP Audio               [alsa]
 │
 ├─ Sinks:
 │      52. Navi 31 HDMI/DP Audio Digital Stereo [vol: 0.40]
 │  *   61. Starship/Matisse HD Audio Analog Stereo [vol: 0.55]
 │
 ├─ Sources:
 │  *   62. Starship/Matisse HD Audio Analog Stereo [vol: 1.00]
 │
 └─ Streams:

Video
 ├─ Sinks:
 │      99. Kamera
";

    #[test]
    fn audiogeraete() {
        let sinks = parse_audio_devices(STATUS, true);
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[1].id, "61");
        assert!(sinks[1].is_default);
        assert_eq!(sinks[0].value, "Navi 31 HDMI/DP Audio Digital Stereo");
        let sources = parse_audio_devices(STATUS, false);
        assert_eq!(sources.len(), 1);
    }

    #[test]
    fn desktop_datei() {
        let app = parse_desktop_file("[Desktop Entry]\nType=Application\nName=Firefox\nName[de]=Feuerfuchs\nIcon=firefox\n[Desktop Action new]\nName=Neu\n", "firefox.desktop");
        let app = app.unwrap();
        assert_eq!(app.id, "firefox");
        assert!(app.name == "Feuerfuchs" || app.name == "Firefox");
        assert!(parse_desktop_file("[Desktop Entry]\nType=Application\nNoDisplay=true\nName=X\n", "x.desktop").is_none());
    }

    #[test]
    fn mpris_text() {
        let out = r#"(<{'xesam:title': <'It\'s me'>, 'xesam:artist': <['Band']>}>,)"#;
        assert_eq!(gvariant_string(out, "xesam:title", false).as_deref(), Some("It's me"));
        assert_eq!(gvariant_string(out, "xesam:artist", true).as_deref(), Some("Band"));
    }

    #[test]
    fn home_pfade() {
        assert_eq!(expand_home("~/a"), home().join("a"));
        assert_eq!(expand_home("/x"), PathBuf::from("/x"));
    }
}
