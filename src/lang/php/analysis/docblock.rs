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

/// A node előtti docblock `@return` / `@param` típusa, ha pontosabb a deklaráltnál (shape `array{…}`, generikus
/// `list<int>` / `Collection<Order>`, tömb-típus `int[]`): `@return array{updated:int}; @param $ids list<int>`,
/// ≤80 karakter (`…`-tal csonkolva). Ha nincs ilyen, `None`.
pub fn doc_types_for(parsed: &ParsedFile<'_>, span: Span) -> Option<String> {
    doc_comment_before(parsed, span.start.offset).and_then(|raw| shape_tags(&raw))
}

/// A `doc_types_for` nyers-szövegű magja.
pub fn shape_tags(raw: &str) -> Option<String> {
    // logikus tagek: a `@`-sorral kezdődnek, a folytatósorok (többsoros shape) hozzáfűződnek
    let mut tags: Vec<String> = Vec::new();
    for line in raw.split('\n') {
        let l = line.trim();
        let l = l
            .strip_prefix("/**")
            .or_else(|| l.strip_prefix("/*"))
            .unwrap_or(l);
        let l = l.strip_suffix("*/").unwrap_or(l);
        let l = l.trim().trim_start_matches('*').trim();
        if l.starts_with('@') {
            tags.push(l.to_owned());
        } else if !l.is_empty()
            && let Some(last) = tags.last_mut()
        {
            last.push(' ');
            last.push_str(l);
        }
    }
    let mut returns: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    for tag in &tags {
        let (name, rest) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        let name = name
            .trim_start_matches("@phpstan-")
            .trim_start_matches("@psalm-")
            .trim_start_matches('@');
        let rest = rest.trim();
        let ty = type_token(rest);
        if !is_specific(ty) {
            continue;
        }
        match name {
            "return" => returns = Some(format!("@return {}", squash(ty))),
            "param" => {
                let var = rest[ty.len()..].split_whitespace().next().unwrap_or("");
                if var.starts_with('$') || var.starts_with("...$") {
                    params.push(format!("@param {var} {}", squash(ty)));
                }
            }
            _ => {}
        }
    }
    let joined = returns
        .into_iter()
        .chain(params)
        .collect::<Vec<_>>()
        .join("; ");
    if joined.is_empty() {
        return None;
    }
    Some(if joined.chars().count() > 80 {
        let head: String = joined.chars().take(79).collect();
        format!("{head}…")
    } else {
        joined
    })
}

/// A tag szövegének első típus-tokenje: a nulla mélységű szóközig (a `{}` / `<>` / `()` / `[]` párokat egyben).
fn type_token(text: &str) -> &str {
    let mut depth = 0i32;
    for (at, c) in text.char_indices() {
        match c {
            '{' | '<' | '(' | '[' => depth += 1,
            '}' | '>' | ')' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => return &text[..at],
            _ => {}
        }
    }
    text
}

/// Pontosabb-e a docblock típusa a deklarálható PHP-típusnál (shape, generikus, `T[]`).
fn is_specific(ty: &str) -> bool {
    ty.contains('{') || ty.contains('<') || ty.contains("[]")
}

/// Az egymást követő szóközök egyre vonva (a többsoros shape egy sorba).
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{clean_doc_comment, shape_tags};

    #[test]
    fn shape_tags_keep_only_specific_types() {
        let raw = "/**\n * @param int $a\n * @param list<int> $ids ids\n * @return array{updated:int,\n *   skipped:int}\n */";
        assert_eq!(
            shape_tags(raw).as_deref(),
            Some("@return array{updated:int, skipped:int}; @param $ids list<int>")
        );
        assert_eq!(shape_tags("/** @return int */"), None);
    }


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
