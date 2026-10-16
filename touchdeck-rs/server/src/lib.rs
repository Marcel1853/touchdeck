// TouchDeck-Server: Editor, Bedienfeld fürs Handy, Aktionen und Integrationen.
// Aufbau wie die bisherige Fassung: core (Daten, Protokoll, Sicherungen), server (HTTP, WebSocket, Suche im WLAN),
// system (Linux-Funktionen) und integrations (OBS, Steam, Twitch, Discord).
pub mod core;
pub mod integrations;
pub mod server;
pub mod system;
