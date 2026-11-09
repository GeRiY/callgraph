//! A Swift frontend Facade-ja: parsolás (Adapter), a visitor futtatása, figyelmeztetések.

use std::path::{Path, PathBuf};

use super::categorize::Categorizer;
use super::parser::{SwiftParser, TreeSitterSwift};
use super::visitor::SwiftVisitor;
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};
use crate::model::{FileGraphBuilder, MAX_PARSE_ERRORS, ParseError};

/// Swift (és SwiftUI) forrásokat a közös fájlgráfba fordító frontend.
pub struct SwiftFrontend {
    parser: Box<dyn SwiftParser>,
    categorizer: Categorizer,
}

impl SwiftFrontend {
    /// Összeállítás a megadott parserrel és kategorizálóval (DI).
    pub(crate) fn new(parser: Box<dyn SwiftParser>, categorizer: Categorizer) -> Self {
        Self {
            parser,
            categorizer,
        }
    }

    /// Az alapértelmezett összeállítás: tree-sitter-swift parser, alap kategorizáló lánc.
    pub fn with_defaults() -> Self {
        Self::new(Box::new(TreeSitterSwift), Categorizer::default())
    }
}

impl Language for SwiftFrontend {
    fn id(&self) -> &'static str {
        "swift"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["swift"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let mut builder = FileGraphBuilder::new(file.path_string());
        builder.framework(Some("swift".to_string()));
        let mut warnings = Vec::new();
        let Some(parsed) = self.parser.parse(&text) else {
            warnings.push(crate::tf!(
                "{}: Swift parser produced no tree",
                "{}: a Swift-parser nem adott szintaxisfát",
                file.path.display()
            ));
            return Ok(Analysis {
                graph: builder.build(),
                warnings,
            });
        };
        if let Some(line) = parsed.first_error_line() {
            warnings.push(crate::tf!(
                "{}: Swift syntax error near line {line}",
                "{}: Swift szintaktikai hiba a(z) {line}. sor körül",
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
        SwiftVisitor::new(&parsed, &self.categorizer, &stem).run(&mut builder);
        Ok(Analysis {
            graph: builder.build(),
            warnings,
        })
    }
}

impl LanguageFrontend for SwiftFrontend {
    /// A Swift `import` modult (nem fájlt) nevez meg, így fájlútra nem oldható fel.
    fn resolve_module(&self, _spec: &str, _from: &Path) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Analysis {
        SwiftFrontend::with_defaults()
            .analyze(&SourceFile::new(
                "/p/A.swift",
                text.as_bytes().to_vec(),
                None,
            ))
            .unwrap()
    }

    #[test]
    fn identity_and_role() {
        let f = SwiftFrontend::with_defaults();
        assert_eq!((f.id(), f.role()), ("swift", LanguageRole::Frontend));
        assert_eq!(f.extensions(), &["swift"]);
        assert!(
            f.resolve_module("SwiftUI", Path::new("/p/A.swift"))
                .is_none()
        );
    }

    #[test]
    fn clean_file_has_no_warning() {
        let a = run("import Foundation\nstruct A {}\n");
        assert!(a.warnings.is_empty());
        assert_eq!(a.graph.framework.as_deref(), Some("swift"));
        let ui = run("import SwiftUI\n");
        assert_eq!(ui.graph.framework.as_deref(), Some("swiftui"));
    }

    #[test]
    fn syntax_error_is_a_warning_not_an_error() {
        let a = run("struct A {\n  func (\n");
        assert_eq!(a.warnings.len(), 1);
        assert!(!a.graph.parse_errors.is_empty());
    }

    #[test]
    fn syntax_error_keeps_earlier_functions_and_reports_the_line() {
        let a = run("func a() -> Int { 1 }\nfunc b() -> Int { 2 }\nfunc c( {\n");
        let names: Vec<&str> = a.graph.methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"a") && names.contains(&"b"), "{names:?}");
        assert_eq!(a.graph.parse_errors[0].line, 3, "{:?}", a.graph.parse_errors);
        assert!(run("struct A {}\n").graph.parse_errors.is_empty());
    }
}
