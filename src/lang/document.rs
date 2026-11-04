//! `LanguageDocument`: dokumentum- / adatformátum (JSON, Markdown, YAML…).

use std::path::{Path, PathBuf};

use super::Language;

/// Dokumentum- / adatformátum: nincs futó kód, de szerkezet, kulcsok és hivatkozások vannak.
pub trait LanguageDocument: Language {
    /// A `from` fájlból hivatkozott cél (pl. `$ref`, relatív link) fájlútja, ha feloldható.
    fn resolve_reference(&self, reference: &str, from: &Path) -> Option<PathBuf>;
}
