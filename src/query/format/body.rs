//! `--body`: kiválasztott metódusok forrásbeli törzse.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;
use super::source::{numbered, source_lines};

pub struct Body;

impl Formatter for Body {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        let mut out = String::new();
        for row in rows {
            let lines = source_lines(row);
            let methods = row.methods.as_ref().map_or_else(
                || row.graph.methods.iter().collect::<Vec<_>>(),
                |selected| {
                    row.graph
                        .methods
                        .iter()
                        .filter(|m| selected.contains(&m.name))
                        .collect()
                },
            );
            for method in methods {
                let end = method.end_line.unwrap_or(method.line);
                let start = doc_start(&lines, method.line, method.doc.is_some());
                let container = method.container.as_deref().unwrap_or("<file>");
                writeln!(
                    out,
                    "== {}  {container}::{}()  L{}-{end}",
                    row.graph.file, method.name, method.line
                )
                .unwrap();
                out.push_str(&numbered(&lines, start, end));
                out.push('\n');
            }
        }
        out
    }
}

fn doc_start(lines: &[String], method_line: u32, has_doc: bool) -> u32 {
    if !has_doc || method_line <= 1 {
        return method_line;
    }
    let mut cursor = method_line.saturating_sub(1) as usize;
    while cursor > 0
        && lines
            .get(cursor - 1)
            .is_some_and(|line| line.trim().is_empty())
    {
        cursor -= 1;
    }
    while cursor > 0 {
        if lines[cursor - 1].contains("/**") {
            return cursor as u32;
        }
        cursor -= 1;
    }
    method_line
}
