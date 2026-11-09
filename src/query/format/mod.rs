//! Formatter Strategy-k és a név szerinti `FormatterRegistry` (Factory).
//! Új formátum: egy fájl ebben a mappában (`impl Formatter`), egy `mod` sor és egy `register` sor a `with_defaults`-ban.

mod api;
mod body;
mod body_filter;
mod constants;
mod coverage;
mod flow;
mod framework;
mod graph_text;
mod hotspots;
mod json;
mod lines;
mod method_index;
mod outline;
mod outline_procedural;
mod parse_errors;
mod reexports;
mod selection;
mod source;
mod stats;
mod strings;
mod summary;

#[cfg(test)]
mod tests;

pub use body::BodyMode;
pub(crate) use outline::{call_lines, grep_outline, method_calls};
pub(crate) use parse_errors::{file_lines as parse_error_lines, issue as parse_issue};
pub use parse_errors::trailer as parse_trailer;

use crate::error::{CallgraphError, Result};
use crate::i18n::Lang;
use crate::query::QueryResult;

/// Egy kimeneti formátum; a szövegeit a kapott nyelven írja.
pub trait Formatter: Send + Sync {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String;
}

/// A callgraph hibaüzenetében felsorolt formátumok (paritás miatt fix szöveg).
const KNOWN: &str = "outline|api|stats|mermaid|dot|json|hotspots|flow|summary|coverage|events|routes|middleware|validation|jobs|providers|magic|di|views";

pub struct FormatterRegistry {
    formatters: Vec<(&'static str, Box<dyn Formatter>)>,
    lang: Lang,
}

/// Üres registry a futás nyelvével.
impl Default for FormatterRegistry {
    fn default() -> Self {
        Self {
            formatters: Vec::new(),
            lang: crate::i18n::lang(),
        }
    }
}

impl FormatterRegistry {
    pub fn with_defaults() -> Self {
        let mut registry = Self::default();
        registry.register("outline", Box::new(outline::Outline));
        // a `--grep` nézete (nem a --format listában): outline + a hívások helye és forrássora
        registry.register("grep-outline", Box::new(outline::GrepOutline));
        registry.register("api", Box::new(api::Api));
        registry.register("stats", Box::new(stats::Stats));
        registry.register("summary", Box::new(summary::Summary));
        registry.register("json", Box::new(json::Json));
        registry.register("jsonl", Box::new(json::Jsonl));
        registry.register("method-index", Box::new(method_index::MethodIndex));
        registry.register("body", Box::new(body::Body::default()));
        registry.register("flow", Box::new(flow::Flow));
        registry.register("hotspots", Box::new(hotspots::Hotspots));
        registry.register("coverage", Box::new(coverage::Coverage));
        registry.register("mermaid", Box::new(graph_text::Mermaid));
        registry.register("dot", Box::new(graph_text::Dot));
        framework::register(&mut registry);
        registry
    }

    /// A kimenet nyelve (alapból a futásé).
    pub fn in_lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    pub fn register(&mut self, name: &'static str, formatter: Box<dyn Formatter>) {
        self.formatters.push((name, formatter));
    }

    pub fn has(&self, name: &str) -> bool {
        self.formatters.iter().any(|(n, _)| *n == name)
    }

    pub fn render(&self, name: &str, rows: &[QueryResult]) -> Result<String> {
        match self.formatters.iter().find(|(n, _)| *n == name) {
            Some((_, formatter)) => Ok(formatter.format(rows, self.lang)),
            None => Err(CallgraphError::Usage(ltf!(
                self.lang,
                "Unknown format: {name} ({KNOWN})",
                "Ismeretlen formátum: {name} ({KNOWN})"
            ))),
        }
    }

    /// Mint a `render`, plusz a paraméteres formátumok (`lines` tartománnyal, `strings` / `const` szűrővel).
    pub fn render_with(
        &self,
        name: &str,
        rows: &[QueryResult],
        range: Option<(u32, u32)>,
        filter: Option<&str>,
    ) -> Result<String> {
        let formatter: Box<dyn Formatter> = match name {
            "lines" => Box::new(lines::Lines {
                range: range.unwrap_or((1, 1)),
            }),
            "strings" => Box::new(strings::Strings {
                filter: filter.unwrap_or_default().to_owned(),
            }),
            "const" => Box::new(constants::Constants {
                filter: filter.unwrap_or_default().to_owned(),
            }),
            _ => return self.render(name, rows),
        };
        Ok(formatter.format(rows, self.lang))
    }

    /// `--body` a megadott nézettel (tömör / docblockkal / nyers).
    pub fn format_body(rows: &[QueryResult], mode: BodyMode) -> String {
        body::Body { mode }.format(rows, crate::i18n::lang())
    }

    /// Rövidítés az alapértelmezett összeállításra.
    pub fn format(name: &str, rows: &[QueryResult]) -> Result<String> {
        Self::with_defaults().render(name, rows)
    }

    /// Rövidítés az alapértelmezett összeállításra, a paraméteres formátumokkal együtt.
    pub fn format_with(
        name: &str,
        rows: &[QueryResult],
        range: Option<(u32, u32)>,
        filter: Option<&str>,
    ) -> Result<String> {
        Self::with_defaults().render_with(name, rows, range, filter)
    }
}
