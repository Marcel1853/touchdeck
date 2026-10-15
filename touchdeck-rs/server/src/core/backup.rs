// Sicherungen der Konfiguration (Seiten, Werte, Vorlagen, Events, Einstellungen) im Ordner backups/.
use crate::t;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::Result;
use chrono::{DateTime, Local, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::server::http::HttpError;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub size: u64,
    pub time: String,
    pub note: String,
    #[serde(skip)]
    modified: SystemTime,
}

/** Prüft den Dateinamen (touchdeck-JJJJ-MM-TT_hh-mm-ss[-notiz].json) und liefert die Notiz. */
fn parse_name(name: &str) -> Option<String> {
    if name.contains('/') {
        return None;
    }
    let rest = name.strip_prefix("touchdeck-")?.strip_suffix(".json")?;
    let (stamp, note) = rest.split_at_checked(19)?;
    let pattern_ok = stamp.char_indices().all(|(i, c)| match i {
        4 | 7 | 13 | 16 => c == '-',
        10 => c == '_',
        _ => c.is_ascii_digit(),
    });
    if !pattern_ok {
        return None;
    }
    match note {
        "" => Some(String::new()),
        n => n.strip_prefix('-').filter(|n| !n.is_empty()).map(|n| n.replace('-', " ")),
    }
}

pub struct Backups {
    dir: PathBuf,
}

impl Backups {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let dir = data_dir.join("backups");
        let _ = fs::create_dir_all(&dir);
        Backups { dir }
    }

    pub fn list(&self) -> Vec<BackupInfo> {
        let mut out: Vec<BackupInfo> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let note = parse_name(&name)?;
                let meta = e.metadata().ok()?;
                let modified = meta.modified().ok()?;
                let time = DateTime::<Utc>::from(modified).to_rfc3339_opts(SecondsFormat::Millis, true);
                Some(BackupInfo { name, size: meta.len(), time, note, modified })
            })
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.modified));
        out
    }

    /** Legt eine Sicherung an; keep = wie viele automatische Sicherungen bleiben. */
    pub fn create(&self, config: &impl Serialize, note: &str, keep: i64) -> Result<String> {
        let stamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
        let mut slug = String::new();
        for c in note.to_lowercase().chars() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || "äöüß".contains(c) {
                slug.push(c);
            } else if !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let slug: String = slug.trim_matches('-').chars().take(30).collect();
        let name = if slug.is_empty() { format!("touchdeck-{stamp}.json") } else { format!("touchdeck-{stamp}-{slug}.json") };
        let path = self.dir.join(&name);
        fs::write(&path, serde_json::to_string_pretty(config)?)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        self.prune(keep);
        Ok(name)
    }

    /** Nur die neuesten N automatischen Sicherungen behalten; benannte bleiben. */
    fn prune(&self, keep: i64) {
        let auto: Vec<_> = self.list().into_iter().filter(|b| b.note == "automatisch").collect();
        for b in auto.iter().skip(keep.max(0) as usize) {
            let _ = fs::remove_file(self.dir.join(&b.name));
        }
    }

    /** Ist eine automatische Sicherung fällig (älter als ein Tag oder noch keine)? */
    pub fn auto_due(&self) -> bool {
        match self.list().into_iter().find(|b| b.note == "automatisch") {
            None => true,
            Some(b) => b.modified.elapsed().is_ok_and(|age| age > DAY),
        }
    }

    pub fn read(&self, name: &str) -> Result<Value, HttpError> {
        let unknown = || HttpError::new(404, "Unbekannte Sicherung.");
        parse_name(name).ok_or_else(unknown)?;
        let raw = fs::read_to_string(self.dir.join(name)).map_err(|_| unknown())?;
        serde_json::from_str(&raw).map_err(|_| HttpError::new(400, t!("backup.backupIsDamaged")))
    }

    pub fn remove(&self, name: &str) -> Result<(), HttpError> {
        parse_name(name).ok_or_else(|| HttpError::new(404, "Unbekannte Sicherung."))?;
        let _ = fs::remove_file(self.dir.join(name));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_name;

    #[test]
    fn dateinamen() {
        assert_eq!(parse_name("touchdeck-2026-10-14_17-20-00.json"), Some(String::new()));
        assert_eq!(parse_name("touchdeck-2026-10-14_17-20-00-vor-wiederherstellung.json").as_deref(), Some("vor wiederherstellung"));
        assert_eq!(parse_name("touchdeck-2026-10-14_17-20-00-.json"), None);
        assert_eq!(parse_name("../config.json"), None);
        assert_eq!(parse_name("touchdeck-2026-10-14_17-20-00-/../../x.json"), None);
    }
}
