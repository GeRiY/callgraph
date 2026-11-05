//! A gráf-lekérdezés CLI-parancsa.

use crate::cli::Command;
use crate::error::Result;
use crate::query::{OutputLimit, QueryEngine, SelectorSet, format::FormatterRegistry};
use std::io::Write;
use std::path::PathBuf;
pub struct ShowCommand {
    paths: Vec<PathBuf>,
    project: Option<PathBuf>,
    label: Option<String>,
    format: String,
    full: bool,
    limit: Option<usize>,
    lines: Option<String>,
    strings: Option<String>,
    const_filter: Option<String>,
    body: bool,
    grep: Option<String>,
    category: Option<Vec<String>>,
    depth: usize,
    guess: bool,
}
impl ShowCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        paths: Vec<PathBuf>,
        project: Option<PathBuf>,
        label: Option<String>,
        format: String,
        full: bool,
        limit: Option<usize>,
        lines: Option<String>,
        strings: Option<String>,
        const_filter: Option<String>,
        body: bool,
        grep: Option<String>,
        category: Option<Vec<String>>,
        depth: usize,
        guess: bool,
    ) -> Self {
        Self {
            paths,
            project,
            label,
            format,
            full,
            limit,
            lines,
            strings,
            const_filter,
            body,
            grep,
            category,
            depth,
            guess,
        }
    }
}
impl Command for ShowCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        let selectors = match SelectorSet::parse(&self.paths) {
            Ok(value) => value,
            Err(error) => return Self::report(_err, error),
        };
        // a projekt-szintű lekérdezések a perzisztens projekt-gráfból olvasnak (crate::graph)
        let engine = QueryEngine::new(self.project.as_deref(), self.label.clone())
            .use_project_graph(self.grep.is_some());
        let rows =
            match engine.query_filtered(&selectors, self.grep.as_deref(), self.category.as_deref())
            {
                Ok(value) => value,
                Err(error) => return Self::report(_err, error),
            };
        if self.depth > 1 && self.grep.is_none() {
            return Self::report(
                _err,
                crate::error::CallgraphError::Usage(
                    crate::t!(
                        "--depth is only meaningful together with --grep.",
                        "A --depth csak --grep-pel együtt értelmezett.",
                    )
                    .into(),
                ),
            );
        }
        if let Some(pattern) = self.grep.as_deref() {
            let scope = engine.depth_scope();
            let text = match engine.grep_text(
                &rows, &scope, pattern, self.depth, self.guess, self.full, self.limit,
            ) {
                Ok(value) => value,
                Err(error) => return Self::report(_err, error),
            };
            write!(out, "{}", engine.relativize(&text))?;
            return Ok(0);
        }
        let text = match self.render(&rows) {
            Ok(value) => value,
            Err(error) => return Self::report(_err, error),
        };
        let index = FormatterRegistry::format("method-index", &rows)?;
        let limiter = OutputLimit::from_env(self.full).with_value(self.limit);
        // A callgraph kimenet-korlátja ezekre a formátumokra él: mappán summary, egy fájlon metódus-index.
        let limited = matches!(
            self.format.as_str(),
            "outline" | "flow" | "api" | "mermaid" | "dot"
        ) && !self.body
            && self.lines.is_none()
            && self.strings.is_none()
            && self.const_filter.is_none();
        let text = if !limited {
            text
        } else if rows.len() > 1 {
            limiter.directory_summary(
                text,
                FormatterRegistry::format("summary", &rows)?,
                &self.format,
                rows.len(),
            )
        } else {
            limiter.method_index_for(&self.format, text, index)
        };
        // A gépi formátumok (json, mermaid, dot) útjai abszolútak maradnak; a szövegesekben a gyökér egyszer áll.
        let machine = matches!(self.format.as_str(), "json" | "jsonl" | "mermaid" | "dot")
            && !self.body
            && self.lines.is_none()
            && self.strings.is_none()
            && self.const_filter.is_none();
        if machine {
            write!(out, "{text}")?;
        } else {
            write!(out, "{}", engine.relativize(&text))?;
        }
        Ok(0)
    }
}

impl ShowCommand {
    fn render(&self, rows: &[crate::query::QueryResult]) -> Result<String> {
        if self.body {
            return FormatterRegistry::format("body", rows);
        }
        if let Some(value) = &self.lines {
            let Some((start, end)) = value.split_once('-') else {
                return Err(crate::error::CallgraphError::Usage(lines_error(value)));
            };
            let Some(range) = start
                .parse::<u32>()
                .ok()
                .zip(end.parse::<u32>().ok())
                .filter(|(start, end)| *start > 0 && *end >= *start)
            else {
                return Err(crate::error::CallgraphError::Usage(lines_error(value)));
            };
            return FormatterRegistry::format_with("lines", rows, Some(range), None);
        }
        if let Some(filter) = &self.strings {
            return FormatterRegistry::format_with("strings", rows, None, Some(filter));
        }
        if let Some(filter) = &self.const_filter {
            return FormatterRegistry::format_with("const", rows, None, Some(filter));
        }
        FormatterRegistry::format(&self.format, rows)
    }

    fn report(err: &mut dyn Write, error: crate::error::CallgraphError) -> Result<i32> {
        match error {
            crate::error::CallgraphError::Usage(message) => writeln!(err, "{message}")?,
            other => writeln!(err, "{other}")?,
        }
        Ok(1)
    }
}

fn lines_error(value: &str) -> String {
    // Az [error-id] sort a callgraph-burok (R6g) teszi hozzá, mint a többi hibánál.
    format!("A --lines alakja: <n>-<m> (kaptam: {value})")
}
