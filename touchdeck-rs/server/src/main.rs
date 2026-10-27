// Startet TouchDeck ohne Fenster: lädt die Daten, startet den Server und öffnet den Editor im Browser.
use touchdeck_server::t;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::Duration;

use touchdeck_server::core::store::Store;
use touchdeck_server::server::{self, Options};
use touchdeck_server::system::os;

const HELP: &str = "TouchDeck – Makro-Fernbedienung für Linux

  --port <n>      Port des Servers (Standard 8770, oder TOUCHDECK_PORT)
  --data <dir>    Datenordner (Standard ./data, oder TOUCHDECK_DATA)
  --public <dir>  Ordner mit Editor und Bedienfeld (oder TOUCHDECK_PUBLIC)
  --no-open       Editor nicht automatisch im Browser öffnen";

struct Args {
    /// None = Port aus den Einstellungen.
    port: Option<u16>,
    data: PathBuf,
    public: PathBuf,
    open: bool,
}

fn parse_args() -> Args {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    // Beim Entwickeln: Editor und Bedienfeld aus dem Quellordner.
    let dev_public = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web");
    let mut args = Args {
        port: env("TOUCHDECK_PORT").and_then(|p| p.parse().ok()),
        data: env("TOUCHDECK_DATA").map_or_else(|| PathBuf::from("data"), PathBuf::from),
        public: env("TOUCHDECK_PUBLIC").map_or(dev_public, PathBuf::from),
        open: true,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| {
            it.next().unwrap_or_else(|| {
                eprintln!("{name} braucht einen Wert.\n\n{HELP}");
                std::process::exit(2)
            })
        };
        match a.as_str() {
            "--port" => {
                args.port = Some(value("--port").parse().unwrap_or_else(|_| {
                    eprintln!("{}", t!("cli.portNotValidNumber"));
                    std::process::exit(2)
                }))
            }
            "--data" => args.data = PathBuf::from(value("--data")),
            "--public" => args.public = PathBuf::from(value("--public")),
            "--no-open" => args.open = false,
            "-h" | "--help" => {
                println!("{HELP}");
                std::process::exit(0)
            }
            other => {
                eprintln!("Unbekannte Option: {other}\n\n{HELP}");
                std::process::exit(2)
            }
        }
    }
    args
}

/** Läuft auf dem Port schon ein TouchDeck? */
async fn touchdeck_running(port: u16) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let check = async {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        s.write_all(b"GET /api/hello HTTP/1.0\r\nHost: localhost\r\n\r\n").await.ok()?;
        let mut buf = String::new();
        s.read_to_string(&mut buf).await.ok()?;
        Some(buf.contains("\"app\":\"touchdeck\""))
    };
    tokio::time::timeout(Duration::from_secs(2), check).await.ok().flatten().unwrap_or(false)
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    let data = std::path::absolute(&args.data).unwrap_or(args.data);
    let public = args.public.canonicalize().unwrap_or(args.public);
    let store = match Store::open(&data) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("{err:#}");
            std::process::exit(1)
        }
    };
    // --port bzw. TOUCHDECK_PORT haben Vorrang, sonst der Port aus den Einstellungen.
    let port = args.port.unwrap_or(store.settings().port);
    let editor_url = format!("http://localhost:{port}/editor/");
    let opts = Options {
        port,
        host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        apk: public.join("../downloads/TouchDeck.apk"),
        public_dir: public,
        on_pair_request: None,
    };
    if let Err(err) = server::start(store, opts).await {
        let in_use = err.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::AddrInUse);
        if !in_use {
            eprintln!("{err:#}");
            std::process::exit(1)
        }
        // Läuft TouchDeck schon (z. B. per Autostart), nur dessen Editor öffnen.
        if !touchdeck_running(port).await {
            eprintln!("{}", t!("cli.portIsUsedBy", port));
            std::process::exit(1)
        }
        println!("{}", t!("cli.touchdeckIsAlreadyRunning", editor_url));
        if args.open {
            let _ = os::open_with_default(&editor_url);
        }
        return;
    }

    println!("{}", t!("cli.touchdeckIsRunningEditor", data.display(), editor_url));
    if args.open {
        let _ = os::open_with_default(&editor_url);
    }
    // Strg+C und SIGTERM fängt der Server selbst ab (speichert noch und beendet).
    std::future::pending::<()>().await;
}
