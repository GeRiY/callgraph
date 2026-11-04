//! Docblock- és forrásrészlet-segédek (a `docOf` / `srcOf` / container-doc szabályai).

use std::sync::LazyLock;

use oxc_ast::ast::Comment;
use regex::Regex;

use crate::lang::js::lines::LineIndex;

/// A blokk kommentjei és forrása; a vezető docblockot a csomópont kezdő offsetjéből találja meg.
pub struct Docs<'a> {
    pub source: &'a str,
    pub comments: &'a [Comment],
}

impl Docs<'_> {
    /// A `start` offsetű csomópont vezető `/** */` docblockja (az utolsó), szóközökkel összevonva, ≤300 kar.
    pub fn doc_of(&self, start: u32) -> Option<String> {
        let comment = self
            .comments
            .iter()
            .filter(|c| c.is_leading() && c.is_block() && c.attached_to == start)
            .rfind(|c| self.value(c).starts_with('*'))?;
        let value = self.value(comment).trim_start_matches('*');
        let joined = value
            .lines()
            .map(|l| strip_star(l.trim_start()).trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        (!joined.is_empty()).then(|| truncate(&joined, 300))
    }

    /// A kommentet a `/*` és `*/` nélkül.
    fn value(&self, c: &Comment) -> &str {
        let s = c.content_span();
        &self.source[s.start as usize..s.end as usize]
    }

    /// A forrásrészlet egy sorba húzva, `max` karakterre vágva (`…`-tal).
    pub fn src_of(&self, start: u32, end: u32, max: usize) -> String {
        let text = self.source[start as usize..end as usize]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if text.chars().count() > max {
            let cut: String = text.chars().take(max - 1).collect();
            format!("{cut}…")
        } else {
            text
        }
    }
}

/// A sor elején álló `*` és az azt követő egy szóköz levágása.
fn strip_star(line: &str) -> &str {
    match line.strip_prefix('*') {
        Some(rest) => rest.strip_prefix(char::is_whitespace).unwrap_or(rest),
        None => line,
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A container (modul / komponens) doc-ja: az első utasítás docblockja, különben az első blokk-komment,
/// ha a blokk első során vagy a következőn kezdődik.
pub fn container_doc(
    docs: &Docs<'_>,
    first_stmt_start: Option<u32>,
    lines: &LineIndex,
) -> Option<String> {
    if let Some(doc) = first_stmt_start.and_then(|s| docs.doc_of(s)) {
        return Some(doc);
    }
    let first = docs.comments.first().filter(|c| c.is_block())?;
    if lines.line_of(first.span.start as usize) > lines.line_of(0) + 1 {
        return None;
    }
    let value = docs.value(first).trim_start_matches('*');
    let text = collapse_stars(value);
    (!text.is_empty()).then(|| truncate(&text, 300))
}

/// `replace(/\s*\*\s?/g, ' ')` majd trim.
fn collapse_stars(value: &str) -> String {
    static STAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\*\s?").expect("static regex"));
    STAR.replace_all(value, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn doc_at(src: &str, start: u32) -> Option<String> {
        let alloc = Allocator::default();
        let ret = Parser::new(&alloc, src, SourceType::jsx()).parse();
        Docs {
            source: src,
            comments: &ret.program.comments,
        }
        .doc_of(start)
    }

    #[test]
    fn joins_docblock_lines() {
        let src = "/**\n * One\n * two\n */\nfoo()";
        assert_eq!(doc_at(src, 22).as_deref(), Some("One two"));
    }

    #[test]
    fn plain_block_is_no_doc() {
        assert_eq!(doc_at("/* x */ foo()", 8), None);
    }

    #[test]
    fn src_of_truncates() {
        let d = Docs {
            source: "a   b c d",
            comments: &[],
        };
        assert_eq!(d.src_of(0, 9, 5), "a b …");
    }
}
