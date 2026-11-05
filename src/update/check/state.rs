//! A háttér-ellenőrzés állapota: `<skill>/update-check.json` (felhasználói adat, a csomagban nincs, a frissítés
//! nem írja felül). Hiányzó vagy sérült fájl = üres állapot (azonnal esedékes ellenőrzés).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Az állapotfájl neve a skill-mappában.
pub(crate) const STATE_FILE: &str = "update-check.json";

/// Mikor néztük meg utoljára, mi a legutóbbi kiadás, és mikor / melyik verzióról szóltunk utoljára.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CheckState {
    /// Az utolsó ellenőrzés (indítás) ideje, unix-másodperc; 0 = még soha.
    pub(crate) checked_at: u64,
    /// A legutóbbi stabil kiadás verziója (`x.y.z`); `null`, ha nincs / ismeretlen.
    pub(crate) latest: Option<String>,
    /// Az utolsó értesítés ideje.
    pub(crate) notified_at: Option<u64>,
    /// Melyik verzióról szólt az utolsó értesítés.
    pub(crate) notified_version: Option<String>,
}

impl CheckState {
    /// Betöltés; hiányzó vagy hibás fájlnál az alapállapot.
    pub(crate) fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Mentés atomikusan (ideiglenes fájl + átnevezés), hogy a párhuzamos futások ne lássanak félkész fájlt.
    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        text.push('\n');
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".tmp-{}", std::process::id()));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_missing_or_broken_files() {
        let dir =
            std::env::temp_dir().join(format!("callgraph-check-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(STATE_FILE);
        assert_eq!(CheckState::load(&file), CheckState::default());
        let state = CheckState {
            checked_at: 100,
            latest: Some("0.2.0".into()),
            notified_at: None,
            notified_version: None,
        };
        state.save(&file).unwrap();
        assert_eq!(CheckState::load(&file), state);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"checked_at\": 100") && text.contains("\"notified_at\": null"));
        std::fs::write(&file, "{ broken").unwrap();
        assert_eq!(CheckState::load(&file), CheckState::default());
        std::fs::write(&file, r#"{"latest": "0.3.0"}"#).unwrap();
        assert_eq!(CheckState::load(&file).latest.as_deref(), Some("0.3.0"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
