//! Szövegszerkesztés sorok felett: a meglévő fájl sorokra bontása (a sorvég-stílus megőrzésével), blokk beszúrása
//! egy metódus után / az osztály végére, importok rendezett beillesztése, a metódus-tartomány kiterjesztése a
//! docblockra és az attribútumokra. Tiszta függvények (I/O nélkül), ezért egyenként tesztelhetők.

/// Sorokra bontás; a sorvég (`\n` vagy `\r\n`) külön jön vissza. A záró újsor egy üres utolsó elemként él tovább,
/// így a `join_lines` bájtra pontosan visszaadja az eredetit.
pub fn split_lines(text: &str) -> (Vec<String>, &'static str) {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned())
        .collect();
    (lines, eol)
}

/// Sorok beszúrása a `at` indexű sor elé.
fn insert_all(lines: &mut Vec<String>, at: usize, items: Vec<String>) {
    let tail = lines.split_off(at.min(lines.len()));
    lines.extend(items);
    lines.extend(tail);
}

pub fn join_lines(lines: &[String], eol: &str) -> String {
    lines.join(eol)
}

/// Blokk beszúrása egy metódus után: `method_end` a metódus utolsó sora (1-alapú). Egy üres sor választja el az
/// előző tagtól, és (ha a következő sor nem üres és nem a záró `}`) a következőtől is. Az első beszúrt tag sorának
/// 0-alapú indexét adja vissza.
pub fn insert_after_line(lines: &mut Vec<String>, method_end: usize, block: &[String]) -> usize {
    let at = method_end.min(lines.len());
    let mut inserted = vec![String::new()];
    inserted.extend(block.iter().cloned());
    if lines
        .get(at)
        .is_some_and(|l| !l.trim().is_empty() && l.trim() != "}")
    {
        inserted.push(String::new());
    }
    insert_all(lines, at, inserted);
    at + 1
}

/// Blokk beszúrása az osztály végére: `close_line` a záró `}` sora (1-alapú); az utolsó tag után, üres sorral
/// elválasztva (üres törzsnél a `{` után azonnal). Az első beszúrt tag sorának 0-alapú indexét adja vissza.
pub fn insert_at_end(lines: &mut Vec<String>, close_line: usize, block: &[String]) -> usize {
    let close_idx = close_line.saturating_sub(1).min(lines.len());
    let last = (0..close_idx).rev().find(|&i| !lines[i].trim().is_empty());
    let at = last.map_or(close_idx, |i| i + 1);
    let spaced = last.is_some_and(|i| !lines[i].trim_end().ends_with('{'));
    let mut inserted = Vec::new();
    if spaced {
        inserted.push(String::new());
    }
    inserted.extend(block.iter().cloned());
    insert_all(lines, at, inserted);
    at + usize::from(spaced)
}

/// Egy sima, felső szintű `use X;` / `use X as Y;` sor: `(FQCN, alias)`.
fn parse_use_line(line: &str) -> Option<(String, Option<String>)> {
    let rest = line.strip_prefix("use ")?.strip_suffix(';')?.trim();
    if rest.starts_with("function ") || rest.starts_with("const ") || rest.contains('{') {
        return None;
    }
    Some(match rest.split_once(" as ") {
        Some((path, alias)) => (path.trim().to_owned(), Some(alias.trim().to_owned())),
        None => (rest.to_owned(), None),
    })
}

fn use_line(fqcn: &str, alias: Option<&str>) -> String {
    match alias {
        Some(a) => format!("use {fqcn} as {a};"),
        None => format!("use {fqcn};"),
    }
}

/// Importok beillesztése a fájl `use` blokkjába (rendezve, duplikáció nélkül); ha nincs blokk, a `namespace` /
/// `declare` / `<?php` sor után új blokk nyílik. `limit`: az első deklaráció 0-alapú sora (afölött keresünk).
/// A beszúrt sorok számát adja vissza.
pub fn add_imports(
    lines: &mut Vec<String>,
    limit: usize,
    new: &[(String, Option<String>)],
) -> usize {
    let before = lines.len();
    let mut wanted: Vec<&(String, Option<String>)> = new.iter().collect();
    wanted.sort_by_key(|(fqcn, _)| fqcn.to_ascii_lowercase());
    for (fqcn, alias) in wanted {
        let limit = limit.min(lines.len());
        let existing: Vec<(usize, String, Option<String>)> = lines[..limit]
            .iter()
            .enumerate()
            .filter_map(|(i, l)| parse_use_line(l).map(|(f, a)| (i, f, a)))
            .collect();
        if existing
            .iter()
            .any(|(_, f, a)| f.eq_ignore_ascii_case(fqcn) && a == alias)
        {
            continue;
        }
        let line = use_line(fqcn, alias.as_deref());
        let key = fqcn.to_ascii_lowercase();
        if let Some((idx, _, _)) = existing
            .iter()
            .find(|(_, f, _)| f.to_ascii_lowercase() > key)
        {
            lines.insert(*idx, line);
        } else if let Some((idx, _, _)) = existing.last() {
            lines.insert(idx + 1, line);
        } else {
            let anchor = lines[..limit]
                .iter()
                .rposition(|l| l.starts_with("namespace "))
                .or_else(|| {
                    lines[..limit]
                        .iter()
                        .rposition(|l| l.starts_with("declare("))
                })
                .or_else(|| lines[..limit].iter().position(|l| l.starts_with("<?php")))
                .map_or(0, |i| i + 1);
            let mut inserted = vec![String::new(), line];
            if lines.get(anchor).is_some_and(|l| !l.trim().is_empty()) {
                inserted.push(String::new());
            }
            insert_all(lines, anchor, inserted);
        }
    }
    lines.len() - before
}

/// A metódus kezdősora (1-alapú) felfelé kiterjesztve a közvetlenül fölötte álló docblockra és attribútum-sorokra.
pub fn extend_over_docs(lines: &[String], start_line: usize) -> usize {
    let mut start = start_line;
    while start >= 2 && start - 2 < lines.len() {
        let above = lines[start - 2].trim();
        if above.starts_with("#[") && above.ends_with(']') {
            start -= 1;
        } else if above.ends_with("*/") {
            match (0..=start - 2)
                .rev()
                .find(|&k| lines[k].trim_start().starts_with("/*"))
            {
                Some(k) => start = k + 1,
                None => break,
            }
        } else {
            break;
        }
    }
    start
}

/// A `[from, to]` (1-alapú, zárt) sortartomány szövege `\n`-nel összefűzve (az `--expect` hash alapja).
pub fn range_text(lines: &[String], from: usize, to: usize) -> String {
    lines[from.saturating_sub(1)..to.min(lines.len())].join("\n")
}
