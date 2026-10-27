// Windows-Fassung der Systemschicht (Gegenstück zu linux.rs, gleiche Funktionen): Programme, Ton,
// Systemwerte, Benachrichtigungen, Dateien. Tastatur, Maus und Fenster stehen in input.rs.
// Was es unter Windows nicht gibt, meldet „unter Windows nicht verfügbar“.
use crate::t;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use tokio::process::Command;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// Kein Konsolenfenster für gestartete Programme (CREATE_NO_WINDOW).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn not_on_windows() -> anyhow::Error {
    anyhow!(t!("system.notOnWindows"))
}

pub fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").map_or_else(|| PathBuf::from("C:\\"), PathBuf::from)
}

/** Ordner für Einstellungen: %APPDATA%. */
pub fn config_dir() -> PathBuf {
    std::env::var_os("APPDATA").map_or_else(|| home().join("AppData\\Roaming"), PathBuf::from)
}

/** Dateirechte gibt es so unter Windows nicht; der Benutzerordner ist ohnehin nur für den Nutzer. */
pub fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

/** Läuft der Prozess noch? */
pub fn process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: Handle wird nur hier geöffnet und wieder geschlossen.
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut code = 0u32;
        let alive = GetExitCodeProcess(h, &mut code).is_ok() && code == 259; // STILL_ACTIVE
        let _ = CloseHandle(h);
        alive
    }
}

pub fn expand_home(p: &str) -> PathBuf {
    if p == "~" {
        home()
    } else if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        home().join(rest)
    } else {
        PathBuf::from(p)
    }
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "TouchDeck".into())
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

/** Neuer Prozess ohne Konsolenfenster; für tokio: `Command::from(command(..))`. */
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/** Ton-Ereignisse gibt es hier nicht als Strom; die Auslöser fragen dann in Abständen nach. */
pub struct AudioWatch {
    _none: (),
}

pub fn audio_watch() -> Option<AudioWatch> {
    None
}

impl AudioWatch {
    pub async fn changed(&mut self) -> bool {
        false
    }
}

/** Bildschirmsperre beobachten: unter Windows noch nicht (Auslöser „PC gesperrt“ fehlt dann). */
pub struct LockWatch {
    _none: (),
}

pub fn lock_watch() -> Option<LockWatch> {
    None
}

impl LockWatch {
    pub async fn next(&mut self) -> Option<bool> {
        None
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
        let code = out.status.code().map_or_else(|| "?".into(), |c| c.to_string());
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
    let mut child = c.spawn().map_err(|e| not_installed(cmd, e))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/** Datei, Ordner oder Adresse mit dem Standardprogramm öffnen. */
pub fn open_with_default(target: &str) -> Result<()> {
    // „start“ ist ein Befehl von cmd; der leere Titel verhindert, dass ein Pfad in Anführungszeichen als Titel gilt.
    spawn_detached("cmd", &["/C", "start", "", target], None)
}

pub fn download_dir() -> PathBuf {
    let d = home().join("Downloads");
    if d.is_dir() { d } else { home() }
}

/** Datei im Download-Ordner speichern; gibt es den Namen schon, wird „ (2)“ usw. angehängt. */
pub fn save_download(file: &str, bytes: &[u8]) -> Result<PathBuf> {
    let dir = download_dir();
    let name: String = file.chars().filter(|c| !['/', '\\', '\0', ':', '*', '?', '"', '<', '>', '|'].contains(c)).collect();
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

/** Datei im Explorer zeigen (markiert). */
pub fn show_in_file_manager(path: &Path) {
    let _ = spawn_detached("explorer", &[&format!("/select,{}", path.display())], None);
}

/** USB-Verbindung über „adb reverse“ (adb gibt es auch für Windows). */
pub async fn usb_reverse(port: u16) -> Result<Vec<String>> {
    let list = run("adb", &["devices"]).await?;
    let devices: Vec<String> = list.lines().skip(1).filter_map(|l| l.split_once('\t')).filter(|(_, st)| st.trim() == "device").map(|(id, _)| id.trim().to_string()).collect();
    let spec = format!("tcp:{port}");
    for d in &devices {
        let current = run("adb", &["-s", d, "reverse", "--list"]).await.unwrap_or_default();
        if !current.lines().any(|l| l.contains(&format!("{spec} {spec}"))) {
            run("adb", &["-s", d, "reverse", &spec, &spec]).await?;
        }
    }
    Ok(devices)
}

/** Befehl ausführen (cmd); wait = auf Ende warten und Ausgabe liefern. */
pub async fn shell(command: &str, wait: bool, timeout: Duration) -> Result<String> {
    let home = home();
    if !wait {
        spawn_detached("cmd", &["/C", command], Some(&home))?;
        return Ok(String::new());
    }
    let child = Command::from(self::command("cmd")).args(["/C", command]).current_dir(&home).kill_on_drop(true).stdin(Stdio::null()).output();
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

// ---------- Medien ----------

/** Medientasten (wirken auf den Player, den Windows gerade steuert). */
pub async fn media(op: &str) -> Result<String> {
    let key = match op {
        "play-pause" | "play" | "pause" => "playpause",
        "next" => "nexttrack",
        "previous" => "prevtrack",
        "stop" => "stop",
        _ => bail!("Unbekannte Medienaktion: {op}"),
    };
    super::input::send_hotkey(key).await?;
    Ok("Windows".into())
}

#[derive(Debug, Clone)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

/** Aktueller Titel: unter Windows noch nicht (Live-Anzeige zeigt dann „Keine Wiedergabe“). */
pub async fn current_track() -> Result<Option<Track>> {
    Ok(None)
}

// ---------- Lautstärke (Core Audio) ----------

pub const SINK: &str = "render";
pub const SOURCE: &str = "capture";

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Volume {
    pub volume: Option<i64>,
    pub muted: bool,
}

/** Lautstärke-Schnittstelle des Standard-Ausgangs (render) bzw. -Eingangs (capture). Nur im selben Thread benutzen. */
fn endpoint(target: &str) -> Result<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
    use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole, eRender};
    use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
    // SAFETY: COM wird je Thread eingerichtet; die Schnittstellen bleiben in diesem Thread.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(if target == SOURCE { eCapture } else { eRender }, eConsole)?;
        Ok(dev.Activate(CLSCTX_ALL, None)?)
    }
}

fn read_volume(target: &str) -> Result<Volume> {
    let ep = endpoint(target)?;
    // SAFETY: gültige Schnittstelle aus endpoint().
    unsafe {
        let v = ep.GetMasterVolumeLevelScalar()?;
        let muted = ep.GetMute()?.as_bool();
        Ok(Volume { volume: Some((v * 100.0).round() as i64), muted })
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| anyhow!("{e}"))?
}

pub async fn get_volume(target: &str) -> Result<Volume> {
    let target = target.to_string();
    blocking(move || read_volume(&target)).await
}

pub async fn volume(op: &str, step: f64, value: f64) -> Result<Volume> {
    let s = if step.is_finite() && step != 0.0 { step.clamp(1.0, 50.0) } else { 5.0 };
    let op = op.to_string();
    blocking(move || {
        let ep = endpoint(SINK)?;
        // SAFETY: gültige Schnittstelle aus endpoint().
        unsafe {
            let cur = f64::from(ep.GetMasterVolumeLevelScalar()?) * 100.0;
            let next = match op.as_str() {
                "up" => cur + s,
                "down" => cur - s,
                "set" => if value.is_finite() { value } else { 0.0 },
                "mute" => {
                    let m = ep.GetMute()?.as_bool();
                    ep.SetMute(!m, std::ptr::null())?;
                    return read_volume(SINK);
                }
                _ => bail!(t!("system.unknownVolumeAction", op)),
            };
            ep.SetMasterVolumeLevelScalar((next.clamp(0.0, 100.0) / 100.0) as f32, std::ptr::null())?;
        }
        read_volume(SINK)
    })
    .await
}

pub async fn mic(op: &str) -> Result<Volume> {
    let op = op.to_string();
    blocking(move || {
        let ep = endpoint(SOURCE)?;
        // SAFETY: gültige Schnittstelle aus endpoint().
        unsafe {
            let m = ep.GetMute()?.as_bool();
            let next = match op.as_str() {
                "mute" => true,
                "unmute" => false,
                _ => !m,
            };
            ep.SetMute(next, std::ptr::null())?;
        }
        read_volume(SOURCE)
    })
    .await
}

pub async fn set_volume_percent(target: &str, v: i64) -> Result<()> {
    let target = target.to_string();
    blocking(move || {
        let ep = endpoint(&target)?;
        // SAFETY: gültige Schnittstelle aus endpoint().
        unsafe { ep.SetMasterVolumeLevelScalar((v.clamp(0, 100) as f32) / 100.0, std::ptr::null())? };
        Ok(())
    })
    .await
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

/** Nur unter Linux (wpctl); hier leer. */
pub fn parse_audio_devices(_status: &str, _sinks: bool) -> Vec<AudioDevice> {
    Vec::new()
}

/** Geräteliste: vorerst nur das Standardgerät (Umschalten gibt es unter Windows noch nicht). */
pub async fn audio_devices(sinks: bool) -> Result<Vec<AudioDevice>> {
    let label = if sinks { t!("system.defaultOutput") } else { t!("system.defaultInput") };
    Ok(vec![AudioDevice { id: "default".into(), value: "default".into(), label: label.into(), is_default: true }])
}

pub async fn set_default_audio(_sinks: bool, _target: &str) -> Result<String> {
    Err(not_on_windows())
}

// ---------- Programme (Startmenü) ----------

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub comment: String,
}

type AppCache = Option<(Instant, Vec<AppInfo>)>;
static APP_CACHE: LazyLock<Mutex<AppCache>> = LazyLock::new(|| Mutex::new(None));

/** Verknüpfungen aus dem Startmenü (für alle Nutzer und den eigenen). */
pub fn list_apps() -> Vec<AppInfo> {
    let mut cache = APP_CACHE.lock().unwrap();
    if let Some((at, apps)) = cache.as_ref()
        && at.elapsed() < Duration::from_secs(60)
    {
        return apps.clone();
    }
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("ProgramData") {
        dirs.push(PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    dirs.push(config_dir().join("Microsoft\\Windows\\Start Menu\\Programs"));
    let mut apps = Vec::new();
    let mut stack = dirs;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                // Deinstallations- und Hilfe-Verknüpfungen weglassen.
                let lower = name.to_lowercase();
                if lower.contains("uninstall") || lower.contains("deinstall") {
                    continue;
                }
                apps.push(AppInfo { id: path.to_string_lossy().into_owned(), name, icon: String::new(), comment: String::new() });
            }
        }
    }
    apps.sort_by_key(|a| crate::system::sort_key(&a.name));
    apps.dedup_by(|a, b| a.name == b.name);
    *cache = Some((Instant::now(), apps.clone()));
    apps
}

pub fn launch_app(id: &str) -> Result<()> {
    open_with_default(id)
}

/** Benachrichtigung unten rechts (Toast über PowerShell, Texte über Umgebungsvariablen statt im Befehl). */
pub async fn notify(title: &str, body: &str) -> Result<()> {
    let title = if title.is_empty() { "TouchDeck" } else { title };
    let script = "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
        $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
        $x = $t.GetElementsByTagName('text'); \
        $x.Item(0).AppendChild($t.CreateTextNode($env:TD_TITLE)) > $null; \
        $x.Item(1).AppendChild($t.CreateTextNode($env:TD_BODY)) > $null; \
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($t))";
    let mut c = Command::from(command("powershell"));
    c.args(["-NoProfile", "-NonInteractive", "-Command", script]).env("TD_TITLE", title).env("TD_BODY", body).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let _ = tokio::time::timeout(DEFAULT_TIMEOUT, c.status()).await;
    Ok(())
}

// ---------- Firewall ----------

/** Ist die Windows-Firewall an? (Dann muss der Port für das Handy freigegeben werden.) */
pub async fn firewall_active() -> bool {
    run_with("netsh", &["advfirewall", "show", "currentprofile", "state"], Duration::from_secs(3), None)
        .await
        .is_ok_and(|s| s.contains(" ON") || s.contains(" EIN"))
}

/** Port freigeben; Windows fragt dabei nach Administratorrechten (UAC). */
pub async fn open_firewall(port: u16) -> Result<()> {
    let rules = format!(
        "netsh advfirewall firewall add rule name=TouchDeck dir=in action=allow protocol=TCP localport={port} profile=private; \
         netsh advfirewall firewall add rule name=TouchDeck-Suche dir=in action=allow protocol=UDP localport=8769 profile=private"
    );
    let arg = format!("-NoProfile -Command \"{rules}\"");
    run_with("powershell", &["-NoProfile", "-Command", &format!("Start-Process powershell -Verb RunAs -Wait -ArgumentList '{}'", arg.replace('\'', "''"))], Duration::from_secs(120), None)
        .await
        .map(|_| ())
}

// ---------- Systemwerte ----------

fn filetime(f: windows::Win32::Foundation::FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

static LAST_CPU: Mutex<Option<(Instant, u64, u64, Option<i64>)>> = Mutex::new(None);

/** CPU-Auslastung in Prozent seit dem letzten Aufruf (Systemzeiten). */
pub fn cpu_usage() -> Option<i64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetSystemTimes;
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: Zeiger auf lokale Werte.
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
    let (idle, total) = (filetime(idle), filetime(kernel) + filetime(user));
    let mut last = LAST_CPU.lock().unwrap();
    if let Some((at, _, _, v)) = *last
        && at.elapsed() < Duration::from_millis(900)
    {
        return v;
    }
    let v = last.and_then(|(_, i0, t0, _)| {
        let (di, dt) = (idle.saturating_sub(i0), total.saturating_sub(t0));
        (dt > 0).then(|| (100.0 * (1.0 - di as f64 / dt as f64)).round() as i64)
    });
    *last = Some((Instant::now(), idle, total, v));
    v
}

pub fn ram_percent() -> Option<i64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: Struktur mit gesetzter Länge.
    unsafe { GlobalMemoryStatusEx(&mut m).ok()? };
    Some(i64::from(m.dwMemoryLoad))
}

/** CPU-Temperatur liest Windows ohne Zusatzprogramm nicht aus. */
pub fn cpu_temp() -> Option<i64> {
    None
}

/** NVIDIA: über nvidia-smi (kommt mit dem Treiber); kurz zwischengespeichert. */
fn nvidia(query: &str) -> Option<i64> {
    static CACHE: Mutex<Option<(Instant, String, Option<i64>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if let Some((at, q, v)) = c.as_ref()
        && q == query
        && at.elapsed() < Duration::from_secs(2)
    {
        return *v;
    }
    let out = command("nvidia-smi").args([&format!("--query-gpu={query}"), "--format=csv,noheader,nounits"]).output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout).lines().next().and_then(|l| l.trim().parse::<f64>().ok()).map(|v| v.round() as i64);
    *c = Some((Instant::now(), query.to_string(), v));
    v
}

pub fn gpu_temp() -> Option<i64> {
    nvidia("temperature.gpu")
}

pub fn gpu_load() -> Option<i64> {
    nvidia("utilization.gpu")
}

/** Belegung des Laufwerks („/“ = Systemlaufwerk). */
pub fn disk_percent(path: &str) -> Option<i64> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::HSTRING;
    let root = if path.is_empty() || path == "/" { std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\" } else { path.to_string() };
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: Zeiger auf lokale Werte, Pfad als nullterminierter Text.
    unsafe { GetDiskFreeSpaceExW(&HSTRING::from(root), None, Some(&mut total), Some(&mut free)).ok()? };
    (total > 0).then(|| (100.0 * (1.0 - free as f64 / total as f64)).round() as i64)
}

/** Netzwerk-Durchsatz: unter Windows noch nicht. */
pub fn net_rate() -> Option<(f64, f64)> {
    None
}

pub fn uptime_secs() -> Option<u64> {
    // SAFETY: einfacher Systemaufruf ohne Zeiger.
    Some(unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000)
}

// ---------- Bildschirm & System ----------

/** Bildschirm ausschalten (Mausbewegung weckt ihn). */
pub async fn screen_off() -> Result<()> {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SC_MONITORPOWER, SendMessageW, WM_SYSCOMMAND};
    // SAFETY: Nachricht an alle Fenster, wie es Windows dafür vorsieht.
    unsafe { SendMessageW(HWND_BROADCAST, WM_SYSCOMMAND, Some(WPARAM(SC_MONITORPOWER as usize)), Some(LPARAM(2))) };
    Ok(())
}

/** Helligkeit: geht unter Windows nur bei eingebauten Bildschirmen (Laptops), über WMI. */
pub async fn screen_brightness(percent: u32) -> Result<&'static str> {
    let p = percent.clamp(10, 100);
    run("powershell", &["-NoProfile", "-Command", &format!("(Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightnessMethods).WmiSetBrightness(1,{p})")])
        .await
        .map(|_| "wmi")
        .map_err(|_| anyhow!(t!("system.noMonitorFound")))
}

/** Bildschirmfoto mit den Windows-Kürzeln: ganzer Bildschirm (Win+Druck), Bereich (Win+Umschalt+S), Fenster (Alt+Druck). */
pub fn screenshot(mode: &str) -> Result<()> {
    let keys = match mode {
        "area" | "interactive" | "" => "super+shift+s",
        "window" => "alt+print",
        _ => "super+print",
    };
    let keys = keys.to_string();
    tokio::spawn(async move {
        let _ = super::input::send_hotkey(&keys).await;
    });
    Ok(())
}

pub async fn lock_screen() -> Result<()> {
    // SAFETY: einfacher Systemaufruf.
    unsafe { windows::Win32::System::Shutdown::LockWorkStation()? };
    Ok(())
}

pub async fn power(op: &str) -> Result<()> {
    match op {
        "suspend" => spawn_detached("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"], None),
        "reboot" => run("shutdown", &["/r", "/t", "0"]).await.map(|_| ()),
        "poweroff" => run("shutdown", &["/s", "/t", "0"]).await.map(|_| ()),
        "logout" => run("shutdown", &["/l"]).await.map(|_| ()),
        _ => bail!(t!("system.unknownPowerAction", op)),
    }
}

pub async fn night_light(_mode: &str) -> Result<bool> {
    Err(not_on_windows())
}

/** Hell/Dunkel für Apps und Windows (Registry); Programme übernehmen es meist sofort. */
pub async fn dark_mode(mode: &str) -> Result<bool> {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    let cur = run("reg", &["query", KEY, "/v", "AppsUseLightTheme"]).await.unwrap_or_default();
    let is_dark = cur.contains("0x0");
    let dark = if mode == "toggle" { !is_dark } else { mode == "on" };
    let v = if dark { "0" } else { "1" };
    for name in ["AppsUseLightTheme", "SystemUsesLightTheme"] {
        run("reg", &["add", KEY, "/v", name, "/t", "REG_DWORD", "/d", v, "/f"]).await?;
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

/** Beendet ein Programm über seinen Namen (taskkill, „.exe“ wird ergänzt). */
pub async fn kill_process(name: &str, force: bool) -> Result<()> {
    let valid = (1..=64).contains(&name.chars().count()) && name.chars().all(|c| c.is_alphanumeric() || " _.+-".contains(c));
    if !valid {
        bail!(t!("system.invalidProgramName"));
    }
    let image = if name.to_lowercase().ends_with(".exe") { name.to_string() } else { format!("{name}.exe") };
    let mut args = vec!["/IM", image.as_str()];
    if force {
        args.push("/F");
    }
    run("taskkill", &args).await.map_err(|_| anyhow!(t!("system.noRunningProgramFound", name)))?;
    Ok(())
}

// ---------- Soundboard ----------

/** Spielt eine WAV-Datei ab (Windows-eigene Wiedergabe; ein Sound zur Zeit, Lautstärke wie Windows). */
pub fn play_sound(file: &str, _volume: f64, _exclusive: bool) -> Result<()> {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    use windows::core::HSTRING;
    let f = expand_home(file);
    if file.is_empty() || !f.exists() {
        bail!(t!("system.audioFileNotFound", file));
    }
    if !f.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
        bail!(t!("system.onlyWavOnWindows"));
    }
    // SAFETY: Pfad als nullterminierter Text; ASYNC kehrt sofort zurück.
    let ok = unsafe { PlaySoundW(&HSTRING::from(f.as_os_str()), None, SND_FILENAME | SND_ASYNC | SND_NODEFAULT) };
    if ok.as_bool() { Ok(()) } else { bail!(t!("system.audioFileNotFound", file)) }
}

pub fn stop_sounds() {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_PURGE};
    // SAFETY: Ohne Datei beendet PlaySound die laufende Wiedergabe.
    unsafe {
        let _ = PlaySoundW(None, None, SND_PURGE);
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
