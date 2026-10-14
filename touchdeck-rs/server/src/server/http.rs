// Kleine HTTP-Hilfen: Antworten senden, Dateien ausliefern, Fehler mit Statuscode.
use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

/** Fehler mit HTTP-Status; die Meldung geht als { error } an den Aufrufer. */
#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub message: String,
    /// Nur interne Fehler (500) kommen ins Protokoll.
    pub internal: bool,
}

impl HttpError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        HttpError { status, message: message.into(), internal: false }
    }
}

impl From<anyhow::Error> for HttpError {
    fn from(err: anyhow::Error) -> Self {
        HttpError { status: 500, message: format!("{err:#}"), internal: true }
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub type ApiResult = Result<serde_json::Value, HttpError>;

pub fn json_response(status: u16, body: &impl Serialize) -> Response {
    let mut res = (StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), axum::Json(body)).into_response();
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

pub fn error_response(status: u16, message: &str) -> Response {
    json_response(status, &json!({ "error": message }))
}

pub fn text_response(status: u16, text: &str) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(text.to_string()))
        .expect("gültige Antwort")
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("webmanifest") => "application/manifest+json",
        Some("log") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/** Sicherer Pfad unterhalb von root (keine „..“, keine absoluten Pfade). */
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel.trim_start_matches('/'));
    if rel.as_os_str().is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(root.join(rel))
}

/** Liefert eine Datei aus; rel ist bereits URL-dekodiert. */
pub async fn serve_file(root: &Path, rel: &str, extra: &[(&'static str, String)]) -> Response {
    let Some(file) = safe_join(root, rel) else { return text_response(403, "Verboten") };
    match tokio::fs::read(&file).await {
        Ok(bytes) => {
            let mut b = Response::builder()
                .status(200)
                .header(header::CONTENT_TYPE, mime(&file))
                .header(header::CACHE_CONTROL, "no-cache");
            for (k, v) in extra {
                b = b.header(*k, v);
            }
            b.body(Body::from(bytes)).expect("gültige Antwort")
        }
        Err(_) => text_response(404, "Nicht gefunden"),
    }
}

/** Vergleich in konstanter Zeit, damit sich der Schlüssel nicht erraten lässt. */
pub fn safe_equal(a: Option<&str>, b: &str) -> bool {
    let a = a.unwrap_or("").as_bytes();
    let b = b.as_bytes();
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pfade() {
        let root = Path::new("/srv");
        assert_eq!(safe_join(root, "/editor/index.html"), Some(PathBuf::from("/srv/editor/index.html")));
        assert_eq!(safe_join(root, "/editor/../../etc/passwd"), None);
        assert_eq!(safe_join(root, ""), None);
    }

    #[test]
    fn vergleich() {
        assert!(safe_equal(Some("abc"), "abc"));
        assert!(!safe_equal(Some("abd"), "abc"));
        assert!(!safe_equal(None, "abc"));
    }
}
