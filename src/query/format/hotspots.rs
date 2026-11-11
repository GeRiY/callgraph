//! `hotspots` formázó: metódusonként be-/kifokszám és ciklomatikus szám (a vezérlési ágak összege), a top 20.
//! Csak a lekérdezett fájlokban definiált metódusok számítanak (framework/native cél nem).

use std::collections::{BTreeMap, HashSet};

use super::Formatter;
use super::selection::selected;
use crate::query::QueryResult;

#[derive(Default)]
struct Degree {
    incoming: usize,
    outgoing: usize,
    cyclomatic: usize,
}

fn symbol(ns: &str, container: Option<&str>, name: &str) -> String {
    format!("{ns}{}::{name}", container.unwrap_or("__top__"))
}

pub struct Hotspots;

impl Formatter for Hotspots {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        let mut own = HashSet::new();
        let mut degrees: BTreeMap<String, Degree> = BTreeMap::new();
        let mut cyclomatic: BTreeMap<String, usize> = BTreeMap::new();
        for row in rows {
            let graph = selected(row);
            let ns = graph
                .namespace
                .as_ref()
                .map(|n| format!("{n}\\"))
                .unwrap_or_default();
            for m in &graph.methods {
                own.insert(symbol(&ns, m.container.as_deref(), &m.name));
            }
            for call in graph.calls.iter().filter(|call| !call.is_dynamic()) {
                let caller = call.caller.as_deref().unwrap_or("");
                let id = symbol(&ns, call.container.as_deref(), caller);
                degrees.entry(id).or_default().outgoing += 1;
                if let Some(target) = &call.resolved {
                    degrees.entry(target.clone()).or_default().incoming += 1;
                }
            }
            for cf in &graph.control_flow {
                let caller = cf.caller.as_deref().unwrap_or("");
                let id = symbol(&ns, cf.container.as_deref(), caller);
                *cyclomatic.entry(id).or_default() += cf.branches.len();
            }
        }
        // A BTreeMap kódpont szerint rendezett (mint a jq group_by), a stabil rendezés ezt a sorrendet tartja holtversenynél.
        let mut ranked: Vec<(String, Degree)> = degrees
            .into_iter()
            .filter(|(id, _)| own.contains(id))
            .map(|(id, mut d)| {
                d.cyclomatic = cyclomatic.get(&id).copied().unwrap_or(0);
                (id, d)
            })
            .collect();
        ranked.sort_by_key(|(_, d)| std::cmp::Reverse(d.incoming + d.outgoing + d.cyclomatic));
        ranked
            .into_iter()
            .take(20)
            .map(|(id, d)| {
                format!(
                    "{id}\tin:{}\tout:{}\tcc:{}\n",
                    d.incoming, d.outgoing, d.cyclomatic
                )
            })
            .collect()
    }
}
