//! `Language`: egy nyelv támogatása — felismeri a fájljait és a közös `FileGraph` sémába elemzi őket.

use crate::core::SourceFile;
use crate::error::Result;
use crate::model::FileGraph;

/// A nyelv szerepe a rendszerben.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageRole {
    /// Szerveroldali nyelv (PHP, később Python, Go…).
    Backend,
    /// Kliensoldali nyelv (később JS / TS / Vue).
    Frontend,
    /// Dokumentum- / adatformátum (JSON, Markdown, később YAML, TOML).
    Document,
}

/// Egy elemzés eredménye: a gráf és a nem végzetes figyelmeztetések.
#[derive(Debug)]
pub struct Analysis {
    pub graph: FileGraph,
    pub warnings: Vec<String>,
}

/// Közös felület minden nyelv-modulhoz.
pub trait Language: Send + Sync {
    /// Stabil azonosító: `"php"`, később `"python"`, `"js"`…
    fn id(&self) -> &'static str;

    fn role(&self) -> LanguageRole;

    /// A nyelv fájlkiterjesztései (pont nélkül, kisbetűvel).
    fn extensions(&self) -> &'static [&'static str];

    /// A fájl elemzése a közös sémába.
    fn analyze(&self, file: &SourceFile) -> Result<Analysis>;
}
