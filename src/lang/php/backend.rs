//! `PhpBackend`: a PHP nyelv-modul — parser (Adapter) + visitorok + `FileGraphBuilder` csővezeték.

use std::cell::RefCell;
use std::path::Path;

use super::analysis::{AnalysisContext, visitor, visitor::FileVisitor};
use super::framework::FrameworkDetector;
use super::parser::{MagoAdapter, PhpParser};
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageBackend, LanguageRole};
use crate::model::FileGraphBuilder;

thread_local! {
    /// Szálanként cache-elt framework-felismerő (a stratégiák `Rc`-k, ezért nem oszthatók meg szálak között).
    static DETECTOR: RefCell<FrameworkDetector> = RefCell::new(FrameworkDetector::new());
}

/// Egy parserből és visitor-listából álló PHP-elemző (a függőségek a konstruktoron át jönnek).
pub struct PhpBackend<P: PhpParser = MagoAdapter> {
    parser: P,
    visitors: Vec<Box<dyn FileVisitor>>,
}

impl<P: PhpParser> PhpBackend<P> {
    pub fn new(parser: P, visitors: Vec<Box<dyn FileVisitor>>) -> Self {
        Self { parser, visitors }
    }

    /// Már beolvasott forrás elemzése.
    pub fn analyze_source(&self, path: &str, source: &[u8]) -> Analysis {
        self.parser.parse(path, source, |parsed| {
            let mut ctx = AnalysisContext::new(path);
            ctx.warnings.extend(
                parsed
                    .errors()
                    .iter()
                    .map(|e| format!("{path}: szintaktikai hiba: {e}")),
            );
            let mut builder = FileGraphBuilder::new(path);
            let dir = Path::new(path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            builder.framework(detect_framework_name(dir));
            for v in &self.visitors {
                v.visit(parsed, &mut ctx, &mut builder);
            }
            Analysis {
                graph: builder.build(),
                warnings: ctx.warnings,
            }
        })
    }
}

impl PhpBackend<MagoAdapter> {
    /// Az alapértelmezett összeállítás: mago-parser + az összes visitor.
    pub fn with_defaults() -> Self {
        Self::new(MagoAdapter, visitor::default_visitors())
    }
}

fn detect_framework_name(dir: &Path) -> Option<String> {
    DETECTOR.with(|d| d.borrow_mut().detect(dir).name().map(str::to_owned))
}

impl<P: PhpParser + Send + Sync> Language for PhpBackend<P> {
    fn id(&self) -> &'static str {
        "php"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Backend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["php"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        Ok(self.analyze_source(&file.path_string(), &file.text))
    }
}

impl<P: PhpParser + Send + Sync> LanguageBackend for PhpBackend<P> {
    fn detect_framework(&self, dir: &Path) -> Option<String> {
        detect_framework_name(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_class_with_method() {
        let src = b"<?php\nnamespace App;\nuse Base\\Ctl;\nclass A extends Ctl {\n  public static function f(int $x): ?int { return 1; }\n}\n";
        let g = PhpBackend::with_defaults()
            .analyze_source("a.php", src)
            .graph;
        assert_eq!(g.namespace.as_deref(), Some("App"));
        assert_eq!(g.containers[0].extends, Some(vec!["Base\\Ctl".into()]));
        assert_eq!((g.containers[0].line, g.containers[0].end_line), (4, 6));
        let m = &g.methods[0];
        assert_eq!((m.name.as_str(), m.line, m.is_static), ("f", 5, true));
        assert_eq!(m.params, vec!["int $x"]);
        assert_eq!(m.return_type.as_deref(), Some("?int"));
    }

    #[test]
    fn language_trait_reports_php_backend() {
        let b = PhpBackend::with_defaults();
        assert_eq!(
            (b.id(), b.role(), b.extensions()),
            ("php", LanguageRole::Backend, &["php"][..])
        );
        let file = SourceFile::new("a.php", b"<?php\nclass A {}\n".to_vec(), None);
        assert_eq!(b.analyze(&file).unwrap().graph.containers[0].line, 2);
    }
}
