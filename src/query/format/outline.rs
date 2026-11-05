//! `outline` formázó (a callgraph alapnézete).

use crate::model::{FileGraph, Method};
use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Outline;
impl Formatter for Outline {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
        let mut text = rows
            .iter()
            .map(|row| outline(row, lang))
            .collect::<Vec<_>>()
            .join("\n---\n");
        text.push_str(lang.pick(
            "\n---\n💡 for the body logic (conditions, string literals, exact order): Read the L<n>-<m> line range above (offset=<n>, limit=<m-n+1>) — not the whole file.\n",
            "\n---\n💡 a törzs-logikához (feltételek, string-literálok, pontos sorrend): Read a fenti L<n>-<m> sortartományra (offset=<n>, limit=<m-n+1>) — ne a teljes fájlt.\n",
        ));
        text
    }
}
fn outline(row: &QueryResult, lang: crate::i18n::Lang) -> String {
    let g = &row.graph;
    let mut s = String::new();
    writeln!(
        s,
        "FILE  {}{}",
        g.file,
        g.namespace
            .as_ref()
            .map(|n| format!("  NS:{n}"))
            .unwrap_or_default()
    )
    .unwrap();
    writeln!(s).unwrap();
    if g.containers.is_empty() {
        super::outline_procedural::procedural(g, &mut s, lang);
        return s;
    }
    for (i, c) in g.containers.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        let extends = c
            .extends
            .as_ref()
            .filter(|e| !e.is_empty())
            .map(|e| format!(" extends {}", e.join(", ")))
            .unwrap_or_default();
        let implements = c
            .implements
            .as_ref()
            .filter(|e| !e.is_empty())
            .map(|e| format!(" implements {}", e.join(", ")))
            .unwrap_or_default();
        let traits = c
            .traits
            .as_ref()
            .filter(|e| !e.is_empty())
            .map(|e| format!(" uses {}", e.join(", ")))
            .unwrap_or_default();
        writeln!(
            s,
            "{} {}{}{}{}  L{}-{}",
            c.kind, c.name, extends, implements, traits, c.line, c.end_line
        )
        .unwrap();
        let cons: Vec<_> = g
            .constants
            .iter()
            .filter(|x| x.container.as_deref() == Some(&c.name))
            .collect();
        if !cons.is_empty() {
            writeln!(s, "  constants ({}):", cons.len()).unwrap();
            // jq: name + (value → " = value") + (enum_case → " [enum case]")
            for x in cons {
                let value = x
                    .value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                let case = if x.kind == "enum_case" {
                    " [enum case]"
                } else {
                    ""
                };
                writeln!(s, "    {}{value}{case}", x.name).unwrap();
            }
        }
        let props: Vec<_> = g
            .properties
            .iter()
            .filter(|p| p.container.as_deref() == Some(&c.name))
            .collect();
        if !props.is_empty() {
            writeln!(s, "  properties ({}):", props.len()).unwrap();
            for p in props {
                writeln!(
                    s,
                    "    {} ${}{}{}",
                    p.type_hint.clone().unwrap_or_else(|| "mixed".to_owned()),
                    p.name.trim_start_matches('$'),
                    if p.readonly { " [readonly]" } else { "" },
                    if p.is_static { " [static]" } else { "" }
                )
                .unwrap();
            }
        }
        let mut methods: Vec<_> = g
            .methods
            .iter()
            .filter(|m| m.container.as_deref() == Some(&c.name))
            .filter(|m| row.methods.as_ref().is_none_or(|ms| ms.contains(&m.name)))
            .collect();
        methods.sort_by_key(|m| m.line);
        for m in methods {
            method_line(&mut s, m);
            for cat in categories(g, m) {
                writeln!(s, "    {:<18}{}", cat.0, cat.1).unwrap();
            }
            for comment in g
                .comments
                .iter()
                .filter(|comment| comment.caller.as_deref() == Some(&m.name))
                .filter(|comment| comment.container.as_deref() == m.container.as_deref())
            {
                match &comment.tag {
                    Some(tag) => writeln!(s, "    💬 [{tag}] {}", comment.text),
                    None => writeln!(s, "    💬 {}", comment.text),
                }
                .unwrap();
            }
        }
    }
    s
}
pub(super) fn method_line(s: &mut String, m: &Method) {
    writeln!(
        s,
        "  {}()  L{}-{}{}{}",
        m.name,
        m.line,
        m.end_line.unwrap_or(m.line),
        if m.is_static { " [static]" } else { "" },
        if m.is_abstract { " [abstract]" } else { "" }
    )
    .unwrap();
}
pub(super) fn categories(g: &FileGraph, m: &Method) -> Vec<(String, String)> {
    let mut grouped: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for c in g
        .calls
        .iter()
        .filter(|c| c.caller.as_deref() == Some(&m.name))
        .filter(|c| c.container.as_deref() == m.container.as_deref())
    {
        grouped
            .entry(c.category.clone())
            .or_default()
            .push(c.resolved.clone().unwrap_or_else(|| c.target.clone()));
    }
    grouped
        .into_iter()
        .map(|(category, targets)| {
            let mut unique = Vec::new();
            for target in targets {
                if !unique.contains(&target) {
                    unique.push(target);
                }
            }
            unique.sort();
            (category, unique.join(", "))
        })
        .collect()
}
