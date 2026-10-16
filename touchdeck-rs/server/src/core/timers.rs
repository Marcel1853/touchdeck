// Timer: Countdown (mit Dauer) oder Stoppuhr (Dauer 0), per Aktion gesteuert, als Live-Anzeige sichtbar.
// Solange ein Timer läuft, werden die Live-Werte jede Sekunde aktualisiert; abgelaufen → Auslöser „Timer abgelaufen“.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[derive(Clone, Default)]
struct State {
    /// Dauer in Millisekunden; 0 = Stoppuhr (zählt hoch).
    duration: u64,
    /// Bisher gelaufene Zeit ohne den aktuellen Lauf.
    elapsed: u64,
    started: Option<Instant>,
    /// Ein Lauf ist begonnen (läuft oder pausiert); erst nach Zurücksetzen oder Ablauf beginnt ein neuer.
    active: bool,
}

impl State {
    fn total(&self) -> u64 {
        self.elapsed + self.started.map_or(0, |s| s.elapsed().as_millis() as u64)
    }

    fn running(&self) -> bool {
        self.started.is_some()
    }
}

#[derive(Default)]
pub struct Timers {
    all: Mutex<HashMap<String, State>>,
    ticking: AtomicBool,
}

/** mm:ss, ab einer Stunde h:mm:ss. */
fn format(ms: u64) -> String {
    let s = ms.div_ceil(1000);
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

impl Timers {
    /** Aktion „Timer“: start, stop (pausieren), toggle, reset. duration_ms gilt beim Starten eines neuen Laufs. */
    pub fn control(&self, name: &str, op: &str, duration_ms: u64) -> bool {
        let mut all = self.all.lock().unwrap();
        let t = all.entry(name.to_string()).or_default();
        let start = |t: &mut State| {
            if !t.running() {
                // Neuer Lauf nach Ablauf oder Zurücksetzen: Dauer übernehmen, sonst geht es weiter.
                if !t.active {
                    t.duration = duration_ms;
                    t.elapsed = 0;
                    t.active = true;
                }
                t.started = Some(Instant::now());
            }
        };
        let stop = |t: &mut State| {
            t.elapsed = t.total();
            t.started = None;
        };
        match op {
            "start" => start(t),
            "stop" => stop(t),
            "reset" => *t = State { duration: duration_ms, ..State::default() },
            _ => {
                if t.running() {
                    stop(t)
                } else {
                    start(t)
                }
            }
        }
        t.running()
    }

    /** Live-Anzeige: Restzeit (Countdown) bzw. gelaufene Zeit (Stoppuhr); „an“, solange er läuft. */
    pub fn live(&self, name: &str) -> Value {
        let all = self.all.lock().unwrap();
        match all.get(name) {
            Some(t) if t.duration > 0 => json!({ "text": format(t.duration.saturating_sub(t.total())), "on": t.running() }),
            Some(t) => json!({ "text": format(t.total()), "on": t.running() }),
            None => json!({ "text": "00:00", "on": false }),
        }
    }

    /** Abgelaufene Countdowns anhalten und ihre Namen liefern. */
    fn finished(&self) -> Vec<String> {
        let mut all = self.all.lock().unwrap();
        let mut done = Vec::new();
        for (name, t) in all.iter_mut() {
            if t.running() && t.duration > 0 && t.total() >= t.duration {
                t.elapsed = t.duration;
                t.started = None;
                t.active = false;
                done.push(name.clone());
            }
        }
        done
    }

    fn any_running(&self) -> bool {
        self.all.lock().unwrap().values().any(State::running)
    }

    /**
     * Läuft ein Timer, jede Sekunde `tick` aufrufen (Live-Werte) und abgelaufene melden (`on_done`).
     * Hört von selbst auf, wenn keiner mehr läuft.
     */
    pub fn ensure_ticker<T: Send + Sync + 'static>(self: &Arc<Self>, ctx: Arc<T>, tick: fn(&Arc<T>), on_done: fn(&Arc<T>, &str)) {
        if self.ticking.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_secs(1));
            loop {
                t.tick().await;
                for name in me.finished() {
                    on_done(&ctx, &name);
                }
                tick(&ctx);
                if !me.any_running() {
                    me.ticking.store(false, Ordering::SeqCst);
                    // Falls genau jetzt einer gestartet wurde, weitermachen.
                    if me.any_running() && !me.ticking.swap(true, Ordering::SeqCst) {
                        continue;
                    }
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_steuern() {
        let t = Timers::default();
        assert_eq!(t.live("x")["text"], "00:00");
        assert!(t.control("x", "start", 90_000));
        assert_eq!(t.live("x")["text"], "01:30");
        assert_eq!(t.live("x")["on"], true);
        assert!(!t.control("x", "toggle", 90_000), "toggle pausiert");
        assert!(t.control("x", "toggle", 5_000), "weiter mit alter Dauer");
        assert!(t.live("x")["text"].as_str().unwrap().starts_with("01:"));
        t.control("x", "reset", 5_000);
        assert_eq!(t.live("x")["text"], "00:05");
        assert_eq!(format(3_725_000), "1:02:05");
        assert!(t.control("uhr", "start", 0), "Stoppuhr");
        assert_eq!(t.live("uhr")["text"], "00:00");
    }
}
