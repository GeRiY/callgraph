//! Formatter Strategy-k és a név szerinti `FormatterRegistry` (Factory).
//! Új formátum: egy fájl ebben a mappában (`impl Formatter`), egy `mod` sor és egy `register` sor a `with_defaults`-ban.

mod api;
mod body;
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
mod selection;
mod source;
mod stats;
mod strings;
mod summary;

use crate::error::{Result, CallgraphError};
use crate::query::QueryResult;

pub trait Formatter: Send + Sync {
    fn format(&self, rows: &[QueryResult]) -> String;
}

/// A callgraph hibaüzenetében felsorolt formátumok (paritás miatt fix szöveg).
const KNOWN: &str = "outline|api|stats|mermaid|dot|json|hotspots|flow|summary|coverage|events|routes|middleware|validation|jobs|providers|magic|di|views";

#[derive(Default)]
pub struct FormatterRegistry {
    formatters: Vec<(&'static str, Box<dyn Formatter>)>,
}

impl FormatterRegistry {
    pub fn with_defaults() -> Self {
        let mut registry = Self::default();
        registry.register("outline", Box::new(outline::Outline));
        registry.register("api", Box::new(api::Api));
        registry.register("stats", Box::new(stats::Stats));
        registry.register("summary", Box::new(summary::Summary));
        registry.register("json", Box::new(json::Json));
        registry.register("jsonl", Box::new(json::Jsonl));
        registry.register("method-index", Box::new(method_index::MethodIndex));
        registry.register("body", Box::new(body::Body));
        registry.register("flow", Box::new(flow::Flow));
        registry.register("hotspots", Box::new(hotspots::Hotspots));
        registry.register("coverage", Box::new(coverage::Coverage));
        registry.register("mermaid", Box::new(graph_text::Mermaid));
        registry.register("dot", Box::new(graph_text::Dot));
        framework::register(&mut registry);
        registry
    }

    pub fn register(&mut self, name: &'static str, formatter: Box<dyn Formatter>) {
        self.formatters.push((name, formatter));
    }

    pub fn has(&self, name: &str) -> bool {
        self.formatters.iter().any(|(n, _)| *n == name)
    }

    pub fn render(&self, name: &str, rows: &[QueryResult]) -> Result<String> {
        match self.formatters.iter().find(|(n, _)| *n == name) {
            Some((_, formatter)) => Ok(formatter.format(rows)),
            None => Err(CallgraphError::Usage(format!(
                "Ismeretlen formátum: {name} ({KNOWN})"
            ))),
        }
    }

    /// Rövidítés az alapértelmezett összeállításra.
    pub fn format(name: &str, rows: &[QueryResult]) -> Result<String> {
        Self::with_defaults().render(name, rows)
    }

    pub fn format_with(
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
            _ => return Self::format(name, rows),
        };
        Ok(formatter.format(rows))
    }
}
