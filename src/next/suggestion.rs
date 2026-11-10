//! Egy javasolt következő lépés: zárt fajta + a futtatható parancs + a rövid címke. A szabályok (`rules.rs`)
//! állítják elő; egy későbbi rangsoroló (2. fázis) csak sorba rendezheti / szűrheti őket, újat nem találhat ki.

use serde::{Deserialize, Serialize};

/// A javaslat fajtája: csak olvasó navigáció (soha nem következtetés, soha nem szerkesztés).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    /// Egy fájl outline-ja.
    Outline,
    /// Egy metódus / függvény törzse (`--body`).
    Body,
    /// Név szerinti hívók (`--grep '(^|::|\.|->)név$'`).
    Callers,
    /// Ugyanaz a hívó-keresés `--depth 2`-vel (hatáselemzés).
    Depth,
    /// Ugyanaz a hívó-keresés rétegenként (`--by-layer`).
    ByLayer,
    /// Egy réteg summary-ja a gyökéren (`--layer X`).
    LayerSummary,
    /// A levágott kimenet szűkítése (amit a figyelmeztetés ajánl).
    Narrow,
    /// A teljes profil (`profile --full`).
    ProfileFull,
}

impl SuggestionKind {
    /// Minden fajta (a rangsoroló és a tesztek számára).
    pub const ALL: [Self; 8] = [
        Self::Outline,
        Self::Body,
        Self::Callers,
        Self::Depth,
        Self::ByLayer,
        Self::LayerSummary,
        Self::Narrow,
        Self::ProfileFull,
    ];

    /// A név a naplóban.
    pub fn name(self) -> &'static str {
        match self {
            Self::Outline => "outline",
            Self::Body => "body",
            Self::Callers => "callers",
            Self::Depth => "depth",
            Self::ByLayer => "by_layer",
            Self::LayerSummary => "layer_summary",
            Self::Narrow => "narrow",
            Self::ProfileFull => "profile_full",
        }
    }
}

/// Egy javaslat: a fajta, a bináris argumentumai (`show --project <gyökér> …` / `profile …`, abszolút utakkal,
/// így bármely munkakönyvtárból ugyanazt futtatja) és a sorban megjelenő címke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub kind: SuggestionKind,
    pub args: Vec<String>,
    pub label: String,
}
