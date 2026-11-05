//! `Analyzer` (Facade): fájl → `registry.for_path` → `language.analyze` → `FileGraph`. A CLI csak ezt látja.

use std::path::Path;

use crate::core::SourceFile;
use crate::error::{CallgraphError, Result};
use crate::lang::{Analysis, LanguageRegistry};

/// A nyelv-registryre épülő elemző.
pub struct Analyzer {
    registry: LanguageRegistry,
}

impl Analyzer {
    pub fn new(registry: LanguageRegistry) -> Self {
        Self { registry }
    }

    /// Az alapértelmezett nyelvekkel (most: PHP).
    pub fn with_defaults() -> Self {
        Self::new(LanguageRegistry::with_defaults())
    }

    pub fn registry(&self) -> &LanguageRegistry {
        &self.registry
    }

    /// Fájl beolvasása és elemzése a kiterjesztése szerinti nyelvvel.
    pub fn analyze_path(&self, path: &Path) -> Result<Analysis> {
        let language = self
            .registry
            .for_path(path)
            .ok_or_else(|| CallgraphError::Unsupported(path.to_path_buf()))?;
        language.analyze(&SourceFile::read(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_extension_is_an_error() {
        let err = Analyzer::with_defaults()
            .analyze_path(Path::new("a.unknown"))
            .unwrap_err();
        assert!(matches!(err, CallgraphError::Unsupported(_)));
    }

    #[test]
    fn missing_php_file_is_read_error() {
        let err = Analyzer::with_defaults()
            .analyze_path(Path::new("/nincs/ilyen.php"))
            .unwrap_err();
        assert!(matches!(err, CallgraphError::Read { .. }));
    }
}
