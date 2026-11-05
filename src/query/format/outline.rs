//! `outline` formázó (a callgraph alapnézete).

use crate::i18n::Lang;
use crate::model::{Call, FileGraph, Method};
use crate::query::QueryResult;
use crate::query::snippet::{self, SourceLines};
use std::fmt::Write as _;

use super::Formatter;

pub struct Outline;
impl Formatter for Outline {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        render(rows, lang, None)
    }
}

/// A `--grep` outline: mint az `outline`, plusz metódusonként a találat-hívások helye és forrássora
/// (`L25 │ $this->run($x)`, interfészen át illesztettnél `L25 via Resolver │ …`).
pub struct GrepOutline;
impl Formatter for GrepOutline {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        grep_outline(rows, lang, &SourceLines::default())
    }
}

/// A `--grep` outline a megadott forrássor-tárral (a projekt-gráf tárolt hívás-soraival előtöltve is lehet).
pub(crate) fn grep_outline(rows: &[QueryResult], lang: Lang, source: &SourceLines) -> String {
    render(rows, lang, Some(source))
}

fn render(rows: &[QueryResult], lang: Lang, source: Option<&SourceLines>) -> String {
    let mut text = rows
        .iter()
        .map(|row| outline(row, lang, source))
        .collect::<Vec<_>>()
        .join("\n---\n");
    text.push_str(lang.pick(
        "\n---\n💡 for the body logic (conditions, string literals, exact order): Read the L<n>-<m> line range above (offset=<n>, limit=<m-n+1>) — not the whole file.\n",
        "\n---\n💡 a törzs-logikához (feltételek, string-literálok, pontos sorrend): Read a fenti L<n>-<m> sortartományra (offset=<n>, limit=<m-n+1>) — ne a teljes fájlt.\n",
    ));
    text
}
fn outline(row: &QueryResult, lang: Lang, source: Option<&SourceLines>) -> String {
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
        super::outline_procedural::procedural(g, &mut s, lang, source.map(|lines| (row, lines)));
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
            if let Some(lines) = source {
                call_lines(&mut s, row, &method_calls(g, m), lines, lang, "    ");
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
/// A metódus hívásai (a `categories` szűrőjével).
pub(crate) fn method_calls<'g>(g: &'g FileGraph, m: &Method) -> Vec<&'g Call> {
    g.calls
        .iter()
        .filter(|c| c.caller.as_deref() == Some(&m.name))
        .filter(|c| c.container.as_deref() == m.container.as_deref())
        .collect()
}

/// A hívások helye soronként egyszer (`L25 [via X] │ kód`); az azonos (és azonos `via`-jú) sorok egy sorba vonva
/// (`L12, L40 │ kód`), legfeljebb `snippet::MAX_PER_METHOD` sor, a többi `… +N more: L55, L61`.
pub(crate) fn call_lines(
    s: &mut String,
    row: &QueryResult,
    calls: &[&Call],
    source: &SourceLines,
    lang: Lang,
    indent: &str,
) {
    let mut lines: Vec<u32> = calls.iter().map(|c| c.line).collect();
    lines.sort_unstable();
    lines.dedup();
    let sites = snippet::merge(lines.iter().map(|&line| {
        let via = calls
            .iter()
            .filter(|c| c.line == line)
            .find_map(|c| row.via_of(c));
        (line, via, source.get(&row.graph.file, line))
    }));
    let (shown, rest) = snippet::cap(sites);
    for site in &shown {
        writeln!(
            s,
            "{indent}{}",
            snippet::sites_line(lang, &site.lines, site.key, site.code.as_deref())
        )
        .unwrap();
    }
    if !rest.is_empty() {
        writeln!(s, "{indent}{}", snippet::more(lang, &rest)).unwrap();
    }
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
