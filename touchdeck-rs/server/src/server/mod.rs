// Server: HTTP-Routen und WebSocket (server.rs), Hilfen (http.rs), Suche im WLAN (discovery.rs).
pub mod discovery;
pub mod http;
// MCP-Server für KI-Assistenten (POST /mcp).
mod mcp;
// Gleicher Name wie in der bisherigen Fassung (server/server.js).
#[allow(clippy::module_inception)]
mod server;

pub use server::{App, Options, PairRequest, start};
