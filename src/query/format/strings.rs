//! `--strings`: string-literálok szűrt listája.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Strings {
    pub filter: String,
}

/// Egy találat a régi `sort -k1,1 -k2,2n` kulcsaival: fájl, sor, hívó, érték.
struct Hit {
    file: String,
    line: u32,
    caller: String,
    value: String,
}

impl Hit {
    /// A régi szűrő soralakja (`\t`-elválasztott); a `sort` holtverseny-feloldása erre megy.
    fn raw_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.file, self.line, self.caller, self.value
        )
    }
}

impl Strings {
    fn collect(&self, rows: &[QueryResult]) -> Vec<Hit> {
        let needle = self.filter.to_lowercase();
        let mut hits = Vec::new();
        for row in rows {
            for s in row
                .graph
                .strings
                .iter()
                .filter(|s| s.value.to_lowercase().contains(&needle))
            {
                let caller = match (&s.container, &s.caller) {
                    (Some(c), Some(m)) => format!("{c}::{m}"),
                    (_, Some(m)) => m.clone(),
                    _ => "<file>".to_owned(),
                };
                let value: String = s.value.chars().take(160).collect();
                hits.push(Hit {
                    file: row.graph.file.clone(),
                    line: s.line,
                    caller: flatten(&caller),
                    value: format!("\"{}\"", flatten(&value)),
                });
            }
        }
        hits.sort_by(|a, b| {
            (&a.file, a.line)
                .cmp(&(&b.file, b.line))
                .then_with(|| a.raw_line().cmp(&b.raw_line()))
        });
        hits
    }
}

fn flatten(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

impl Formatter for Strings {
    fn format(&self, rows: &[QueryResult]) -> String {
        let hits = self.collect(rows);
        if hits.is_empty() {
            return "(nincs találat)\n".to_owned();
        }
        let mut out = String::new();
        let mut prev: Option<&str> = None;
        for hit in &hits {
            if prev != Some(hit.file.as_str()) {
                writeln!(out, "FILE  {}", hit.file).unwrap();
                prev = Some(hit.file.as_str());
            }
            writeln!(out, "  L{:<5} {:<48} {}", hit.line, hit.caller, hit.value).unwrap();
        }
        out
    }
}
