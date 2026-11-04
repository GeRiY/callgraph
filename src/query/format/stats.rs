//! Fájl-statisztikák és híváskategória-számlálók.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Stats;

impl Formatter for Stats {
    fn format(&self, rows: &[QueryResult]) -> String {
        let mut out = String::new();
        for row in rows {
            let graph = &row.graph;
            let kinds = graph
                .containers
                .iter()
                .map(|c| c.kind.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                out,
                "== {}{} ==",
                graph.file,
                graph
                    .namespace
                    .as_ref()
                    .map(|n| format!("  NS:{n}"))
                    .unwrap_or_default()
            )
            .unwrap();
            writeln!(out, "containerek: {} ({kinds})", graph.containers.len()).unwrap();
            writeln!(out, "property-k: {}", graph.properties.len()).unwrap();
            writeln!(out, "konstansok: {}", graph.constants.len()).unwrap();
            writeln!(out, "metódusok: {}", graph.methods.len()).unwrap();
            writeln!(out, "hívások összesen: {}", graph.calls.len()).unwrap();
            if !graph.calls.is_empty() {
                out.push_str("kategória szerint:\n");
                let mut counts = std::collections::BTreeMap::<&str, usize>::new();
                for call in &graph.calls {
                    *counts.entry(&call.category).or_default() += 1;
                }
                let mut counts: Vec<_> = counts.into_iter().collect();
                counts.sort_by(|(left_name, left_count), (right_name, right_count)| {
                    right_count
                        .cmp(left_count)
                        .then_with(|| left_name.cmp(right_name))
                });
                for (category, count) in counts {
                    writeln!(out, "  {count}\t{category}").unwrap();
                }
            }
        }
        out
    }
}
