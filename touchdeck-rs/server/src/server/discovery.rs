// Automatisches Finden im WLAN: Die Handy-App ruft per UDP-Broadcast, TouchDeck antwortet mit Name und Port.
// Der Kopplungsschlüssel wird dabei nie verschickt – den gibt es erst nach Bestätigung am PC.
use crate::t;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use serde_json::json;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use crate::server::App;
use crate::system::linux as system;

pub const DISCOVERY_PORT: u16 = 8769;
const QUESTION: &str = "TOUCHDECK_DISCOVER";

fn bind() -> std::io::Result<UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    sock.set_nonblocking(true)?;
    sock.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT)).into())?;
    UdpSocket::from_std(sock.into())
}

pub fn start(app: Arc<App>) {
    tokio::spawn(async move {
        let sock = match bind() {
            Ok(s) => s,
            Err(err) => return app.log.warn(t!("server.discovery"), t!("server.automaticDiscoveryNotAvailable", err)),
        };
        app.log.debug(t!("server.discovery"), format!("Antworte auf Suchanfragen (UDP {DISCOVERY_PORT})"));
        let mut buf = [0u8; 256];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else { continue };
            let lan = app.store.lock().unwrap().settings().lan_enabled;
            if !lan || String::from_utf8_lossy(&buf[..n]).trim() != QUESTION {
                continue;
            }
            let answer = json!({ "app": "touchdeck", "name": system::hostname(), "port": app.port, "version": app.version });
            let _ = sock.send_to(answer.to_string().as_bytes(), from).await;
        }
    });
}
