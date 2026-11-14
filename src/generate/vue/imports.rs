//! Import-sorok: a szükséges importok egyesítése (forrásonként egy sor, duplikáció nélkül), kiírása, és a meglévő
//! script `import` utasításainak beolvasása / bővítése (`--into`, `--replace`).

use super::model::Import;

/// A `vue` az első, utána az Inertia, a többi ábécében.
fn rank(source: &str) -> (u8, String) {
    let group = match source {
        "vue" => 0,
        "@inertiajs/vue3" => 1,
        _ => 2,
    };
    (group, source.to_ascii_lowercase())
}

/// Forrásonként egyesített, rendezett import-lista.
#[derive(Debug, Clone, Default)]
pub struct ImportSet {
    items: Vec<Import>,
}

impl ImportSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hozzáadás: ugyanazon forrás importjai egyesülnek (név-duplikáció nélkül).
    pub fn add(&mut self, import: Import) {
        match self.items.iter_mut().find(|i| i.source == import.source) {
            Some(existing) => {
                if existing.default.is_none() {
                    existing.default = import.default;
                }
                for name in import.named {
                    if !existing.named.contains(&name) {
                        existing.named.push(name);
                    }
                }
            }
            None => self.items.push(import),
        }
    }

    pub fn extend(&mut self, imports: impl IntoIterator<Item = Import>) {
        for i in imports {
            self.add(i);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Rendezve (`vue`, `@inertiajs/vue3`, a többi ábécében); a névsorok is rendezettek.
    pub fn sorted(&self) -> Vec<Import> {
        let mut items = self.items.clone();
        for i in &mut items {
            i.named.sort_by_key(|n| n.to_ascii_lowercase());
        }
        items.sort_by_key(|i| rank(&i.source));
        items
    }

    pub fn lines(&self, semi: bool) -> Vec<String> {
        self.sorted().iter().map(|i| import_line(i, semi)).collect()
    }
}

/// Egy `import … from '…'` sor.
pub fn import_line(import: &Import, semi: bool) -> String {
    let end = if semi { ";" } else { "" };
    let named = if import.named.is_empty() {
        None
    } else {
        Some(format!("{{ {} }}", import.named.join(", ")))
    };
    let clause = match (&import.default, named) {
        (Some(d), Some(n)) => format!("{d}, {n}"),
        (Some(d), None) => d.clone(),
        (None, Some(n)) => n,
        (None, None) => return format!("import '{}'{end}", import.source),
    };
    format!("import {clause} from '{}'{end}", import.source)
}

/// Egy meglévő importálási utasítás a fájlban.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Az első és az utolsó sor (0-alapú, zárt).
    pub start: usize,
    pub end: usize,
    pub import: Import,
    /// `import type …`, `import * as x …`, `import 'x'`: ezekbe nem egyesítünk.
    pub fixed: bool,
}

fn complete(text: &str) -> bool {
    let t = text.trim_end().trim_end_matches(';').trim_end();
    t.ends_with('\'') || t.ends_with('"')
}

fn unquote(text: &str) -> &str {
    text.trim().trim_matches(|c| c == '\'' || c == '"')
}

/// Az utasítás-szöveg (egy sorba fűzve) értelmezése.
fn parse_statement(text: &str) -> Option<(Import, bool)> {
    let rest = text.trim().strip_prefix("import")?.trim();
    let rest = rest.trim_end_matches(';').trim();
    if rest.starts_with('\'') || rest.starts_with('"') {
        return Some((
            Import {
                source: unquote(rest).to_owned(),
                default: None,
                named: Vec::new(),
            },
            true,
        ));
    }
    let at = rest.rfind(" from ")?;
    let (clause, source) = (rest[..at].trim(), unquote(&rest[at + 6..]));
    let fixed = clause.starts_with("type ") || clause.contains('*');
    let mut import = Import {
        source: source.to_owned(),
        default: None,
        named: Vec::new(),
    };
    let (head, braces) = match clause.find('{') {
        Some(open) => (
            clause[..open].trim().trim_end_matches(',').trim(),
            clause[open + 1..].split('}').next().unwrap_or(""),
        ),
        None => (clause, ""),
    };
    if !head.is_empty() && !head.contains('*') && !head.starts_with("type ") {
        import.default = Some(head.to_owned());
    }
    import.named = braces
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .collect();
    Some((import, fixed))
}

/// A `[from, to)` sorok importálási utasításai (többsoros is).
pub fn scan(lines: &[String], from: usize, to: usize) -> Vec<Found> {
    let mut found = Vec::new();
    let to = to.min(lines.len());
    let mut i = from;
    while i < to {
        let first = lines[i].trim();
        let starts = first.starts_with("import ")
            || first.starts_with("import{")
            || first.starts_with("import'")
            || first.starts_with("import\"");
        if !starts {
            i += 1;
            continue;
        }
        let mut text = first.to_owned();
        let mut j = i;
        while !complete(&text) && j + 1 < to {
            j += 1;
            text.push(' ');
            text.push_str(lines[j].trim());
        }
        if let Some((import, fixed)) = parse_statement(&text) {
            found.push(Found {
                start: i,
                end: j,
                import,
                fixed,
            });
        }
        i = j + 1;
    }
    found
}

/// A névből a lokális kötés (`a as b` → `b`).
fn bound(name: &str) -> &str {
    name.rsplit(" as ").next().unwrap_or(name).trim()
}

/// Egy szerkesztés: a `[start, end)` sorok helyére az `lines` kerül (`start == end`: beszúrás).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub start: usize,
    pub end: usize,
    pub lines: Vec<String>,
}

/// A szükséges importok beillesztésének / egyesítésének terve a `[from, to)` tartományra; a szerkesztések
/// kezdete szerint csökkenő sorrendben jönnek vissza (így egymás után alkalmazhatók). Az új importok az utolsó
/// meglévő import után kerülnek; ha nincs import, a tartomány elejére.
pub fn plan(
    lines: &[String],
    from: usize,
    to: usize,
    wanted: &ImportSet,
    semi: bool,
) -> Vec<LineEdit> {
    let found = scan(lines, from, to);
    let taken: Vec<String> = found
        .iter()
        .flat_map(|f| {
            f.import
                .default
                .iter()
                .map(String::as_str)
                .chain(f.import.named.iter().map(|n| bound(n)))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    let is_taken = |name: &str| taken.iter().any(|t| t == name);
    let mut edits: Vec<LineEdit> = Vec::new();
    let mut fresh: Vec<String> = Vec::new();
    for want in wanted.sorted() {
        let existing = found
            .iter()
            .find(|f| !f.fixed && f.import.source == want.source);
        let default = want.default.clone().filter(|d| !is_taken(d));
        let named: Vec<String> = want
            .named
            .iter()
            .filter(|n| !is_taken(bound(n)) && existing.is_none_or(|f| !f.import.named.contains(n)))
            .cloned()
            .collect();
        match existing {
            Some(f) => {
                let add_default = default.filter(|_| f.import.default.is_none());
                if add_default.is_none() && named.is_empty() {
                    continue;
                }
                let mut merged = f.import.clone();
                if add_default.is_some() {
                    merged.default = add_default;
                }
                merged.named.extend(named);
                edits.push(LineEdit {
                    start: f.start,
                    end: f.end + 1,
                    lines: vec![import_line(&merged, semi)],
                });
            }
            None => {
                if default.is_none() && named.is_empty() {
                    continue;
                }
                let new = Import {
                    source: want.source.clone(),
                    default,
                    named,
                };
                fresh.push(import_line(&new, semi));
            }
        }
    }
    if !fresh.is_empty() {
        match found.last() {
            Some(last) => edits.push(LineEdit {
                start: last.end + 1,
                end: last.end + 1,
                lines: fresh,
            }),
            None => {
                let mut block = fresh;
                if lines.get(from).is_some_and(|l| !l.trim().is_empty()) {
                    block.push(String::new());
                }
                edits.push(LineEdit {
                    start: from,
                    end: from,
                    lines: block,
                });
            }
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse((e.start, e.end)));
    edits
}
