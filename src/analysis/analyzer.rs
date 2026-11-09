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

    /// Fájl beolvasása és elemzése a kiterjesztése szerinti nyelvvel. Nem támogatott kiterjesztésre és generált
    /// fájlra (`package-lock.json`, `*.min.js`, `*.min.css`) hiba; bináris és minifikált / bundlolt tartalomra
    /// (`filetype::looks_minified`) üres, `binary` jelölésű gráf (a lekérdezések kihagyják, a közvetlenül megadott
    /// fájlnál hibát adnak: `filetype::skipped_content_error`).
    pub fn analyze_path(&self, path: &Path) -> Result<Analysis> {
        let language = self
            .registry
            .for_path(path)
            .ok_or_else(|| CallgraphError::Unsupported(path.to_path_buf()))?;
        if crate::lang::filetype::is_generated(path) {
            return Err(CallgraphError::Generated(path.to_path_buf()));
        }
        let source = SourceFile::read(path)?;
        if crate::lang::filetype::looks_binary(&source.text)
            || crate::lang::filetype::looks_minified(path, &source.text)
        {
            let mut builder = crate::model::FileGraphBuilder::new(source.path_string());
            builder.binary(true);
            return Ok(Analysis {
                graph: builder.build(),
                warnings: Vec::new(),
            });
        }
        language.analyze(&source)
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
