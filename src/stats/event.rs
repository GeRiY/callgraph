//! Egy mért hívás (parancs vagy MCP-eszköz) leírása. Minden mező zárt felsorolás vagy sáv: a típus szerkezetileg
//! nem hordozhat utat, fájl-, szimbólum- vagy projektnevet, keresési kifejezést, hibaüzenetet (és ezek hash-ét sem).

use serde::{Deserialize, Serialize};

use super::buckets::{LatencyBucket, SizeBucket};
use crate::error::CallgraphError;

/// A parancs / eszköz neve a rögzített listából; minden más `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommandKind {
    Show,
    Outline,
    MethodBody,
    FindCallers,
    Search,
    FrameworkView,
    Locate,
    Profile,
    ProjectProfile,
    Refresh,
    Callers,
    Notes,
    Guide,
    Projects,
    Other,
}

impl CommandKind {
    /// Minden változat (a tesztekhez).
    #[cfg(test)]
    pub(crate) const ALL: [Self; 15] = [
        Self::Show,
        Self::Outline,
        Self::MethodBody,
        Self::FindCallers,
        Self::Search,
        Self::FrameworkView,
        Self::Locate,
        Self::Profile,
        Self::ProjectProfile,
        Self::Refresh,
        Self::Callers,
        Self::Notes,
        Self::Guide,
        Self::Projects,
        Self::Other,
    ];

    /// A név a riportban (azonos a napi fájl kulcsával).
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Outline => "outline",
            Self::MethodBody => "method_body",
            Self::FindCallers => "find_callers",
            Self::Search => "search",
            Self::FrameworkView => "framework_view",
            Self::Locate => "locate",
            Self::Profile => "profile",
            Self::ProjectProfile => "project_profile",
            Self::Refresh => "refresh",
            Self::Callers => "callers",
            Self::Notes => "notes",
            Self::Guide => "guide",
            Self::Projects => "projects",
            Self::Other => "other",
        }
    }
}

/// Honnan jött a hívás.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Client {
    /// Parancssor (`callgraph …`, `callgraph.sh …`).
    Cli,
    /// MCP-eszköz (`callgraph mcp`).
    Mcp,
    /// A Claude Code-hook által indított háttér-frissítés.
    Hook,
}

impl Client {
    pub(crate) const ALL: [Self; 3] = [Self::Cli, Self::Mcp, Self::Hook];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Mcp => "mcp",
            Self::Hook => "hook",
        }
    }
}

/// A hiba kategóriája (a szövege soha).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorKind {
    /// Hibás használat (argumentum, paraméter, ismeretlen projekt).
    Usage,
    /// A kért fájl / metódus nincs meg.
    NotFound,
    /// Olvasási / írási hiba.
    Io,
    /// Nem támogatott, generált, bináris vagy kizárt fájl.
    Unsupported,
    /// Egyéb: nem nulla kilépési kód, belső hiba.
    Failed,
}

impl ErrorKind {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 5] = [
        Self::Usage,
        Self::NotFound,
        Self::Io,
        Self::Unsupported,
        Self::Failed,
    ];

    /// A CLI-hiba kategóriája.
    pub(crate) fn of(error: &CallgraphError) -> Self {
        match error {
            CallgraphError::Usage(_) => Self::Usage,
            CallgraphError::Read { .. } | CallgraphError::Write(_) => Self::Io,
            CallgraphError::Unsupported(_)
            | CallgraphError::Generated(_)
            | CallgraphError::Binary(_)
            | CallgraphError::Excluded(_) => Self::Unsupported,
            CallgraphError::Json(_) => Self::Failed,
        }
    }

    /// Egy MCP-eszköz hibaüzenetének kategóriája. A szöveget csak itt nézi (a query-motor „nincs meg” és
    /// „használat” előtagjai mindkét nyelven), nem tárolja.
    pub(crate) fn of_message(message: &str) -> Self {
        const NOT_FOUND: [&str; 4] = [
            "Not found",
            "No such method",
            "Nem található",
            "Nincs ilyen metódus",
        ];
        const USAGE: [&str; 4] = ["usage:", "használat:", "Unknown project", "Ismeretlen projekt"];
        if NOT_FOUND.iter().any(|p| message.starts_with(p)) {
            Self::NotFound
        } else if USAGE.iter().any(|p| message.starts_with(p)) {
            Self::Usage
        } else {
            Self::Failed
        }
    }
}

/// Sikerült-e a hívás.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Ok,
    Error(ErrorKind),
}

/// Egy mért hívás: csak zárt felsorolások és sávok.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Event {
    pub(crate) command: CommandKind,
    pub(crate) client: Client,
    pub(crate) outcome: Outcome,
    pub(crate) latency: LatencyBucket,
    pub(crate) size: SizeBucket,
    /// A kimeneti korlát levágta-e (a „would be N lines” figyelmeztetés).
    pub(crate) truncated: bool,
}

/// Levágta-e a kimeneti korlát a kimenetet (`query::limit`: az első sorok egyike a `⚠ … output would be N
/// lines` / `⚠ A … kimenete N sor lenne` figyelmeztetés).
pub(crate) fn truncated(head: &str) -> bool {
    head.lines().take(3).any(|line| {
        line.starts_with('⚠') && (line.contains(" output would be ") || line.contains(" sor lenne"))
    })
}
