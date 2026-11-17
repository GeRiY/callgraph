//! A PHP konfig- / nyelvfájl (`return [ … ];`) outline-ja: a kulcs-útvonalak (`pages.start.uri = '/'`), mint a JSON
//! fájlé — a fájl különben csak a hívásait mutatná, a kulcsai nem látszanak (H29 / F10).

use std::fmt::Write as _;

use crate::core::phpval::{self, Entry, Value};
use crate::core::text::flat_clip;
use crate::i18n::Lang;
use crate::query::QueryResult;

/// Legfeljebb ennyi kulcs-sor (a többi `+N`).
const MAX_KEYS: usize = 120;
/// A beágyazás mélysége, ameddig a kulcs-útvonalak bomlanak.
const MAX_DEPTH: usize = 8;
/// Egy skalár érték kiírt hossza (karakter).
const MAX_VALUE: usize = 48;

/// A kulcs-útvonalak blokkja, ha a (procedurális) PHP-fájl egy `return [ … ];` tömb; más fájlra `None`.
pub(super) fn keys(row: &QueryResult, lang: Lang) -> Option<String> {
    let file = &row.graph.file;
    if !file.to_ascii_lowercase().ends_with(".php") || !row.graph.methods.is_empty() {
        return None;
    }
    let text = crate::io::read_source_lossy(std::path::Path::new(file)).ok()?;
    let entries = phpval::parse_return(&text).filter(|entries| !entries.is_empty())?;
    let mut rows: Vec<(u32, String, String)> = Vec::new();
    walk(&entries, "", 1, &mut rows);
    let total = rows.len();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}",
        match lang {
            Lang::En => format!("  keys ({total}):"),
            Lang::Hu => format!("  kulcsok ({total}):"),
        }
    );
    for (line, path, shown) in rows.iter().take(MAX_KEYS) {
        let _ = writeln!(out, "    L{line}  {path} = {shown}");
    }
    if total > MAX_KEYS {
        let _ = writeln!(out, "    … +{}", total - MAX_KEYS);
    }
    Some(out)
}

/// A tömb elemei kulcs-útvonallal, a forrás sorrendjében.
fn walk(entries: &[Entry], prefix: &str, depth: usize, out: &mut Vec<(u32, String, String)>) {
    for entry in entries {
        let path = if prefix.is_empty() {
            entry.key.clone()
        } else {
            format!("{prefix}.{}", entry.key)
        };
        match &entry.value {
            Value::Array(children) => {
                // a csupa skalár lista (`'a', 'b'`) egy sor; az objektum-szerű tömb tovább bomlik
                let list = children.iter().enumerate().all(|(i, child)| {
                    child.key == i.to_string() && child.value.entries().is_none()
                });
                let kind = if list { "list" } else { "array" };
                out.push((
                    entry.line,
                    path.clone(),
                    format!("{kind} ({})", children.len()),
                ));
                if !list && depth < MAX_DEPTH {
                    walk(children, &path, depth + 1, out);
                }
            }
            other => out.push((entry.line, path, show(other))),
        }
    }
}

fn show(value: &Value) -> String {
    match value {
        Value::Str(text) => format!("'{}'", flat_clip(text, MAX_VALUE)),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_owned(),
        Value::Class(name) => format!("{name}::class"),
        Value::Expr(text) => flat_clip(text, MAX_VALUE),
        Value::Array(_) => String::new(),
    }
}
