//! `--body`: kiválasztott metódusok forrásbeli törzse. Alapból tömör: a metódus fölötti docblock / doc-komment és az
//! üres sorok kimaradnak (a sorszámok a forrásbeliek, a hézag jelzi a kihagyást), a törzsön belüli megjegyzések és az
//! attribútumok (`#[…]`, `@…`) maradnak; a kihagyott docblockot a fejléc jelzi. `--doc`: a docblockkal együtt;
//! `--raw`: a pontos, szűretlen forrás.

use crate::query::QueryResult;
use std::fmt::Write as _;

use super::Formatter;
use super::body_filter::{LineKind, LineScanner, Syntax, lead_start};
use super::source::{numbered, numbered_line, source_lines};

/// A `--body` nézete.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BodyMode {
    /// Tömör (alapértelmezett): docblock és üres sorok nélkül; a törzsön belüli megjegyzések maradnak.
    #[default]
    Compact,
    /// Mint a tömör, plusz a metódus fölötti docblock / doc-kommentek (fejléc-megjegyzés nélkül).
    Doc,
    /// A pontos forrás (docblockkal, üres és megjegyzés-sorokkal).
    Raw,
}

#[derive(Default)]
pub struct Body {
    pub mode: BodyMode,
}

impl Formatter for Body {
    fn format(&self, rows: &[QueryResult], lang: crate::i18n::Lang) -> String {
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
                let span = MethodSpan {
                    line: method.line,
                    end,
                    has_doc: method.doc.is_some(),
                };
                let source = method_source(&lines, syntax, span, self.mode);
                let parens = super::outline::parens(method);
                write!(
                    out,
                    "== {}  {container}::{}{parens}  L{}-{end}{}",
                    row.graph.file,
                    method.name,
                    method.line,
                    crate::io::protect_mark(&row.graph.file)
                )
                .unwrap();
                if let Some((first, last)) = source.omitted_doc {
                    out.push_str(&omitted_doc_note(lang, first, last));
                }
                out.push('\n');
                crate::notes::body_lines(
                    &mut out,
                    &row.notes,
                    method.container.as_deref(),
                    &method.name,
                );
                out.push_str(&source.text);
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

/// A fejléc végére: a tömör nézet kihagyta a metódus fölötti docblockot (`--doc` hozza vissza).
fn omitted_doc_note(lang: crate::i18n::Lang, first: u32, last: u32) -> String {
    let lines = if first == last {
        format!("L{first}")
    } else {
        format!("L{first}-{last}")
    };
    ltf!(
        lang,
        " (docblock {lines} omitted: --doc)",
        " (docblock {lines} kihagyva: --doc)"
    )
}

/// Egy metódus megjelenített forrása.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct MethodSource {
    /// A számozott sorok.
    pub text: String,
    /// A tömör nézetben kihagyott, metódus fölötti docblock / doc-komment sorok első és utolsó sora.
    pub omitted_doc: Option<(u32, u32)>,
}

/// A metódus számozott forrássorai a nézet szerint; ismeretlen nyelvnél szűrés nélkül (a docblocktól).
/// Tömör nézetben csak a metódus fölötti megjegyzések és az üres sorok esnek ki; a törzsbeliek maradnak.
pub(super) fn method_source(
    lines: &[String],
    syntax: Option<Syntax>,
    span: MethodSpan,
    mode: BodyMode,
) -> MethodSource {
    let Some(syntax) = syntax else {
        return MethodSource {
            text: numbered(lines, doc_start(lines, span.line, span.has_doc), span.end),
            omitted_doc: None,
        };
    };
    let start = lead_start(lines, span.line, syntax, span.has_doc);
    if mode == BodyMode::Raw {
        return MethodSource {
            text: numbered(lines, start, span.end),
            omitted_doc: None,
        };
    }
    let mut scanner = LineScanner::new(syntax);
    let mut source = MethodSource::default();
    for number in start..=span.end {
        let Some(line) = lines.get(number.saturating_sub(1) as usize) else {
            break;
        };
        let kind = scanner.classify(line);
        if mode == BodyMode::Compact && number < span.line && kind == LineKind::Comment {
            let first = source.omitted_doc.map_or(number, |(first, _)| first);
            source.omitted_doc = Some((first, number));
        } else if kind != LineKind::Blank {
            source.text.push_str(&numbered_line(number, line));
        }
    }
    source
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

    fn source(source: &str, path: &str, span: MethodSpan, mode: BodyMode) -> MethodSource {
        method_source(&lines(source), Syntax::for_path(path), span, mode)
    }

    fn render(text: &str, path: &str, span: MethodSpan, mode: BodyMode) -> String {
        source(text, path, span, mode).text
    }

    const RUN: MethodSpan = MethodSpan {
        line: 7,
        end: 19,
        has_doc: true,
    };

    #[test]
    fn compact_drops_docblock_and_blank_lines_keeps_body_comments_and_numbers() {
        let compact = source(PHP, "A.php", RUN, BodyMode::Compact);
        assert_eq!(
            compact.text,
            "     7      #[Route('/run')]\n     8      public function run(): string\n     9      {\n    10          // step one\n    11          $a = 1; // trailing\n    13          $sql = \"SELECT 1\n    14  -- not dropped\n    15  // not dropped either\n    16  \";\n    17          # hash comment\n    18          return $sql;\n    19      }\n"
        );
        assert_eq!(compact.omitted_doc, Some((4, 6)));
    }

    #[test]
    fn doc_adds_the_docblock_raw_is_the_exact_source() {
        let doc = source(PHP, "A.php", RUN, BodyMode::Doc);
        assert!(
            doc.text.starts_with(
                "     4      /**\n     5       * Runs it.\n     6       */\n     7      #[Route"
            ),
            "{doc:?}"
        );
        assert!(
            doc.text.contains("    10          // step one\n"),
            "{doc:?}"
        );
        assert!(!doc.text.contains("\n    12  \n"), "{doc:?}");
        assert_eq!(doc.omitted_doc, None);
        let raw = source(PHP, "A.php", RUN, BodyMode::Raw);
        assert_eq!(raw.text.lines().count(), 16, "{raw:?}");
        assert!(
            raw.text.contains("    10          // step one\n"),
            "{raw:?}"
        );
        assert!(raw.text.contains("\n    12  \n"), "{raw:?}");
        assert_eq!(raw.omitted_doc, None);
    }

    #[test]
    fn rust_keeps_attributes_and_body_comments_drops_doc_comments() {
        let text = "/// Adds.\n#[inline]\n/// More doc.\npub fn add(a: u8) -> u8 {\n    // why\n    a + 1\n}\n";
        let span = MethodSpan {
            line: 4,
            end: 7,
            has_doc: true,
        };
        let compact = source(text, "src/lib.rs", span, BodyMode::Compact);
        assert_eq!(
            compact.text,
            "     2  #[inline]\n     4  pub fn add(a: u8) -> u8 {\n     5      // why\n     6      a + 1\n     7  }\n"
        );
        assert_eq!(compact.omitted_doc, Some((1, 3)));
        assert!(
            render(text, "src/lib.rs", span, BodyMode::Doc)
                .starts_with("     1  /// Adds.\n     2  #[inline]\n     3  /// More doc.\n")
        );
    }

    #[test]
    fn block_comments_inside_the_body_are_kept() {
        let text =
            "/** Doc. */\nfunction f() {\n  /* block\n     comment */\n\n  return 1; // why\n}\n";
        let span = MethodSpan {
            line: 2,
            end: 7,
            has_doc: true,
        };
        let compact = source(text, "a.ts", span, BodyMode::Compact);
        assert_eq!(
            compact.text,
            "     2  function f() {\n     3    /* block\n     4       comment */\n     6    return 1; // why\n     7  }\n"
        );
        assert_eq!(compact.omitted_doc, Some((1, 1)));
    }

    #[test]
    fn no_docblock_means_no_omission_note() {
        let text = "fn add(a: u8) -> u8 {\n    // why\n    a + 1\n}\n";
        let span = MethodSpan {
            line: 1,
            end: 4,
            has_doc: false,
        };
        let compact = source(text, "src/lib.rs", span, BodyMode::Compact);
        assert_eq!(compact.omitted_doc, None);
        assert!(compact.text.contains("     2      // why\n"), "{compact:?}");
    }

    #[test]
    fn omission_note_follows_the_language() {
        use crate::i18n::Lang;
        assert_eq!(
            omitted_doc_note(Lang::En, 112, 119),
            " (docblock L112-119 omitted: --doc)"
        );
        assert_eq!(
            omitted_doc_note(Lang::Hu, 28, 28),
            " (docblock L28 kihagyva: --doc)"
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
