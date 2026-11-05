//! Egy jegyzet (a `notes.jsonl` egy sora) és a megjelenítés apró segédei.

use serde::{Deserialize, Serialize};

/// A tárolt sorok formátum-verziója (`"v": 1`).
pub const FORMAT_VERSION: u32 = 1;

/// Egy jegyzet: a horgony (út + opcionális konténer / metódus NÉV szerint), a szöveg és a létrehozás ideje.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// A tárolási formátum verziója.
    #[serde(default = "default_version")]
    pub v: u32,
    /// Rövid, stabil azonosító (8 hexa-jegy).
    pub id: String,
    /// A fájl útja a projekt-gyökérhez képest, `/` elválasztóval.
    pub path: String,
    /// A metódus konténere (osztály, impl-típus …), ha ismert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// A metódus neve (`None` = fájl-jegyzet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// A jegyzet szövege (egy sor).
    pub text: String,
    /// Létrehozás ideje, ISO-8601 UTC, másodpercre (`2026-11-05T20:27:00Z`).
    pub created_at: String,
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

impl Note {
    /// A horgony kiírt alakja: `út`, `út::metódus` vagy `út::Konténer::metódus`.
    pub fn anchor(&self) -> String {
        match (&self.container, &self.method) {
            (Some(c), Some(m)) => format!("{}::{c}::{m}", self.path),
            (None, Some(m)) => format!("{}::{m}", self.path),
            _ => self.path.clone(),
        }
    }

    /// Erre a metódusra (név + konténer) szól-e; konténer nélküli jegyzet bármelyik azonos nevűre illeszkedik.
    pub fn is_for_method(&self, container: Option<&str>, name: &str) -> bool {
        self.method.as_deref() == Some(name)
            && self
                .container
                .as_deref()
                .is_none_or(|c| Some(c) == container)
    }
}

/// `2026-11-05T20:27:13Z` → `2026-11-05 20:27` (UTC); más alakot változatlanul ad vissza.
pub fn short_time(iso: &str) -> String {
    let bytes = iso.as_bytes();
    let ok = bytes.len() >= 16 && iso.is_ascii() && bytes[10] == b'T' && bytes[13] == b':';
    if ok {
        format!("{} {}", &iso[..10], &iso[11..16])
    } else {
        iso.to_owned()
    }
}

/// Legfeljebb `max` karakter (karakterhatáron); a levágott szöveg végén `…`.
pub fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_time_keeps_date_hour_minute() {
        assert_eq!(short_time("2026-11-05T20:27:13Z"), "2026-11-05 20:27");
        assert_eq!(short_time("tegnap"), "tegnap");
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        assert_eq!(truncate("árvíztűrő", 3), "árv…");
        assert_eq!(truncate("rövid", 10), "rövid");
    }

    #[test]
    fn anchor_and_method_match() {
        let note = Note {
            v: 1,
            id: "a".into(),
            path: "src/a.rs".into(),
            container: Some("Shop".into()),
            method: Some("run".into()),
            text: "x".into(),
            created_at: "2026-11-05T20:27:00Z".into(),
        };
        assert_eq!(note.anchor(), "src/a.rs::Shop::run");
        assert!(note.is_for_method(Some("Shop"), "run"));
        assert!(!note.is_for_method(Some("Other"), "run"));
        let loose = Note {
            container: None,
            ..note
        };
        assert!(loose.is_for_method(Some("Other"), "run"));
        assert_eq!(loose.anchor(), "src/a.rs::run");
    }
}
