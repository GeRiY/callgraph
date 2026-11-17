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

/// A docblock logikus tagjai: a `@`-sorral kezdődnek, a folytatósorok (többsoros shape) hozzáfűződnek.
pub fn logical_tags(raw: &str) -> Vec<String> {
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
    tags
}

/// Egy logikus tag: (név `@` és `@phpstan-` / `@psalm-` előtag nélkül, szöveg).
fn split_tag(tag: &str) -> (&str, &str) {
    let (name, rest) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
    let name = name
        .trim_start_matches("@phpstan-")
        .trim_start_matches("@psalm-")
        .trim_start_matches('@');
    (name, rest.trim())
}

/// A `doc_types_for` nyers-szövegű magja.
pub fn shape_tags(raw: &str) -> Option<String> {
    let tags = logical_tags(raw);
    let mut returns: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    for tag in &tags {
        let (name, rest) = split_tag(tag);
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

/// Pszeudo- és beépített típusok, amelyek nem osztályok (kisbetűvel).
const NON_CLASS: &[&str] = &[
    "array",
    "callable",
    "bool",
    "boolean",
    "float",
    "double",
    "int",
    "integer",
    "iterable",
    "mixed",
    "null",
    "object",
    "string",
    "void",
    "never",
    "resource",
    "false",
    "true",
    "list",
    "scalar",
    "numeric",
    "self",
    "static",
    "parent",
    "class-string",
    "callable-string",
    "non-empty-string",
    "numeric-string",
    "positive-int",
    "negative-int",
    "non-empty-array",
    "non-empty-list",
];

/// A docblock-típus egyértelmű osztálya: `Book`, `?Book`, `Book|null`, `\App\Models\Book`; a több osztályos unió,
/// a generikus / tömb / shape típus és a beépített típus `None`. A név feloldatlan (a hívó oldja fel).
pub fn class_type(ty: &str) -> Option<String> {
    let mut parts = ty
        .trim()
        .split('|')
        .map(|part| part.trim().trim_start_matches('?'))
        .filter(|part| !part.eq_ignore_ascii_case("null"));
    let name = parts.next()?;
    if parts.next().is_some() || name.is_empty() {
        return None;
    }
    let junk = [
        '<', '>', '[', ']', '{', '}', '(', ')', '&', '$', '*', ' ', ':', '\'', '"',
    ];
    if name.contains(junk) || NON_CLASS.contains(&name.to_ascii_lowercase().as_str()) {
        return None;
    }
    let first = name.chars().next()?;
    (first == '\\' || name.contains('\\') || first.is_uppercase()).then(|| name.to_owned())
}

/// Egy iterálható docblock-típus elemének osztálya: `Book[]`, `list<Book>`, `Collection<int, Book>`
/// (az utolsó generikus argumentum). Ha nem egyértelmű, `None`.
pub fn element_class(ty: &str) -> Option<String> {
    let ty = ty.trim();
    if let Some(inner) = ty.strip_suffix("[]") {
        return class_type(inner);
    }
    let (_, args) = ty.split_once('<')?;
    let args = args.strip_suffix('>')?;
    let mut depth = 0i32;
    let mut last = 0;
    for (at, c) in args.char_indices() {
        match c {
            '<' | '{' | '(' | '[' => depth += 1,
            '>' | '}' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => last = at + 1,
            _ => {}
        }
    }
    class_type(&args[last..])
}

/// A docblock `@var` tagjai: (típus, a változó neve `$`-ral, ha meg van adva).
pub fn var_tags(raw: &str) -> Vec<(String, Option<String>)> {
    logical_tags(raw)
        .iter()
        .filter_map(|tag| {
            let (name, rest) = split_tag(tag);
            if name != "var" {
                return None;
            }
            let ty = type_token(rest);
            let var = rest[ty.len()..]
                .split_whitespace()
                .next()
                .filter(|v| v.starts_with('$'))
                .map(str::to_owned);
            (!ty.is_empty()).then(|| (ty.to_owned(), var))
        })
        .collect()
}

/// A docblock `@param` tagjai: (a változó neve `$`-ral, típus).
pub fn param_tags(raw: &str) -> Vec<(String, String)> {
    logical_tags(raw)
        .iter()
        .filter_map(|tag| {
            let (name, rest) = split_tag(tag);
            if name != "param" {
                return None;
            }
            let ty = type_token(rest);
            let var = rest[ty.len()..].split_whitespace().next()?;
            var.starts_with('$')
                .then(|| (var.to_owned(), ty.to_owned()))
        })
        .collect()
}

/// A docblock `@return` típusa.
pub fn return_tag(raw: &str) -> Option<String> {
    logical_tags(raw).iter().find_map(|tag| {
        let (name, rest) = split_tag(tag);
        (name == "return").then(|| type_token(rest).to_owned())
    })
}

/// Az osztály-docblock `@mixin` típusai (feloldatlan nevek).
pub fn mixin_tags(raw: &str) -> Vec<String> {
    logical_tags(raw)
        .iter()
        .filter_map(|tag| {
            let (name, rest) = split_tag(tag);
            (name == "mixin").then(|| type_token(rest).to_owned())
        })
        .collect()
}

/// Az osztály-docblock `@property` / `@property-read` / `@property-write` tagjai: (név `$` nélkül, típus).
pub fn property_tags(raw: &str) -> Vec<(String, String)> {
    logical_tags(raw)
        .iter()
        .filter_map(|tag| {
            let (name, rest) = split_tag(tag);
            if !matches!(name, "property" | "property-read" | "property-write") {
                return None;
            }
            let ty = type_token(rest);
            let var = rest[ty.len()..].split_whitespace().next()?;
            Some((var.strip_prefix('$')?.to_owned(), ty.to_owned()))
        })
        .collect()
}

/// Az osztály-docblock `@method [static] [Típus] név(…)` tagjai: (metódusnév, visszatérési típus, ha van).
pub fn method_tags(raw: &str) -> Vec<(String, Option<String>)> {
    logical_tags(raw)
        .iter()
        .filter_map(|tag| {
            let (name, rest) = split_tag(tag);
            if name != "method" {
                return None;
            }
            let rest = rest
                .strip_prefix("static")
                .filter(|r| r.starts_with(char::is_whitespace))
                .map_or(rest, str::trim_start);
            let head = rest[..rest.find('(')?].trim();
            let (ret, method) = match head.rsplit_once(char::is_whitespace) {
                Some((ret, method)) => (Some(ret.trim().to_owned()), method),
                None => (None, head),
            };
            let valid = !method.is_empty()
                && method.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !method.starts_with(|c: char| c.is_ascii_digit());
            valid.then(|| (method.to_owned(), ret))
        })
        .collect()
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
    crate::core::text::collapse_ws(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_type_is_unambiguous_only() {
        assert_eq!(class_type("Book").as_deref(), Some("Book"));
        assert_eq!(class_type("?Book").as_deref(), Some("Book"));
        assert_eq!(class_type("Book|null").as_deref(), Some("Book"));
        assert_eq!(
            class_type("null|\\App\\Book").as_deref(),
            Some("\\App\\Book")
        );
        for no in [
            "int",
            "string|null",
            "Book|Order",
            "Book[]",
            "Collection<Book>",
            "array",
            "mixed",
            "self",
            "$this",
        ] {
            assert_eq!(class_type(no), None, "{no}");
        }
    }

    #[test]
    fn element_class_of_iterables() {
        assert_eq!(element_class("Book[]").as_deref(), Some("Book"));
        assert_eq!(element_class("Collection<Book>").as_deref(), Some("Book"));
        assert_eq!(
            element_class("Collection<int, Book>").as_deref(),
            Some("Book")
        );
        assert_eq!(element_class("list<int>"), None);
        assert_eq!(element_class("Book"), None);
    }

    #[test]
    fn tag_parsers() {
        let raw = "/**\n * @var ?Book $book\n * @param Book|null $b the book\n * @return Book\n * @mixin \\App\\Models\\Book\n * @property-read Book $book\n * @method static Builder stocked()\n * @method Book find(int $id)\n * @method sync()\n */";
        assert_eq!(
            var_tags(raw),
            vec![("?Book".to_string(), Some("$book".to_string()))]
        );
        assert_eq!(
            param_tags(raw),
            vec![("$b".to_string(), "Book|null".to_string())]
        );
        assert_eq!(return_tag(raw).as_deref(), Some("Book"));
        assert_eq!(mixin_tags(raw), vec!["\\App\\Models\\Book".to_string()]);
        assert_eq!(
            property_tags(raw),
            vec![("book".to_string(), "Book".to_string())]
        );
        assert_eq!(
            method_tags(raw),
            vec![
                ("stocked".to_string(), Some("Builder".to_string())),
                ("find".to_string(), Some("Book".to_string())),
                ("sync".to_string(), None),
            ]
        );
        assert_eq!(
            var_tags("/** @var Book */"),
            vec![("Book".to_string(), None)]
        );
    }

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
