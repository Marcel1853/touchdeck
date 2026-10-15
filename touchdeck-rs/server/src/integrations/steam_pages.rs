// Erzeugt Seiten mit allen installierten Steam-Spielen (Coverbild, Start-Aktion, „läuft“-Anzeige).
use crate::t;
use std::path::Path;

use serde_json::{Value, json};

use super::steam;
use crate::core::store::{Page, new_id, sanitize_page};
use crate::server::http::HttpError;

const COLS: usize = 6;
const MAX_ROWS: usize = 4;

fn action(kind: &str, params: Value) -> Value {
    json!({ "id": new_id("a_"), "type": kind, "enabled": true, "params": params })
}

fn game_button(appid: &str, title: &str, row: usize, col: usize, icon_dir: &Path) -> Value {
    let src = steam::cover_image(appid);
    let mut icon = json!({ "type": "emoji", "value": "🎮" });
    if let Some(src) = &src {
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let name = format!("steam_{appid}{ext}");
        if std::fs::copy(src, icon_dir.join(&name)).is_ok() {
            icon = json!({ "type": "image", "value": name });
        }
    }
    json!({
        "row": row, "col": col, "title": title, "icon": icon,
        "iconFit": if src.is_some() { "cover" } else { "contain" },
        "titlePos": if src.is_some() { "hidden" } else { "bottom" },
        "color": "#1b2838", "colorOn": "#4c8a2f",
        "actions": [action("steam_game", json!({ "game": appid }))],
        "live": { "source": "steam_running" },
    })
}

/** reuse_ids: IDs vorhandener Steam-Seiten, damit Verweise darauf gültig bleiben. */
pub fn build_steam_pages(icon_dir: &Path, reuse_ids: &[String]) -> Result<Vec<Page>, HttpError> {
    let games = steam::list_games();
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
                "name": if chunks.len() > 1 { format!("Steam-Spiele {}", n + 1) } else { "Steam-Spiele".into() },
                "rows": rows, "cols": COLS, "background": "#171a21", "template": "steam", "buttons": buttons,
            }))
        })
        .collect())
}
