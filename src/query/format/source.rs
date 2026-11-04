//! Közös, sor-alapú forrásnézet-segédek.

use crate::query::QueryResult;

pub(super) fn source_lines(row: &QueryResult) -> Vec<String> {
    std::fs::read_to_string(&row.graph.file)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

pub(super) fn numbered(lines: &[String], start: u32, end: u32) -> String {
    let mut out = String::new();
    for number in start..=end {
        if let Some(line) = lines.get(number.saturating_sub(1) as usize) {
            out.push_str(&format!("{number:>6}  {line}\n"));
        }
    }
    out
}
