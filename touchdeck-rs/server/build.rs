// Bettet Editor, Bedienfeld und Vorschau (Ordner ../web) in das Programm ein: Aus allen Dateien entsteht
// eine Tabelle (Pfad, Inhalt, ETag) in $OUT_DIR/web_assets.rs. So braucht das fertige Programm keinen
// web-Ordner mehr neben sich.
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).expect("web-Ordner lesbar").flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(&p, out);
        } else {
            println!("cargo:rerun-if-changed={}", p.display());
            out.push(p);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../web").canonicalize().expect("Ordner touchdeck-rs/web");
    let mut files = Vec::new();
    collect(&root, &mut files);
    let mut table = String::from("pub static ASSETS: &[(&str, &[u8], &str)] = &[\n");
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(f).expect("Datei lesbar");
        // ETag: Länge und Hash des Inhalts – ändert sich die Datei, lädt der Browser sie neu.
        let mut h = DefaultHasher::new();
        h.write(&bytes);
        writeln!(table, "    ({rel:?}, include_bytes!({:?}), \"\\\"{:x}-{:016x}\\\"\"),", f.display().to_string(), bytes.len(), h.finish()).unwrap();
    }
    table.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("web_assets.rs");
    std::fs::write(out, table).expect("web_assets.rs schreiben");
}
