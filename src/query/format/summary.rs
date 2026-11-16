//! Fájlonkénti, tömör projekt-áttekintés.

use crate::i18n::Lang;
use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Summary;

/// A container utáni számláló: kódnál metódus, stíluslapnál szabály, JSON-dokumentumnál kulcs, más dokumentumnál
/// (Markdown / HTML) szakasz — a `(0 metódus)` hamis állítás lenne rájuk.
fn count_label(
    row: &QueryResult,
    container: &crate::model::Container,
    methods: usize,
    lang: Lang,
) -> String {
    let plural = |n: usize, en: [&str; 2], hu: &str| match lang {
        Lang::En if n == 1 => format!("  ({n} {})", en[0]),
        Lang::En => format!("  ({n} {})", en[1]),
        Lang::Hu => format!("  ({n} {hu})"),
    };
    match container.kind.as_str() {
        "stylesheet" => plural(methods, ["rule", "rules"], "szabály"),
        "document" => {
            let keys = row
                .graph
                .constants
                .iter()
                .filter(|k| {
                    k.kind == "key" && row.graph.owns(container, k.container.as_deref(), k.line)
                })
                .count();
            if keys > 0 {
                plural(keys, ["key", "keys"], "kulcs")
            } else {
                plural(methods, ["section", "sections"], "szakasz")
            }
        }
        _ => plural(methods, ["method", "methods"], "metódus"),
    }
}

impl Formatter for Summary {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let mut out = String::new();
        for row in rows {
            // a fájlnév mindkét elválasztóval (Windowson az út `\`-es)
            let file = row
                .graph
                .file
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&row.graph.file);
            if row.graph.containers.is_empty() {
                let n = row.graph.calls.len();
                let line = match lang {
                    Lang::En if n == 1 => format!("{file}  procedural ({n} call)"),
                    Lang::En => format!("{file}  procedural ({n} calls)"),
                    Lang::Hu => format!("{file}  procedurális ({n} hívás)"),
                };
                writeln!(out, "{line}{}", crate::io::protect_mark(&row.graph.file)).unwrap();
            } else {
                let mut containers: Vec<_> = row.graph.containers.iter().collect();
                containers.sort_by_key(|container| container.line);
                for container in containers {
                    let methods = row
                        .graph
                        .methods
                        .iter()
                        .filter(|m| row.graph.owns(container, m.container.as_deref(), m.line))
                        .count();
                    write!(out, "{file}  {} {}", container.kind, container.name).unwrap();
                    if let Some(extends) = &container.extends
                        && !extends.is_empty()
                    {
                        write!(out, " extends {}", extends.join(", ")).unwrap();
                    }
                    if let Some(implements) = &container.implements
                        && !implements.is_empty()
                    {
                        write!(out, " implements {}", implements.join(", ")).unwrap();
                    }
                    let count = count_label(row, container, methods, lang);
                    out.push_str(&count);
                    if let Some(doc) = &container.doc {
                        let doc = if doc.chars().count() > 100 {
                            format!("{}…", doc.chars().take(100).collect::<String>())
                        } else {
                            doc.clone()
                        };
                        write!(out, " — {doc}").unwrap();
                    }
                    out.push_str(crate::io::protect_mark(&row.graph.file));
                    out.push('\n');
                }
            }
        }
        out
    }
}
