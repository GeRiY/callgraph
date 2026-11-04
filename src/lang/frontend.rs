//! `LanguageFrontend`: kliensoldali nyelv.

use std::path::{Path, PathBuf};

use super::Language;

/// Kliensoldali nyelv: modul-specifikátor és alias (jsconfig / tsconfig) feloldása.
pub trait LanguageFrontend: Language {
    /// A `from` fájlból hivatkozott modul-specifikátor fájlútja, ha feloldható.
    fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf>;
}
