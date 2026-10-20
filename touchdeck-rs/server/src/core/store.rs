// Speichert Seiten, Buttons, Werte, Vorlagen, Events und Einstellungen als JSON im Datenordner.
// Gleiches Format wie die bisherige Version: config.json kann zwischen beiden hin- und hergetauscht werden.
use crate::t;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::sync::broadcast;

use crate::core::catalog;

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}{}", hex::encode(rand::random::<[u8; 6]>()))
}

pub fn random_token() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}

/** Erlaubte Namen für Werte: 1–40 Zeichen aus Buchstaben, Ziffern, Leerzeichen, _ . - */
pub fn valid_variable_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=40).contains(&n) && name.chars().all(|c| c.is_alphabetic() || c.is_numeric() || " _.-".contains(c))
}

pub const TEMPLATES: [&str; 3] = ["steam", "system", "obs"];

const MAX_TEXT: usize = 4000;
const MAX_ACTIONS: usize = 100;

// ---------- Hilfen, die sich wie die JavaScript-Fassung verhalten ----------

/** Wie parseInt(v, 10): Zahlen werden abgeschnitten, Text bis zur ersten Nicht-Ziffer gelesen. */
fn parse_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64),
        Value::String(s) => {
            let s = s.trim_start();
            let (sign, rest) = match s.as_bytes().first() {
                Some(b'-') => (-1, &s[1..]),
                Some(b'+') => (1, &s[1..]),
                _ => (1, s),
            };
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).take(18).collect();
            digits.parse::<i64>().ok().map(|n| sign * n)
        }
        _ => None,
    }
}

fn clamp(v: &Value, min: i64, max: i64, fallback: i64) -> i64 {
    parse_int(v).map_or(fallback, |n| n.clamp(min, max))
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/** Text übernehmen (gekürzt), sonst Ersatzwert. */
fn text(v: &Value, fallback: &str, max: usize) -> String {
    v.as_str().map_or_else(|| fallback.to_string(), |s| truncate(s, max))
}

/** Wie str(v) || fallback: leerer Text zählt als fehlend. */
fn text_or(v: &Value, fallback: &str, max: usize) -> String {
    let t = text(v, "", max);
    if t.is_empty() { fallback.to_string() } else { t }
}

fn id_or_new(v: &Value, prefix: &str) -> String {
    let t = text(v, "", MAX_TEXT);
    if t.is_empty() { new_id(prefix) } else { t }
}

fn color(v: &Value, fallback: &str) -> String {
    match v.as_str() {
        Some(s) if s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit()) => s.to_lowercase(),
        _ => fallback.to_string(),
    }
}

/** enabled !== false */
fn not_false(v: &Value) -> bool {
    v != &Value::Bool(false)
}

/** So wie JavaScript String(value ?? '') einen Wert in Text umwandelt. */
pub fn js_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i128),
            _ => n.to_string(),
        },
        Value::Array(a) => a.iter().map(js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

// ---------- Datentypen ----------

pub type Params = Map<String, Value>;

#[derive(Clone, Debug, Serialize)]
pub struct Action {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub enabled: bool,
    pub params: Params,
}

#[derive(Clone, Debug, Serialize)]
pub struct Trigger {
    #[serde(rename = "type")]
    pub kind: String,
    pub params: Params,
}

#[derive(Clone, Debug, Serialize)]
pub struct ButtonEvent {
    pub id: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Icon {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub source: String,
    pub command: String,
    pub variable: String,
    /// Zahlenwert (0–100) zusätzlich als Fortschrittsbalken zeigen.
    pub bar: bool,
    /// Verlauf als Graph: "" (keiner), line, area, bars.
    pub graph: String,
    /// Wie viele Werte der Graph zeigt (10–300) und alle wie viele Sekunden einer dazukommt (1–3600).
    pub graph_points: i64,
    pub graph_interval: i64,
    /// Feste Unter-/Obergrenze; None = automatisch aus den Werten.
    pub graph_min: Option<f64>,
    pub graph_max: Option<f64>,
    /// Farbe des Graphen ("" = Textfarbe).
    pub graph_color: String,
    /// Text unter dem Graphen: left, center, right.
    pub graph_align: String,
    /// Weitere Datenreihen im selben Graphen (je eigene Quelle, Name, Farbe).
    pub series: Vec<GraphSeries>,
}

/** Weitere Datenreihe eines Graphen. */
#[derive(Clone, Debug, Serialize)]
pub struct GraphSeries {
    pub source: String,
    pub variable: String,
    pub label: String,
    pub color: String,
}

/** Ursprünge (schema://host:port) aller Webseiten-Buttons; das Programmfenster lässt sie in Rahmen laden. */
static WEB_ORIGINS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| format!("{}://{}", scheme.to_lowercase(), host.to_lowercase()))
}

fn remember_web_origins(config: &Config) {
    let list: Vec<String> = config.pages.iter().flat_map(|p| &p.buttons).filter(|b| b.mode == "web").filter_map(|b| origin_of(&b.web.url)).collect();
    *WEB_ORIGINS.lock().unwrap() = list;
}

/** Adresse eines Webseiten-Buttons: „www.beispiel.de“ wird zu „https://www.beispiel.de“, andere Schemata fallen weg. */
fn web_url(raw: &str) -> String {
    let u = raw.trim();
    if u.is_empty() {
        return String::new();
    }
    let u = if u.contains("://") { u.to_string() } else { format!("https://{u}") };
    if u.starts_with("http://") || u.starts_with("https://") { truncate(&u, 1000) } else { String::new() }
}

/** Gehört die Adresse zu einem Webseiten-Button? (gleicher Ursprung wie dessen Adresse) */
pub fn is_web_button_url(url: &str) -> bool {
    origin_of(url).is_some_and(|o| WEB_ORIGINS.lock().unwrap().contains(&o))
}

/** Webseite im Button (Art „web“): Adresse, Zoom in Prozent, bedienbar, Neu laden alle n Sekunden. */
#[derive(Clone, Debug, Serialize)]
pub struct Web {
    pub url: String,
    pub zoom: i64,
    pub interactive: bool,
    pub refresh: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Slider {
    pub target: String,
    pub input: String,
    pub command: String,
    /// Aussehen: fill (Fläche), rail (Schiene mit Griff), knob (Drehregler), dial (Drehregler mit Zeiger).
    pub shape: String,
    /// Farbe des Reglers ("" = aus der Textfarbe) und Dicke von Schiene / Ring (2–30).
    pub color: String,
    pub thickness: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Button {
    pub id: String,
    pub row: i64,
    pub col: i64,
    pub row_span: i64,
    pub col_span: i64,
    pub title: String,
    pub icon: Icon,
    pub color: String,
    pub color_on: String,
    /// Deckkraft des Hintergrunds in Prozent (0 = durchsichtig, nur Symbol und Text).
    pub bg_opacity: i64,
    /// Symbol und Beschriftung, wenn der Button „an“ ist (Umschalter oder Live-Anzeige); leer = wie sonst.
    pub icon_on: Icon,
    pub title_on: String,
    pub text_color: String,
    pub font_size: i64,
    /// Schrift für Beschriftung und Live-Text: "" (normal), serif, mono, condensed, handwriting.
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    /// Umriss um die Schrift in Pixeln (0 = keiner).
    pub text_outline: i64,
    pub title_pos: String,
    pub icon_fit: String,
    /// Größe des Symbols in Prozent der normalen Größe (30–200).
    pub icon_size: i64,
    /// Textur: cover (füllen), contain (ganz zeigen), stretch (strecken), tile (kacheln).
    pub tex_fit: String,
    /// Farbschleier der Buttonfarbe über der Textur in Prozent (0 = Textur unverändert).
    pub tex_veil: i64,
    /// Textur: Hintergrundbild (eigenes Bild oder aus einem Icon-Paket) und Overlay darüber.
    pub bg_image: Icon,
    pub overlay: String,
    pub mode: String,
    pub live: Live,
    pub slider: Slider,
    /// Zweites Ziel für den Drehknopf beim Regler „Fader + Drehknopf“ (combo).
    pub slider2: Slider,
    pub web: Web,
    pub actions: Vec<Action>,
    pub actions_off: Vec<Action>,
    /// Gedrückthalten: Aktionen beim langen Drücken (optional wiederholt) und beim Loslassen danach.
    pub hold_actions: Vec<Action>,
    pub release_actions: Vec<Action>,
    /// Doppelt drücken: eigene Aktionen statt der normalen (der Umschalter wechselt dabei nicht).
    pub double_actions: Vec<Action>,
    pub hold_repeat: i64,
    /// Bei Event: der Button reagiert selbst auf Ereignisse.
    pub events: Vec<ButtonEvent>,
    /// Modus „states“: Zustände mit eigenem Aussehen und eigenen Aktionen (jeder Druck schaltet weiter).
    pub states: Vec<ButtonState>,
    /// Zustand nach einem Wert statt per Druck: Name des Werts (leer = per Druck weiterschalten).
    pub state_by: String,
    /// Modus „switch“: Kippschalter waagerecht oder senkrecht.
    pub switch_style: String,
    /// Sicherheitsabfrage: erst der zweite Druck (innerhalb von 3 s) löst aus.
    pub confirm: bool,
    /// Löst nur aus, wenn ein anderer Button an ist bzw. ein Wert passt.
    pub requires: Requires,
    /// Anfangs versteckt (einblenden mit der Aktion „Button zeigen / verstecken“).
    pub hidden: bool,
    /// Position und Größe auf einer freien Seite (Prozent der Fläche); im Raster ungenutzt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free: Option<FreeRect>,
}

/** Ein Zustand eines Buttons mit mehreren Zuständen; leere Felder übernehmen das Aussehen des Buttons. */
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonState {
    pub title: String,
    pub icon: Icon,
    pub color: String,
    pub text_color: String,
    /// Bei „Zustand nach Wert“: wann dieser Zustand gilt – Text, Zahl, Bereich „10..50“, „>80“ oder „<5“.
    pub when: String,
    pub actions: Vec<Action>,
}

/** Bedingung, ohne die ein Button nicht auslöst (z. B. erst „Scharf schalten“). */
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requires {
    /// Dieser Button muss an sein (Umschalter, Schalter) – leer = keine Bedingung an einen Button.
    pub button: String,
    /// Dieser Wert muss `equals` haben – leer = keine Bedingung an einen Wert.
    pub variable: String,
    pub equals: String,
    /// Nach dem Auslösen den anderen Button wieder ausschalten.
    pub reset: bool,
}

/** Passt ein Wert zu einer Bedingung wie „ja“, „3“, „10..50“, „>80“, „<=5“? */
pub fn value_matches(value: &str, when: &str) -> bool {
    let when = when.trim();
    let num = |t: &str| t.trim().replace(',', ".").parse::<f64>().ok().filter(|n| n.is_finite());
    let v = num(value);
    if let (Some((a, b)), Some(v)) = (when.split_once(".."), v)
        && let (Some(a), Some(b)) = (num(a), num(b))
    {
        return (a..=b).contains(&v);
    }
    for (op, f) in [(">=", (|v: f64, x: f64| v >= x) as fn(f64, f64) -> bool), ("<=", |v, x| v <= x), (">", |v, x| v > x), ("<", |v, x| v < x)] {
        if let Some(rest) = when.strip_prefix(op) {
            return matches!((v, num(rest)), (Some(v), Some(x)) if f(v, x));
        }
    }
    match (v, num(when)) {
        (Some(v), Some(x)) => v == x,
        _ => value.trim().eq_ignore_ascii_case(when),
    }
}

/** Lage eines Buttons auf einer freien Seite, alles in Prozent der Seitenfläche. */
#[derive(Clone, Debug, Serialize)]
pub struct FreeRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub id: String,
    pub name: String,
    pub rows: i64,
    pub cols: i64,
    pub background: String,
    /// Woher die Seite stammt, damit Vorlagen sie wiederfinden statt doppelt anzulegen.
    pub template: String,
    /// Ordner in der Seitenliste des Editors ("" = keiner).
    pub folder: String,
    /// grid: Buttons im Raster; free: Buttons frei auf einer Fläche mit Seitenverhältnis ratio_w:ratio_h.
    pub layout: String,
    pub ratio_w: i64,
    pub ratio_h: i64,
    /// Eigene Spaltenbreiten und Zeilenhöhen im Raster (Anteile, 1 = normal); leer = alle gleich.
    pub col_sizes: Vec<f64>,
    pub row_sizes: Vec<f64>,
    pub buttons: Vec<Button>,
    /// Logik der Seite: Events wie bei Buttons (z. B. „Seite geöffnet“ → Aktionen).
    pub events: Vec<ButtonEvent>,
}

/** Vorlage: eine wiederverwendbare Aktionsliste. */
#[derive(Clone, Debug, Serialize)]
pub struct Flow {
    pub id: String,
    pub name: String,
    /// Ordner in der Liste der Vorlagen ("" = keiner).
    pub folder: String,
    /** Eingaben, die „Vorlage ausführen“ mitgibt; im Ablauf als {Eingabe.Name}. */
    pub inputs: Vec<FlowInput>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FlowInput {
    pub name: String,
    pub default: String,
}

/** Globales Event: Wenn <Auslöser>, dann <Aktionen>; optional mit Abklingzeit. */
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub id: String,
    pub name: String,
    /// Ordner in der Liste der Events ("" = keiner).
    pub folder: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub cooldown: i64,
    pub actions: Vec<Action>,
}

/** Beschreibung eines Werts (der aktuelle Inhalt steht in variables). */
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueMeta {
    pub default: String,
    pub description: String,
    pub folder: String,
    pub reset_on_start: bool,
}

// ---------- Bereinigen (alles, was von außen kommt, läuft hier durch) ----------

fn sanitize_params(raw: &Value) -> Params {
    let mut params = Map::new();
    if let Some(obj) = raw.as_object() {
        for (k, v) in obj {
            let v = match v {
                Value::String(s) => Value::String(truncate(s, MAX_TEXT)),
                Value::Number(_) | Value::Bool(_) => v.clone(),
                _ => continue,
            };
            params.insert(truncate(k, 64), v);
        }
    }
    params
}

fn sanitize_action(a: &Value) -> Option<Action> {
    let kind = a.get("type")?.as_str()?;
    Some(Action {
        id: id_or_new(&a["id"], "a_"),
        kind: truncate(kind, 64),
        enabled: not_false(&a["enabled"]),
        params: sanitize_params(&a["params"]),
    })
}

pub fn action_list(list: &Value) -> Vec<Action> {
    list.as_array().map_or_else(Vec::new, |l| l.iter().take(MAX_ACTIONS).filter_map(sanitize_action).collect())
}

fn sanitize_trigger(t: &Value) -> Trigger {
    Trigger {
        kind: t["type"].as_str().map_or_else(|| "value_change".into(), |s| truncate(s, 64)),
        params: sanitize_params(&t["params"]),
    }
}

fn sanitize_button_event(e: &Value) -> Option<ButtonEvent> {
    e.as_object()?;
    Some(ButtonEvent {
        id: id_or_new(&e["id"], "e_"),
        enabled: not_false(&e["enabled"]),
        trigger: sanitize_trigger(&e["trigger"]),
        actions: action_list(&e["actions"]),
    })
}

fn one_of(v: &Value, allowed: &[&str], fallback: &str) -> String {
    v.as_str().filter(|s| allowed.contains(s)).unwrap_or(fallback).to_string()
}

fn sanitize_icon(icon: &Value) -> Icon {
    match icon["type"].as_str() {
        Some(t @ ("emoji" | "image")) if icon.is_object() => Icon { kind: t.into(), value: text(&icon["value"], "", 256) },
        _ => Icon { kind: "emoji".into(), value: String::new() },
    }
}

/** Zahl auf eine Nachkommastelle runden und begrenzen. */
fn decimal(v: &Value, min: f64, max: f64) -> Option<f64> {
    let n = v.as_f64().filter(|n| n.is_finite())?;
    Some((n.clamp(min, max) * 10.0).round() / 10.0)
}

fn sanitize_free(v: &Value) -> Option<FreeRect> {
    let w = decimal(&v["w"], 2.0, 100.0)?;
    let h = decimal(&v["h"], 2.0, 100.0)?;
    Some(FreeRect { x: decimal(&v["x"], 0.0, 100.0 - w)?, y: decimal(&v["y"], 0.0, 100.0 - h)?, w, h })
}

/** Liste von Größenanteilen: genau `count` Werte zwischen 0,1 und 10, sonst leer (= alle gleich). */
fn sizes(v: &Value, count: i64) -> Vec<f64> {
    let list: Vec<f64> = v.as_array().into_iter().flatten().filter_map(|n| decimal(n, 0.1, 10.0)).collect();
    if list.len() as i64 == count && list.iter().any(|&n| n != 1.0) { list } else { Vec::new() }
}

/** Ein Raster-Feld als freie Lage (mit etwas Abstand), wenn eine Seite von Raster auf frei wechselt. */
fn free_from_cell(row: i64, col: i64, row_span: i64, col_span: i64, rows: i64, cols: i64) -> FreeRect {
    let (cw, ch) = (100.0 / cols as f64, 100.0 / rows as f64);
    let r = |n: f64| (n * 10.0).round() / 10.0;
    FreeRect {
        x: r(col as f64 * cw + cw * 0.05),
        y: r(row as f64 * ch + ch * 0.05),
        w: r((col_span as f64 * cw - cw * 0.1).max(2.0)),
        h: r((row_span as f64 * ch - ch * 0.1).max(2.0)),
    }
}

/** Overlays über Buttons (reines CSS, keine Bilddateien). */
pub const OVERLAYS: [&str; 6] = ["gloss", "frame", "stripes", "dots", "vignette", "glow"];

fn sanitize_button(b: &Value, rows: i64, cols: i64, free_layout: bool) -> Option<Button> {
    b.as_object()?;
    let mut row = parse_int(&b["row"])?;
    let mut col = parse_int(&b["col"])?;
    if free_layout {
        // Auf freien Seiten zählt die freie Lage; Raster-Position nur gültig halten.
        row = row.clamp(0, rows - 1);
        col = col.clamp(0, cols - 1);
    } else if !(0..rows).contains(&row) || !(0..cols).contains(&col) {
        return None;
    }
    let icon = sanitize_icon(&b["icon"]);
    let live = &b["live"];
    let slider = &b["slider"];
    Some(Button {
        id: id_or_new(&b["id"], "b_"),
        row,
        col,
        row_span: clamp(&b["rowSpan"], 1, rows - row, 1),
        col_span: clamp(&b["colSpan"], 1, cols - col, 1),
        title: text(&b["title"], "", 200),
        icon,
        color: color(&b["color"], "#2b2f3a"),
        color_on: color(&b["colorOn"], "#2f9e6b"),
        bg_opacity: clamp(&b["bgOpacity"], 0, 100, 100),
        icon_on: sanitize_icon(&b["iconOn"]),
        title_on: text(&b["titleOn"], "", 200),
        text_color: color(&b["textColor"], "#ffffff"),
        font_size: clamp(&b["fontSize"], 8, 64, 14),
        font_family: one_of(&b["fontFamily"], &["serif", "mono", "condensed", "handwriting"], ""),
        bold: b["bold"] != Value::Bool(false),
        italic: b["italic"] == Value::Bool(true),
        text_outline: clamp(&b["textOutline"], 0, 6, 0),
        title_pos: one_of(&b["titlePos"], &["top", "center", "bottom", "hidden"], "bottom"),
        icon_fit: one_of(&b["iconFit"], &["cover"], "contain"),
        icon_size: clamp(&b["iconSize"], 30, 200, 100),
        tex_fit: one_of(&b["texFit"], &["contain", "stretch", "tile"], "cover"),
        tex_veil: clamp(&b["texVeil"], 0, 100, 35),
        bg_image: sanitize_icon(&b["bgImage"]),
        overlay: one_of(&b["overlay"], &OVERLAYS, ""),
        mode: one_of(&b["mode"], &["toggle", "slider", "web", "display", "states", "switch"], "press"),
        live: Live {
            source: live["source"]
                .as_str()
                .filter(|s| (s.starts_with("plugin:") || s.starts_with("plugin-result:")) && s.len() <= 80 && s.chars().all(|c| c.is_ascii_alphanumeric() || ":-_".contains(c)))
                .map_or_else(|| one_of(&live["source"], &catalog::live_source_ids(), "none"), String::from),
            command: text(&live["command"], "", 1000),
            variable: text(&live["variable"], "", 40),
            bar: live["bar"] == Value::Bool(true),
            graph: one_of(&live["graph"], &["line", "area", "stack", "bars"], ""),
            graph_points: clamp(&live["graphPoints"], 10, 300, 60),
            graph_interval: clamp(&live["graphInterval"], 1, 3600, 2),
            graph_min: live["graphMin"].as_f64().filter(|n| n.is_finite()),
            graph_max: live["graphMax"].as_f64().filter(|n| n.is_finite()),
            graph_color: color(&live["graphColor"], ""),
            graph_align: one_of(&live["graphAlign"], &["center", "right"], "left"),
            series: live["series"]
                .as_array()
                .into_iter()
                .flatten()
                .take(8)
                .map(|x| GraphSeries {
                    source: x["source"]
                        .as_str()
                        .filter(|s| s.starts_with("plugin:") && s.len() <= 80 && s.chars().all(|c| c.is_ascii_alphanumeric() || ":-_".contains(c)))
                        .map_or_else(|| one_of(&x["source"], &catalog::live_source_ids(), "none"), String::from),
                    variable: text(&x["variable"], "", 40),
                    label: text(&x["label"], "", 40),
                    color: color(&x["color"], ""),
                })
                .filter(|x| x.source != "none")
                .collect(),
        },
        slider: Slider {
            target: one_of(&slider["target"], &catalog::slider_target_ids(), "volume"),
            input: text(&slider["input"], "", 200),
            command: text(&slider["command"], "", 1000),
            shape: one_of(&slider["shape"], &["rail", "fader", "knob", "dial", "mixer", "combo"], "fill"),
            color: slider["color"].as_str().filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())).unwrap_or("").to_lowercase(),
            thickness: clamp(&slider["thickness"], 2, 30, 10),
        },
        slider2: {
            let s2 = &b["slider2"];
            Slider {
                target: one_of(&s2["target"], &catalog::slider_target_ids(), "mic"),
                input: text(&s2["input"], "", 200),
                command: text(&s2["command"], "", 1000),
                // Lage des Drehknopfs: mixer = oben/links, mixer-last = unten/rechts.
                shape: one_of(&s2["shape"], &["mixer-last"], "mixer"),
                color: s2["color"].as_str().filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())).unwrap_or("").to_lowercase(),
                thickness: 10,
            }
        },
        web: Web {
            url: web_url(b["web"]["url"].as_str().unwrap_or("")),
            zoom: clamp(&b["web"]["zoom"], 25, 300, 100),
            interactive: b["web"]["interactive"] != Value::Bool(false),
            refresh: clamp(&b["web"]["refresh"], 0, 86_400, 0),
        },
        actions: action_list(&b["actions"]),
        actions_off: action_list(&b["actionsOff"]),
        hold_actions: action_list(&b["holdActions"]),
        release_actions: action_list(&b["releaseActions"]),
        double_actions: action_list(&b["doubleActions"]),
        hold_repeat: clamp(&b["holdRepeat"], 0, 10000, 0),
        events: b["events"]
            .as_array()
            .map_or_else(Vec::new, |l| l.iter().take(20).filter_map(sanitize_button_event).collect()),
        free: sanitize_free(&b["free"]),
        states: b["states"]
            .as_array()
            .into_iter()
            .flatten()
            .take(16)
            .filter(|x| x.is_object())
            .map(|x| ButtonState {
                title: text(&x["title"], "", 200),
                icon: sanitize_icon(&x["icon"]),
                color: color(&x["color"], ""),
                text_color: color(&x["textColor"], ""),
                when: text(&x["when"], "", 60),
                actions: action_list(&x["actions"]),
            })
            .collect(),
        state_by: text(&b["stateBy"], "", 40),
        switch_style: one_of(&b["switchStyle"], &["vertical"], "horizontal"),
        confirm: b["confirm"] == Value::Bool(true),
        requires: Requires {
            button: text(&b["requires"]["button"], "", 40),
            variable: text(&b["requires"]["variable"], "", 40),
            equals: text(&b["requires"]["equals"], "", 200),
            reset: b["requires"]["reset"] == Value::Bool(true),
        },
        hidden: b["hidden"] == Value::Bool(true),
    })
}

pub fn sanitize_page(p: &Value) -> Option<Page> {
    p.as_object()?;
    let rows = clamp(&p["rows"], 1, 12, 3);
    let cols = clamp(&p["cols"], 1, 16, 5);
    let layout = one_of(&p["layout"], &["grid", "free"], "grid");
    let free_layout = layout == "free";
    let mut page = Page {
        id: id_or_new(&p["id"], "p_"),
        name: text_or(&p["name"], t!("store.page"), 100),
        rows,
        cols,
        background: color(&p["background"], "#14161b"),
        template: one_of(&p["template"], &TEMPLATES, ""),
        folder: folder_name(&p["folder"]),
        layout,
        ratio_w: clamp(&p["ratioW"], 1, 32, 16),
        ratio_h: clamp(&p["ratioH"], 1, 32, 10),
        col_sizes: sizes(&p["colSizes"], cols),
        row_sizes: sizes(&p["rowSizes"], rows),
        buttons: Vec::new(),
        events: p["events"].as_array().map_or_else(Vec::new, |l| l.iter().take(20).filter_map(sanitize_button_event).collect()),
    };
    // Raster: überlappende Buttons verwerfen, der erste gewinnt. Frei: Überlappen ist erlaubt.
    let mut taken = std::collections::HashSet::new();
    for raw in p["buttons"].as_array().into_iter().flatten() {
        let Some(mut b) = sanitize_button(raw, rows, cols, free_layout) else { continue };
        if free_layout {
            if b.free.is_none() {
                b.free = Some(free_from_cell(b.row, b.col, b.row_span, b.col_span, rows, cols));
            }
            page.buttons.push(b);
            continue;
        }
        let cells: Vec<(i64, i64)> = (b.row..b.row + b.row_span)
            .flat_map(|r| (b.col..b.col + b.col_span).map(move |c| (r, c)))
            .collect();
        if cells.iter().any(|c| taken.contains(c)) {
            continue;
        }
        taken.extend(cells);
        page.buttons.push(b);
    }
    Some(page)
}

/** Name eines Ordners: höchstens 60 Zeichen, ohne Zeilenumbrüche. */
fn folder_name(v: &Value) -> String {
    v.as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(60).collect::<String>().trim().to_string()
}

/** Eigene Ordner (auch leere) für Seiten, Vorlagen und Events: { pages: [...], flows: [...], events: [...] }. */
pub fn sanitize_folders(v: &Value) -> Value {
    let list = |k: &str| {
        let mut out: Vec<String> = Vec::new();
        for f in v[k].as_array().into_iter().flatten().take(100) {
            let name = folder_name(f);
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
        out
    };
    json!({ "pages": list("pages"), "flows": list("flows"), "events": list("events") })
}

fn sanitize_flow(f: &Value) -> Option<Flow> {
    f.as_object()?;
    let mut inputs: Vec<FlowInput> = Vec::new();
    for i in f["inputs"].as_array().into_iter().flatten().take(10) {
        let name: String = i["name"].as_str().unwrap_or("").chars().filter(|c| !"{}.".contains(*c)).take(40).collect::<String>().trim().to_string();
        if !name.is_empty() && !inputs.iter().any(|x| x.name == name) {
            inputs.push(FlowInput { name, default: truncate(i["default"].as_str().unwrap_or(""), MAX_TEXT) });
        }
    }
    Some(Flow { id: id_or_new(&f["id"], "f_"), name: text_or(&f["name"], t!("store.template"), 100), folder: folder_name(&f["folder"]), inputs, actions: action_list(&f["actions"]) })
}

fn sanitize_event(e: &Value) -> Option<Event> {
    e.as_object()?;
    Some(Event {
        id: id_or_new(&e["id"], "ev_"),
        name: text_or(&e["name"], "Event", 100),
        folder: folder_name(&e["folder"]),
        enabled: not_false(&e["enabled"]),
        trigger: sanitize_trigger(&e["trigger"]),
        cooldown: clamp(&e["cooldown"], 0, 86400, 0),
        actions: action_list(&e["actions"]),
    })
}

fn sanitize_values(values: &Value) -> IndexMap<String, ValueMeta> {
    let mut out = IndexMap::new();
    for (name, v) in values.as_object().into_iter().flatten() {
        if valid_variable_name(name) {
            out.insert(
                name.clone(),
                ValueMeta {
                    default: text(&v["default"], "", 1000),
                    description: text(&v["description"], "", 300),
                    folder: text(&v["folder"], "", 60),
                    reset_on_start: v["resetOnStart"] == Value::Bool(true),
                },
            );
        }
    }
    out
}

fn list<T>(v: &Value, f: impl Fn(&Value) -> Option<T>) -> Vec<T> {
    v.as_array().map_or_else(Vec::new, |l| l.iter().filter_map(f).collect())
}

// ---------- Einstellungen ----------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSettings {
    pub press_animation: String,
    pub vibrate: bool,
    pub sound: bool,
    pub show_labels: bool,
    /// Ausrichtung auf Handy und Tablet: auto, portrait oder landscape.
    #[serde(default = "default_orientation")]
    pub orientation: String,
    /// Form der Felder: fill (dürfen sich bis 1,6-fach dehnen, nutzen die Fläche) oder square.
    #[serde(default = "default_cell_shape")]
    pub cell_shape: String,
    /// Fußleiste im Bedienfeld: Verbindung und (in der App) PC wechseln.
    #[serde(default)]
    pub footer: bool,
    /// Kurzes Häkchen am Button, wenn die Aktionen geklappt haben.
    #[serde(default = "yes")]
    pub feedback: bool,
    /// Ruhezustand nach so vielen Minuten ohne Berührung (0 = nie).
    #[serde(default)]
    pub sleep_after: i64,
    /// Ruhezustand: dim (abdunkeln), clock (Uhr) oder black (schwarz).
    #[serde(default = "default_sleep_mode")]
    pub sleep_mode: String,
    /// PC gesperrt → Bedienfeld gesperrt, bis der PC entsperrt ist.
    #[serde(default)]
    pub lock_with_pc: bool,
}

fn default_sleep_mode() -> String {
    "dim".into()
}

fn yes() -> bool {
    true
}

fn default_orientation() -> String {
    "auto".into()
}

fn default_cell_shape() -> String {
    "fill".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObsSettings {
    pub host: String,
    pub port: u16,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchSettings {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Value,
    pub user_id: String,
    pub login: String,
    pub display_name: String,
    pub last_error: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordSettings {
    pub client_id: String,
    pub client_secret: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Value,
    pub user: String,
    pub last_error: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub mode: String,
    pub accent: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupSettings {
    pub auto: bool,
    pub keep: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogSettings {
    pub level: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub token: String,
    pub start_page: String,
    pub lan_enabled: bool,
    pub pin: String,
    pub deck: DeckSettings,
    pub obs: ObsSettings,
    pub twitch: TwitchSettings,
    pub discord: DiscordSettings,
    pub theme: ThemeSettings,
    pub backups: BackupSettings,
    pub logs: LogSettings,
    /// Sprache der Oberfläche: de oder en (Standard: Systemsprache).
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firewall_opened: Option<bool>,
    /// Port des Servers (gilt ab dem nächsten Start; --port bzw. TOUCHDECK_PORT haben Vorrang).
    #[serde(default = "default_port")]
    pub port: u16,
    /// Adresse für Links und QR-Codes ("" = alle Adressen im Heimnetz), z. B. 192.168.1.20 oder pc.local.
    #[serde(default)]
    pub public_host: String,
    /// USB-Verbindung über „adb reverse“ für angeschlossene Android-Geräte.
    #[serde(default)]
    pub usb: bool,
    /// Unbekannte Einträge bleiben erhalten.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/** Sprache des Systems (LANGUAGE, LC_ALL, LC_MESSAGES, LANG): Deutsch bei de_…, sonst Englisch. */
pub fn system_language() -> String {
    let env = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty() && v != "C" && v != "POSIX"));
    if env.is_some_and(|v| v.to_lowercase().starts_with("de")) { "de".into() } else { "en".into() }
}

fn default_port() -> u16 {
    8770
}

fn default_language() -> String {
    system_language()
}

pub const LANGUAGES: [&str; 2] = ["de", "en"];

const SETTING_GROUPS: [&str; 7] = ["deck", "obs", "twitch", "discord", "theme", "backups", "logs"];

fn default_settings(start_page: &str) -> Settings {
    Settings {
        token: random_token(),
        start_page: start_page.into(),
        lan_enabled: true,
        port: default_port(),
        public_host: String::new(),
        usb: false,
        pin: String::new(),
        deck: DeckSettings { press_animation: "shrink".into(), vibrate: true, sound: false, show_labels: true, orientation: default_orientation(), cell_shape: default_cell_shape(), footer: false, feedback: true, sleep_after: 0, sleep_mode: default_sleep_mode(), lock_with_pc: false },
        obs: ObsSettings { host: "127.0.0.1".into(), port: 4455, password: String::new() },
        twitch: TwitchSettings {
            client_id: String::new(),
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: json!(0),
            user_id: String::new(),
            login: String::new(),
            display_name: String::new(),
            last_error: Value::Null,
        },
        discord: DiscordSettings {
            client_id: String::new(),
            client_secret: String::new(),
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: json!(0),
            user: String::new(),
            last_error: Value::Null,
        },
        theme: ThemeSettings { mode: "dark".into(), accent: "#7c8cff".into() },
        backups: BackupSettings { auto: true, keep: 10 },
        logs: LogSettings { level: "info".into() },
        language: default_language(),
        firewall_opened: None,
        extra: Map::new(),
    }
}

/** Gespeicherte Einstellungen über die Standardwerte legen (fehlende Einträge ergänzen). */
fn merge_settings(saved: &Value, start_page: &str) -> Settings {
    let defaults = default_settings(start_page);
    let mut merged = serde_json::to_value(&defaults).expect("Einstellungen lassen sich immer umwandeln");
    if let (Some(m), Some(saved)) = (merged.as_object_mut(), saved.as_object()) {
        for (k, v) in saved {
            match (SETTING_GROUPS.contains(&k.as_str()), m.get_mut(k), v) {
                (true, Some(Value::Object(group)), Value::Object(vals)) => {
                    for (gk, gv) in vals {
                        group.insert(gk.clone(), gv.clone());
                    }
                }
                (true, _, _) => {}
                _ => {
                    m.insert(k.clone(), v.clone());
                }
            }
        }
    }
    match serde_json::from_value::<Settings>(merged) {
        Ok(mut s) => {
            if s.token.is_empty() {
                s.token = defaults.token;
            }
            s
        }
        Err(err) => {
            eprintln!("{}", t!("store.settingsDamagedUsingDefaults", err));
            let mut s = defaults;
            // Den Kopplungsschlüssel möglichst behalten, damit Handys verbunden bleiben.
            if let Some(t) = saved["token"].as_str().filter(|t| !t.is_empty()) {
                s.token = t.into();
            }
            s
        }
    }
}

// ---------- Gesamte Konfiguration ----------

#[derive(Clone, Debug, Serialize, Default)]
pub struct Overrides {
    pub buttons: Map<String, Value>,
    pub pages: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub version: Value,
    pub settings: Settings,
    pub pages: Vec<Page>,
    pub flows: Vec<Flow>,
    pub events: Vec<Event>,
    pub values: IndexMap<String, ValueMeta>,
    pub toggles: Map<String, Value>,
    pub variables: Map<String, Value>,
    pub overrides: Overrides,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

const KNOWN_KEYS: [&str; 9] = ["version", "settings", "pages", "flows", "events", "values", "toggles", "variables", "overrides"];

fn sample_pages() -> Vec<Page> {
    let btn = |row, col, title: &str, emoji: &str, color: &str, kind: &str, params: Value| {
        json!({ "row": row, "col": col, "title": title, "icon": { "type": "emoji", "value": emoji }, "color": color,
                "actions": [{ "type": kind, "enabled": true, "params": params }] })
    };
    // Stumm-Button: zeigt den Zustand über zwei Events am Button (sichtbar unter „Bei Event“).
    let mut mute = btn(1, 2, t!("store.soundOn"), "🔊", "#2b2f3a", "volume", json!({ "op": "mute" }));
    mute["events"] = json!([
        { "enabled": true, "trigger": { "type": "audio_mute", "params": { "target": "output", "state": "muted" } },
          "actions": [{ "type": "change_visuals", "enabled": true, "params": { "target": "self", "icon": "🔇", "title": t!("common.muted"), "color": "#8f3b3b" } }] },
        { "enabled": true, "trigger": { "type": "audio_mute", "params": { "target": "output", "state": "unmuted" } },
          "actions": [{ "type": "restore_visuals", "enabled": true, "params": { "target": "self" } }] },
    ]);
    let page = json!({
        "name": "Start", "rows": 3, "cols": 5,
        "buttons": [
            btn(0, 0, t!("store.play"), "⏯️", "#3b3f8f", "media", json!({ "op": "play-pause" })),
            btn(0, 1, t!("common.back"), "⏮️", "#2b2f3a", "media", json!({ "op": "previous" })),
            btn(0, 2, t!("common.next"), "⏭️", "#2b2f3a", "media", json!({ "op": "next" })),
            btn(1, 0, t!("store.quieter"), "🔉", "#2b2f3a", "volume", json!({ "op": "down", "step": 5 })),
            btn(1, 1, t!("store.louder"), "🔊", "#2b2f3a", "volume", json!({ "op": "up", "step": 5 })),
            mute,
            btn(2, 0, "Terminal", "🖥️", "#2b2f3a", "hotkey", json!({ "keys": "ctrl+alt+t" })),
            btn(2, 1, t!("store.files"), "📁", "#2b2f3a", "open_path", json!({ "path": "~" })),
        ],
    });
    vec![sanitize_page(&page).expect("Beispielseite ist gültig")]
}

impl Config {
    fn sample() -> Self {
        let pages = sample_pages();
        Config {
            version: json!(1),
            settings: default_settings(&pages[0].id),
            pages,
            flows: Vec::new(),
            events: Vec::new(),
            values: IndexMap::new(),
            toggles: Map::new(),
            variables: Map::new(),
            overrides: Overrides::default(),
            extra: Map::new(),
        }
    }

    /** Liest eine Konfiguration (Datei oder Sicherung) und bringt sie in Ordnung. */
    pub fn from_value(cfg: &Value) -> Result<Self> {
        let Some(obj) = cfg.as_object() else { bail!(t!("store.configurationIsNotJson")) };
        let mut pages = list(&cfg["pages"], sanitize_page);
        if pages.is_empty() {
            pages = sample_pages();
        }
        let values = sanitize_values(&cfg["values"]);
        let mut variables = cfg["variables"].as_object().cloned().unwrap_or_default();
        let mut settings = merge_settings(&cfg["settings"], &pages[0].id);
        if !pages.iter().any(|p| p.id == settings.start_page) {
            settings.start_page = pages[0].id.clone();
        }
        // Werte mit „beim Start zurücksetzen“ und noch nie gesetzte Werte bekommen ihren Startwert.
        for (name, meta) in &values {
            if meta.reset_on_start || !variables.contains_key(name) {
                variables.insert(name.clone(), Value::String(meta.default.clone()));
            }
        }
        let ov = &cfg["overrides"];
        Ok(Config {
            version: if cfg["version"].is_null() { json!(1) } else { cfg["version"].clone() },
            settings,
            pages,
            flows: list(&cfg["flows"], sanitize_flow),
            events: list(&cfg["events"], sanitize_event),
            values,
            toggles: cfg["toggles"].as_object().cloned().unwrap_or_default(),
            variables,
            overrides: Overrides {
                buttons: ov["buttons"].as_object().cloned().unwrap_or_default(),
                pages: ov["pages"].as_object().cloned().unwrap_or_default(),
            },
            extra: obj.iter().filter(|(k, _)| !KNOWN_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect(),
        })
    }
}

// ---------- Speicher ----------

/** Was sich geändert hat: pages, settings, security, variables, toggles, overrides. */
#[derive(Clone, Debug)]
pub struct Change {
    pub what: &'static str,
    pub detail: Value,
}

pub type ChangeHook = Box<dyn Fn(&Change) + Send + Sync>;

pub struct Store {
    pub data_dir: PathBuf,
    pub icon_dir: PathBuf,
    file: PathBuf,
    pub config: Config,
    /// Zählt Änderungen am Projekt, damit ein veralteter Editor-Tab nichts überschreibt.
    pub revision: u64,
    changes: broadcast::Sender<Change>,
    /// Wird bei jeder Änderung sofort aufgerufen (z. B. für Events „Wert ändert sich“). Darf den Speicher nicht sperren.
    hook: std::sync::OnceLock<ChangeHook>,
    /// Werte, Umschalter und optische Änderungen werden gesammelt gespeichert (siehe `flush`).
    dirty: bool,
    save_soon: Arc<tokio::sync::Notify>,
}

fn write_json(file: &Path, value: &impl Serialize) -> Result<()> {
    let tmp = file.with_extension("json.tmp");
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    // Kompakt: halb so groß wie formatiert, schneller geschrieben (Export und Sicherungen bleiben lesbar).
    f.write_all(&serde_json::to_vec(value)?)?;
    f.sync_all()?;
    fs::rename(&tmp, file)?;
    Ok(())
}

impl Store {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let icon_dir = data_dir.join("icons");
        fs::create_dir_all(&icon_dir).with_context(|| format!("Datenordner {} anlegen", data_dir.display()))?;
        let file = data_dir.join("config.json");
        let config = Self::load(&file)?;
        super::i18n::set_lang(&config.settings.language);
        remember_web_origins(&config);
        Ok(Store {
            data_dir: data_dir.to_path_buf(),
            icon_dir,
            file,
            config,
            revision: 1,
            changes: broadcast::channel(256).0,
            hook: std::sync::OnceLock::new(),
            dirty: false,
            save_soon: Arc::new(tokio::sync::Notify::new()),
        })
    }

    fn load(file: &Path) -> Result<Config> {
        if !file.exists() {
            let cfg = Config::sample();
            write_json(file, &cfg)?;
            return Ok(cfg);
        }
        let raw = fs::read_to_string(file).with_context(|| format!("{} lesen", file.display()))?;
        let value: Value = serde_json::from_str(&raw).with_context(|| t!("store.isNotValidJson", file.display()))?;
        Config::from_value(&value)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.changes.subscribe()
    }

    /** Sofortige Benachrichtigung bei Änderungen (einmalig setzbar). */
    pub fn set_hook(&self, f: ChangeHook) {
        let _ = self.hook.set(f);
    }

    /** Signal für den Speicher-Task: es gibt gesammelte Änderungen (Werte, Umschalter, Aussehen). */
    pub fn save_signal(&self) -> Arc<tokio::sync::Notify> {
        self.save_soon.clone()
    }

    /** Gesammelte Änderungen jetzt schreiben (Speicher-Task, Beenden, Neustart). */
    pub fn flush(&mut self) {
        if self.dirty {
            self.save_now();
        }
    }

    fn save_now(&mut self) {
        self.dirty = false;
        if let Err(err) = write_json(&self.file, &self.config) {
            eprintln!("{}", t!("store.saveFailed", format!("{err:#}")));
        }
    }

    fn changed(&mut self, what: &'static str, detail: Value) {
        // Laufzeit-Zustand ändert sich oft (Plugins, Zähler): nicht jedes Mal die ganze Datei schreiben.
        if matches!(what, "variables" | "toggles" | "overrides") {
            self.dirty = true;
            self.save_soon.notify_one();
        } else {
            self.save_now();
        }
        remember_web_origins(&self.config);
        let change = Change { what, detail };
        if let Some(hook) = self.hook.get() {
            hook(&change);
        }
        let _ = self.changes.send(change);
    }

    pub fn settings(&self) -> &Settings {
        &self.config.settings
    }

    pub fn page(&self, id: &str) -> Option<&Page> {
        self.config.pages.iter().find(|p| p.id == id)
    }

    pub fn find_button(&self, id: &str) -> Option<(&Page, &Button)> {
        self.config.pages.iter().find_map(|p| p.buttons.iter().find(|b| b.id == id).map(|b| (p, b)))
    }

    /** Speichert alles, was der Editor bearbeitet: Seiten, Vorlagen, Events, Werte. */
    pub fn set_project(&mut self, body: &Value) -> Result<()> {
        let pages = match body["pages"].as_array() {
            Some(l) => l.iter().filter_map(sanitize_page).collect(),
            None => self.config.pages.clone(),
        };
        if pages.is_empty() {
            bail!(t!("store.atLeastOnePage"));
        }
        let cfg = &mut self.config;
        cfg.pages = pages;
        if body["flows"].is_array() {
            cfg.flows = list(&body["flows"], sanitize_flow);
        }
        if body["events"].is_array() {
            cfg.events = list(&body["events"], sanitize_event);
        }
        if body["folders"].is_object() {
            cfg.extra.insert("folders".into(), sanitize_folders(&body["folders"]));
        }
        if body["values"].is_object() {
            cfg.values = sanitize_values(&body["values"]);
            for (name, meta) in &cfg.values {
                if !cfg.variables.contains_key(name) {
                    cfg.variables.insert(name.clone(), Value::String(meta.default.clone()));
                }
            }
        }
        self.revision += 1;
        if !cfg.pages.iter().any(|p| p.id == cfg.settings.start_page) {
            cfg.settings.start_page = cfg.pages[0].id.clone();
        }
        let ids: std::collections::HashSet<&str> = cfg.pages.iter().flat_map(|p| p.buttons.iter().map(|b| b.id.as_str())).collect();
        cfg.toggles.retain(|id, _| ids.contains(id.as_str()));
        cfg.overrides.buttons.retain(|id, _| ids.contains(id.as_str()));
        self.changed("pages", Value::Null);
        Ok(())
    }

    pub fn set_pages(&mut self, pages: Vec<Page>) -> Result<()> {
        let pages = serde_json::to_value(pages)?;
        self.set_project(&json!({ "pages": pages }))
    }

    /** Ersetzt alle Seiten einer Vorlage an der Stelle der ersten; gibt es noch keine, kommen sie ans Ende. */
    pub fn replace_template_pages(&mut self, template: &str, pages: Vec<Page>) -> Result<()> {
        let mut result = Vec::new();
        let mut new_pages = Some(pages);
        for p in &self.config.pages {
            if p.template != template {
                result.push(p.clone());
            } else if let Some(n) = new_pages.take() {
                result.extend(n);
            }
        }
        if let Some(n) = new_pages {
            result.extend(n);
        }
        self.set_pages(result)
    }

    pub fn set_settings(&mut self, patch: &Value) {
        let page_exists = |id: &str| self.config.pages.iter().any(|p| p.id == id);
        let start = patch["startPage"].as_str().filter(|id| page_exists(id)).map(String::from);
        let s = &mut self.config.settings;
        if let Some(id) = start {
            s.start_page = id;
        }
        if let Some(b) = patch["lanEnabled"].as_bool() {
            s.lan_enabled = b;
        }
        if let Some(pin) = patch["pin"].as_str().filter(|p| p.len() <= 8 && p.chars().all(|c| c.is_ascii_digit())) {
            s.pin = pin.into();
        }
        if let Some(b) = patch["firewallOpened"].as_bool() {
            s.firewall_opened = Some(b);
        }
        if let Some(p) = patch["port"].as_u64().filter(|p| (1024..65536).contains(p)) {
            s.port = p as u16;
        }
        if let Some(b) = patch["usb"].as_bool() {
            s.usb = b;
        }
        if let Some(h) = patch["publicHost"].as_str() {
            let h = h.trim();
            if h.is_empty() || ((1..=253).contains(&h.len()) && h.chars().all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))) {
                s.public_host = h.to_string();
            }
        }
        let d = &patch["deck"];
        if d.is_object() {
            if let Some(a) = d["pressAnimation"].as_str().filter(|a| ["none", "shrink", "squeeze", "wobble", "glow"].contains(a)) {
                s.deck.press_animation = a.into();
            }
            if let Some(b) = d["vibrate"].as_bool() {
                s.deck.vibrate = b;
            }
            if let Some(b) = d["sound"].as_bool() {
                s.deck.sound = b;
            }
            if let Some(b) = d["showLabels"].as_bool() {
                s.deck.show_labels = b;
            }
            if let Some(o) = d["orientation"].as_str().filter(|o| ["auto", "portrait", "landscape"].contains(o)) {
                s.deck.orientation = o.into();
            }
            if let Some(c) = d["cellShape"].as_str().filter(|c| ["fill", "square"].contains(c)) {
                s.deck.cell_shape = c.into();
            }
            if let Some(f) = d["footer"].as_bool() {
                s.deck.footer = f;
            }
            if let Some(f) = d["feedback"].as_bool() {
                s.deck.feedback = f;
            }
            if let Some(m) = d["sleepAfter"].as_i64() {
                s.deck.sleep_after = m.clamp(0, 240);
            }
            if let Some(m) = d["sleepMode"].as_str().filter(|m| ["dim", "clock", "black"].contains(m)) {
                s.deck.sleep_mode = m.into();
            }
            if let Some(b) = d["lockWithPc"].as_bool() {
                s.deck.lock_with_pc = b;
            }
        }
        let o = &patch["obs"];
        if o.is_object() {
            if let Some(h) = o["host"].as_str().filter(|h| {
                (1..=253).contains(&h.len()) && h.chars().all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
            }) {
                s.obs.host = h.into();
            }
            if let Some(p) = o["port"].as_u64().filter(|p| (1..65536).contains(p)) {
                s.obs.port = p as u16;
            }
            if let Some(p) = o["password"].as_str() {
                s.obs.password = truncate(p, 200);
            }
        }
        if let Some(id) = patch["twitch"]["clientId"].as_str() {
            let id = truncate(id.trim(), 64);
            // Neue Client-ID: alte Anmeldung gilt nicht mehr.
            if id != s.twitch.client_id {
                let t = &mut s.twitch;
                t.client_id = id;
                for f in [&mut t.access_token, &mut t.refresh_token, &mut t.user_id, &mut t.login, &mut t.display_name] {
                    f.clear();
                }
                t.last_error = Value::Null;
            }
        }
        let dp = &patch["discord"];
        if dp.is_object() {
            let d = &mut s.discord;
            let id = dp["clientId"].as_str().map_or_else(|| d.client_id.clone(), |v| truncate(v.trim(), 32));
            let secret = dp["clientSecret"].as_str().filter(|v| !v.is_empty()).map_or_else(|| d.client_secret.clone(), |v| truncate(v.trim(), 64));
            // Andere Anwendung: alte Anmeldung gilt nicht mehr.
            if id != d.client_id {
                d.access_token.clear();
                d.refresh_token.clear();
                d.user.clear();
                d.last_error = Value::Null;
            }
            d.client_id = id;
            d.client_secret = secret;
        }
        let t = &patch["theme"];
        if t.is_object() {
            if let Some(m) = t["mode"].as_str().filter(|m| ["dark", "light", "system"].contains(m)) {
                s.theme.mode = m.into();
            }
            s.theme.accent = color(&t["accent"], &s.theme.accent);
        }
        let b = &patch["backups"];
        if b.is_object() {
            if let Some(a) = b["auto"].as_bool() {
                s.backups.auto = a;
            }
            s.backups.keep = clamp(&b["keep"], 1, 100, s.backups.keep);
        }
        if let Some(l) = patch["language"].as_str().filter(|l| LANGUAGES.contains(l)) {
            s.language = l.into();
            super::i18n::set_lang(l);
        }
        if let Some(l) = patch["logs"]["level"].as_str().filter(|l| ["debug", "info", "warn", "error"].contains(l)) {
            s.logs.level = l.into();
        }
        let regenerate = patch["regenerateToken"] == Value::Bool(true);
        if regenerate {
            s.token = random_token();
        }
        let security = regenerate || patch["lanEnabled"] == Value::Bool(false) || patch["pin"].is_string();
        self.changed(if security { "security" } else { "settings" }, Value::Null);
    }

    /** Twitch-Anmeldung speichern (nur der Server ruft das auf; Tokens gehen nie an Editor oder Handy). */
    pub fn set_twitch_auth(&mut self, f: impl FnOnce(&mut TwitchSettings)) {
        f(&mut self.config.settings.twitch);
        self.changed("settings", Value::Null);
    }

    pub fn set_discord_auth(&mut self, f: impl FnOnce(&mut DiscordSettings)) {
        f(&mut self.config.settings.discord);
        self.changed("settings", Value::Null);
    }

    /** Einstellungen ohne Geheimnisse für den Editor. */
    pub fn public_settings(&self) -> Value {
        let s = &self.config.settings;
        let mut v = serde_json::to_value(s).expect("Einstellungen lassen sich immer umwandeln");
        if let Some(t) = v["twitch"].as_object_mut() {
            t.shift_remove("accessToken");
            t.shift_remove("refreshToken");
            t.insert("connected".into(), json!(!s.twitch.access_token.is_empty()));
        }
        if let Some(d) = v["discord"].as_object_mut() {
            d.shift_remove("accessToken");
            d.shift_remove("refreshToken");
            d.shift_remove("clientSecret");
            d.insert("hasSecret".into(), json!(!s.discord.client_secret.is_empty()));
            d.insert("connected".into(), json!(!s.discord.access_token.is_empty()));
        }
        v
    }

    pub fn set_variable(&mut self, name: &str, value: &Value) -> Result<()> {
        if !valid_variable_name(name) {
            bail!("Variablenname: 1–40 Zeichen, Buchstaben, Ziffern, Leerzeichen, _ . -");
        }
        let v = truncate(&js_string(value), 1000);
        let old = self.config.variables.get(name).cloned();
        if old.as_ref().and_then(Value::as_str) == Some(v.as_str()) {
            return Ok(());
        }
        self.config.variables.insert(name.into(), Value::String(v.clone()));
        self.changed("variables", json!({ "name": name, "old": old, "value": v }));
        Ok(())
    }

    pub fn delete_variable(&mut self, name: &str) {
        let Some(old) = self.config.variables.shift_remove(name) else { return };
        self.changed("variables", json!({ "name": name, "old": old, "value": null }));
    }

    pub fn set_toggle(&mut self, button_id: &str, on: bool) {
        self.config.toggles.insert(button_id.into(), Value::Bool(on));
        self.changed("toggles", Value::Null);
    }

    /** Zustand eines Buttons mit mehreren Zuständen (0 = erster). */
    pub fn set_state(&mut self, button_id: &str, index: usize) {
        self.config.toggles.insert(button_id.into(), json!(index));
        self.changed("toggles", Value::Null);
    }

    /** Aktueller Zustand: per Wert (stateBy) oder gespeichert; immer gültig für die Zahl der Zustände. */
    pub fn state_index(&self, b: &Button) -> usize {
        if b.states.is_empty() {
            return 0;
        }
        if !b.state_by.is_empty() {
            let v = self.config.variables.get(&b.state_by).map(js_string).unwrap_or_default();
            return b.states.iter().position(|s| value_matches(&v, &s.when)).unwrap_or(0);
        }
        self.config.toggles.get(&b.id).and_then(Value::as_u64).map_or(0, |i| i as usize % b.states.len())
    }

    /** Wird ein Auslöser dieser Art irgendwo benutzt (Events, Seiten-Logik, Button-Events)? */
    pub fn uses_trigger(&self, kind: &str) -> bool {
        let c = &self.config;
        c.events.iter().any(|e| e.enabled && e.trigger.kind == kind)
            || c.pages.iter().any(|p| p.events.iter().chain(p.buttons.iter().flat_map(|b| &b.events)).any(|e| e.enabled && e.trigger.kind == kind))
    }

    /** Ist die Bedingung des Buttons erfüllt (anderer Button an, Wert passt)? */
    pub fn requirement_met(&self, b: &Button) -> bool {
        let r = &b.requires;
        let button_ok = r.button.is_empty() || self.config.toggles.get(&r.button) == Some(&Value::Bool(true));
        let value_ok = r.variable.is_empty() || value_matches(&self.config.variables.get(&r.variable).map(js_string).unwrap_or_default(), &r.equals);
        button_ok && value_ok
    }

    /** Optische Änderung zur Laufzeit (Aktion „Visuelles“); None stellt das Original wieder her. */
    pub fn set_override(&mut self, page: bool, id: &str, patch: Option<&Map<String, Value>>) {
        let map = if page { &mut self.config.overrides.pages } else { &mut self.config.overrides.buttons };
        match patch {
            None => {
                map.shift_remove(id);
            }
            Some(patch) => {
                let entry = map.entry(id).or_insert_with(|| json!({}));
                if let Some(e) = entry.as_object_mut() {
                    for (k, v) in patch {
                        e.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        self.changed("overrides", Value::Null);
    }

    pub fn reset_overrides(&mut self) {
        self.config.overrides = Overrides::default();
        self.changed("overrides", Value::Null);
    }

    /** Ersetzt die komplette Konfiguration (Backup wiederherstellen). */
    pub fn restore(&mut self, cfg: &Value) -> Result<()> {
        self.config = Config::from_value(cfg)?;
        self.revision += 1;
        self.changed("pages", Value::Null);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freie_seite_und_eigene_groessen() {
        let p = sanitize_page(&json!({ "layout": "free", "rows": 2, "cols": 2, "buttons": [
            { "row": 0, "col": 1 },
            { "row": 0, "col": 1, "free": { "x": 95, "y": 0, "w": 10, "h": 10 } },
        ] }))
        .unwrap();
        assert_eq!(p.buttons.len(), 2, "überlappen ist auf freien Seiten erlaubt");
        assert_eq!(p.buttons[1].free.as_ref().unwrap().x, 90.0, "bleibt innerhalb der Fläche");
        let derived = p.buttons[0].free.as_ref().unwrap();
        assert_eq!((derived.x, derived.w), (52.5, 45.0), "aus dem Rasterfeld abgeleitet");
        let g = sanitize_page(&json!({ "rows": 2, "cols": 2, "colSizes": [1, 2], "rowSizes": [1, 1] })).unwrap();
        assert_eq!(g.col_sizes, vec![1.0, 2.0]);
        assert!(g.row_sizes.is_empty(), "alle gleich = leer");
        assert!(sanitize_page(&json!({ "rows": 2, "cols": 2, "colSizes": [1] })).unwrap().col_sizes.is_empty(), "falsche Anzahl");
    }

    #[test]
    fn parse_int_wie_javascript() {
        assert_eq!(parse_int(&json!("12px")), Some(12));
        assert_eq!(parse_int(&json!(3.9)), Some(3));
        assert_eq!(parse_int(&json!(" -4")), Some(-4));
        assert_eq!(parse_int(&json!("x")), None);
        assert_eq!(parse_int(&json!(true)), None);
    }

    #[test]
    fn seite_wird_bereinigt() {
        let p = sanitize_page(&json!({
            "name": "", "rows": "40", "cols": 2, "background": "rot",
            "buttons": [
                { "row": 0, "col": 0, "colSpan": 5, "actions": [{ "type": "media", "params": { "op": "next", "x": [1] } }, { "kein": "typ" }] },
                { "row": 0, "col": 1 },
                { "row": 9, "col": 9 }
            ]
        }))
        .unwrap();
        assert_eq!((p.name.as_str(), p.rows, p.cols, p.background.as_str()), ("Seite", 12, 2, "#14161b"));
        assert_eq!(p.buttons.len(), 1, "überlappender und außerhalb liegender Button fallen weg");
        let b = &p.buttons[0];
        assert_eq!(b.col_span, 2);
        assert_eq!(b.actions.len(), 1);
        assert_eq!(b.actions[0].params.len(), 1);
    }

    #[test]
    fn variablennamen() {
        assert!(valid_variable_name("Zähler 1"));
        assert!(!valid_variable_name(""));
        assert!(!valid_variable_name("a/b"));
    }

    #[test]
    fn einstellungen_werden_ergaenzt() {
        let s = merge_settings(&json!({ "token": "abc", "obs": { "port": 4456 }, "eigenes": 1 }), "p1");
        assert_eq!(s.token, "abc");
        assert_eq!(s.obs.port, 4456);
        assert_eq!(s.obs.host, "127.0.0.1");
        assert_eq!(s.extra["eigenes"], json!(1));
    }

    #[test]
    fn wert_bedingungen() {
        assert!(value_matches("42", "10..50"));
        assert!(!value_matches("60", "10..50"));
        assert!(value_matches("85,5", ">80"));
        assert!(value_matches("5", "<=5"));
        assert!(value_matches("3", "3.0"));
        assert!(value_matches("An", "an"));
        assert!(!value_matches("aus", ">1"));
    }

    #[test]
    fn zustaende_und_abhaengigkeit() {
        let dir = std::env::temp_dir().join(format!("touchdeck-test-{}", new_id("")));
        let mut store = Store::open(&dir).unwrap();
        let b = sanitize_button(
            &json!({ "id": "b_x", "row": 0, "col": 0, "mode": "states", "states": [{ "title": "A" }, { "title": "B" }, { "title": "C", "when": ">50" }],
                     "requires": { "button": "b_arm", "reset": true } }),
            3, 5, false,
        )
        .unwrap();
        assert_eq!(b.states.len(), 3);
        assert_eq!(store.state_index(&b), 0);
        store.set_state("b_x", 4);
        assert_eq!(store.state_index(&b), 1, "Index läuft um");
        // Nach Wert: erster passender Zustand.
        let mut by = b.clone();
        by.state_by = "Last".into();
        store.set_variable("Last", &json!(70)).unwrap();
        assert_eq!(store.state_index(&by), 2);
        // Abhängigkeit: erst wenn b_arm an ist.
        assert!(!store.requirement_met(&b));
        store.set_toggle("b_arm", true);
        assert!(store.requirement_met(&b));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn speichern_und_laden() {
        let dir = std::env::temp_dir().join(format!("touchdeck-test-{}", new_id("")));
        let mut store = Store::open(&dir).unwrap();
        store.set_variable("Zähler", &json!(5)).unwrap();
        store.set_settings(&json!({ "pin": "1234" }));
        // Werte allein werden gesammelt geschrieben.
        store.set_variable("Später", &json!(1)).unwrap();
        assert!(Store::open(&dir).unwrap().config.variables.get("Später").is_none());
        store.flush();
        let again = Store::open(&dir).unwrap();
        assert_eq!(again.config.variables["Zähler"], json!("5"));
        assert_eq!(again.settings().pin, "1234");
        assert!(again.public_settings()["twitch"].get("accessToken").is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
