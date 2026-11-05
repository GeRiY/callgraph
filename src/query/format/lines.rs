//! `--lines`: a forrás nyers, számozott sorai.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;
use super::source::{numbered, source_lines};

pub struct Lines {
    pub range: (u32, u32),
}

impl Formatter for Lines {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        let mut out = String::new();
        for row in rows {
            let (start, end) = self.range;
            writeln!(out, "FILE  {}  L{}-{}", row.graph.file, start, end).unwrap();
            out.push_str(&numbered(&source_lines(row), start, end));
        }
        out
    }
}
