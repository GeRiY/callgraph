//! A Kotlin frontend Facade-ja: parsolás (Adapter), a visitor futtatása, figyelmeztetések.

use std::path::{Path, PathBuf};

use super::categorize::Categorizer;
use super::parser::{KotlinParser, TreeSitterKotlin};
use super::visitor::KotlinVisitor;
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageFrontend, LanguageRole};
use crate::model::{FileGraphBuilder, MAX_PARSE_ERRORS, ParseError};

/// Kotlin (Android / Jetpack Compose) forrásokat a közös fájlgráfba fordító frontend.
pub struct KotlinFrontend {
    parser: Box<dyn KotlinParser>,
    categorizer: Categorizer,
}

impl KotlinFrontend {
    /// Összeállítás a megadott parserrel és kategorizálóval (DI).
    pub(crate) fn new(parser: Box<dyn KotlinParser>, categorizer: Categorizer) -> Self {
        Self {
            parser,
            categorizer,
        }
    }

    /// Az alapértelmezett összeállítás: tree-sitter-kotlin-ng parser, alap kategorizáló lánc.
    pub fn with_defaults() -> Self {
        Self::new(Box::new(TreeSitterKotlin), Categorizer::default())
    }
}

impl Language for KotlinFrontend {
    fn id(&self) -> &'static str {
        "kotlin"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Frontend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["kt", "kts"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        let text = String::from_utf8_lossy(&file.text);
        let mut builder = FileGraphBuilder::new(file.path_string());
        builder.framework(Some("kotlin".to_string()));
        let mut warnings = Vec::new();
        let Some(parsed) = self.parser.parse(&text) else {
            warnings.push(crate::tf!(
                "{}: Kotlin parser produced no tree",
                "{}: a Kotlin-parser nem adott szintaxisfát",
                file.path.display()
            ));
            return Ok(Analysis {
                graph: builder.build(),
                warnings,
            });
        };
        if let Some(line) = parsed.first_error_line() {
            warnings.push(crate::tf!(
                "{}: Kotlin syntax error near line {line}",
                "{}: Kotlin szintaktikai hiba a(z) {line}. sor körül",
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
        // a `*.gradle.kts` build-szkript: ugyanúgy elemezzük, de `gradle` jelölést kap (nem alkalmazás-kód)
        let gradle = stem.to_ascii_lowercase().ends_with(".gradle.kts");
        KotlinVisitor::new(&parsed, &self.categorizer, &stem, gradle).run(&mut builder);
        Ok(Analysis {
            graph: builder.build(),
            warnings,
        })
    }
}

impl LanguageFrontend for KotlinFrontend {
    /// A Kotlin `import` csomagot / osztályt (nem fájlt) nevez meg, így fájlútra nem oldható fel.
    fn resolve_module(&self, _spec: &str, _from: &Path) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Analysis {
        KotlinFrontend::with_defaults()
            .analyze(&SourceFile::new("/p/A.kt", text.as_bytes().to_vec(), None))
            .unwrap()
    }

    #[test]
    fn identity_and_role() {
        let f = KotlinFrontend::with_defaults();
        assert_eq!((f.id(), f.role()), ("kotlin", LanguageRole::Frontend));
        assert_eq!(f.extensions(), &["kt", "kts"]);
        assert!(
            f.resolve_module("androidx.compose", Path::new("/p/A.kt"))
                .is_none()
        );
    }

    #[test]
    fn clean_file_has_no_warning() {
        let a = run("import kotlin.math.max\nclass A\n");
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert_eq!(a.graph.framework.as_deref(), Some("kotlin"));
        let ui = run("import androidx.compose.runtime.Composable\n");
        assert_eq!(ui.graph.framework.as_deref(), Some("compose"));
        let android = run("import android.content.Context\n");
        assert_eq!(android.graph.framework.as_deref(), Some("android"));
    }

    #[test]
    fn syntax_error_is_a_warning_not_an_error() {
        let a = run("class A {\n  fun (\n");
        assert_eq!(a.warnings.len(), 1);
        assert!(!a.graph.parse_errors.is_empty());
    }

    #[test]
    fn gradle_script_is_marked() {
        let a = KotlinFrontend::with_defaults()
            .analyze(&SourceFile::new(
                "/p/build.gradle.kts",
                b"plugins { id(\"com.android.application\") }\n".to_vec(),
                None,
            ))
            .unwrap();
        assert_eq!(a.graph.framework.as_deref(), Some("gradle"));
    }
}
