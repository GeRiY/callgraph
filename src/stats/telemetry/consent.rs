//! A beküldés hozzájárulási állapota: `<skill>/stats/telemetry.json` (egyetlen kis objektum, mint az
//! `update-check.json`). Alapból (nincs fájl / nincs döntés) KI.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A felhasználó döntése.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Choice {
    /// Igen (be).
    Yes,
    /// Most nem: egyszer még megkérdezzük (~14 nap múlva, a `stats` kimenetében).
    Later,
    /// Soha: végleges, többé nem kérdezünk és nem szólunk.
    Never,
    /// Kikapcsolta (`telemetry off`).
    Off,
}

/// Az állapotfájl.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Consent {
    /// Be van-e kapcsolva a beküldés (csak `Choice::Yes` mellett igaz).
    pub enabled: bool,
    /// A döntés; `None` = még nem döntött (KI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice: Option<Choice>,
    /// A döntés ideje (Unix-másodperc).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<u64>,
    /// Hányszor kérdeztük meg (telepítéskor / frissítéskor, és a „most nem” utáni egy emlékeztető).
    pub asked_count: u32,
    /// Az utolsó sikeresen beküldött nap (`YYYY-MM-DD`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sent_day: Option<String>,
    /// Az utolsó sikeres beküldés ideje.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_send: Option<u64>,
    /// Az utolsó beküldési kísérlet ideje (sikertelen szervernél se induljon minden futásnál).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attempt: Option<u64>,
    /// Hányszor kérte meg az agentet, hogy szóljon a felhasználónak (legfeljebb 3).
    pub hint_count: u32,
    /// Az utolsó ilyen sor ideje.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint_at: Option<u64>,
}

impl Consent {
    /// Betöltés; hiányzó vagy sérült fájlnál az alapállapot (nem döntött, KI).
    pub(crate) fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Mentés atomikusan.
    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = serde_json::to_string(self).map_err(std::io::Error::other)?;
        text.push('\n');
        crate::stats::store::write_atomic(path, &text)
    }

    /// Döntés rögzítése.
    pub(crate) fn decide(&mut self, choice: Choice, now: u64) {
        self.choice = Some(choice);
        self.enabled = choice == Choice::Yes;
        self.decided_at = Some(now);
    }

    /// Még nem döntött, vagy „most nem”-et mondott (csak ilyenkor szabad kérdezni / szólni).
    pub(crate) fn open(&self) -> bool {
        matches!(self.choice, None | Some(Choice::Later))
    }
}
