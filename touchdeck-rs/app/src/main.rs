// Desktop-Programm: startet den TouchDeck-Server, zeigt den Editor im eigenen Fenster
// und bleibt über das Symbol in der Systemleiste im Hintergrund erreichbar.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use touchdeck_server::core::store::Store;
use touchdeck_server::server::{self, Options};
use touchdeck_server::system::linux;
use touchdeck_server::t;

static QUITTING: AtomicBool = AtomicBool::new(false);
static HINT_SHOWN: AtomicBool = AtomicBool::new(false);

/** Port dieses Starts: TOUCHDECK_PORT, sonst aus den Einstellungen (beim Start gesetzt), sonst 8770. */
static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

fn port() -> u16 {
    *PORT.get_or_init(|| std::env::var("TOUCHDECK_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8770))
}

fn base() -> String {
    format!("http://localhost:{}", port())
}

/** Datenordner: wie bisher ~/.config/TouchDeck/data, damit alle Seiten und Einstellungen übernommen werden. */
fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("TOUCHDECK_DATA").filter(|d| !d.is_empty()) {
        return PathBuf::from(d);
    }
    let config = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| linux::home().join(".config"), PathBuf::from);
    config.join("TouchDeck/data")
}

/** Editor und Bedienfeld: im Paket als Ressource, beim Entwickeln aus dem Quellordner touchdeck-rs/web. */
fn public_dir(app: &AppHandle) -> PathBuf {
    if let Some(d) = std::env::var_os("TOUCHDECK_PUBLIC") {
        return PathBuf::from(d);
    }
    let bundled = app.path().resource_dir().map(|r| r.join("web"));
    match bundled {
        Ok(p) if p.join("editor/index.html").exists() => p,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web"),
    }
}

// ---------- Fenster ----------

fn create_editor(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let url = format!("{}/editor/", base()).parse().expect("gültige Adresse");
    let handle = app.clone();
    let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
        .title("TouchDeck")
        .inner_size(1440.0, 900.0)
        .min_inner_size(1000.0, 640.0)
        .visible(false)
        .icon(window_icon())?
        // Vorschau im eigenen kleinen Fenster, alle anderen Links im normalen Browser.
        .on_new_window(move |url, _| {
            if url.as_str().starts_with(&format!("{}/deck/", base())) {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move { open_preview_window(&h) });
            } else {
                open_external(url.as_str());
            }
            NewWindowResponse::Deny
        })
        .on_navigation(|url| {
            // Nur TouchDeck selbst und die Adressen von Webseiten-Buttons (in deren Rahmen) dürfen laden.
            // Alles andere wird still verworfen – nie an den Browser gegeben, sonst könnte eine
            // eingebettete Seite endlos Fenster öffnen.
            url.as_str().starts_with(&format!("{}/", base())) || touchdeck_server::core::store::is_web_button_url(url.as_str())
        })
        .build()?;
    // Schließen schließt nur das Fenster (und gibt dessen Speicher frei): der Server läuft weiter,
    // damit das Handy verbunden bleibt. Über die Systemleiste entsteht das Fenster neu.
    window.on_window_event(|event| {
        if let WindowEvent::Destroyed = event
            && !QUITTING.load(Ordering::Relaxed)
            && !HINT_SHOWN.swap(true, Ordering::Relaxed)
        {
            tauri::async_runtime::spawn(async {
                let _ = linux::notify(t!("desktop.keepsRunning"), t!("desktop.keepsRunningText")).await;
            });
        }
    });
    Ok(window)
}

/** Link im Standard-Browser öffnen – nur http(s) und höchstens einmal alle 3 Sekunden (Schutz vor Fensterfluten). */
fn open_external(url: &str) {
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    let mut last = LAST.lock().unwrap();
    if last.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(3)) {
        return;
    }
    *last = Some(std::time::Instant::now());
    let _ = linux::open_with_default(url);
}

/** Fenstersymbol (für Taskleiste und Alt+Tab). Unter Linux geht das beim Anlegen eines unsichtbaren Fensters
 *  verloren, darum wird es nach dem Anzeigen noch einmal gesetzt. */
fn window_icon() -> Image<'static> {
    Image::from_bytes(include_bytes!("../icons/icon.png")).expect("Symbol")
}

fn show_window(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    window.show()?;
    window.set_icon(window_icon())
}

fn show_editor(app: &AppHandle, hash: &str) {
    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => match create_editor(app) {
            Ok(w) => w,
            Err(err) => return eprintln!("Fenster: {err}"),
        },
    };
    if !hash.is_empty() {
        let _ = window.eval(format!("location.hash = {}", serde_json_string(hash)));
    }
    let _ = show_window(&window);
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn serde_json_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn open_preview_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("preview") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    // Vorschau mit Geräteauswahl (Handy, Tablet, eigene Geräte); darin läuft das Bedienfeld.
    let url = format!("{}/preview/", base()).parse().expect("gültige Adresse");
    let built = WebviewWindowBuilder::new(app, "preview", WebviewUrl::External(url))
        .title(t!("desktop.previewTitle"))
        .inner_size(1000.0, 900.0)
        .icon(window_icon())
        .and_then(|b| b.build());
    if let Err(err) = built {
        eprintln!("Vorschau: {err}");
    }
}

// ---------- Autostart ----------

fn autostart_file() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| linux::home().join(".config"), PathBuf::from);
    config.join("autostart/touchdeck.desktop")
}

fn launch_command() -> String {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return format!("\"{}\" --hidden", appimage.to_string_lossy());
    }
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "touchdeck".into());
    format!("\"{exe}\" --hidden")
}

/** Laufender Server, damit beim Beenden gesammelte Änderungen noch gespeichert werden. */
static SERVER: std::sync::OnceLock<std::sync::Arc<server::App>> = std::sync::OnceLock::new();

fn save_before_exit() {
    if let Some(app) = SERVER.get() {
        app.store.lock().unwrap().flush();
    }
}

/** Neu starten: dieselbe Datei noch einmal starten (beim AppImage die AppImage-Datei) und dieses Programm beenden. */
fn restart(app: &AppHandle) {
    let program = std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok());
    let Some(program) = program else { return eprintln!("Neustart: Programmdatei nicht gefunden") };
    let visible = app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false);
    let mut cmd = linux::command(&program);
    cmd.arg(format!("--restart-of={}", std::process::id()));
    if !visible {
        cmd.arg("--hidden");
    }
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    if let Err(err) = cmd.spawn() {
        return eprintln!("Neustart: {err}");
    }
    save_before_exit();
    QUITTING.store(true, Ordering::Relaxed);
    app.exit(0);
}

/** Beim Neustart: warten, bis das alte Programm beendet ist (Port frei, kein „läuft schon“), höchstens 15 Sekunden. */
fn wait_for_restart() {
    let Some(pid) = std::env::args().find_map(|a| a.strip_prefix("--restart-of=").and_then(|p| p.parse::<u32>().ok())) else { return };
    let proc = PathBuf::from(format!("/proc/{pid}"));
    for _ in 0..150 {
        if !proc.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn set_autostart(on: bool) -> std::io::Result<()> {
    let file = autostart_file();
    if !on {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let entry = [
        "[Desktop Entry]",
        "Type=Application",
        "Name=TouchDeck",
        "Comment=Makro-Fernbedienung im Hintergrund starten",
        &format!("Exec={}", launch_command()),
        "Icon=touchdeck",
        "Terminal=false",
        "X-GNOME-Autostart-enabled=true",
        "",
    ];
    std::fs::write(file, entry.join("\n"))
}

// ---------- Starter-Eintrag ----------

/** Starter-Eintrag und Symbol im Benutzerordner, damit Taskleiste und Startmenü TouchDeck mit Symbol zeigen.
 *  Die Taskleiste ordnet das Fenster über StartupWMClass zu. Beim .deb-Paket gibt es den Eintrag schon. */
fn install_desktop_entry() -> std::io::Result<()> {
    if std::path::Path::new("/usr/share/applications/TouchDeck.desktop").exists() {
        return Ok(());
    }
    let data = std::env::var_os("XDG_DATA_HOME").map_or_else(|| linux::home().join(".local/share"), PathBuf::from);
    // Vollständiger Pfad statt Symbolname: ein veralteter icon-theme.cache würde den Namen sonst nicht finden.
    let icon_dir = data.join("icons/hicolor/512x512/apps");
    std::fs::create_dir_all(&icon_dir)?;
    let icon = icon_dir.join("touchdeck.png");
    std::fs::write(&icon, include_bytes!("../icons/icon.png"))?;
    let apps = data.join("applications");
    std::fs::create_dir_all(&apps)?;
    let exec = launch_command().trim_end_matches(" --hidden").to_string();
    let entry = [
        "[Desktop Entry]",
        "Type=Application",
        "Name=TouchDeck",
        "Comment=Makro-Fernbedienung fürs Handy",
        "Comment[en]=Macro remote for your phone",
        &format!("Exec={exec}"),
        &format!("Icon={}", icon.display()),
        "StartupWMClass=touchdeck",
        "Categories=Utility;",
        "Terminal=false",
        "",
    ];
    std::fs::write(apps.join("touchdeck.desktop"), entry.join("\n"))
}

// ---------- Systemleiste ----------

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let editor = MenuItem::with_id(app, "editor", t!("desktop.openEditor"), true, None::<&str>)?;
    let pair = MenuItem::with_id(app, "pair", t!("desktop.connectPhone"), true, None::<&str>)?;
    let preview = MenuItem::with_id(app, "preview", t!("desktop.openPreview"), true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", t!("desktop.startAtLogin"), true, autostart_file().exists(), None::<&str>)?;
    let restart_item = MenuItem::with_id(app, "restart", t!("desktop.restart"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t!("desktop.quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&editor, &pair, &preview, &PredefinedMenuItem::separator(app)?, &autostart, &PredefinedMenuItem::separator(app)?, &restart_item, &quit])?;
    let check = autostart.clone();
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("TouchDeck")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "editor" => show_editor(app, ""),
            "pair" => show_editor(app, "pair"),
            "preview" => {
                show_editor(app, "");
                open_preview_window(app);
            }
            "autostart" => {
                let on = check.is_checked().unwrap_or(false);
                if let Err(err) = set_autostart(on) {
                    eprintln!("Autostart: {err}");
                }
                let _ = check.set_checked(autostart_file().exists());
            }
            "restart" => restart(app),
            "quit" => {
                save_before_exit();
                QUITTING.store(true, Ordering::Relaxed);
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

// ---------- Start ----------

fn main() {
    wait_for_restart();
    let hidden = std::env::args().any(|a| a == "--hidden");
    tauri::Builder::default()
        // Zweiter Start (z. B. Klick im Startmenü, während TouchDeck im Hintergrund läuft): Editor zeigen.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_editor(app, "")))
        .setup(move |app| {
            let handle = app.handle().clone();
            let public = public_dir(&handle);
            let data = data_dir();
            let store = Store::open(&data).map_err(|e| e.to_string())?;
            // Ohne TOUCHDECK_PORT gilt der Port aus den Einstellungen.
            if std::env::var("TOUCHDECK_PORT").ok().and_then(|p| p.parse::<u16>().ok()).is_none() {
                let _ = PORT.set(store.settings().port);
            }
            // Kopplungsanfrage vom Handy: Fenster nach vorn holen.
            let pair_handle = handle.clone();
            let opts = Options {
                port: port(),
                host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                apk: public.join("../downloads/TouchDeck.apk"),
                public_dir: public,
                on_pair_request: Some(Box::new(move |_| {
                    let h = pair_handle.clone();
                    let _ = pair_handle.run_on_main_thread(move || {
                        show_editor(&h, "");
                        if let Some(w) = h.get_webview_window("main") {
                            let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational));
                        }
                    });
                })),
            };
            let started = tauri::async_runtime::block_on(server::start(store, opts));
            if let Ok(app) = &started {
                let _ = SERVER.set(app.clone());
            }
            if let Err(err) = started {
                let in_use = err.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::AddrInUse);
                let msg = if in_use {
                    t!("desktop.portInUse", port())
                } else {
                    err.to_string()
                };
                eprintln!("{}: {msg}", t!("desktop.cannotStart"));
                let _ = std::process::Command::new("notify-send").args(["-a", "TouchDeck", t!("desktop.cannotStart"), &msg]).status();
                std::process::exit(1);
            }
            println!("{}", t!("desktop.running", base(), data.display()));
            // Eingeschalteten Autostart auf diese Version zeigen lassen (Dateiname ändert sich mit jeder Version).
            if autostart_file().exists()
                && let Err(err) = set_autostart(true)
            {
                eprintln!("Autostart: {err}");
            }
            if let Err(err) = install_desktop_entry() {
                eprintln!("Starter-Eintrag: {err}");
            }
            create_tray(&handle)?;
            let window = create_editor(&handle)?;
            if !hidden {
                show_window(&window)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("TouchDeck konnte nicht gestartet werden")
        // Ohne offenes Fenster weiterlaufen – das Symbol in der Systemleiste hält das Programm.
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event
                && code.is_none()
                && !QUITTING.load(Ordering::Relaxed)
            {
                api.prevent_exit();
            }
        });
}
