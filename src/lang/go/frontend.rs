//! A Go frontend Facade-ja: parsolás (Adapter), a visitor futtatása, figyelmeztetések.

use std::path::{Path, PathBuf};

use super::categorize::Categorizer;
use super::parser::{GoParser, TreeSitterGo};
use super::visitor::GoVisitor;
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};
use crate::model::{FileGraphBuilder, MAX_PARSE_ERRORS, ParseError};

/// Go forrásokat a közös fájlgráfba fordító frontend.
pub struct GoFrontend {
    parser: Box<dyn GoParser>,
    categorizer: Categorizer,
}

impl GoFrontend {
    /// Összeállítás a megadott parserrel és kategorizálóval (DI).
    pub(crate) fn new(parser: Box<dyn GoParser>, categorizer: Categorizer) -> Self {
        Self {
            parser,
            categorizer,
        }
    }

    /// Az alapértelmezett összeállítás: tree-sitter-go parser, alap kategorizáló lánc.
    pub fn with_defaults() -> Self {
        Self::new(Box::new(TreeSitterGo), Categorizer::default())
    }
}

impl Language for GoFrontend {
    fn id(&self) -> &'static str {
        "go"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let mut builder = FileGraphBuilder::new(file.path_string());
        let mut warnings = Vec::new();
        let Some(parsed) = self.parser.parse(&text) else {
            warnings.push(crate::tf!(
                "{}: Go parser produced no tree",
                "{}: a Go-parser nem adott szintaxisfát",
                file.path.display()
            ));
            return Ok(Analysis {
                graph: builder.build(),
                warnings,
            });
        };
        if let Some(line) = parsed.first_error_line() {
            warnings.push(crate::tf!(
                "{}: Go syntax error near line {line}",
                "{}: Go szintaktikai hiba a(z) {line}. sor körül",
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
        GoVisitor::new(&parsed, &self.categorizer, &stem).run(&mut builder);
        Ok(Analysis {
            graph: builder.build(),
            warnings,
        })
    }
}

impl LanguageFrontend for GoFrontend {
    /// A Go `import` csomag-útvonalat nevez meg (a `go.mod` modulútvonalától függ), így itt nem oldható fel.
    fn resolve_module(&self, _spec: &str, _from: &Path) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Analysis {
        GoFrontend::with_defaults()
            .analyze(&SourceFile::new("/p/a.go", text.as_bytes().to_vec(), None))
            .unwrap()
    }

    #[test]
    fn identity_and_role() {
        let f = GoFrontend::with_defaults();
        assert_eq!((f.id(), f.role()), ("go", LanguageRole::Frontend));
        assert_eq!(f.extensions(), &["go"]);
        assert!(f.resolve_module("fmt", Path::new("/p/a.go")).is_none());
    }

    #[test]
    fn clean_file_has_no_warning_and_framework() {
        let a = run("package a\n\nimport \"fmt\"\n\ntype A struct{}\n");
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert_eq!(a.graph.framework.as_deref(), Some("go"));
        let g = run("package a\n\nimport \"github.com/gin-gonic/gin\"\n");
        assert_eq!(g.graph.framework.as_deref(), Some("gin"));
    }

    #[test]
    fn syntax_error_is_a_warning_not_an_error() {
        let a = run("package a\n\nfunc (\n");
        assert_eq!(a.warnings.len(), 1);
        assert!(!a.graph.parse_errors.is_empty());
    }

    #[test]
    fn syntax_error_keeps_earlier_functions_and_reports_the_line() {
        let a = run(
            "package a\n\nfunc a() int {\n\treturn 1\n}\n\nfunc b() int {\n\treturn 2\n}\n\nfunc c( {\n}\n",
        );
        let names: Vec<&str> = a.graph.methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"a") && names.contains(&"b"), "{names:?}");
        assert!(
            a.graph.parse_errors[0].line >= 11,
            "{:?}",
            a.graph.parse_errors
        );
        assert!(run("package a\n\ntype A int\n").graph.parse_errors.is_empty());
    }
}
