// Editor, Bedienfeld und Vorschau, fest im Programm eingebettet (Tabelle aus build.rs).
include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

/** Eingebettete Datei zu einem Pfad wie „editor/index.html“: (Inhalt, ETag). */
pub fn get(rel: &str) -> Option<(&'static [u8], &'static str)> {
    let rel = rel.trim_start_matches('/');
    ASSETS.iter().find(|(path, _, _)| *path == rel).map(|(_, bytes, etag)| (*bytes, *etag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alles_eingebettet() {
        for page in ["editor/index.html", "deck/index.html", "preview/index.html", "shared/boot.js", "shared/i18n.js", "editor/js/main.js", "deck/deck.js"] {
            assert!(get(page).is_some(), "{page} fehlt");
        }
        for lang in crate::core::i18n::LANGUAGES {
            assert!(get(&format!("shared/lang/{lang}.json")).is_some(), "Sprache {lang} fehlt");
        }
        // Was die Seiten laden, muss dabei sein (href/src mit festem Pfad).
        for page in ["editor/index.html", "deck/index.html", "preview/index.html"] {
            let html = std::str::from_utf8(get(page).unwrap().0).unwrap();
            let dir = page.rsplit_once('/').unwrap().0;
            for part in html.split(['"', '\'']).filter(|p| [".js", ".css", ".svg", ".webmanifest"].iter().any(|e| p.ends_with(e)) && !p.contains("://")) {
                let rel = if let Some(abs) = part.strip_prefix('/') { abs.to_string() } else { format!("{dir}/{part}") };
                assert!(get(&rel).is_some(), "{page} lädt {part}, aber {rel} ist nicht eingebettet");
            }
        }
        assert!(get("../Cargo.toml").is_none() && get("gibt/es/nicht.js").is_none());
        let (_, etag) = get("deck/deck.js").unwrap();
        assert!(etag.starts_with('"') && etag.ends_with('"'));
    }
}
