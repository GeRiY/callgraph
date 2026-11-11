//! `flow` formázó: minden if/match/switch ága a sortartományával, és az ágban lévő hívások kategóriánként.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::Formatter;
use super::selection::{pad, selected};
use crate::query::QueryResult;

const EMPTY_EN: &str = "(no if/match/switch branches in the selected part — for the body: --format outline, then a targeted Read)\n";
const EMPTY_HU: &str = "(nincs if/match/switch elágazás a kiválasztott részben — a törzshöz: --format outline, utána célzott Read)\n";

pub struct Flow;

impl Formatter for Flow {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
        let mut out = String::new();
        for row in rows {
            let graph = selected(row);
            if graph.control_flow.is_empty() {
                continue;
            }
            writeln!(
                out,
                "FILE  {}{}",
                graph.file,
                crate::io::protect_mark(&graph.file)
            )
            .unwrap();
            out.push_str(&super::parse_error_lines(&graph, lang));
            out.push('\n');
            for cf in &graph.control_flow {
                writeln!(
                    out,
                    "{} ({})  {}::{}  L{}",
                    cf.kind,
                    cf.cond.as_deref().unwrap_or(""),
                    cf.container.as_deref().unwrap_or(""),
                    cf.caller.as_deref().unwrap_or(""),
                    cf.line
                )
                .unwrap();
                for b in &cf.branches {
                    let fall = if b.fall_through == Some(true) {
                        " [fall-through]"
                    } else {
                        ""
                    };
                    writeln!(out, "  {}  L{}-{}{fall}", b.label, b.start_line, b.end_line).unwrap();
                    let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
                    for call in graph.calls.iter().filter(|c| {
                        c.caller == cf.caller
                            && c.container == cf.container
                            && c.line >= b.start_line
                            && c.line <= b.end_line
                    }) {
                        let target = call.resolved.as_deref().unwrap_or(&call.target);
                        groups.entry(&call.category).or_default().push(target);
                    }
                    let mut groups: Vec<_> = groups.into_iter().collect();
                    groups.sort_by_key(|(category, _)| crate::model::category_rank(category));
                    for (category, mut targets) in groups {
                        targets.sort_unstable();
                        targets.dedup();
                        writeln!(out, "    {}{}", pad(category, 18), targets.join(", ")).unwrap();
                    }
                }
                out.push('\n');
            }
        }
        // A callgraph a kimenet végi üres sorokat levágja ($(cat)), és egy sortöréssel zár.
        let trimmed = out.trim_end_matches('\n');
        if trimmed.is_empty() {
            lang.pick(EMPTY_EN, EMPTY_HU).to_string()
        } else {
            format!("{trimmed}\n")
        }
    }
}
