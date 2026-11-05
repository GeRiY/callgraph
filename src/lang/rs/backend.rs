//! `RustBackend`: a Rust nyelv-modul — parser (Adapter) + visitorok + `FileGraphBuilder` csővezeték.

use std::cell::RefCell;
use std::path::Path;

use super::context::RsContext;
use super::framework::FrameworkDetector;
use super::module_path::ModulePath;
use super::parser::{RustParser, SynAdapter};
use super::visitor::{self, FileVisitor};
use crate::core::SourceFile;
use crate::error::Result;
use crate::lang::{Analysis, Language, LanguageBackend, LanguageRole};
use crate::model::{Container, FileGraphBuilder};

thread_local! {
    /// Szálanként cache-elt framework-felismerő (könyvtáranként egy `Cargo.toml`-olvasás).
    static DETECTOR: RefCell<FrameworkDetector> = RefCell::new(FrameworkDetector::new());
}

/// Egy parserből és visitor-listából álló Rust-elemző (a függőségek a konstruktoron át jönnek).
pub struct RustBackend<P: RustParser = SynAdapter> {
    parser: P,
    visitors: Vec<Box<dyn FileVisitor>>,
}

impl<P: RustParser> RustBackend<P> {
    pub(crate) fn new(parser: P, visitors: Vec<Box<dyn FileVisitor>>) -> Self {
        Self { parser, visitors }
    }

    /// Már beolvasott forrás elemzése. Szintaktikai hibánál a gráf csak a fájl-containert tartalmazza,
    /// a hiba a `warnings`-ba kerül.
    pub fn analyze_source(&self, path: &str, source: &[u8]) -> Analysis {
        let text = String::from_utf8_lossy(source);
        let mut ctx = RsContext::new(path);
        let mut builder = FileGraphBuilder::new(path);
        let dir = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        builder.framework(detect_framework_name(dir));
        self.parser.parse(&text, |parsed| match parsed {
            Ok(parsed) => visitor::run(&self.visitors, parsed, &mut ctx, &mut builder),
            Err(error) => {
                let line = error.line;
                let message = error.message;
                ctx.warnings.push(crate::tf!(
                    "{path}: syntax error (line {line}): {message}",
                    "{path}: szintaktikai hiba ({line}. sor): {message}"
                ));
                builder.container(file_container(path, &text));
            }
        });
        Analysis {
            graph: builder.build(),
            warnings: ctx.warnings,
        }
    }
}

/// Hibás fájlnál is legyen egy fájl-szintű container (a sortartománnyal).
fn file_container(path: &str, text: &str) -> Container {
    Container {
        kind: "module".to_owned(),
        name: ModulePath::from_file(path).container_name(),
        line: 1,
        end_line: text.lines().count().max(1) as u32,
        is_abstract: None,
        is_final: None,
        extends: None,
        implements: None,
        traits: None,
        doc: None,
    }
}

impl RustBackend<SynAdapter> {
    /// Az alapértelmezett összeállítás: syn-parser + az összes visitor.
    pub fn with_defaults() -> Self {
        Self::new(SynAdapter, visitor::default_visitors())
    }
}

fn detect_framework_name(dir: &Path) -> Option<String> {
    DETECTOR.with(|d| d.borrow_mut().detect(dir).map(|f| f.name().to_owned()))
}

impl<P: RustParser + Send + Sync> Language for RustBackend<P> {
    fn id(&self) -> &'static str {
        "rust"
    }

    fn role(&self) -> LanguageRole {
        LanguageRole::Backend
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn analyze(&self, file: &SourceFile) -> Result<Analysis> {
        Ok(self.analyze_source(&file.path_string(), &file.text))
    }
}

impl<P: RustParser + Send + Sync> LanguageBackend for RustBackend<P> {
    fn detect_framework(&self, dir: &Path) -> Option<String> {
        detect_framework_name(dir)
    }
}
