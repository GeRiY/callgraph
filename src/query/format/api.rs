//! A publikus szerződés és a docblock-kivonat formázója.

use crate::i18n::Lang;
use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Api;

impl Formatter for Api {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let mut out = String::new();
        for row in rows {
            writeln!(
                out,
                "FILE  {}{}{}",
                row.graph.file,
                row.graph
                    .namespace
                    .as_ref()
                    .map(|n| format!("  NS:{n}"))
                    .unwrap_or_default(),
                crate::notes::mark(crate::notes::file_count(&row.notes))
            )
            .unwrap();
            out.push_str(&super::parse_error_lines(&row.graph, lang));
            out.push('\n');
            if row.graph.containers.is_empty() {
                out.push_str(lang.pick(
                    "(procedural file — no class/interface/trait; --format outline shows the top-level calls)\n\n",
                    "(procedurális fájl — nincs class/interface/trait; --format outline mutatja a top-level hívásokat)\n\n",
                ));
                let mut groups = std::collections::BTreeMap::<String, Vec<String>>::new();
                for call in row
                    .graph
                    .calls
                    .iter()
                    .filter(|call| call.caller.as_deref() == Some("__top__"))
                {
                    groups
                        .entry(call.category.clone())
                        .or_default()
                        .push(call.resolved.clone().unwrap_or_else(|| call.target.clone()));
                }
                for (category, mut targets) in groups {
                    targets.sort();
                    targets.dedup();
                    writeln!(out, "  {category:<18}{}", targets.join(", ")).unwrap();
                }
                out.push_str("---\n");
                continue;
            }
            for container in &row.graph.containers {
                write_container(&mut out, row, container, lang);
            }
        }
        out.push_str(lang.pick(
            "💡 for the method bodies' logic: Read from the L<n> line above — not the whole file. For the call graph: --format outline.\n",
            "💡 a metódusok törzs-logikájához: Read a fenti L<n> sortól — ne a teljes fájlt. Hívás-gráfhoz: --format outline.\n",
        ));
        out
    }
}

fn write_container(
    out: &mut String,
    row: &QueryResult,
    container: &crate::model::Container,
    lang: Lang,
) {
    let extends = join_non_empty(&container.extends, "extends");
    let implements = join_non_empty(&container.implements, "implements");
    let traits = join_non_empty(&container.traits, "uses");
    writeln!(
        out,
        "{} {}{}{}{}  L{}-{}",
        container.kind,
        container.name,
        extends,
        implements,
        traits,
        container.line,
        container.end_line
    )
    .unwrap();
    if let Some(doc) = &container.doc {
        writeln!(out, "  {doc}").unwrap();
    }
    out.push('\n');
    let constants: Vec<_> = row
        .graph
        .constants
        .iter()
        .filter(|c| {
            row.graph.owns(container, c.container.as_deref(), c.line)
                && is_public(c.visibility.as_deref())
        })
        .collect();
    if !constants.is_empty() {
        let n = constants.len();
        out.push_str(&ltf!(
            lang,
            "  public constants ({n}):\n",
            "  publikus konstansok ({n}):\n"
        ));
        for constant in constants {
            writeln!(
                out,
                "    {}{}",
                constant.name,
                constant
                    .value
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default()
            )
            .unwrap();
        }
        out.push('\n');
    }
    let properties: Vec<_> = row
        .graph
        .properties
        .iter()
        // a JS/Vue-propnak a régi motorban nincs visibility-je, így a jq `== "public"` kihagyja
        .filter(|p| {
            !p.js
                && row.graph.owns(container, p.container.as_deref(), p.line)
                && is_public(Some(p.visibility.as_str()))
        })
        .collect();
    if !properties.is_empty() {
        let n = properties.len();
        out.push_str(&ltf!(
            lang,
            "  public properties ({n}):\n",
            "  publikus property-k ({n}):\n"
        ));
        for property in properties {
            writeln!(
                out,
                "    {} ${}{}",
                property.type_hint.as_deref().unwrap_or("mixed"),
                property.name,
                if property.readonly { " [readonly]" } else { "" }
            )
            .unwrap();
        }
        out.push('\n');
    }
    let mut methods: Vec<_> = row
        .graph
        .methods
        .iter()
        .filter(|m| {
            row.graph.owns(container, m.container.as_deref(), m.line)
                && is_public(m.visibility.as_deref())
        })
        .collect();
    if methods.is_empty() {
        out.push_str(lang.pick("  (no public methods)\n", "  (nincs publikus metódus)\n"));
    } else {
        let n = methods.len();
        out.push_str(&ltf!(
            lang,
            "  public API ({n}):\n",
            "  publikus felület ({n}):\n"
        ));
        methods.sort_by_key(|m| m.line);
        for method in methods {
            write!(out, "    {}({})", method.name, method.params.join(", ")).unwrap();
            if let Some(return_type) = &method.return_type {
                write!(out, ": {return_type}").unwrap();
            }
            write!(out, "  L{}", method.line).unwrap();
            if method.is_static {
                out.push_str(" [static]");
            }
            let notes =
                crate::notes::method_count(&row.notes, method.container.as_deref(), &method.name);
            out.push_str(&crate::notes::mark(notes));
            out.push('\n');
            if let Some(doc) = &method.doc {
                writeln!(out, "      {doc}").unwrap();
            }
        }
    }
    out.push_str("---\n");
}

/// Publikus elem: `public` (PHP / JS / Swift) vagy Rust `pub` (a `pub(crate)` / `pub(super)` nem).
/// A Rust trait-implementációk metódusai is `pub`-ok (a trait-en át hívhatók), így ezek is listázódnak.
fn is_public(visibility: Option<&str>) -> bool {
    matches!(visibility, Some("public" | "pub"))
}

fn join_non_empty(values: &Option<Vec<String>>, label: &str) -> String {
    values
        .as_ref()
        .filter(|v| !v.is_empty())
        .map(|v| format!(" {label} {}", v.join(", ")))
        .unwrap_or_default()
}
