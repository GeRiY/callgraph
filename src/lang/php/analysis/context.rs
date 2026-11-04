//! Elemzési kontextus: fájl-szintű, a visitorok között megosztott állapot.

/// A visitorok közös, módosítható kontextusa egy fájl elemzése alatt.
#[derive(Debug, Default)]
pub struct AnalysisContext {
    /// Az elemzett fájl útja (ahogy a kimenet `file` mezőjében szerepel).
    pub path: String,
    /// Nem végzetes figyelmeztetések (pl. szintaktikai hibák), a CLI a stderr-re írja.
    pub warnings: Vec<String>,
}

impl AnalysisContext {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            warnings: Vec::new(),
        }
    }
}
