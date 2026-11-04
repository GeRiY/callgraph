//! `mermaid` és `dot` formázók: öröklési + hívási élek, bájtsorrendben, ismétlés nélkül (a callgraph `sort -u`-ja).

use std::collections::BTreeSet;

use super::Formatter;
use super::selection::{sanitize, selected};
use crate::model::{Call, Container};
use crate::query::QueryResult;

fn target(call: &Call) -> &str {
    call.resolved.as_deref().unwrap_or(&call.target)
}

fn caller(call: &Call) -> &str {
    call.caller.as_deref().unwrap_or("")
}

fn inherits(c: &Container) -> impl Iterator<Item = (&'static str, &String)> {
    let extends = c.extends.iter().flatten().map(|p| ("extends", p));
    let implements = c.implements.iter().flatten().map(|p| ("implements", p));
    extends.chain(implements)
}

pub struct Mermaid;

impl Formatter for Mermaid {
    fn format(&self, rows: &[QueryResult]) -> String {
        let mut lines = BTreeSet::new();
        for row in rows {
            let graph = selected(row);
            for c in &graph.containers {
                for (label, parent) in inherits(c) {
                    lines.insert(format!(
                        "  {} -.->|{label}| {}",
                        sanitize(&c.name),
                        sanitize(parent)
                    ));
                }
            }
            for call in &graph.calls {
                lines.insert(format!(
                    "  {} -->|{}| {}",
                    sanitize(caller(call)),
                    call.category,
                    sanitize(target(call))
                ));
            }
        }
        let mut out = String::from("flowchart LR\n");
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

pub struct Dot;

impl Formatter for Dot {
    fn format(&self, rows: &[QueryResult]) -> String {
        let mut lines = BTreeSet::new();
        for row in rows {
            let graph = selected(row);
            for c in &graph.containers {
                for (label, parent) in inherits(c) {
                    lines.insert(format!(
                        "  \"{}\" -> \"{parent}\" [label=\"{label}\",style=dashed];",
                        c.name
                    ));
                }
            }
            for call in &graph.calls {
                lines.insert(format!(
                    "  \"{}\" -> \"{}\" [label=\"{}\"];",
                    caller(call),
                    target(call),
                    call.category
                ));
            }
        }
        let mut out = String::from("digraph callgraph {\n  rankdir=LR;\n");
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("}\n");
        out
    }
}
