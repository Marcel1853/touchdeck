// Linux-Funktionen: Programme, Audio, Medien, Systemwerte (linux.rs) sowie Tastatur, Maus und Fenster (x11.rs).
pub mod linux;
pub mod x11;

/** Sortierschlüssel wie localeCompare(…, 'de'): ohne Groß/klein, Umlaute wie ihre Grundbuchstaben. */
pub fn sort_key(s: &str) -> String {
    s.to_lowercase().replace('ä', "a").replace('ö', "o").replace('ü', "u").replace('ß', "ss")
}
