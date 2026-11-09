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

    /// Már beolvasott forrás elemzése. Szintaktikai hibánál (a `syn` nem hibatűrő) a hiba a `warnings`-ba és a
    /// gráf `parse_errors`-ába kerül, a gráf pedig a hiba előtti ép előtagból készül (`recover`); ha az sem
    /// elemezhető, csak a fájl-container marad.
    pub fn analyze_source(&self, path: &str, source: &[u8]) -> Analysis {
        let text = String::from_utf8_lossy(source);
        let mut ctx = RsContext::new(path);
        let mut builder = FileGraphBuilder::new(path);
        let dir = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        builder.framework(detect_framework_name(dir));
        let failed = self.parser.parse(&text, |parsed| match parsed {
            Ok(parsed) => {
                visitor::run(&self.visitors, parsed, &mut ctx, &mut builder);
                None
            }
            Err(error) => Some(error),
        });
        if let Some(error) = failed {
            let line = error_line(&text, error.line);
            let message = error.message;
            ctx.warnings.push(crate::tf!(
                "{path}: syntax error (line {line}): {message}",
                "{path}: szintaktikai hiba ({line}. sor): {message}"
            ));
            builder.parse_error(crate::model::ParseError::new(line, &message));
            if !self.recover(&text, line, &mut ctx, &mut builder) {
                builder.container(file_container(path, &text));
            }
        }
        Analysis {
            graph: builder.build(),
            warnings: ctx.warnings,
        }
    }

    /// Az ép előtag elemzése: a hibás sor elejéig, utána korábbi felső szintű határokig (`core::prefix_cuts`).
    /// Igaz, ha valamelyik előtag elemezhető volt (a visitorok lefutottak).
    fn recover(
        &self,
        text: &str,
        line: u32,
        ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) -> bool {
        for cut in crate::core::prefix_cuts(text, line_offset(text, line)) {
            let done = self.parser.parse_prefix(text, cut, |parsed| match parsed {
                Ok(parsed) => {
                    visitor::run(&self.visitors, parsed, ctx, builder);
                    true
                }
                Err(_) => false,
            });
            if done {
                return true;
            }
        }
        false
    }
}

/// A hiba sora: a `syn` a fájlvégi hibát a hívási helyre (0. sor) vagy a záró üres sorra teheti — ilyenkor az
/// utolsó tartalmas sor.
fn error_line(text: &str, line: u32) -> u32 {
    let last = text
        .lines()
        .rposition(|l| !l.trim().is_empty())
        .map_or(1, |i| i as u32 + 1);
    if line == 0 { last } else { line.min(last) }
}

/// Az 1-alapú `line` sor elejének bájt-eltolása.
fn line_offset(text: &str, line: u32) -> usize {
    text.split_inclusive('\n')
        .take(line.saturating_sub(1) as usize)
        .map(str::len)
        .sum()
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
