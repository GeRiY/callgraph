use std::path::{Path, PathBuf};

use super::scanner::ScssScanner;
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};

/// SCSS-forrásokat elemző frontend.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScssFrontend;

impl ScssFrontend {
    pub const fn new() -> Self {
        Self
    }
}

impl Language for ScssFrontend {
    fn id(&self) -> &'static str {
        "scss"
    }
    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["scss"]
    }
    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let mut parser = Parser::new(&text, Syntax::Scss);
        let error = parser.parse::<Stylesheet>().err().map(|e| format!("{e:?}"));
        let mut result = ScssScanner::new(&file.path_string(), &text).scan();
        if let Some(error) = error {
            result.warnings.push(format!(
                "{}: SCSS szintaktikai hiba: {error}",
                file.path.display()
            ));
        }
        Ok(result)
    }
}

impl LanguageFrontend for ScssFrontend {
    fn resolve_module(&self, spec: &str, from: &Path) -> Option<PathBuf> {
        let path = Path::new(spec);
        if path.is_absolute() {
            Some(path.to_path_buf())
        } else {
            Some(from.parent()?.join(path))
        }
    }
}
use raffia::{Parser, Syntax, ast::Stylesheet};
