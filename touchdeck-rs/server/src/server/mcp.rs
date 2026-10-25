// MCP-Server für KI-Assistenten (Model Context Protocol, Transport „Streamable HTTP“ ohne Streams):
// POST /mcp mit JSON-RPC 2.0, Antwort direkt als JSON. Nur vom PC selbst und mit eigenem Schlüssel
// (Einstellungen › KI-Assistenten). Lesen und Auslösen sind erlaubt, sobald der Server an ist; Ändern
// von Seiten/Buttons und beliebige Aktionen nur mit dem zweiten Haken „darf ändern“.
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Response};
use serde_json::{Map, Value, json};

use super::http::json_response;
use super::server::{App, is_loopback, press, run};
use crate::core::catalog::{self, Section};
use crate::core::store::{action_list, js_string};
use crate::t;

/** Protokoll-Versionen, die wir sprechen (neueste zuerst). */
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub(super) async fn handle(app: &Arc<App>, addr: &SocketAddr, headers: &HeaderMap, method: &Method, body: &[u8]) -> Response<Body> {
    let (enabled, allow_edit, token) = {
        let store = app.store.lock().unwrap();
        let m = &store.settings().mcp;
        (m.enabled, m.allow_edit, m.token.clone())
    };
    if !enabled {
        return json_response(404, &json!({ "error": t!("mcp.disabled") }));
    }
    // Nur vom PC selbst; fremde Webseiten (Origin nicht localhost) abweisen.
    let origin_ok = headers.get("origin").and_then(|o| o.to_str().ok()).is_none_or(|o| {
        let o = o.to_lowercase();
        o.starts_with("http://localhost") || o.starts_with("http://127.0.0.1") || o.starts_with("http://[::1]")
    });
    if !is_loopback(addr) || !origin_ok {
        return json_response(403, &json!({ "error": t!("mcp.onlyLocal") }));
    }
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("");
    if token.is_empty() || auth.strip_prefix("Bearer ").map(str::trim) != Some(token.as_str()) {
        return json_response(401, &json!({ "error": t!("mcp.wrongKey") }));
    }
    if method != Method::POST {
        return json_response(405, &json!({ "error": "POST /mcp" }));
    }
    let Ok(msg) = serde_json::from_slice::<Value>(body) else {
        return json_response(400, &rpc_error(Value::Null, -32700, "Parse error"));
    };
    // Einzelne Nachricht oder Liste (Batch).
    let reply = if let Some(list) = msg.as_array() {
        let mut out = Vec::new();
        for m in list {
            if let Some(r) = one(app, m, allow_edit).await {
                out.push(r);
            }
        }
        (!out.is_empty()).then(|| Value::Array(out))
    } else {
        one(app, &msg, allow_edit).await
    };
    match reply {
        Some(r) => json_response(200, &r),
        // Nur Benachrichtigungen: 202 ohne Inhalt.
        None => Response::builder().status(202).body(Body::empty()).expect("gültige Antwort"),
    }
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/** Eine JSON-RPC-Nachricht; None bei Benachrichtigungen (ohne id). */
async fn one(app: &Arc<App>, msg: &Value, allow_edit: bool) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg["method"].as_str().unwrap_or("");
    let params = &msg["params"];
    let result = match method {
        "initialize" => {
            let wanted = params["protocolVersion"].as_str().unwrap_or("");
            let version = VERSIONS.iter().find(|v| **v == wanted).unwrap_or(&VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "TouchDeck", "version": app.version },
                "instructions": "TouchDeck is a macro remote for Linux: pages with buttons shown on phones and tablets. \
                    Use list_pages to see pages and buttons, press_button to trigger a button, get_values/set_value for values. \
                    Editing tools are only available if the user allowed it in TouchDeck's settings.",
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools(allow_edit) }),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params["arguments"].clone();
            app.log.info("MCP", t!("mcp.toolCalled", name));
            match call(app, name, &args, allow_edit).await {
                Ok(v) => json!({ "content": [{ "type": "text", "text": text_of(&v) }], "structuredContent": wrap(v), "isError": false }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            }
        }
        _ => return Some(rpc_error(id, -32601, &format!("Method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/** structuredContent muss ein Objekt sein. */
fn wrap(v: Value) -> Value {
    if v.is_object() { v } else { json!({ "result": v }) }
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({ "name": name, "description": description, "inputSchema": { "type": "object", "properties": properties, "required": required } })
}

fn tools(allow_edit: bool) -> Vec<Value> {
    let s = |d: &str| json!({ "type": "string", "description": d });
    let mut list = vec![
        tool("list_pages", "List all pages with their buttons (id, title, mode, position, live display).", json!({}), &[]),
        tool("get_button", "Full details of one button: look, actions, live display, current state.", json!({ "button": s("Button id") }), &["button"]),
        tool("get_live", "Current live values (CPU, clock, plugin values …) of all buttons on a page.", json!({ "page": s("Page id or name") }), &["page"]),
        tool("press_button", "Press a button as if tapped on the deck (runs its actions).", json!({ "button": s("Button id"), "double": { "type": "boolean", "description": "Double press" } }), &["button"]),
        tool("get_values", "All TouchDeck values (variables) with their current content.", json!({}), &[]),
        tool("set_value", "Set a TouchDeck value (variable).", json!({ "name": s("Value name"), "value": s("New content") }), &["name", "value"]),
        tool("show_page", "Show a page on all connected phones and tablets.", json!({ "page": s("Page id or name") }), &["page"]),
        tool("get_status", "TouchDeck version, connected devices and their pages, OBS/Twitch/Discord state.", json!({}), &[]),
    ];
    if allow_edit {
        let obj = |d: &str| json!({ "type": "object", "description": d });
        let arr = |d: &str| json!({ "type": "array", "items": { "type": "object" }, "description": d });
        list.extend([
            tool("list_action_types", "All action types (type, label, fields) for buttons and run_actions, including plugin actions.", json!({}), &[]),
            tool("run_actions", "Run a list of actions directly, e.g. [{\"type\":\"notify\",\"params\":{\"title\":\"Hi\",\"target\":\"deck\"}}].",
                json!({ "actions": arr("Actions: { type, params }") }), &["actions"]),
            tool("create_page", "Create a new page.", json!({ "name": s("Page name"), "rows": { "type": "integer" }, "cols": { "type": "integer" }, "folder": s("Optional folder") }), &["name"]),
            tool("create_button", "Create a button on a page at row/col (0-based). props: title, icon {type:\"emoji\",value}, color, textColor, mode (press|toggle|switch|states|display), actions [{type, params}], live {source} …",
                json!({ "page": s("Page id or name"), "row": { "type": "integer" }, "col": { "type": "integer" }, "props": obj("Button properties") }), &["page", "row", "col"]),
            tool("update_button", "Change properties of a button (merged into the existing ones, same keys as create_button).",
                json!({ "button": s("Button id"), "props": obj("Properties to change") }), &["button", "props"]),
            tool("delete_button", "Delete a button.", json!({ "button": s("Button id") }), &["button"]),
        ]);
    }
    list
}

fn lang(app: &App) -> String {
    app.store.lock().unwrap().settings().language.clone()
}

/** Seite nach ID oder Name (ohne Groß-/Kleinschreibung). */
fn find_page(pages: &[Value], key: &str) -> Option<usize> {
    pages.iter().position(|p| p["id"] == key).or_else(|| pages.iter().position(|p| p["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(key))))
}

/** Seiten als JSON lesen, ändern und über den normalen Speicherweg (mit Prüfung) zurückschreiben. */
fn edit_pages(app: &Arc<App>, f: impl FnOnce(&mut Vec<Value>) -> Result<Value, String>) -> Result<Value, String> {
    let mut store = app.store.lock().unwrap();
    let mut pages: Vec<Value> = serde_json::to_value(&store.config.pages).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let out = f(&mut pages)?;
    store.set_project(&json!({ "pages": pages })).map_err(|e| e.to_string())?;
    Ok(out)
}

async fn call(app: &Arc<App>, name: &str, args: &Value, allow_edit: bool) -> Result<Value, String> {
    let arg = |k: &str| args[k].as_str().map(str::trim).unwrap_or("").to_string();
    let edit_tool = ["list_action_types", "run_actions", "create_page", "create_button", "update_button", "delete_button"].contains(&name);
    if edit_tool && !allow_edit {
        return Err(t!("mcp.editNotAllowed").to_string());
    }
    match name {
        "list_pages" => {
            let store = app.store.lock().unwrap();
            Ok(json!({ "pages": store.config.pages.iter().map(|p| json!({
                "id": p.id, "name": p.name, "folder": p.folder, "rows": p.rows, "cols": p.cols,
                "buttons": p.buttons.iter().map(|b| json!({
                    "id": b.id, "title": b.title, "icon": b.icon.value, "mode": b.mode, "row": b.row, "col": b.col,
                    "live": if b.live.source == "none" { Value::Null } else { json!(b.live.source) },
                    "actions": b.actions.len(),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>() }))
        }
        "get_button" => {
            let store = app.store.lock().unwrap();
            let id = arg("button");
            let (page, b) = store.find_button(&id).ok_or_else(|| t!("mcp.buttonNotFound", id))?;
            let mut v = serde_json::to_value(b).map_err(|e| e.to_string())?;
            v["page"] = json!({ "id": page.id, "name": page.name });
            v["state"] = store.config.toggles.get(&b.id).cloned().unwrap_or(Value::Null);
            Ok(v)
        }
        "get_live" => {
            let (page, variables) = {
                let store = app.store.lock().unwrap();
                let pages: Vec<Value> = store.config.pages.iter().map(|p| json!({ "id": p.id, "name": p.name })).collect();
                let i = find_page(&pages, &arg("page")).ok_or_else(|| t!("mcp.pageNotFound", arg("page")))?;
                (store.config.pages[i].clone(), store.config.variables.clone())
            };
            let live = crate::core::live::collect_live(app, std::slice::from_ref(&page), &variables).await;
            Ok(json!({ "page": page.name, "values": page.buttons.iter().filter_map(|b| live.get(&b.id).map(|v| json!({
                "button": b.id, "title": b.title, "text": v["text"], "value": v["value"], "on": v["on"],
            }))).collect::<Vec<_>>() }))
        }
        "press_button" => {
            let id = arg("button");
            let page = app.store.lock().unwrap().find_button(&id).map(|(p, _)| p.id.clone()).ok_or_else(|| t!("mcp.buttonNotFound", id.clone()))?;
            Ok(press(app, &page, &id, args["double"] == true).await)
        }
        "get_values" => Ok(json!({ "values": app.store.lock().unwrap().config.variables })),
        "set_value" => {
            let mut store = app.store.lock().unwrap();
            store.set_variable(&arg("name"), &json!(js_string(&args["value"]))).map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true }))
        }
        "show_page" => {
            let id = {
                let store = app.store.lock().unwrap();
                let pages: Vec<Value> = store.config.pages.iter().map(|p| json!({ "id": p.id, "name": p.name })).collect();
                let i = find_page(&pages, &arg("page")).ok_or_else(|| t!("mcp.pageNotFound", arg("page")))?;
                store.config.pages[i].id.clone()
            };
            app.show_page_on_devices(&id);
            Ok(json!({ "ok": true, "page": id }))
        }
        "get_status" => Ok(json!({
            "version": app.version,
            "devices": app.device_list(),
            "obs": app.obs.connected(),
            "twitch": app.twitch.status()["state"],
            "discord": app.discord.status()["state"],
        })),
        "list_action_types" => {
            let lang = lang(app);
            let mut list = catalog::localized(&catalog::ACTION_TYPES, Section::Actions, &lang).as_array().cloned().unwrap_or_default();
            list.extend(app.plugins.catalog(&lang).0);
            Ok(json!({ "actionTypes": list.iter().map(|a| json!({
                "type": a["type"], "label": a["label"],
                "fields": a["fields"].as_array().into_iter().flatten().map(|f| json!({ "key": f["key"], "type": f["type"], "label": f["label"], "options": f["options"] })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>() }))
        }
        "run_actions" => {
            let actions = action_list(&args["actions"]);
            if actions.is_empty() {
                return Err(t!("mcp.noActions").to_string());
            }
            Ok(run(app, &actions, None, "MCP").await)
        }
        "create_page" => {
            let name = arg("name");
            if name.is_empty() {
                return Err(t!("mcp.nameMissing").to_string());
            }
            let page = json!({ "name": name, "rows": args["rows"].as_i64().unwrap_or(3), "cols": args["cols"].as_i64().unwrap_or(5), "folder": arg("folder"), "buttons": [] });
            edit_pages(app, |pages| {
                pages.push(page);
                Ok(Value::Null)
            })?;
            let store = app.store.lock().unwrap();
            let p = store.config.pages.last().ok_or("?")?;
            Ok(json!({ "ok": true, "id": p.id, "name": p.name }))
        }
        "create_button" => {
            let (row, col) = (args["row"].as_i64().unwrap_or(-1), args["col"].as_i64().unwrap_or(-1));
            let mut b = args["props"].as_object().cloned().unwrap_or_default();
            b.insert("row".into(), json!(row));
            b.insert("col".into(), json!(col));
            b.remove("id");
            let key = arg("page");
            edit_pages(app, |pages| {
                let i = find_page(pages, &key).ok_or_else(|| t!("mcp.pageNotFound", key.clone()))?;
                let (rows, cols) = (pages[i]["rows"].as_i64().unwrap_or(0), pages[i]["cols"].as_i64().unwrap_or(0));
                if !(0..rows).contains(&row) || !(0..cols).contains(&col) {
                    return Err(t!("mcp.outsidePage", rows, cols).to_string());
                }
                let taken = pages[i]["buttons"].as_array().into_iter().flatten().any(|x| {
                    let (r, c) = (x["row"].as_i64().unwrap_or(-1), x["col"].as_i64().unwrap_or(-1));
                    let (rs, cs) = (x["rowSpan"].as_i64().unwrap_or(1), x["colSpan"].as_i64().unwrap_or(1));
                    (r..r + rs).contains(&row) && (c..c + cs).contains(&col)
                });
                if taken {
                    return Err(t!("mcp.cellTaken").to_string());
                }
                if let Some(list) = pages[i]["buttons"].as_array_mut() {
                    list.push(Value::Object(b));
                }
                Ok(Value::Null)
            })?;
            // ID des neuen Buttons: der an dieser Stelle.
            let store = app.store.lock().unwrap();
            let id = store.config.pages.iter().find(|p| p.id == key || p.name.eq_ignore_ascii_case(&key)).and_then(|p| p.buttons.iter().find(|x| x.row == row && x.col == col)).map(|x| x.id.clone());
            Ok(json!({ "ok": true, "id": id }))
        }
        "update_button" => {
            let id = arg("button");
            let props: Map<String, Value> = args["props"].as_object().cloned().unwrap_or_default();
            edit_pages(app, |pages| {
                let b = pages
                    .iter_mut()
                    .flat_map(|p| p["buttons"].as_array_mut().into_iter().flatten())
                    .find(|b| b["id"] == id.as_str())
                    .ok_or_else(|| t!("mcp.buttonNotFound", id.clone()))?;
                for (k, v) in props {
                    if k != "id" {
                        b[k.as_str()] = v;
                    }
                }
                Ok(Value::Null)
            })?;
            Ok(json!({ "ok": true }))
        }
        "delete_button" => {
            let id = arg("button");
            edit_pages(app, |pages| {
                let mut found = false;
                for p in pages.iter_mut() {
                    if let Some(list) = p["buttons"].as_array_mut() {
                        let before = list.len();
                        list.retain(|b| b["id"] != id.as_str());
                        found |= list.len() != before;
                    }
                }
                if found { Ok(Value::Null) } else { Err(t!("mcp.buttonNotFound", id.clone()).to_string()) }
            })?;
            Ok(json!({ "ok": true }))
        }
        _ => Err(t!("mcp.unknownTool", name).to_string()),
    }
}
