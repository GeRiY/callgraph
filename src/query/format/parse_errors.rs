//! A szintaktikai hibák jelzése a szöveges kimenetekben: fájlonként a `FILE` sor alatt (`file_lines`), és egy záró
//! sor (`trailer`), ha a beolvasott fájlok közül bármelyik hibás volt. Hiba nélkül mindkettő üres: a kimenet bájtra
//! változatlan.

use std::fmt::Write as _;
use std::path::Path;

use crate::i18n::Lang;
use crate::model::FileGraph;

/// Fájlonként legfeljebb ennyi `⚠` sor (a többi: „… és még N”).
const MAX_FILE_LINES: usize = 3;

/// A záró sorban legfeljebb ennyi fájl.
const MAX_TRAILER_FILES: usize = 5;

/// A `FILE` sor alá: hibánként egy `⚠` sor (legfeljebb 3, utána „… és még N”).
pub(crate) fn file_lines(graph: &FileGraph, lang: Lang) -> String {
    let mut out = String::new();
    for error in graph.parse_errors.iter().take(MAX_FILE_LINES) {
        let line = ltf!(
            lang,
            "⚠ syntax error at L{}: {} — this file's outline may be incomplete",
            "⚠ szintaktikai hiba itt: L{}: {} — a fájl vázlata hiányos lehet",
            error.line,
            error.message
        );
        writeln!(out, "{line}").unwrap();
    }
    let rest = graph.parse_errors.len().saturating_sub(MAX_FILE_LINES);
    if rest > 0 {
        let line = ltf!(
            lang,
            "⚠ … and {rest} more syntax errors",
            "⚠ … és még {rest} szintaktikai hiba"
        );
        writeln!(out, "{line}").unwrap();
    }
    out
}

/// Egy beolvasott fájl a záró sorhoz: a gyökérhez relatív út és az első hiba sora; hiba nélkül `None`.
pub(crate) fn issue(root: &Path, path: &Path, graph: &FileGraph) -> Option<(String, u32)> {
    let first = graph.parse_errors.first()?;
    let shown = path.strip_prefix(root).unwrap_or(path);
    Some((shown.to_string_lossy().into_owned(), first.line))
}

/// A záró sor: hány beolvasott fájl volt hibás, és melyek (legfeljebb 5, a sor számával); hiba nélkül üres.
pub fn trailer(issues: &[(String, u32)], lang: Lang) -> String {
    if issues.is_empty() {
        return String::new();
    }
    let count = issues.len();
    let mut list = issues
        .iter()
        .take(MAX_TRAILER_FILES)
        .map(|(path, line)| format!("{path} L{line}"))
        .collect::<Vec<_>>()
        .join(", ");
    let rest = count.saturating_sub(MAX_TRAILER_FILES);
    if rest > 0 {
        list.push_str(&ltf!(lang, ", … and {rest} more", ", … és még {rest}"));
    }
    if count == 1 {
        ltf!(
            lang,
            "⚠ 1 file had syntax errors (results may be incomplete): {list}\n",
            "⚠ 1 fájlban szintaktikai hiba van (az eredmény hiányos lehet): {list}\n"
        )
    } else {
        ltf!(
            lang,
            "⚠ {count} files had syntax errors (results may be incomplete): {list}\n",
            "⚠ {count} fájlban szintaktikai hiba van (az eredmény hiányos lehet): {list}\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileGraphBuilder, ParseError};

    fn broken(errors: u32) -> FileGraph {
        let mut builder = FileGraphBuilder::new("/p/src/tail.js");
        for line in 1..=errors {
            builder.parse_error(ParseError::new(line + 2, "Unexpected token"));
        }
        builder.build()
    }

    #[test]
    fn file_lines_are_empty_without_errors() {
        assert_eq!(file_lines(&broken(0), Lang::En), "");
        assert_eq!(trailer(&[], Lang::En), "");
    }

    #[test]
    fn file_lines_name_the_line_and_cap_at_three() {
        assert_eq!(
            file_lines(&broken(1), Lang::En),
            "⚠ syntax error at L3: Unexpected token — this file's outline may be incomplete\n"
        );
        let many = file_lines(&broken(5), Lang::Hu);
        assert_eq!(many.lines().count(), 4, "{many}");
        assert!(many.starts_with("⚠ szintaktikai hiba itt: L3: Unexpected token"));
        assert!(many.ends_with("⚠ … és még 2 szintaktikai hiba\n"));
    }

    #[test]
    fn trailer_lists_at_most_five_files() {
        let one = issue(Path::new("/p"), Path::new("/p/src/tail.js"), &broken(1)).unwrap();
        assert_eq!(one.1, 3);
        assert_eq!(
            trailer(std::slice::from_ref(&one), Lang::En),
            format!(
                "⚠ 1 file had syntax errors (results may be incomplete): {} L3\n",
                one.0
            )
        );
        let seven: Vec<(String, u32)> = (1..=7).map(|n| (format!("f{n}.js"), n)).collect();
        let text = trailer(&seven, Lang::En);
        assert!(text.starts_with("⚠ 7 files had syntax errors"));
        assert!(text.contains("f5.js L5, … and 2 more"), "{text}");
        assert!(!text.contains("f6.js"));
    }
}
