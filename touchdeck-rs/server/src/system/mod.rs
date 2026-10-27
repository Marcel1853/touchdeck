// Betriebssystem-Schicht. Alles, was direkt mit dem System spricht, steckt in zwei Modulen:
// - os:    Programme, Audio, Medien, Systemwerte, Benachrichtigungen, Dateien, Netzwerk
// - input: Tastatur, Maus, Fenster, Zwischenablage, globale Tastenkürzel
// Unter Linux sind das linux.rs und x11.rs, unter Windows windows/os.rs und windows/input.rs
// mit denselben Funktionen. Was es auf einem System nicht gibt, meldet einen verständlichen Fehler.
#[cfg(not(windows))]
#[path = "linux.rs"]
pub mod os;
#[cfg(not(windows))]
#[path = "x11.rs"]
pub mod input;
#[cfg(windows)]
#[path = "windows/os.rs"]
pub mod os;
#[cfg(windows)]
#[path = "windows/input.rs"]
pub mod input;

/** Sortierschlüssel wie localeCompare(…, 'de'): ohne Groß/klein, Umlaute wie ihre Grundbuchstaben. */
pub fn sort_key(s: &str) -> String {
    s.to_lowercase().replace('ä', "a").replace('ö', "o").replace('ü', "u").replace('ß', "ss")
}
