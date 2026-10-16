// Tastatur, Maus und Fenster unter X11 steuern (XTest und EWMH).
// Lädt libX11 und libXtst erst zur Laufzeit – die sind auf jedem X11-Desktop vorhanden.
// Alle Funktionen blockieren (Warten zwischen Tastendrücken) und laufen daher in spawn_blocking.
use crate::t;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_long, c_uchar, c_ulong};
use std::ptr;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use x11_dl::xlib::{self, Display, Xlib};
use x11_dl::xtest::Xf86vmode as XTest;

type KeySym = c_ulong;

const CLIENT_MESSAGE: c_int = 33;
const SUBSTRUCTURE_REDIRECT_AND_NOTIFY: c_long = (1 << 20) | (1 << 19);
const SHIFT: &str = "Shift_L";
const LEVEL3: &str = "ISO_Level3_Shift";

/** Namen, wie sie der Editor erzeugt → X11-Keysym-Namen. */
fn key_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "ctrl" | "control" => "Control_L",
        "shift" => "Shift_L",
        "alt" => "Alt_L",
        "altgr" => "ISO_Level3_Shift",
        "super" | "meta" | "win" => "Super_L",
        "enter" | "return" => "Return",
        "esc" | "escape" => "Escape",
        "space" => "space",
        "tab" => "Tab",
        "backspace" => "BackSpace",
        "delete" | "del" => "Delete",
        "insert" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" => "Prior",
        "pagedown" => "Next",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "print" | "printscreen" => "Print",
        "pause" => "Pause",
        "menu" => "Menu",
        "capslock" => "Caps_Lock",
        "numlock" => "Num_Lock",
        "scrolllock" => "Scroll_Lock",
        "plus" => "plus",
        "minus" => "minus",
        "comma" => "comma",
        "period" => "period",
        "playpause" => "XF86AudioPlay",
        "nexttrack" => "XF86AudioNext",
        "prevtrack" => "XF86AudioPrev",
        "stop" => "XF86AudioStop",
        "volumeup" => "XF86AudioRaiseVolume",
        "volumedown" => "XF86AudioLowerVolume",
        "volumemute" => "XF86AudioMute",
        _ => return None,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Window {
    pub id: u64,
    pub class: String,
    pub title: String,
}

struct Keyboard {
    x: Xlib,
    t: XTest,
    dpy: *mut Display,
    lo: c_int,
    per: c_int,
    map: Vec<Vec<KeySym>>,
    spare: Option<c_int>,
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        // SAFETY: dpy stammt aus XOpenDisplay und wird nur hier geschlossen.
        unsafe {
            (self.x.XSync)(self.dpy, 0);
            (self.x.XCloseDisplay)(self.dpy);
        }
    }
}

impl Keyboard {
    fn open() -> Result<Self> {
        let x = Xlib::open().map_err(|e| anyhow!(t!("system.libMissing", "libX11", e)))?;
        let t = XTest::open().map_err(|e| anyhow!(t!("system.libMissing", "libXtst", e)))?;
        // SAFETY: Aufrufe der Xlib mit gültigen Zeigern; die Tastaturbelegung wird kopiert und danach freigegeben.
        unsafe {
            let dpy = (x.XOpenDisplay)(ptr::null());
            if dpy.is_null() {
                bail!(t!("system.noConnectionToX"));
            }
            let (mut lo, mut hi) = (0, 0);
            (x.XDisplayKeycodes)(dpy, &mut lo, &mut hi);
            let count = hi - lo + 1;
            let mut per = 0;
            let mapping = (x.XGetKeyboardMapping)(dpy, lo as c_uchar, count, &mut per);
            let mut map = Vec::with_capacity(count as usize);
            for i in 0..count {
                map.push((0..per).map(|j| *mapping.add((i * per + j) as usize)).collect::<Vec<_>>());
            }
            (x.XFree)(mapping.cast());
            let spare = map.iter().position(|syms| syms.iter().all(|s| *s == 0)).map(|i| lo + i as c_int);
            Ok(Keyboard { x, t, dpy, lo, per, map, spare })
        }
    }

    fn keysym(&self, name: &str) -> Result<KeySym> {
        let c = CString::new(name)?;
        // SAFETY: c ist ein gültiger, nullterminierter Text.
        let sym = unsafe { (self.x.XStringToKeysym)(c.as_ptr()) };
        if sym == 0 { Err(anyhow!(t!("system.unknownKey", name))) } else { Ok(sym) }
    }

    /** Liefert (Keycode, nötige Umschalttasten) für ein Keysym. */
    fn locate(&self, sym: KeySym) -> Option<(c_int, Vec<&'static str>)> {
        let levels: [(usize, &[&'static str]); 4] = [(0, &[]), (1, &[SHIFT]), (4, &[LEVEL3]), (5, &[LEVEL3, SHIFT])];
        for (idx, mods) in levels {
            for (i, syms) in self.map.iter().enumerate() {
                if syms.get(idx) == Some(&sym) {
                    return Some((self.lo + i as c_int, mods.to_vec()));
                }
            }
        }
        None
    }

    fn tap(&self, code: c_int, down: bool, up: bool) {
        // SAFETY: XTest-Aufrufe mit offener Verbindung.
        unsafe {
            if down {
                (self.t.XTestFakeKeyEvent)(self.dpy, code as u32, 1, 0);
            }
            if up {
                (self.t.XTestFakeKeyEvent)(self.dpy, code as u32, 0, 0);
            }
            (self.x.XFlush)(self.dpy);
        }
    }

    fn set_mapping(&self, code: c_int, syms: &mut [KeySym]) {
        // SAFETY: syms hat genau per Einträge, wie XChangeKeyboardMapping es erwartet.
        unsafe {
            (self.x.XChangeKeyboardMapping)(self.dpy, code, self.per, syms.as_mut_ptr(), 1);
            (self.x.XSync)(self.dpy, 0);
        }
    }

    /** Belegt kurzzeitig einen freien Keycode, falls ein Zeichen nicht im Layout ist. */
    fn with_temp_code(&self, sym: KeySym, f: impl FnOnce(c_int) -> Result<()>) -> Result<()> {
        let spare = self.spare.ok_or_else(|| anyhow!(t!("system.noFreeKeycodeFound")))?;
        let mut syms = vec![0; self.per as usize];
        syms[0] = sym;
        if syms.len() > 1 {
            syms[1] = sym;
        }
        self.set_mapping(spare, &mut syms);
        sleep(Duration::from_millis(30));
        let result = f(spare);
        // SAFETY: wie oben, Verbindung ist offen.
        unsafe { (self.x.XSync)(self.dpy, 0) };
        sleep(Duration::from_millis(30));
        self.set_mapping(spare, &mut vec![0; self.per as usize]);
        result
    }

    fn char_sym(ch: char) -> KeySym {
        let cp = ch as u32;
        if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) { cp as KeySym } else { (0x0100_0000 | cp) as KeySym }
    }

    /** Ein Teil eines Kürzels: einzelnes Zeichen („a“, „#“, „ü“) oder Tastenname („enter“, „F5“). */
    fn name_sym(&self, part: &str) -> Result<KeySym> {
        let mut chars = part.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            return Ok(Self::char_sym(c.to_lowercase().next().unwrap_or(c)));
        }
        let lower = part.to_lowercase();
        let mut name = key_name(&lower).map_or_else(|| part.to_string(), String::from);
        if name.len() <= 3 && name.starts_with(['f', 'F']) && name[1..].chars().all(|c| c.is_ascii_digit()) {
            name = name.to_uppercase();
        }
        self.keysym(&name)
    }

    fn code_of(&self, sym: KeySym) -> Result<c_int> {
        self.locate(sym).map(|(c, _)| c).ok_or_else(|| anyhow!(t!("system.keyDoesNotExist")))
    }

    /** Drückt ein Keysym, gehalten zusammen mit den Modifikator-Keysyms extra_mods. */
    fn press_sym(&self, sym: KeySym, extra_mods: &[KeySym]) -> Result<()> {
        let run = |code: c_int, mods: &[KeySym]| -> Result<()> {
            let codes = mods.iter().map(|m| self.code_of(*m)).collect::<Result<Vec<_>>>()?;
            for c in &codes {
                self.tap(*c, true, false);
            }
            self.tap(code, true, true);
            for c in codes.iter().rev() {
                self.tap(*c, false, true);
            }
            Ok(())
        };
        match self.locate(sym) {
            None => self.with_temp_code(sym, |c| run(c, extra_mods)),
            Some((code, level_mods)) => {
                let mut mods = extra_mods.to_vec();
                for m in level_mods {
                    let s = self.keysym(m)?;
                    if !mods.contains(&s) {
                        mods.push(s);
                    }
                }
                run(code, &mods)
            }
        }
    }

    fn parts(combo: &str) -> Vec<&str> {
        combo.split('+').map(str::trim).filter(|p| !p.is_empty()).collect()
    }

    fn hotkey(&self, combo: &str) -> Result<()> {
        let syms = Self::parts(combo).into_iter().map(|p| self.name_sym(p)).collect::<Result<Vec<_>>>()?;
        let Some((last, mods)) = syms.split_last() else { bail!(t!("system.emptyShortcut")) };
        self.press_sym(*last, mods)
    }

    fn type_text(&self, text: &str) -> Result<()> {
        for ch in text.chars() {
            let sym = match ch {
                '\n' => self.keysym("Return")?,
                '\t' => self.keysym("Tab")?,
                ' ' => self.keysym("space")?,
                c => Self::char_sym(c),
            };
            self.press_sym(sym, &[])?;
            sleep(Duration::from_millis(8));
        }
        Ok(())
    }

    /** Wie hotkey, hält die Tasten aber für ms Millisekunden gedrückt. */
    fn hold(&self, combo: &str, ms: u64) -> Result<()> {
        let codes = Self::parts(combo).into_iter().map(|p| self.name_sym(p).and_then(|s| self.code_of(s))).collect::<Result<Vec<_>>>()?;
        for c in &codes {
            self.tap(*c, true, false);
        }
        sleep(Duration::from_millis(ms.min(60000)));
        for c in codes.iter().rev() {
            self.tap(*c, false, true);
        }
        Ok(())
    }

    fn button(&self, n: u32) {
        // SAFETY: XTest-Aufrufe mit offener Verbindung.
        unsafe {
            (self.t.XTestFakeButtonEvent)(self.dpy, n, 1, 0);
            (self.t.XTestFakeButtonEvent)(self.dpy, n, 0, 0);
            (self.x.XFlush)(self.dpy);
        }
    }

    fn click(&self, which: &str) -> Result<()> {
        let n = match which {
            "double" => {
                self.button(1);
                sleep(Duration::from_millis(60));
                self.button(1);
                return Ok(());
            }
            "left" => 1,
            "middle" => 2,
            "right" => 3,
            "scrollup" => 4,
            "scrolldown" => 5,
            _ => bail!("Unbekannte Maustaste: {which}"),
        };
        self.button(n);
        Ok(())
    }

    fn move_to(&self, x: i32, y: i32, relative: bool) {
        // SAFETY: XTest-Aufrufe mit offener Verbindung.
        unsafe {
            let (mut tx, mut ty) = (x, y);
            if relative {
                // Relativ: von der aktuellen Zeigerposition aus verschieben.
                let root = (self.x.XDefaultRootWindow)(self.dpy);
                let (mut r, mut c, mut rx, mut ry, mut wx, mut wy, mut mask) = (0, 0, 0, 0, 0, 0, 0);
                (self.x.XQueryPointer)(self.dpy, root, &mut r, &mut c, &mut rx, &mut ry, &mut wx, &mut wy, &mut mask);
                tx = rx + x;
                ty = ry + y;
            }
            (self.t.XTestFakeMotionEvent)(self.dpy, -1, tx, ty, 0);
            (self.x.XFlush)(self.dpy);
        }
    }

    // ---------- Fenster (EWMH) ----------

    fn atom(&self, name: &str) -> c_ulong {
        let c = CString::new(name).expect("Atomnamen enthalten keine Nullbytes");
        // SAFETY: gültiger Text, offene Verbindung.
        unsafe { (self.x.XInternAtom)(self.dpy, c.as_ptr(), 0) }
    }

    /** Liest eine Fenster-Eigenschaft: (Bytes, Format, Anzahl). */
    fn prop(&self, win: c_ulong, name: &str) -> Option<(Vec<u8>, c_int, usize)> {
        let (mut rtype, mut rfmt, mut n, mut after) = (0, 0, 0, 0);
        let mut data: *mut c_uchar = ptr::null_mut();
        // SAFETY: Ausgabezeiger sind gültig; die gelieferten Daten werden kopiert und mit XFree freigegeben.
        unsafe {
            let ok = (self.x.XGetWindowProperty)(self.dpy, win, self.atom(name), 0, 65536, 0, 0, &mut rtype, &mut rfmt, &mut n, &mut after, &mut data);
            if ok != 0 || data.is_null() {
                return None;
            }
            let unit = match rfmt {
                16 => 2,
                32 => std::mem::size_of::<c_long>(),
                _ => 1,
            };
            let bytes = std::slice::from_raw_parts(data, unit * n as usize).to_vec();
            (self.x.XFree)(data.cast());
            Some((bytes, rfmt, n as usize))
        }
    }

    fn windows(&self) -> Vec<Window> {
        // SAFETY: offene Verbindung.
        let root = unsafe { (self.x.XDefaultRootWindow)(self.dpy) };
        let Some((raw, 32, n)) = self.prop(root, "_NET_CLIENT_LIST") else { return Vec::new() };
        let size = std::mem::size_of::<c_ulong>();
        (0..n)
            .map(|i| {
                let id = c_ulong::from_ne_bytes(raw[i * size..(i + 1) * size].try_into().expect("passende Länge"));
                let text = |name| self.prop(id, name).map(|(b, _, _)| String::from_utf8_lossy(&b).into_owned());
                let title = text("_NET_WM_NAME").or_else(|| text("WM_NAME")).unwrap_or_default();
                let class_raw = self.prop(id, "WM_CLASS").map(|(b, _, _)| b).unwrap_or_default();
                let parts: Vec<&[u8]> = class_raw.split(|b| *b == 0).collect();
                let class = parts.get(1).filter(|p| !p.is_empty()).or(parts.first()).map(|p| String::from_utf8_lossy(p).into_owned()).unwrap_or_default();
                Window { id: id as u64, class, title }
            })
            .collect()
    }

    /** Programm (WM_CLASS) und Titel des Fensters im Vordergrund. */
    fn active(&self) -> Option<Window> {
        // SAFETY: offene Verbindung.
        let root = unsafe { (self.x.XDefaultRootWindow)(self.dpy) };
        let (raw, 32, 1..) = self.prop(root, "_NET_ACTIVE_WINDOW")? else { return None };
        let size = std::mem::size_of::<c_ulong>();
        let id = c_ulong::from_ne_bytes(raw.get(..size)?.try_into().ok()?);
        self.windows().into_iter().find(|w| w.id == id as u64)
    }

    fn activate(&self, query: &str) -> Result<String> {
        let q = query.to_lowercase();
        let wins = self.windows();
        let found = wins
            .iter()
            .find(|w| w.class.to_lowercase() == q)
            .or_else(|| wins.iter().find(|w| w.class.to_lowercase().contains(&q)))
            .or_else(|| wins.iter().find(|w| w.title.to_lowercase().contains(&q)))
            .ok_or_else(|| anyhow!(t!("system.noOpenWindowFound", query)))?;
        // SAFETY: Ereignis wird vollständig initialisiert und an das Wurzelfenster geschickt.
        unsafe {
            let mut ev: xlib::XEvent = std::mem::zeroed();
            ev.client_message.type_ = CLIENT_MESSAGE;
            ev.client_message.send_event = 1;
            ev.client_message.window = found.id as c_ulong;
            ev.client_message.message_type = self.atom("_NET_ACTIVE_WINDOW");
            ev.client_message.format = 32;
            ev.client_message.data.as_longs_mut()[0] = 2; // Quelle: Werkzeug wie Pager/Taskleiste
            let root = (self.x.XDefaultRootWindow)(self.dpy);
            (self.x.XSendEvent)(self.dpy, root, 0, SUBSTRUCTURE_REDIRECT_AND_NOTIFY, &mut ev);
            (self.x.XFlush)(self.dpy);
        }
        Ok(found.title.clone())
    }
}

/** Führt eine X11-Aufgabe in einem eigenen Thread aus (Tastendrücke warten zwischendurch). */
async fn with_keyboard<T: Send + 'static>(f: impl FnOnce(&Keyboard) -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(move || f(&Keyboard::open()?)).await.map_err(|e| anyhow!("X11: {e}"))?
}

pub async fn send_hotkey(keys: &str) -> Result<()> {
    let keys = keys.to_string();
    with_keyboard(move |kb| kb.hotkey(&keys)).await
}

pub async fn type_text(text: &str) -> Result<()> {
    let text = text.to_string();
    with_keyboard(move |kb| kb.type_text(&text)).await
}

pub async fn hold_keys(keys: &str, ms: u64) -> Result<()> {
    let keys = keys.to_string();
    with_keyboard(move |kb| kb.hold(&keys, ms)).await
}

pub async fn mouse_click(which: &str) -> Result<()> {
    let which = which.to_string();
    with_keyboard(move |kb| kb.click(&which)).await
}

pub async fn mouse_move(x: i32, y: i32, relative: bool) -> Result<()> {
    with_keyboard(move |kb| {
        kb.move_to(x, y, relative);
        Ok(())
    })
    .await
}

pub async fn activate_window(query: &str) -> Result<String> {
    let query = query.to_string();
    with_keyboard(move |kb| kb.activate(&query)).await
}

/** Fenster im Vordergrund: (Programm, Titel); None, wenn keins aktiv ist. */
pub async fn active_window() -> Result<Option<(String, String)>> {
    with_keyboard(|kb| Ok(kb.active().map(|w| (w.class, w.title)))).await
}

/** Offene Fenster, je Programm (WM_CLASS) ein Eintrag. */
pub async fn open_windows() -> Result<Vec<(String, String)>> {
    let list = with_keyboard(|kb| Ok(kb.windows())).await?;
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(String, String)> = Vec::new();
    for w in list {
        let skip = ["nemo-desktop", "xfdesktop", "desktop_window"].contains(&w.class.to_lowercase().as_str());
        if w.class.is_empty() || skip || !seen.insert(w.class.clone()) {
            continue;
        }
        let title: String = w.title.chars().take(50).collect();
        out.push((w.class.clone(), format!("{} – {title}", w.class)));
    }
    out.sort_by_key(|(_, label)| super::sort_key(label));
    Ok(out)
}

// ---------- Globale Tastenkürzel ----------

/** Kürzel in einheitlicher Form: Umschalttasten in fester Reihenfolge (wie der Editor), Taste zuletzt. */
pub fn normalize_combo(combo: &str) -> String {
    let lower = combo.to_lowercase();
    let parts = Keyboard::parts(&lower);
    let alias = |p: &str| match p {
        "control" => "ctrl".to_string(),
        "meta" | "win" => "super".to_string(),
        other => other.to_string(),
    };
    let parts: Vec<String> = parts.iter().map(|p| alias(p)).collect();
    let mut out: Vec<String> = ["ctrl", "alt", "shift", "super", "altgr"].iter().filter(|m| parts.iter().any(|p| p == *m)).map(|m| m.to_string()).collect();
    out.extend(parts.into_iter().filter(|p| !["ctrl", "alt", "shift", "super", "altgr"].contains(&p.as_str())));
    out.join("+")
}

static GRAB_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/** X-Fehler (z. B. „Kürzel schon belegt“) nur merken, statt das Programm zu beenden. */
unsafe extern "C" fn on_x_error(_dpy: *mut Display, _ev: *mut xlib::XErrorEvent) -> c_int {
    GRAB_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
    0
}

/**
 * Hört global auf Tastenkürzel (X11, auch wenn TouchDeck nicht im Vordergrund ist), bis `stop` gesetzt wird.
 * Ruft `on_press` mit dem Kürzel in einheitlicher Form auf; `on_busy` für Kürzel, die ein anderes Programm belegt.
 */
pub fn listen_hotkeys(combos: &[String], stop: &std::sync::atomic::AtomicBool, on_press: impl Fn(&str), on_busy: impl Fn(&str)) -> Result<()> {
    use std::sync::atomic::Ordering;
    let kb = Keyboard::open()?;
    let x = &kb.x;
    let mods = |m: &str| -> Option<u32> {
        Some(match m {
            "ctrl" => xlib::ControlMask,
            "alt" => xlib::Mod1Mask,
            "shift" => xlib::ShiftMask,
            "super" => xlib::Mod4Mask,
            "altgr" => xlib::Mod5Mask,
            _ => return None,
        })
    };
    let relevant = xlib::ControlMask | xlib::Mod1Mask | xlib::ShiftMask | xlib::Mod4Mask | xlib::Mod5Mask;
    // Kürzel auch bei aktivem Num- oder Feststelltaste erkennen.
    let locks = [0, xlib::LockMask, xlib::Mod2Mask, xlib::LockMask | xlib::Mod2Mask];
    let mut grabbed: Vec<(c_int, u32, String)> = Vec::new();
    // SAFETY: Xlib-Aufrufe mit offener Verbindung; der Fehler-Handler gibt nur 0 zurück.
    unsafe {
        (x.XSetErrorHandler)(Some(on_x_error));
        let root = (x.XDefaultRootWindow)(kb.dpy);
        for combo in combos {
            let norm = normalize_combo(combo);
            let parts: Vec<&str> = norm.split('+').collect();
            let Some((key, modifiers)) = parts.split_last() else { continue };
            let mask = modifiers.iter().filter_map(|m| mods(m)).fold(0, |a, b| a | b);
            let Ok(code) = kb.name_sym(key).and_then(|sym| kb.code_of(sym)) else {
                on_busy(&norm);
                continue;
            };
            GRAB_FAILED.store(false, Ordering::Relaxed);
            for lock in locks {
                (x.XGrabKey)(kb.dpy, code, mask | lock, root, 0, xlib::GrabModeAsync, xlib::GrabModeAsync);
            }
            (x.XSync)(kb.dpy, 0);
            if GRAB_FAILED.load(Ordering::Relaxed) {
                on_busy(&norm);
            }
            grabbed.push((code, mask, norm));
        }
        (x.XSync)(kb.dpy, 0);
        let mut last: Option<(String, std::time::Instant)> = None;
        while !stop.load(Ordering::Relaxed) {
            while (x.XPending)(kb.dpy) > 0 {
                let mut ev: xlib::XEvent = std::mem::zeroed();
                (x.XNextEvent)(kb.dpy, &mut ev);
                if ev.get_type() != xlib::KeyPress {
                    continue;
                }
                let k = ev.key;
                let state = k.state & relevant;
                if let Some((_, _, combo)) = grabbed.iter().find(|(c, m, _)| *c == k.keycode as c_int && *m == state) {
                    // Gedrückt halten wiederholt die Taste: höchstens alle 0,3 s auslösen.
                    let repeat = last.as_ref().is_some_and(|(c, t)| c == combo && t.elapsed() < Duration::from_millis(300));
                    last = Some((combo.clone(), std::time::Instant::now()));
                    if !repeat {
                        on_press(combo);
                    }
                }
            }
            sleep(Duration::from_millis(25));
        }
        for (code, mask, _) in &grabbed {
            for lock in locks {
                (x.XUngrabKey)(kb.dpy, *code, mask | lock, root);
            }
        }
        (x.XSync)(kb.dpy, 0);
    }
    Ok(())
}

/** Name eines Keysyms (für Tests und Fehlermeldungen). */
#[allow(dead_code)]
fn keysym_name(x: &Xlib, sym: KeySym) -> Option<String> {
    // SAFETY: XKeysymToString liefert einen statischen Text oder NULL.
    unsafe {
        let p = (x.XKeysymToString)(sym);
        (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeichen_zu_keysym() {
        assert_eq!(Keyboard::char_sym('a'), 0x61);
        assert_eq!(Keyboard::char_sym('ü'), 0xfc);
        assert_eq!(Keyboard::char_sym('€'), 0x0100_20ac);
    }

    #[test]
    fn kuerzel_teile() {
        assert_eq!(Keyboard::parts("ctrl + shift+s"), vec!["ctrl", "shift", "s"]);
        assert_eq!(key_name("pageup"), Some("Prior"));
    }

    #[test]
    fn kuerzel_einheitlich() {
        assert_eq!(normalize_combo("Shift+Ctrl+F5"), "ctrl+shift+f5");
        assert_eq!(normalize_combo("win + alt + a"), "alt+super+a");
        assert_eq!(normalize_combo("ctrl+alt+plus"), "ctrl+alt+plus");
    }
}
