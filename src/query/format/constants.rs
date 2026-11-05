//! `--const`: konstansnevek szűrt listája.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Constants {
    pub filter: String,
}

impl Formatter for Constants {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
        let mut out = String::new();
        let mut found = false;
        for row in rows {
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
            if matches.is_empty() {
                continue;
            }
            found = true;
            writeln!(out, "FILE  {}", row.graph.file).unwrap();
            for constant in matches {
                let caller = match (&constant.container, &constant.caller) {
                    (Some(container), Some(caller)) => format!("{container}::{caller}"),
                    (Some(container), None) => format!("{container}::__top__"),
                    (_, Some(caller)) => caller.clone(),
                    _ => "<file>".to_owned(),
                };
                let value = constant.resolved.as_deref().unwrap_or(&constant.target);
                writeln!(out, "  L{:<5} {:<49}{value}", constant.line, caller).unwrap();
            }
        }
        if !found {
            out.push_str(lang.pick("(no matches)\n", "(nincs találat)\n"));
        }
        out
    }
}
