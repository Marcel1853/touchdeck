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

static QUITTING: AtomicBool = AtomicBool::new(false);
static HINT_SHOWN: AtomicBool = AtomicBool::new(false);

fn port() -> u16 {
    std::env::var("TOUCHDECK_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8770)
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
        .icon(Image::from_bytes(include_bytes!("../icons/icon.png"))?)?
        // Vorschau im eigenen kleinen Fenster, alle anderen Links im normalen Browser.
        .on_new_window(move |url, _| {
            if url.as_str().starts_with(&format!("{}/deck/", base())) {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move { open_preview_window(&h) });
            } else {
                let _ = linux::open_with_default(url.as_str());
            }
            NewWindowResponse::Deny
        })
        .on_navigation(|url| {
            let inside = url.as_str().starts_with(&format!("{}/", base()));
            if !inside {
                let _ = linux::open_with_default(url.as_str());
            }
            inside
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
                let _ = linux::notify("TouchDeck läuft weiter", "Über das Symbol in der Systemleiste öffnest du den Editor wieder oder beendest TouchDeck.").await;
            });
        }
    });
    Ok(window)
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
    let _ = window.show();
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
    let url = format!("{}/deck/", base()).parse().expect("gültige Adresse");
    let built = WebviewWindowBuilder::new(app, "preview", WebviewUrl::External(url))
        .title("TouchDeck – Vorschau")
        .inner_size(480.0, 820.0)
        .icon(Image::from_bytes(include_bytes!("../icons/icon.png")).expect("Symbol"))
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

// ---------- Systemleiste ----------

fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let editor = MenuItem::with_id(app, "editor", "Editor öffnen", true, None::<&str>)?;
    let pair = MenuItem::with_id(app, "pair", "Handy verbinden …", true, None::<&str>)?;
    let preview = MenuItem::with_id(app, "preview", "Vorschau öffnen", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Beim Anmelden starten", true, autostart_file().exists(), None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "TouchDeck beenden", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&editor, &pair, &preview, &PredefinedMenuItem::separator(app)?, &autostart, &PredefinedMenuItem::separator(app)?, &quit])?;
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
            "quit" => {
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
    let hidden = std::env::args().any(|a| a == "--hidden");
    tauri::Builder::default()
        // Zweiter Start (z. B. Klick im Startmenü, während TouchDeck im Hintergrund läuft): Editor zeigen.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_editor(app, "")))
        .setup(move |app| {
            let handle = app.handle().clone();
            let public = public_dir(&handle);
            let data = data_dir();
            let store = Store::open(&data).map_err(|e| e.to_string())?;
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
            if let Err(err) = tauri::async_runtime::block_on(server::start(store, opts)) {
                let in_use = err.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::AddrInUse);
                let msg = if in_use {
                    format!("Port {} ist schon belegt. Läuft TouchDeck bereits (z. B. die ältere Fassung)? Diese bitte zuerst beenden.", port())
                } else {
                    err.to_string()
                };
                eprintln!("TouchDeck kann nicht starten: {msg}");
                let _ = std::process::Command::new("notify-send").args(["-a", "TouchDeck", "TouchDeck kann nicht starten", &msg]).status();
                std::process::exit(1);
            }
            println!("TouchDeck läuft auf {} – Daten: {}", base(), data.display());
            create_tray(&handle)?;
            let window = create_editor(&handle)?;
            if !hidden {
                window.show()?;
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
