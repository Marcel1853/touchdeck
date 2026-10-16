// Erzeugt Seiten mit allen installierten Steam-Spielen: Coverbild, Start-Aktion und Events,
// die den Button grün färben, solange das Spiel läuft (sichtbar unter „Bei Event“).
// Cover kommen aus dem Steam-Cache am PC; fehlt dort eins, wird es einmal von Steams Bildserver geladen.
use crate::t;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::steam;
use crate::core::store::{Page, new_id, sanitize_page};
use crate::server::http::HttpError;

const COLS: usize = 6;
const MAX_ROWS: usize = 4;

fn action(kind: &str, params: Value) -> Value {
    json!({ "id": new_id("a_"), "type": kind, "enabled": true, "params": params })
}

fn on_event(kind: &str, params: Value, actions: Vec<Value>) -> Value {
    json!({ "id": new_id("e_"), "enabled": true, "trigger": { "type": kind, "params": params }, "actions": actions })
}

/** Bildserver von Steam, wichtigstes Format zuerst (hochkant wie in der Bibliothek, sonst das Banner). */
const COVER_URLS: [&str; 3] = [
    "https://shared.steamstatic.com/store_item_assets/steam/apps/{id}/library_600x900.jpg",
    "https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/library_600x900.jpg",
    "https://shared.steamstatic.com/store_item_assets/steam/apps/{id}/header.jpg",
];
const MAX_COVER: usize = 5 * 1024 * 1024;

/** Dateiname eines heruntergeladenen Covers im Icon-Ordner. */
fn web_cover(appid: &str) -> String {
    format!("steam_{appid}_web.jpg")
}

/** Holt fehlende Cover von Steam (nur Spiele ohne Bild im Steam-Cache, die noch nicht geladen wurden). Liefert die Anzahl. */
pub async fn download_missing_covers(icon_dir: &Path, games: &[(String, String)]) -> usize {
    let Ok(client) = reqwest::Client::builder().user_agent("TouchDeck").timeout(Duration::from_secs(10)).build() else { return 0 };
    let mut loaded = 0;
    for (appid, _) in games {
        if !appid.chars().all(|c| c.is_ascii_digit()) || steam::cover_image(appid).is_some() || icon_dir.join(web_cover(appid)).exists() {
            continue;
        }
        for url in COVER_URLS.map(|u| u.replace("{id}", appid)) {
            let Ok(res) = client.get(&url).send().await else { continue };
            let is_image = res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("image/"));
            if !res.status().is_success() || !is_image {
                continue;
            }
            if let Ok(bytes) = res.bytes().await
                && !bytes.is_empty()
                && bytes.len() <= MAX_COVER
                && std::fs::write(icon_dir.join(web_cover(appid)), &bytes).is_ok()
            {
                loaded += 1;
                break;
            }
        }
    }
    loaded
}

/** Cover für ein Spiel: aus dem Steam-Cache kopiert oder vorher heruntergeladen. */
fn cover_icon(appid: &str, icon_dir: &Path) -> Option<String> {
    if let Some(src) = steam::cover_image(appid) {
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let name = format!("steam_{appid}{ext}");
        if std::fs::copy(src, icon_dir.join(&name)).is_ok() {
            return Some(name);
        }
    }
    let web = web_cover(appid);
    icon_dir.join(&web).exists().then_some(web)
}

fn game_button(appid: &str, title: &str, row: usize, col: usize, icon_dir: &Path) -> Value {
    let cover = cover_icon(appid, icon_dir);
    let icon = match &cover {
        Some(name) => json!({ "type": "image", "value": name }),
        None => json!({ "type": "emoji", "value": "🎮" }),
    };
    json!({
        "row": row, "col": col, "title": title, "icon": icon,
        "iconFit": if cover.is_some() { "cover" } else { "contain" },
        "titlePos": if cover.is_some() { "hidden" } else { "bottom" },
        "color": "#1b2838",
        "actions": [action("steam_game", json!({ "game": appid }))],
        "events": [
            on_event("steam_game", json!({ "game": appid, "state": "started" }), vec![action("change_visuals", json!({ "target": "self", "color": "#4c8a2f" }))]),
            on_event("steam_game", json!({ "game": appid, "state": "stopped" }), vec![action("restore_visuals", json!({ "target": "self" }))]),
        ],
    })
}

/** reuse_ids: IDs vorhandener Steam-Seiten, damit Verweise darauf gültig bleiben. */
pub fn build_steam_pages(games: &[(String, String)], icon_dir: &Path, reuse_ids: &[String]) -> Result<Vec<Page>, HttpError> {
    if games.is_empty() {
        return Err(HttpError::new(404, t!("steam.noSteamGamesFound")));
    }
    // Letzte Zeile jeder Seite: Navigation und Steam-Funktionen.
    let per_page = COLS * (MAX_ROWS - 1);
    let chunks: Vec<&[(String, String)]> = games.chunks(per_page).collect();
    let ids: Vec<String> = (0..chunks.len()).map(|i| reuse_ids.get(i).cloned().unwrap_or_else(|| new_id("p_"))).collect();

    Ok(chunks
        .iter()
        .enumerate()
        .filter_map(|(n, chunk)| {
            let rows = chunk.len().div_ceil(COLS) + 1;
            let last = rows - 1;
            let mut buttons: Vec<Value> = chunk.iter().enumerate().map(|(i, (id, name))| game_button(id, name, i / COLS, i % COLS, icon_dir)).collect();
            let mut tool = |col: usize, title: &str, emoji: &str, actions: Vec<Value>, extra: Value| {
                let mut b = json!({ "row": last, "col": col, "title": title, "icon": { "type": "emoji", "value": emoji }, "color": "#2a475e", "actions": actions });
                if let (Some(b), Some(extra)) = (b.as_object_mut(), extra.as_object()) {
                    b.extend(extra.clone());
                }
                buttons.push(b);
            };
            tool(0, t!("steam.library"), "📚", vec![action("steam_open", json!({ "view": "library" }))], json!({}));
            tool(1, "Downloads", "⬇️", vec![action("steam_open", json!({ "view": "downloads" }))], json!({ "live": { "source": "steam_updates" }, "colorOn": "#a3501d" }));
            tool(2, "Big Picture", "📺", vec![action("steam_open", json!({ "view": "bigpicture" }))], json!({}));
            tool(3, t!("steam.friends"), "👥", vec![action("steam_open", json!({ "view": "friends" }))], json!({}));
            if n > 0 {
                tool(4, t!("common.back"), "◀️", vec![action("go_page", json!({ "page": ids[n - 1] }))], json!({}));
            }
            if n < chunks.len() - 1 {
                tool(5, t!("common.next"), "▶️", vec![action("go_page", json!({ "page": ids[n + 1] }))], json!({}));
            }
            sanitize_page(&json!({
                "id": ids[n],
                "name": if chunks.len() > 1 { format!("{} {}", t!("steam.pageName"), n + 1) } else { t!("steam.pageName").into() },
                "rows": rows, "cols": COLS, "background": "#171a21", "template": "steam", "buttons": buttons,
            }))
        })
        .collect())
}
