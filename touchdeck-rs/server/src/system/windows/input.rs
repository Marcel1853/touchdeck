// Windows-Fassung von Tastatur, Maus, Fenstern, Zwischenablage und globalen Tastenkürzeln
// (Gegenstück zu x11.rs, gleiche Funktionen und dieselben Tastennamen wie „ctrl+shift+s“).
use crate::t;
use std::ffi::c_void;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, TRUE};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PWSTR};

#[derive(Debug, Clone, Serialize)]
pub struct Window {
    pub id: u64,
    pub class: String,
    pub title: String,
}

/** Tastenname → virtueller Tastencode (gleiche Namen wie unter Linux). */
fn vk(name: &str) -> Option<u16> {
    let n = name.to_lowercase();
    Some(match n.as_str() {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" => 0x12,
        "altgr" => 0xA5,
        "super" | "meta" | "win" => 0x5B,
        "enter" | "return" => 0x0D,
        "esc" | "escape" => 0x1B,
        "space" => 0x20,
        "tab" => 0x09,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "print" | "printscreen" => 0x2C,
        "pause" => 0x13,
        "menu" => 0x5D,
        "capslock" => 0x14,
        "numlock" => 0x90,
        "scrolllock" => 0x91,
        "plus" => 0xBB,
        "minus" => 0xBD,
        "comma" => 0xBC,
        "period" => 0xBE,
        "playpause" => 0xB3,
        "nexttrack" => 0xB0,
        "prevtrack" => 0xB1,
        "stop" => 0xB2,
        "volumeup" => 0xAF,
        "volumedown" => 0xAE,
        "volumemute" => 0xAD,
        _ => {
            // F1–F24
            if let Some(num) = n.strip_prefix('f').and_then(|x| x.parse::<u16>().ok()).filter(|x| (1..=24).contains(x)) {
                return Some(0x70 + num - 1);
            }
            let mut chars = n.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else { return None };
            if c.is_ascii_alphanumeric() {
                return Some(c.to_ascii_uppercase() as u16);
            }
            // Andere Zeichen über die aktuelle Tastaturbelegung.
            // SAFETY: einfacher Systemaufruf.
            let r = unsafe { VkKeyScanW(c as u16) };
            if r == -1 {
                return None;
            }
            (r as u16) & 0xFF
        }
    })
}

fn key_input(vk: u16, up: bool) -> INPUT {
    // Pfeile, Pos1, Ende usw. sind „erweiterte“ Tasten.
    let extended = matches!(vk, 0x21..=0x28 | 0x2D | 0x2E | 0x5B | 0x5D | 0xA5);
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

fn send(inputs: &[INPUT]) -> Result<()> {
    // SAFETY: gültige INPUT-Strukturen.
    let n = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if n as usize != inputs.len() {
        bail!(t!("system.keysBlocked"));
    }
    Ok(())
}

fn parts(combo: &str) -> Vec<&str> {
    combo.split('+').map(str::trim).filter(|p| !p.is_empty()).collect()
}

fn codes(keys: &str) -> Result<Vec<u16>> {
    parts(&keys.to_lowercase()).iter().map(|k| vk(k).ok_or_else(|| anyhow!(t!("system.unknownKey", k)))).collect()
}

/** Kürzel drücken: alle Tasten nacheinander runter, dann rückwärts wieder hoch. */
fn press(keys: &str) -> Result<()> {
    let list = codes(keys)?;
    if list.is_empty() {
        bail!(t!("actions.noShortcutGiven"));
    }
    let mut inputs: Vec<INPUT> = list.iter().map(|k| key_input(*k, false)).collect();
    inputs.extend(list.iter().rev().map(|k| key_input(*k, true)));
    send(&inputs)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| anyhow!("{e}"))?
}

pub async fn send_hotkey(keys: &str) -> Result<()> {
    let keys = keys.to_string();
    blocking(move || press(&keys)).await
}

pub async fn type_text(text: &str) -> Result<()> {
    let text = text.to_string();
    blocking(move || {
        // Jedes Zeichen als Unicode-Eingabe – unabhängig von der Tastaturbelegung.
        for unit in text.encode_utf16() {
            let mk = |up: bool| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: unit, dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
                },
            };
            send(&[mk(false), mk(true)])?;
            sleep(Duration::from_millis(4));
        }
        Ok(())
    })
    .await
}

pub async fn hold_keys(keys: &str, ms: u64) -> Result<()> {
    let keys = keys.to_string();
    blocking(move || {
        let list = codes(&keys)?;
        send(&list.iter().map(|k| key_input(*k, false)).collect::<Vec<_>>())?;
        sleep(Duration::from_millis(ms.min(60_000)));
        send(&list.iter().rev().map(|k| key_input(*k, true)).collect::<Vec<_>>())
    })
    .await
}

fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32) -> INPUT {
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}

pub async fn mouse_click(which: &str) -> Result<()> {
    let which = which.to_string();
    blocking(move || {
        let (down, up) = match which.as_str() {
            "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            "middle" => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
            _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        };
        let times = if which == "double" { 2 } else { 1 };
        for _ in 0..times {
            send(&[mouse(down, 0, 0), mouse(up, 0, 0)])?;
            sleep(Duration::from_millis(40));
        }
        Ok(())
    })
    .await
}

pub async fn mouse_move(x: i32, y: i32, relative: bool) -> Result<()> {
    blocking(move || {
        if relative {
            send(&[mouse(MOUSEEVENTF_MOVE, x, y)])
        } else {
            // SAFETY: einfacher Systemaufruf.
            unsafe { SetCursorPos(x, y)? };
            Ok(())
        }
    })
    .await
}

// ---------- Fenster ----------

/** Programmname eines Fensters („factorio“ für …\factorio.exe) – entspricht der Fensterklasse unter Linux. */
fn program_of(hwnd: HWND) -> String {
    let mut pid = 0u32;
    // SAFETY: Zeiger auf lokale Werte; Handle wird wieder geschlossen.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

fn title_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: Puffer mit fester Größe.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn hwnd(id: u64) -> HWND {
    HWND(id as usize as *mut c_void)
}

/** Sichtbare Hauptfenster mit Titel. */
fn windows_list() -> Vec<Window> {
    unsafe extern "system" fn collect(h: HWND, data: LPARAM) -> BOOL {
        // SAFETY: data zeigt auf den Vec aus windows_list(), solange EnumWindows läuft.
        unsafe {
            let list = &mut *(data.0 as *mut Vec<Window>);
            if IsWindowVisible(h).as_bool() && GetWindowTextLengthW(h) > 0 {
                list.push(Window { id: h.0 as usize as u64, class: program_of(h), title: title_of(h) });
            }
        }
        TRUE
    }
    let mut list: Vec<Window> = Vec::new();
    // SAFETY: Rückruf schreibt nur in die lokale Liste.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut Vec<Window> as isize));
    }
    list
}

fn active() -> Option<Window> {
    // SAFETY: einfacher Systemaufruf.
    let h = unsafe { GetForegroundWindow() };
    if h.0.is_null() {
        return None;
    }
    Some(Window { id: h.0 as usize as u64, class: program_of(h), title: title_of(h) })
}

/**
 * Fenster nach vorn holen. Windows lässt das nur zu, wenn TouchDeck „gerade eine Eingabe hatte“ –
 * ein kurzes Alt-Drücken erfüllt das.
 */
fn raise(id: u64) {
    let h = hwnd(id);
    // SAFETY: gültiges Fensterhandle aus EnumWindows/GetForegroundWindow.
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let _ = send(&[key_input(0x12, false), key_input(0x12, true)]);
        let _ = SetForegroundWindow(h);
    }
}

fn find(query: &str, by_title: bool) -> Option<Window> {
    let q = query.to_lowercase();
    let list = windows_list();
    list.iter()
        .find(|w| w.class.to_lowercase() == q)
        .or_else(|| list.iter().find(|w| w.class.to_lowercase().contains(&q)))
        .or_else(|| if by_title { list.iter().find(|w| w.title.to_lowercase().contains(&q)) } else { None })
        .cloned()
}

pub async fn send_hotkey_to(window: &str, keys: &str, back: bool) -> Result<()> {
    let (window, keys) = (window.to_string(), keys.to_string());
    blocking(move || {
        let before = active();
        // Nur nach dem Programm suchen, nie nach dem Titel (sonst bekäme ein Editor mit dem Namen im Titel die Tasten).
        let target = find(&window, false).ok_or_else(|| anyhow!(t!("system.programNotRunningKeysNotSent", window)))?;
        if before.as_ref().is_none_or(|w| w.id != target.id) {
            raise(target.id);
        }
        let mut focused = false;
        for _ in 0..40 {
            if active().is_some_and(|w| w.id == target.id) {
                focused = true;
                break;
            }
            sleep(Duration::from_millis(25));
        }
        if !focused {
            bail!(t!("system.windowNotInFrontKeysNotSent", window));
        }
        sleep(Duration::from_millis(60));
        let result = press(&keys);
        if back && let Some(w) = before {
            sleep(Duration::from_millis(60));
            raise(w.id);
        }
        result
    })
    .await
}

pub async fn activate_window(query: &str) -> Result<String> {
    let query = query.to_string();
    blocking(move || {
        let w = find(&query, true).ok_or_else(|| anyhow!(t!("system.noOpenWindowFound", query)))?;
        raise(w.id);
        Ok(w.title)
    })
    .await
}

pub async fn active_window() -> Result<Option<(String, String)>> {
    blocking(|| Ok(active().map(|w| (w.class, w.title)))).await
}

/** Offene Fenster, je Programm ein Eintrag. */
pub async fn open_windows() -> Result<Vec<(String, String)>> {
    let list = blocking(|| Ok(windows_list())).await?;
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for w in list {
        if w.class.is_empty() || ["explorer", "applicationframehost", "textinputhost"].contains(&w.class.to_lowercase().as_str()) || !seen.insert(w.class.clone()) {
            continue;
        }
        let title: String = w.title.chars().take(50).collect();
        out.push((w.class.clone(), format!("{} – {title}", w.class)));
    }
    out.sort_by_key(|(_, label)| super::sort_key(label));
    Ok(out)
}

// ---------- Zwischenablage ----------

const CF_UNICODETEXT: u32 = 13;

pub async fn clipboard_get() -> Result<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    blocking(|| {
        // SAFETY: Zwischenablage wird geöffnet, gelesen und wieder geschlossen.
        unsafe {
            OpenClipboard(None)?;
            let text = match GetClipboardData(CF_UNICODETEXT) {
                Ok(h) => {
                    let g = HGLOBAL(h.0);
                    let p = GlobalLock(g) as *const u16;
                    let mut s = String::new();
                    if !p.is_null() {
                        let mut len = 0;
                        while *p.add(len) != 0 {
                            len += 1;
                        }
                        s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                        let _ = GlobalUnlock(g);
                    }
                    s
                }
                Err(_) => String::new(),
            };
            let _ = CloseClipboard();
            Ok(text)
        }
    })
    .await
}

/** Text in die Zwischenablage legen (höchstens 200 000 Zeichen). */
pub async fn clipboard_set(text: &str) -> Result<()> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    let text: String = text.chars().take(200_000).collect();
    blocking(move || {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: Speicher in passender Größe; nach SetClipboardData gehört er der Zwischenablage.
        unsafe {
            let g = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
            let p = GlobalLock(g) as *mut u16;
            if p.is_null() {
                bail!(t!("system.clipboardNotTaken"));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
            let _ = GlobalUnlock(g);
            OpenClipboard(None)?;
            let _ = EmptyClipboard();
            let r = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(g.0)));
            let _ = CloseClipboard();
            r.map_err(|_| anyhow!(t!("system.clipboardNotTaken")))?;
        }
        Ok(())
    })
    .await
}

// ---------- Globale Tastenkürzel ----------

/** Kürzel in einheitlicher Form („ctrl+shift+f5“): Umschalttasten zuerst, in fester Reihenfolge. */
pub fn normalize_combo(combo: &str) -> String {
    let lower = combo.to_lowercase();
    let alias = |p: &str| match p {
        "control" => "ctrl".to_string(),
        "meta" | "win" => "super".to_string(),
        other => other.to_string(),
    };
    let parts: Vec<String> = parts(&lower).iter().map(|p| alias(p)).collect();
    let mut out: Vec<String> = ["ctrl", "alt", "shift", "super", "altgr"].iter().filter(|m| parts.iter().any(|p| p == *m)).map(|m| m.to_string()).collect();
    out.extend(parts.into_iter().filter(|p| !["ctrl", "alt", "shift", "super", "altgr"].contains(&p.as_str())));
    out.join("+")
}

/**
 * Hört global auf Tastenkürzel (RegisterHotKey), bis `stop` gesetzt wird. Belegte Kürzel meldet `on_busy`.
 * Läuft im eigenen Thread; die Kürzel-Nachrichten kommen in dessen Nachrichtenschlange.
 */
pub fn listen_hotkeys(combos: &[String], stop: &std::sync::atomic::AtomicBool, on_press: impl Fn(&str), on_busy: impl Fn(&str)) -> Result<()> {
    use std::sync::atomic::Ordering;
    let mut registered: Vec<(i32, String)> = Vec::new();
    for (i, combo) in combos.iter().enumerate() {
        let norm = normalize_combo(combo);
        let p = parts(&norm);
        let Some((key, modifiers)) = p.split_last() else { continue };
        let mut mods = MOD_NOREPEAT;
        for m in modifiers {
            mods |= match *m {
                "ctrl" => MOD_CONTROL,
                "alt" | "altgr" => MOD_ALT,
                "shift" => MOD_SHIFT,
                "super" => MOD_WIN,
                _ => HOT_KEY_MODIFIERS(0),
            };
        }
        let Some(code) = vk(key) else {
            on_busy(&norm);
            continue;
        };
        let id = i as i32 + 1;
        // SAFETY: Kürzel für diesen Thread (ohne Fenster).
        if unsafe { RegisterHotKey(None, id, mods, u32::from(code)) }.is_err() {
            on_busy(&norm);
            continue;
        }
        registered.push((id, norm));
    }
    let mut msg = MSG::default();
    while !stop.load(Ordering::Relaxed) {
        // SAFETY: Nachrichten dieses Threads abholen.
        while unsafe { PeekMessageW(&mut msg, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE) }.as_bool() {
            if let Some((_, combo)) = registered.iter().find(|(id, _)| msg.wParam.0 as i32 == *id) {
                on_press(combo);
            }
        }
        sleep(Duration::from_millis(100));
    }
    for (id, _) in &registered {
        // SAFETY: eigene Kürzel wieder freigeben.
        unsafe {
            let _ = UnregisterHotKey(None, *id);
        }
    }
    Ok(())
}
