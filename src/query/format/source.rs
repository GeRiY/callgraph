//! Közös, sor-alapú forrásnézet-segédek.

use crate::query::QueryResult;

pub(super) fn source_lines(row: &QueryResult) -> Vec<String> {
    crate::io::read_source_lossy(std::path::Path::new(&row.graph.file))
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

pub(super) fn numbered(lines: &[String], start: u32, end: u32) -> String {
    let mut out = String::new();
    for number in start..=end {
        if let Some(line) = lines.get(number.saturating_sub(1) as usize) {
            out.push_str(&numbered_line(number, line));
        }
    }
    out
}

/// Egy számozott sor (`{sorszám:>6}  {sor}\n`), mint a `--lines` / `--body` kimenetében.
pub(super) fn numbered_line(number: u32, line: &str) -> String {
    format!("{number:>6}  {line}\n")
}
