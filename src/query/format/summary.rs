//! Fájlonkénti, tömör projekt-áttekintés.

use crate::i18n::Lang;
use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;

pub struct Summary;

impl Formatter for Summary {
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let mut out = String::new();
        for row in rows {
            let file = row.graph.file.rsplit('/').next().unwrap_or(&row.graph.file);
            if row.graph.containers.is_empty() {
                let n = row.graph.calls.len();
                let line = match lang {
                    Lang::En if n == 1 => format!("{file}  procedural ({n} call)"),
                    Lang::En => format!("{file}  procedural ({n} calls)"),
                    Lang::Hu => format!("{file}  procedurális ({n} hívás)"),
                };
                writeln!(out, "{line}").unwrap();
            } else {
                let mut containers: Vec<_> = row.graph.containers.iter().collect();
                containers.sort_by_key(|container| container.line);
                for container in containers {
                    let methods = row
                        .graph
                        .methods
                        .iter()
                        .filter(|m| m.container.as_deref() == Some(&container.name))
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
                    let count = match lang {
                        Lang::En if methods == 1 => format!("  ({methods} method)"),
                        Lang::En => format!("  ({methods} methods)"),
                        Lang::Hu => format!("  ({methods} metódus)"),
                    };
                    out.push_str(&count);
                    if let Some(doc) = &container.doc {
                        let doc = if doc.chars().count() > 100 {
                            format!("{}…", doc.chars().take(100).collect::<String>())
                        } else {
                            doc.clone()
                        };
                        write!(out, " — {doc}").unwrap();
                    }
                    out.push('\n');
                }
            }
        }
        out
    }
}
