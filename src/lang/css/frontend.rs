//! A CSS frontend Facade-ja és a modul-feloldó Strategy-je.

use std::path::{Path, PathBuf};

use raffia::{Parser, Syntax, ast::Stylesheet};

use super::scanner::CssScanner;
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};

/// CSS-forrásokat a közös fájlgráfba fordító frontend.
#[derive(Debug, Default, Clone, Copy)]
pub struct CssFrontend;

impl CssFrontend {
    /// Alapértelmezett CSS frontend, külső függőség nélkül.
    pub const fn new() -> Self {
        Self
    }
}

impl Language for CssFrontend {
    fn id(&self) -> &'static str {
        "css"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["css"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let mut parser = Parser::new(&text, Syntax::Css);
        let parse_error = parser.parse::<Stylesheet>().err().map(|e| format!("{e:?}"));
        let mut result = CssScanner::new(&file.path_string(), &text).scan();
        if let Some(error) = parse_error {
            result.warnings.push(crate::tf!(
                "{}: CSS syntax error: {error}",
                "{}: CSS szintaktikai hiba: {error}",
                file.path.display()
            ));
        }
        Ok(result)
    }
}

impl LanguageFrontend for CssFrontend {
    fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf> {
        let path = Path::new(spec);
        if path.is_absolute() {
            return Some(path.to_path_buf());
        }
        Some(from.parent()?.join(path))
    }
}
