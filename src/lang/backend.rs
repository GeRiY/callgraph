//! `LanguageBackend`: szerveroldali nyelv.

use std::path::Path;

use super::Language;

/// Szerveroldali nyelv: framework-felismerés (composer.json, pyproject…).
pub trait LanguageBackend: Language {
    /// A könyvtárhoz tartozó framework neve, ha felismerhető.
    fn detect_framework(&self, dir: &Path) -> Option<String>;
}
