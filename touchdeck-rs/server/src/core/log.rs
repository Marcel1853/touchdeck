// Protokoll: die letzten Einträge im Speicher, alles zusätzlich in logs/touchdeck.log (wird bei 2 MB rotiert).
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use tokio::sync::broadcast;

const KEEP: usize = 1500;
const MAX_FILE: u64 = 2 * 1024 * 1024;
const LEVELS: [&str; 4] = ["debug", "info", "warn", "error"];

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub id: u64,
    pub time: String,
    pub level: &'static str,
    pub source: String,
    pub message: String,
}

pub struct Log {
    file: PathBuf,
    min_level: AtomicU8,
    seq: AtomicU64,
    entries: Mutex<Vec<Entry>>,
    tx: broadcast::Sender<Entry>,
}

impl Log {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Log {
            file: dir.join("touchdeck.log"),
            min_level: AtomicU8::new(1),
            seq: AtomicU64::new(0),
            entries: Mutex::new(Vec::new()),
            tx: broadcast::channel(256).0,
        }
    }

    /** Ab welcher Stufe protokolliert wird (debug, info, warn, error). */
    pub fn set_level(&self, level: &str) {
        let i = LEVELS.iter().position(|l| *l == level).unwrap_or(1);
        self.min_level.store(i as u8, Ordering::Relaxed);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Entry> {
        self.tx.subscribe()
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    fn add(&self, level: usize, source: &str, message: &str) {
        if (level as u8) < self.min_level.load(Ordering::Relaxed) {
            return;
        }
        let entry = Entry {
            id: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            level: LEVELS[level],
            source: source.into(),
            message: message.chars().take(2000).collect(),
        };
        {
            let mut entries = self.entries.lock().unwrap();
            entries.push(entry.clone());
            if entries.len() > KEEP {
                let extra = entries.len() - KEEP;
                entries.drain(..extra);
            }
        }
        self.append(&entry);
        if level == 3 || std::env::var_os("TOUCHDECK_DEBUG").is_some() {
            eprintln!("[{}] {}: {}", entry.level, entry.source, entry.message);
        }
        let _ = self.tx.send(entry);
    }

    fn append(&self, e: &Entry) {
        // Ein volles Laufwerk soll TouchDeck nicht lahmlegen, daher werden Fehler hier ignoriert.
        if fs::metadata(&self.file).is_ok_and(|m| m.len() > MAX_FILE) {
            let _ = fs::rename(&self.file, self.file.with_extension("log.1"));
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.file) {
            let _ = writeln!(f, "{} {:<5} [{}] {}", e.time, e.level.to_uppercase(), e.source, e.message);
        }
    }

    pub fn debug(&self, source: &str, msg: impl AsRef<str>) {
        self.add(0, source, msg.as_ref());
    }
    pub fn info(&self, source: &str, msg: impl AsRef<str>) {
        self.add(1, source, msg.as_ref());
    }
    pub fn warn(&self, source: &str, msg: impl AsRef<str>) {
        self.add(2, source, msg.as_ref());
    }
    pub fn error(&self, source: &str, msg: impl AsRef<str>) {
        self.add(3, source, msg.as_ref());
    }
}
