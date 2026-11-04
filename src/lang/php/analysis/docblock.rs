//! Docblock-segéd: a node előtti `/** … */` megkeresése a trivia-listából és a
//! `cleanDocComment` (ast_graph.php) szerinti tisztítás. Más visitor (pl. properties) is használja.

use mago_span::Span;
use mago_syntax::cst::TriviaKind;

use crate::lang::php::parser::ParsedFile;

/// A `start_offset` előtt álló docblock nyers szövege (nikic `getDocComment` megfelelő).
///
/// A node elé közvetlenül (token nélkül) csatlakozó trivia-sorozatból az utolsó
/// `/** … */` komment; köztük whitespace és más kommentek lehetnek.
pub fn doc_comment_before(parsed: &ParsedFile<'_>, start_offset: u32) -> Option<String> {
    let trivia = parsed.program.trivia.as_slice();
    // Az utolsó trivia, ami a node kezdete előtt végződik (rendezett lista → bináris keresés).
    let upto = trivia.partition_point(|t| t.span.end.offset <= start_offset);
    let mut edge = start_offset;
    for t in trivia[..upto].iter().rev() {
        if t.span.end.offset != edge {
            return None; // token áll közte → nem ehhez a node-hoz tartozik
        }
        if matches!(t.kind, TriviaKind::DocBlockComment) {
            return Some(parsed.text(t.span));
        }
        edge = t.span.start.offset;
    }
    None
}

/// A node `span`-je előtti docblock tisztított összefoglalója (`cleanDocComment`).
pub fn doc_for(parsed: &ParsedFile<'_>, span: Span) -> Option<String> {
    doc_comment_before(parsed, span.start.offset).and_then(|raw| clean_doc_comment(&raw))
}

/// Docblock összefoglaló: a `@tag` sorok előtti szöveg egy sorba fűzve, a komment-jelölők nélkül.
pub fn clean_doc_comment(raw: &str) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    for line in raw.split('\n') {
        let mut l = line.trim();
        l = l
            .strip_prefix("/**")
            .or_else(|| l.strip_prefix("/*"))
            .unwrap_or(l);
        l = l.strip_suffix("*/").unwrap_or(l);
        l = l
            .strip_prefix('*')
            .map_or(l, |r| r.strip_prefix(' ').unwrap_or(r));
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        if l.starts_with('@') {
            break;
        }
        lines.push(l);
    }
    let summary = lines.join(" ");
    let summary = summary.trim();
    (!summary.is_empty()).then(|| summary.to_string())
}

#[cfg(test)]
mod tests {
    use super::clean_doc_comment;

    #[test]
    fn joins_summary_and_stops_at_tag() {
        let raw = "/**\n * Első sor.\n * Második.\n *\n * @param int $a\n */";
        assert_eq!(
            clean_doc_comment(raw).as_deref(),
            Some("Első sor. Második.")
        );
    }

    #[test]
    fn single_line_and_empty() {
        assert_eq!(clean_doc_comment("/** Szia */").as_deref(), Some("Szia"));
        assert_eq!(clean_doc_comment("/** @return int */"), None);
        assert_eq!(clean_doc_comment("/** */"), None);
    }
}
