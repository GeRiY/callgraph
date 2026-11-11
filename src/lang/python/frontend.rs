//! A Swift frontend Facade-ja: parsolás (Adapter), a visitor futtatása, figyelmeztetések.

use std::path::{Path, PathBuf};

use super::categorize::Categorizer;
use super::parser::{PythonParser, TreeSitterPython};
use super::visitor::PythonVisitor;
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};
use crate::model::{FileGraphBuilder, MAX_PARSE_ERRORS, ParseError};

/// Python forrásokat a közös fájlgráfba fordító frontend.
pub struct PythonFrontend {
    parser: Box<dyn PythonParser>,
    categorizer: Categorizer,
}

impl PythonFrontend {
    /// Összeállítás a megadott parserrel és kategorizálóval (DI).
    pub(crate) fn new(parser: Box<dyn PythonParser>, categorizer: Categorizer) -> Self {
        Self {
            parser,
            categorizer,
        }
    }

    /// Az alapértelmezett összeállítás: tree-sitter-python parser, alap kategorizáló lánc.
    pub fn with_defaults() -> Self {
        Self::new(Box::new(TreeSitterPython), Categorizer::default())
    }
}

impl Language for PythonFrontend {
    fn id(&self) -> &'static str {
        "python"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["python"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let mut builder = FileGraphBuilder::new(file.path_string());
        let mut warnings = Vec::new();
        let Some(parsed) = self.parser.parse(&text) else {
            warnings.push(crate::tf!(
                "{}: Python parser produced no tree",
                "{}: a Python-parser nem adott szintaxisfát",
                file.path.display()
            ));
            return Ok(Analysis {
                graph: builder.build(),
                warnings,
            });
        };
        if let Some(line) = parsed.first_error_line() {
            warnings.push(crate::tf!(
                "{}: Python syntax error near line {line}",
                "{}: Python szintaktikai hiba a(z) {line}. sor körül",
                file.path.display()
            ));
        }
        // a tree-sitter hibatűrő: a gráf a hibák mellett is elkészül, a hibák a gráfba is bekerülnek
        for (line, message) in parsed.errors(MAX_PARSE_ERRORS) {
            builder.parse_error(ParseError::new(line, &message));
        }
        let stem = file
            .path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        PythonVisitor::new(&parsed, &self.categorizer, &stem).run(&mut builder);
        Ok(Analysis {
            graph: builder.build(),
            warnings,
        })
    }
}

impl LanguageFrontend for PythonFrontend {
    /// A Python `import` modult nevez meg (a csomag-útvonal a gyökértől függ), így itt nem oldható fel.
    fn resolve_module(&self, _spec: &str, _from: &Path) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Analysis {
        PythonFrontend::with_defaults()
            .analyze(&SourceFile::new(
                "/p/a.py",
                text.as_bytes().to_vec(),
                None,
            ))
            .unwrap()
    }

    #[test]
    fn identity_and_role() {
        let f = PythonFrontend::with_defaults();
        assert_eq!((f.id(), f.role()), ("python", LanguageRole::Frontend));
        assert_eq!(f.extensions(), &["python"]);
        assert!(
            f.resolve_module("SwiftUI", Path::new("/p/a.py"))
                .is_none()
        );
    }

    #[test]
    fn clean_file_has_no_warning_and_framework() {
        let a = run("import os\nclass A:\n    pass\n");
        assert!(a.warnings.is_empty());
        assert_eq!(a.graph.framework.as_deref(), Some("python"));
        let d = run("from django.db import models\n");
        assert_eq!(d.graph.framework.as_deref(), Some("django"));
    }

    #[test]
    fn syntax_error_is_a_warning_not_an_error() {
        let a = run("class A:\n    def (\n");
        assert_eq!(a.warnings.len(), 1);
        assert!(!a.graph.parse_errors.is_empty());
    }

    #[test]
    fn syntax_error_keeps_earlier_functions_and_reports_the_line() {
        let a = run("def a():\n    return 1\n\ndef b():\n    return 2\n\ndef c(:\n    pass\n");
        let names: Vec<&str> = a.graph.methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"a") && names.contains(&"b"), "{names:?}");
        assert_eq!(
            a.graph.parse_errors[0].line, 7,
            "{:?}",
            a.graph.parse_errors
        );
        assert!(run("class A:\n    pass\n").graph.parse_errors.is_empty());
    }
}
