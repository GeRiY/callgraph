//! `--body`: kiválasztott metódusok forrásbeli törzse. Alapból tömör: a docblock / doc-komment, az üres és a csak
//! megjegyzést tartalmazó sorok kimaradnak (a sorszámok a forrásbeliek, a hézag jelzi a kihagyást); az attribútumok
//! (`#[…]`, `@…`) maradnak. `--doc`: a docblockkal együtt; `--raw`: a pontos, szűretlen forrás.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;
use super::body_filter::{LineKind, LineScanner, Syntax, lead_start};
use super::source::{numbered, numbered_line, source_lines};

/// A `--body` nézete.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BodyMode {
    /// Tömör (alapértelmezett): docblock, üres és megjegyzés-sorok nélkül.
    #[default]
    Compact,
    /// Mint a tömör, plusz a metódus fölötti docblock / doc-kommentek.
    Doc,
    /// A pontos forrás (docblockkal, üres és megjegyzés-sorokkal).
    Raw,
}

#[derive(Default)]
pub struct Body {
    pub mode: BodyMode,
}

impl Formatter for Body {
    fn format(&self, rows: &[QueryResult], _lang: crate::i18n::Lang) -> String {
        let mut out = String::new();
        for row in rows {
            let lines = source_lines(row);
            let syntax = Syntax::for_path(&row.graph.file);
            let methods = row.methods.as_ref().map_or_else(
                || row.graph.methods.iter().collect::<Vec<_>>(),
                |selected| {
                    row.graph
                        .methods
                        .iter()
                        .filter(|m| selected.contains(&m.name))
                        .collect()
                },
            );
            for method in methods {
                let end = method.end_line.unwrap_or(method.line);
                let container = method.container.as_deref().unwrap_or("<file>");
                writeln!(
                    out,
                    "== {}  {container}::{}()  L{}-{end}",
                    row.graph.file, method.name, method.line
                )
                .unwrap();
                let span = MethodSpan {
                    line: method.line,
                    end,
                    has_doc: method.doc.is_some(),
                };
                crate::notes::body_lines(
                    &mut out,
                    &row.notes,
                    method.container.as_deref(),
                    &method.name,
                );
                out.push_str(&method_source(&lines, syntax, span, self.mode));
                out.push('\n');
            }
        }
        out
    }
}

/// Egy metódus helye a forrásban.
#[derive(Debug, Clone, Copy)]
pub(super) struct MethodSpan {
    pub line: u32,
    pub end: u32,
    pub has_doc: bool,
}

/// A metódus számozott forrássorai a nézet szerint; ismeretlen nyelvnél szűrés nélkül (a docblocktól).
pub(super) fn method_source(
    lines: &[String],
    syntax: Option<Syntax>,
    span: MethodSpan,
    mode: BodyMode,
) -> String {
    let Some(syntax) = syntax else {
        return numbered(lines, doc_start(lines, span.line, span.has_doc), span.end);
    };
    let start = lead_start(lines, span.line, syntax, span.has_doc);
    if mode == BodyMode::Raw {
        return numbered(lines, start, span.end);
    }
    let mut scanner = LineScanner::new(syntax);
    let mut out = String::new();
    for number in start..=span.end {
        let Some(line) = lines.get(number.saturating_sub(1) as usize) else {
            break;
        };
        let kind = scanner.classify(line);
        let doc = mode == BodyMode::Doc && number < span.line && kind == LineKind::Comment;
        if kind == LineKind::Code || doc {
            out.push_str(&numbered_line(number, line));
        }
    }
    out
}

/// A régi, nyelvfüggetlen docblock-kezdet (a `/**` sora), a szűrés nélküli nyelvekhez.
fn doc_start(lines: &[String], method_line: u32, has_doc: bool) -> u32 {
    if !has_doc || method_line <= 1 {
        return method_line;
    }
    let mut cursor = method_line.saturating_sub(1) as usize;
    while cursor > 0
        && lines
            .get(cursor - 1)
            .is_some_and(|line| line.trim().is_empty())
    {
        cursor -= 1;
    }
    while cursor > 0 {
        if lines[cursor - 1].contains("/**") {
            return cursor as u32;
        }
        cursor -= 1;
    }
    method_line
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHP: &str = "<?php\nclass A\n{\n    /**\n     * Runs it.\n     */\n    #[Route('/run')]\n    public function run(): string\n    {\n        // step one\n        $a = 1; // trailing\n\n        $sql = \"SELECT 1\n-- not dropped\n// not dropped either\n\";\n        # hash comment\n        return $sql;\n    }\n}\n";

    fn lines(source: &str) -> Vec<String> {
        source.lines().map(str::to_owned).collect()
    }

    fn render(source: &str, path: &str, span: MethodSpan, mode: BodyMode) -> String {
        method_source(&lines(source), Syntax::for_path(path), span, mode)
    }

    const RUN: MethodSpan = MethodSpan {
        line: 7,
        end: 19,
        has_doc: true,
    };

    #[test]
    fn compact_drops_docblock_blank_and_comment_lines_keeps_numbers() {
        assert_eq!(
            render(PHP, "A.php", RUN, BodyMode::Compact),
            "     7      #[Route('/run')]\n     8      public function run(): string\n     9      {\n    11          $a = 1; // trailing\n    13          $sql = \"SELECT 1\n    14  -- not dropped\n    15  // not dropped either\n    16  \";\n    18          return $sql;\n    19      }\n"
        );
    }

    #[test]
    fn doc_adds_the_docblock_raw_is_the_exact_source() {
        let doc = render(PHP, "A.php", RUN, BodyMode::Doc);
        assert!(
            doc.starts_with(
                "     4      /**\n     5       * Runs it.\n     6       */\n     7      #[Route"
            ),
            "{doc}"
        );
        assert!(!doc.contains("// step one"), "{doc}");
        let raw = render(PHP, "A.php", RUN, BodyMode::Raw);
        assert_eq!(raw.lines().count(), 16, "{raw}");
        assert!(raw.contains("    10          // step one\n"), "{raw}");
        assert!(raw.contains("\n    12  \n"), "{raw}");
    }

    #[test]
    fn rust_keeps_attributes_drops_doc_comments() {
        let source = "/// Adds.\n#[inline]\n/// More doc.\npub fn add(a: u8) -> u8 {\n    // why\n    a + 1\n}\n";
        let span = MethodSpan {
            line: 4,
            end: 7,
            has_doc: true,
        };
        assert_eq!(
            render(source, "src/lib.rs", span, BodyMode::Compact),
            "     2  #[inline]\n     4  pub fn add(a: u8) -> u8 {\n     6      a + 1\n     7  }\n"
        );
        assert!(
            render(source, "src/lib.rs", span, BodyMode::Doc)
                .starts_with("     1  /// Adds.\n     2  #[inline]\n     3  /// More doc.\n")
        );
    }

    #[test]
    fn unknown_language_is_not_filtered() {
        let source = "/** Doc */\nfoo {\n  // x\n}\n";
        let span = MethodSpan {
            line: 2,
            end: 4,
            has_doc: true,
        };
        assert_eq!(
            render(source, "a.scss", span, BodyMode::Compact),
            "     1  /** Doc */\n     2  foo {\n     3    // x\n     4  }\n"
        );
    }
}
