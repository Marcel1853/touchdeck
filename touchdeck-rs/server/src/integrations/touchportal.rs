// Übernimmt Seiten aus Touch Portal (~/.config/TouchPortal/pages/*.tml).
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};

use crate::core::store::{Page, new_id, sanitize_page};
use crate::system::linux;

pub fn default_dir() -> PathBuf {
    linux::home().join(".config/TouchPortal")
}

/** Java-AWT-Tastencodes (KeyEvent.VK_*) → Tastennamen von TouchDeck. */
fn awt_key(code: i64) -> Option<String> {
    Some(match code {
        65..=90 => char::from_u32((code + 32) as u32)?.to_string(),
        48..=57 => char::from_u32(code as u32)?.to_string(),
        112..=123 => format!("f{}", code - 111),
        61440..=61451 => format!("f{}", code - 61427),
        _ => match code {
            8 => "backspace",
            9 => "tab",
            10 => "enter",
            16 => "shift",
            17 => "ctrl",
            18 => "alt",
            19 => "pause",
            20 => "capslock",
            27 => "esc",
            32 => "space",
            33 => "pageup",
            34 => "pagedown",
            35 => "end",
            36 => "home",
            37 => "left",
            38 => "up",
            39 => "right",
            40 => "down",
            44 => "comma",
            45 => "minus",
            46 => "period",
            127 => "delete",
            144 => "numlock",
            145 => "scrolllock",
            154 => "print",
            155 => "insert",
            157 | 524 => "super",
            521 => "plus",
            525 => "menu",
            65406 => "altgr",
            _ => return None,
        }
        .to_string(),
    })
}

/** Java speichert Farben als vorzeichenbehaftete ARGB-Ganzzahl. */
fn argb_to_hex(v: &Value, fallback: &str) -> String {
    match v.as_i64() {
        Some(n) => format!("#{:06x}", (n as u32) & 0x00ff_ffff),
        None => fallback.into(),
    }
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn convert_action(a: &Value, page_ids: &HashMap<String, String>) -> Value {
    let act = |kind: &str, params: Value| json!({ "id": new_id("a_"), "type": kind, "enabled": a["kIEn"] != false, "params": params });
    let quote = |p: &str| if p.contains(' ') { format!("\"{p}\"") } else { p.to_string() };
    match s(a, "KEY_TYPE") {
        "OPEN_URL_ACTION" => return act("open_url", json!({ "url": s(a, "KEY_URL") })),
        "OPEN_FILE_ACTION" => return act("open_path", json!({ "path": s(a, "KEY_FP") })),
        "OPEN_FOLDER_ACTION" => return act("open_path", json!({ "path": s(a, "KEY_PATH") })),
        "START_APPLICATION_ACTION" => {
            let cmd = [quote(s(a, "KEY_PATH")), s(a, "KEY_COMMANDLINE").to_string()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
            return act("run_command", json!({ "command": cmd, "wait": false }));
        }
        "START_BATCH_SCRIPT_ACTION" => {
            let script = if s(a, "kP").is_empty() { String::new() } else { format!("\"{}\"", s(a, "kP")) };
            let cmd = [script, s(a, "kC").to_string()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
            return act("run_command", json!({ "command": cmd, "wait": false }));
        }
        "KEY_TEXT_WRITE" => return act("type_text", json!({ "text": s(a, "KEY_TEXT_TO_WRITE") })),
        "KEY_PRESS_ACTION" => {
            let keys: Option<Vec<String>> = a["KEY_KEYCODES"].as_array().map(|l| l.iter().map(|c| c.as_i64().and_then(awt_key)).collect()).unwrap_or(Some(Vec::new()));
            if let Some(keys) = keys.filter(|k| !k.is_empty()) {
                return act("hotkey", json!({ "keys": keys.join("+") }));
            }
        }
        "GOTO_PAGE_ACTION" => {
            let raw = js_text(&a["KEY_PAGE_NAME"]);
            let raw = raw.strip_suffix(".tml").unwrap_or(&raw);
            let name = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
            if name == "(Previous Page)" {
                return act("go_page", json!({ "page": "__back" }));
            }
            if let Some(id) = page_ids.get(name) {
                return act("go_page", json!({ "page": id }));
            }
        }
        _ => {}
    }
    let original = a["KEY_TYPE"].as_str().map_or_else(|| "unbekannt".to_string(), String::from);
    act("unsupported", json!({ "original": original }))
}

fn js_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn icon_for(b: &Value, tp_dir: &Path, icon_dir: &Path, copied: &mut HashSet<String>) -> Value {
    let empty = json!({ "type": "emoji", "value": "" });
    let Some(name) = b["I"].as_str().filter(|n| !n.is_empty()) else { return empty };
    let base = Path::new(name).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let src = tp_dir.join("icons").join(&base);
    if base.is_empty() || !src.exists() {
        return empty;
    }
    let target = format!("tp_{base}");
    if !copied.contains(&target) && fs::copy(&src, icon_dir.join(&target)).is_ok() {
        copied.insert(target.clone());
    }
    json!({ "type": "image", "value": target })
}

fn display_name(name: &str) -> String {
    if name == "(main)" { "Hauptseite (Touch Portal)".into() } else { name.into() }
}

fn find_page_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            find_page_files(&p, out);
        } else if name.ends_with(".tml") && name != "null.tml" {
            out.push(p);
        }
    }
}

/** Liest alle Touch-Portal-Seiten und liefert TouchDeck-Seiten samt Statistik.
 * existing_ids: Seitenname → ID bereits importierter Seiten, damit ein erneuter Import sie ersetzt. */
pub fn import(tp_dir: &Path, icon_dir: &Path, existing_ids: &HashMap<String, String>) -> Result<(Vec<Page>, Value)> {
    let pages_dir = tp_dir.join("pages");
    if !pages_dir.exists() {
        bail!("Kein Touch-Portal-Ordner gefunden unter {}", pages_dir.display());
    }
    let mut files = Vec::new();
    find_page_files(&pages_dir, &mut files);
    files.sort();
    let mut raw = Vec::new();
    for file in files {
        let name = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let id = existing_ids.get(&display_name(&name)).cloned().unwrap_or_else(|| new_id("p_"));
        let json: Value = serde_json::from_str(&fs::read_to_string(&file)?).map_err(|e| anyhow!("{}: {e}", file.display()))?;
        raw.push((name, id, json));
    }
    let page_ids: HashMap<String, String> = raw.iter().map(|(n, id, _)| (n.clone(), id.clone())).collect();
    let mut copied = HashSet::new();
    let (mut n_pages, mut n_buttons, mut n_actions) = (0, 0, 0);
    let mut unsupported: BTreeMap<String, u64> = BTreeMap::new();

    let mut pages = Vec::new();
    for (name, id, json) in &raw {
        let cols = json["KEY_COLUMNS"].as_i64().unwrap_or(4);
        let rows = json["KEY_ROWS"].as_i64().unwrap_or(2);
        let mut buttons = Vec::new();
        // BUTTONS[x][y]: x ist die Spalte, y die Zeile.
        for (x, column) in json["BUTTONS"].as_array().into_iter().flatten().enumerate() {
            for (y, b) in column.as_array().into_iter().flatten().enumerate() {
                if !b.is_object() || b["id"].is_null() || b["id"] == false || x as i64 >= cols || y as i64 >= rows {
                    continue;
                }
                let actions: Vec<Value> = b["A"].as_array().into_iter().flatten().map(|a| convert_action(a, &page_ids)).collect();
                for a in &actions {
                    n_actions += 1;
                    if a["type"] == "unsupported" {
                        *unsupported.entry(a["params"]["original"].as_str().unwrap_or("").to_string()).or_default() += 1;
                    }
                }
                let ts = b["TS"].as_i64().filter(|t| *t > 0).unwrap_or(14);
                let title_pos = match b["TP"].as_i64() {
                    Some(1) => "top",
                    Some(3) => "center",
                    _ => "bottom",
                };
                buttons.push(json!({
                    "id": new_id("b_"), "row": y, "col": x,
                    "rowSpan": b["ROWS"].as_i64().unwrap_or(1), "colSpan": b["COLS"].as_i64().unwrap_or(1),
                    "title": b["T"].as_str().unwrap_or(""),
                    "icon": icon_for(b, tp_dir, icon_dir, &mut copied),
                    "color": argb_to_hex(&b["BG"], "#2b2f3a"), "textColor": argb_to_hex(&b["TC"], "#ffffff"),
                    "fontSize": ts, "titlePos": title_pos, "actions": actions,
                }));
                n_buttons += 1;
            }
        }
        n_pages += 1;
        if let Some(p) = sanitize_page(&json!({
            "id": id, "name": display_name(name), "template": "touchportal", "rows": rows, "cols": cols,
            "background": argb_to_hex(&json["BG"], "#14161b"), "buttons": buttons,
        })) {
            pages.push(p);
        }
    }
    let stats = json!({ "pages": n_pages, "buttons": n_buttons, "actions": n_actions, "icons": copied.len(), "unsupported": unsupported });
    Ok((pages, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasten_und_farben() {
        assert_eq!(awt_key(65).as_deref(), Some("a"));
        assert_eq!(awt_key(116).as_deref(), Some("f5"));
        assert_eq!(awt_key(17).as_deref(), Some("ctrl"));
        assert_eq!(awt_key(99999), None);
        assert_eq!(argb_to_hex(&json!(-16777216), "#000001"), "#000000");
        assert_eq!(argb_to_hex(&json!(-1), "#000001"), "#ffffff");
        assert_eq!(argb_to_hex(&Value::Null, "#000001"), "#000001");
    }

    #[test]
    fn aktionen_umwandeln() {
        let ids = HashMap::from([("Seite 2".to_string(), "p_2".to_string())]);
        let a = convert_action(&json!({ "KEY_TYPE": "KEY_PRESS_ACTION", "KEY_KEYCODES": [17, 84] }), &ids);
        assert_eq!(a["params"]["keys"], "ctrl+t");
        let g = convert_action(&json!({ "KEY_TYPE": "GOTO_PAGE_ACTION", "KEY_PAGE_NAME": "pages/Seite 2.tml" }), &ids);
        assert_eq!(g["params"]["page"], "p_2");
        let u = convert_action(&json!({ "KEY_TYPE": "SOMETHING" }), &ids);
        assert_eq!(u["type"], "unsupported");
    }
}
