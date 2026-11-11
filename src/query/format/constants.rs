//! `--const`: konstans-hivatkozások (`const_fetch`) és -definíciók (`const` / `static`) szűrt listája.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;
use super::body_filter::Syntax;
use super::reexports;

pub struct Constants {
    pub filter: String,
}

impl Formatter for Constants {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
        let mut out = String::new();
        let mut found = false;
        for row in rows {
            // (sor, hívó-oszlop, érték): a hivatkozások (`const_fetch`) és a definíciók egy listában, sorrendben
            let mut lines: Vec<(u32, String, String)> = Vec::new();
            let mut matches: Vec<_> = row
                .graph
                .calls
                .iter()
                .filter(|c| {
                    c.kind == "const_fetch"
                        && (c.target.contains(&self.filter)
                            || c.resolved
                                .as_deref()
                                .is_some_and(|value| value.contains(&self.filter)))
                })
                .collect();
            matches.sort_by(|left, right| {
                left.line.cmp(&right.line).then_with(|| {
                    left.resolved
                        .as_deref()
                        .unwrap_or(&left.target)
                        .cmp(right.resolved.as_deref().unwrap_or(&right.target))
                })
            });
            for constant in matches {
                let caller = match (&constant.container, &constant.caller) {
                    (Some(container), Some(caller)) => format!("{container}::{caller}"),
                    (Some(container), None) => format!("{container}::__top__"),
                    (_, Some(caller)) => caller.clone(),
                    _ => "<file>".to_owned(),
                };
                let value = constant.resolved.as_deref().unwrap_or(&constant.target);
                lines.push((constant.line, caller, value.to_owned()));
            }
            for definition in row.graph.constants.iter().filter(|c| {
                matches!(c.kind.as_str(), "const" | "static" | "static mut")
                    && c.name.contains(&self.filter)
            }) {
                let name = match &definition.container {
                    Some(container) => format!("{container}::{}", definition.name),
                    None => definition.name.clone(),
                };
                let value = definition
                    .value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let label = lang.pick("[definition]", "[definíció]");
                lines.push((definition.line, name, format!("{label}{value}")));
            }
            // továbbexportálások (`pub use …`, `export { … } from`, `use const …`): a forrásszövegből
            if let Some(syntax) = Syntax::for_path(&row.graph.file) {
                let source = crate::io::read_source_lossy(std::path::Path::new(&row.graph.file))
                    .unwrap_or_default();
                if source.contains(&self.filter) {
                    let label = lang.pick("[re-export]", "[továbbexportálás]");
                    for found in reexports::find(&source, syntax, &self.filter) {
                        lines.push((found.line, found.name, format!("{label} {}", found.origin)));
                    }
                }
            }
            if lines.is_empty() {
                continue;
            }
            // stabil: azonos soron a hivatkozások (már rendezve) maradnak a definíció előtt
            lines.sort_by_key(|(line, _, _)| *line);
            found = true;
            writeln!(
                out,
                "FILE  {}{}",
                row.graph.file,
                crate::io::protect_mark(&row.graph.file)
            ).unwrap();
            for (line, caller, value) in lines {
                writeln!(out, "  L{line:<5} {caller:<49}{value}").unwrap();
            }
        }
        if !found {
            out.push_str(lang.pick("(no matches)\n", "(nincs találat)\n"));
        }
        out
    }
}
