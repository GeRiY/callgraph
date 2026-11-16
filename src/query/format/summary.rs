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
        // Blade: a szakaszon kívüli rész `template` ál-metódus, nem szakasz
        "view" => {
            let sections = row
                .graph
                .methods
                .iter()
                .filter(|m| {
                    m.name != "template" && m.container.as_deref() == Some(container.name.as_str())
                })
                .count();
            plural(sections.min(methods), ["section", "sections"], "szakasz")
        }
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

/// A fájl útjának mappa-része (záró elválasztóval), üres ha a fájl a gyökérben van.
fn dir_of(file: &str) -> &str {
    file.rfind(['/', '\\']).map_or("", |i| &file[..=i])
}

impl Formatter for Summary {
    /// Több fájl esetén a mappa is látszik, hogy az azonos nevű fájlok megkülönböztethetők legyenek.
    /// Az egymást követő, azonos mappájú sorok egy `mappa/` fejléc alá kerülnek (két szóközzel
    /// behúzva) — ez olcsóbb, mint minden sorban megismételni az utat; az egyedül álló sor
    /// inkább a teljes útját kapja fejléc nélkül (a fejléc + behúzás ott drágább lenne).
    /// Egyetlen fájl kimenete változatlan: csak a fájlnév.
    fn format(&self, rows: &[QueryResult], lang: Lang) -> String {
        let multi = rows
            .iter()
            .skip(1)
            .any(|row| row.graph.file != rows[0].graph.file);
        let mut out = String::new();
        let mut i = 0;
        while i < rows.len() {
            if !multi {
                write_row(&mut out, &rows[i], "", "", lang);
                i += 1;
                continue;
            }
            let dir = dir_of(&rows[i].graph.file);
            let run = rows[i..]
                .iter()
                .take_while(|row| dir_of(&row.graph.file) == dir)
                .count();
            if run == 1 {
                write_row(&mut out, &rows[i], dir, "", lang);
            } else {
                let header = if dir.is_empty() { "./" } else { dir };
                writeln!(out, "{header}").unwrap();
                for row in &rows[i..i + run] {
                    write_row(&mut out, row, "", "  ", lang);
                }
            }
            i += run;
        }
        out
    }
}

/// Egy fájl sorai: `indent` + `dir` + fájlnév + tartalom. Az `dir` üres, ha fejléc alatt áll.
fn write_row(out: &mut String, row: &QueryResult, dir: &str, indent: &str, lang: Lang) {
    // a fájlnév mindkét elválasztóval (Windowson az út `\`-es)
    let name = row
        .graph
        .file
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&row.graph.file);
    let file = format!("{indent}{dir}{name}");
    if row.graph.containers.is_empty() {
        let n = row.graph.calls.len();
        let line = match lang {
            Lang::En if n == 1 => format!("{file}  procedural ({n} call)"),
            Lang::En => format!("{file}  procedural ({n} calls)"),
            Lang::Hu => format!("{file}  procedurális ({n} hívás)"),
        };
        writeln!(out, "{line}{}", crate::io::protect_mark(&row.graph.file)).unwrap();
        return;
    }
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
