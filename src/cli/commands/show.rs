//! A gráf-lekérdezés CLI-parancsa.

use crate::cli::Command;
use crate::error::Result;
use crate::query::format::{BodyMode, FormatterRegistry};
use crate::query::{OutputLimit, QueryEngine, SelectorSet};
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
    body_mode: BodyMode,
    grep: Option<String>,
    category: Option<Vec<String>>,
    depth: usize,
    guess: bool,
    /// `--notes <minta>` (PILOT): a jegyzetek keresése a célok alatt, elemzés helyett.
    notes: Option<String>,
    /// A réteg-kapcsolók (`--layer`, `--layers`, `--by-layer`; `crate::profile`); alapból mind ki.
    layers: crate::cli::LayerArgs,
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
        body_mode: BodyMode,
        grep: Option<String>,
        category: Option<Vec<String>>,
        depth: usize,
        guess: bool,
        notes: Option<String>,
        layers: crate::cli::LayerArgs,
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
            body_mode,
            grep,
            category,
            depth,
            guess,
            notes,
            layers,
        }
    }
}
impl Command for ShowCommand {
    fn run(&self, out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        if let Some(pattern) = self.notes.as_deref() {
            return self.search_notes(pattern, out, _err);
        }
        let selectors = match SelectorSet::parse(&self.paths) {
            Ok(value) => value,
            Err(error) => return Self::report(_err, error),
        };
        // minden lekérdezés a perzisztens projekt-gráfból olvas (crate::graph: a pack a fájlonkénti cache is)
        let engine = QueryEngine::new(self.project.as_deref(), self.label.clone())
            .use_project_graph(true)
            .use_slim_rows(self.slim_rows());
        // réteg-kapcsolók nélkül nincs osztályozó: a kimenet bájtra azonos
        let engine = if self.layers.any() {
            match engine.with_layers(self.layers.layer.as_deref(), self.layers.by_layer) {
                Ok(value) => value,
                Err(error) => return Self::report(_err, error),
            }
        } else {
            engine
        };
        let mut rows =
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
        // `--grep … --body`: a talált hívó metódusok forrása (a szűrt sorok a hívókat tartják meg);
        // találat nélkül a `--grep` szokásos „nincs találat” kimenete marad
        if self.grep.is_some() && self.body && self.depth <= 1 && !rows.is_empty() {
            let mut text = FormatterRegistry::format_body(&rows, self.body_mode);
            let trailer = parse_trailer(&engine, &rows, &text, true);
            text.push_str(&trailer);
            write!(out, "{}", engine.relativize(&text))?;
            return Ok(0);
        }
        if self.layers.by_layer {
            engine.sort_by_layer(&mut rows);
        }
        if let Some(pattern) = self.grep.as_deref() {
            let scope = engine.depth_scope();
            let mut text = match engine.grep_text(
                &rows, &scope, pattern, self.depth, self.guess, self.full, self.limit,
            ) {
                Ok(value) => value,
                Err(error) => return Self::report(_err, error),
            };
            // a beolvasott (nem csak a talált) fájlok hibái: a „nincs találat” se tűnjön biztosnak
            let trailer = parse_trailer(&engine, &rows, &text, true);
            text.push_str(&trailer);
            write!(out, "{}", engine.relativize(&text))?;
            return Ok(0);
        }
        if self.body
            || (self.format == "outline"
                && self.lines.is_none()
                && self.strings.is_none()
                && self.const_filter.is_none())
        {
            // jegyzetek (PILOT) csak az outline-ba és a --body-ba; jegyzet nélkül a kimenet változatlan
            crate::notes::annotate(&mut rows, engine.root());
        }
        let text = match self.render(&rows) {
            Ok(value) => value,
            Err(error) => return Self::report(_err, error),
        };
        // `--layers`: a summary rétegenként, az outline `FILE` soraiban a szerep
        let text = match engine.layer_scope().filter(|_| self.layers.layers) {
            Some(scope) if self.format == "summary" => {
                crate::profile::layered_summary(&rows, scope, crate::i18n::lang())?
            }
            Some(scope) if self.format == "outline" => crate::profile::tag_file_lines(&text, scope),
            _ => text,
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
            let summary = match engine.layer_scope().filter(|_| self.layers.layers) {
                Some(scope) => crate::profile::layered_summary(&rows, scope, crate::i18n::lang())?,
                None => FormatterRegistry::format("summary", &rows)?,
            };
            limiter.directory_summary(text, summary, &self.format, rows.len())
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
            // a json / jsonl a `parseErrors` mezőben jelzi; a mermaid / dot gépi kimenet, nincs záró sor
            write!(out, "{text}")?;
        } else {
            let mut text = text;
            let trailer = parse_trailer(&engine, &rows, &text, false);
            text.push_str(&trailer);
            write!(out, "{}", engine.relativize(&text))?;
        }
        Ok(0)
    }
}

impl ShowCommand {
    fn render(&self, rows: &[crate::query::QueryResult]) -> Result<String> {
        if self.body {
            return Ok(FormatterRegistry::format_body(rows, self.body_mode));
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

    /// `show <cél>... --notes <minta>`: a célok alatti, a mintára illeszkedő jegyzetek (`root:` konvencióval).
    fn search_notes(&self, pattern: &str, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32> {
        let engine = QueryEngine::new(self.project.as_deref(), self.label.clone());
        let root = engine.root();
        let text = crate::notes::NoteFilter::new(root, &self.paths, Some(pattern))
            .and_then(|filter| crate::notes::search_text(root, &filter, crate::i18n::lang()));
        match text {
            Ok(text) => {
                write!(out, "{}", engine.relativize(&text))?;
                Ok(0)
            }
            Err(error) => Self::report(err, error),
        }
    }

    /// A sorok elég-e a szűkített tényekből (`facts.bin`): a `--grep` (a `--grep … --body` kivételével) és a
    /// `--strings` / `--const` csak azok mezőit olvassa; minden más nézet a teljes rekordot kapja.
    fn slim_rows(&self) -> bool {
        if self.grep.is_some() {
            return !(self.body && self.depth <= 1);
        }
        !self.body
            && self.lines.is_none()
            && (self.strings.is_some() || self.const_filter.is_some())
    }

    fn report(err: &mut dyn Write, error: crate::error::CallgraphError) -> Result<i32> {
        match error {
            crate::error::CallgraphError::Usage(message) => writeln!(err, "{message}")?,
            other => writeln!(err, "{other}")?,
        }
        Ok(1)
    }
}

/// A kimenet záró `⚠` sora, ha a lekérdezés beolvasott fájljai közül valamelyik szintaktikai hibás. Egyetlen fájl
/// nézeténél (nem `--grep`) elmarad, ha a fájl `⚠` sora már a kimenetben van (a `FILE` sor alatt); ha a
/// kimenet-korlát vagy a nézet elhagyta (metódus-index, `--body`, `--lines`, üres `flow`), itt pótolja.
fn parse_trailer(
    engine: &QueryEngine,
    rows: &[crate::query::QueryResult],
    text: &str,
    grep: bool,
) -> String {
    let issues = engine.parse_issues();
    if issues.is_empty() {
        return String::new();
    }
    let lang = crate::i18n::lang();
    if let [row] = rows {
        let shown = crate::query::format::parse_error_lines(&row.graph, lang);
        let first = shown.lines().next().unwrap_or_default();
        if !grep && issues.len() == 1 && !first.is_empty() && text.contains(first) {
            return String::new();
        }
    }
    let mut trailer = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        trailer.push('\n');
    }
    trailer.push_str(&crate::query::format::parse_trailer(&issues, lang));
    trailer
}

fn lines_error(value: &str) -> String {
    // Az [error-id] sort a callgraph-burok (R6g) teszi hozzá, mint a többi hibánál.
    format!("A --lines alakja: <n>-<m> (kaptam: {value})")
}
