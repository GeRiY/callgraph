//! `method-index` formázó: a kimenet-korlát egyfájlos tartaléknézete.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct MethodIndex;
impl Formatter for MethodIndex {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
        let mut s = String::new();
        for r in rows {
            writeln!(s, "FILE  {}", r.graph.file).unwrap();
            for c in &r.graph.containers {
                writeln!(s, "{} {}  L{}-{}", c.kind, c.name, c.line, c.end_line).unwrap();
                let mut methods: Vec<_> = r
                    .graph
                    .methods
                    .iter()
                    .filter(|m| r.graph.owns(c, m.container.as_deref(), m.line))
                    .collect();
                methods.sort_by_key(|m| m.line);
                for m in methods {
                    writeln!(
                        s,
                        "  {}()  L{}-{}  [{}]  {} {}",
                        m.name,
                        m.line,
                        m.end_line.unwrap_or(m.line),
                        m.visibility.as_deref().unwrap_or("public"),
                        lang.pick("calls:", "hívás:"),
                        r.graph
                            .calls
                            .iter()
                            .filter(|x| x.container == m.container
                                && x.caller.as_deref() == Some(&m.name))
                            .count()
                    )
                    .unwrap();
                }
            }
        }
        s
    }
}
