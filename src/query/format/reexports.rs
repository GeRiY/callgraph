//! `--const`: a továbbexportálások (re-exportok) a forrásszövegből — Rust `pub use a::{B, C};` /
//! `pub(crate) use …`, JS/TS `export { A, B as C } from './x'` / `export { A }`, PHP `use const App\NAME;`.
//! A gráf ezeket nem tárolja (a cache és a JSON alakja változatlan), ezért a formázó a fájlból olvassa ki.

use regex::Regex;
use std::sync::LazyLock;

use super::body_filter::{LineKind, LineScanner, Syntax};

/// Egy továbbexportált név.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReExport {
    pub line: u32,
    /// A kívülről látható név (`as` után az alias).
    pub name: String,
    /// Az eredeti (alias előtti) név.
    pub original: String,
    /// Honnan: Rust / PHP a teljes út (`state::STATE_FILE`, `App\NAME`); JS az eredeti név és a `from` forrás.
    pub origin: String,
}

static RUST_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^pub\s*(\([^)]*\))?\s*use\s").expect("valid regex"));
static JS_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^export\s+(type\s+)?\{").expect("valid regex"));
static PHP_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^use\s+const\s").expect("valid regex"));

/// A forrás továbbexportálásai közül azok, amelyek neve (vagy eredeti neve) tartalmazza a `filter`-t.
pub(super) fn find(source: &str, syntax: Syntax, filter: &str) -> Vec<ReExport> {
    let start = match syntax {
        Syntax::Rust => &*RUST_START,
        Syntax::Js => &*JS_START,
        Syntax::Php => &*PHP_START,
        Syntax::Swift => return Vec::new(),
    };
    let mut scanner = LineScanner::new(syntax);
    let mut out = Vec::new();
    // a folyamatban lévő (többsoros) utasítás: kezdősor és szöveg
    let mut pending: Option<(u32, String)> = None;
    for (index, line) in source.lines().enumerate() {
        let number = index as u32 + 1;
        if scanner.classify(line) != LineKind::Code {
            continue;
        }
        let statement = match pending.take() {
            Some((first, mut text)) => {
                text.push('\n');
                text.push_str(line);
                (first, text)
            }
            None if start.is_match(line.trim_start()) => (number, line.to_owned()),
            None => continue,
        };
        if complete(&statement.1, syntax) {
            out.extend(
                parse(&statement.1, syntax, statement.0)
                    .into_iter()
                    .filter(|r| r.name.contains(filter) || r.original.contains(filter)),
            );
        } else {
            pending = Some(statement);
        }
    }
    out
}

/// Az utasítás véget ért-e: Rust / PHP `;`, JS a `{…}` lista zárása.
fn complete(text: &str, syntax: Syntax) -> bool {
    match syntax {
        Syntax::Js => text.contains('}'),
        _ => text.contains(';'),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    Sep,
    Open,
    Close,
    Comma,
    Str(String),
    Other,
}

/// Tokenek a sorszámukkal (a `first` kezdősorhoz képest).
fn tokens(text: &str, syntax: Syntax, first: u32) -> Vec<(Token, u32)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut line = first;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80;
        let (token, len) = match c {
            b'\n' => {
                line += 1;
                i += 1;
                continue;
            }
            _ if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b':' if syntax == Syntax::Rust && bytes.get(i + 1) == Some(&b':') => (Token::Sep, 2),
            b'\\' if syntax == Syntax::Php => (Token::Sep, 1),
            b'{' => (Token::Open, 1),
            b'}' => (Token::Close, 1),
            b',' => (Token::Comma, 1),
            b'\'' | b'"' => {
                let end = bytes[i + 1..]
                    .iter()
                    .position(|b| *b == c)
                    .map_or(bytes.len(), |p| i + 1 + p + 1);
                (Token::Str(text[i..end].to_owned()), end - i)
            }
            _ if ident(c) => {
                let len = bytes[i..].iter().take_while(|b| ident(**b)).count();
                (Token::Ident(text[i..i + len].to_owned()), len)
            }
            _ => (Token::Other, 1),
        };
        out.push((token, line));
        i += len;
    }
    out
}

fn parse(text: &str, syntax: Syntax, first: u32) -> Vec<ReExport> {
    let tokens = tokens(text, syntax, first);
    let mut out = Vec::new();
    match syntax {
        Syntax::Js => parse_js(&tokens, &mut out),
        _ => {
            // a bevezetés (`pub(crate) use`, `use const`) átlépése: az út az első `use` után kezdődik
            let is_word = |at: usize, word: &str| matches!(tokens.get(at), Some((Token::Ident(w), _)) if w == word);
            let Some(mut at) = (0..tokens.len()).find(|at| is_word(*at, "use")) else {
                return out;
            };
            at += 1;
            if syntax == Syntax::Php && is_word(at, "const") {
                at += 1;
            }
            let sep = if syntax == Syntax::Php { "\\" } else { "::" };
            let mut cursor = Cursor {
                tokens: &tokens,
                at,
                sep,
            };
            cursor.tree("", &mut out);
        }
    }
    out
}

/// Rust / PHP use-fa bejárója (`a::b::{c, d::{e as f}}`, `App\{A, B}`).
struct Cursor<'a> {
    tokens: &'a [(Token, u32)],
    at: usize,
    sep: &'static str,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|(token, _)| token)
    }

    fn tree(&mut self, prefix: &str, out: &mut Vec<ReExport>) {
        let mut path = prefix.to_owned();
        let mut last: Option<(String, u32)> = None;
        while self.peek() == Some(&Token::Sep) {
            self.at += 1;
        }
        loop {
            match self.tokens.get(self.at) {
                Some((Token::Ident(word), line)) if word != "as" => {
                    if !path.is_empty() && !path.ends_with(self.sep) {
                        path.push_str(self.sep);
                    }
                    path.push_str(word);
                    last = Some((word.clone(), *line));
                    self.at += 1;
                }
                Some((Token::Sep, _)) => self.at += 1,
                Some((Token::Open, _)) => {
                    self.at += 1;
                    if !path.is_empty() && !path.ends_with(self.sep) {
                        path.push_str(self.sep);
                    }
                    self.group(&path, out);
                    return;
                }
                _ => break,
            }
        }
        let Some((name, line)) = last else { return };
        let mut original = name;
        // `self` a csoportban (`state::{self, X}`): a modul maga
        if original == "self" {
            path = path.trim_end_matches("::self").to_owned();
            original = path.rsplit(self.sep).next().unwrap_or_default().to_owned();
        }
        let alias = match (self.peek(), self.tokens.get(self.at + 1)) {
            (Some(Token::Ident(word)), Some((Token::Ident(alias), _))) if word == "as" => {
                Some(alias.clone())
            }
            _ => None,
        };
        if alias.is_some() {
            self.at += 2;
        }
        let shown = alias.unwrap_or_else(|| original.clone());
        out.push(ReExport {
            line,
            name: shown,
            original,
            origin: path,
        });
    }

    fn group(&mut self, prefix: &str, out: &mut Vec<ReExport>) {
        loop {
            match self.peek() {
                None => return,
                Some(Token::Close) => {
                    self.at += 1;
                    return;
                }
                Some(Token::Comma) => self.at += 1,
                Some(Token::Ident(_) | Token::Sep | Token::Open) => {
                    let before = self.at;
                    self.tree(prefix, out);
                    if self.at == before {
                        self.at += 1;
                    }
                }
                Some(_) => self.at += 1,
            }
        }
    }
}

/// JS/TS `export [type] { A, B as C, type D } [from './x']`.
fn parse_js(tokens: &[(Token, u32)], out: &mut Vec<ReExport>) {
    let Some(open) = tokens.iter().position(|(t, _)| *t == Token::Open) else {
        return;
    };
    let close = tokens[open..]
        .iter()
        .position(|(t, _)| *t == Token::Close)
        .map_or(tokens.len(), |p| open + p);
    let source = match (tokens.get(close + 1), tokens.get(close + 2)) {
        (Some((Token::Ident(from), _)), Some((Token::Str(path), _))) if from == "from" => {
            Some(path.clone())
        }
        _ => None,
    };
    for item in tokens[open + 1..close].split(|(t, _)| *t == Token::Comma) {
        let words: Vec<(&str, u32)> = item
            .iter()
            .filter_map(|(t, line)| match t {
                Token::Ident(word) => Some((word.as_str(), *line)),
                _ => None,
            })
            .collect();
        let words = match words.first() {
            Some(("type", _)) if words.len() > 1 && words[1].0 != "as" => &words[1..],
            _ => &words[..],
        };
        let Some(&(name, line)) = words.first() else {
            continue;
        };
        let shown = match words {
            [_, ("as", _), (alias, _), ..] => *alias,
            _ => name,
        };
        let origin = match &source {
            Some(path) => format!("{name} from {path}"),
            None => name.to_owned(),
        };
        out.push(ReExport {
            line,
            name: shown.to_owned(),
            original: name.to_owned(),
            origin,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(source: &str, syntax: Syntax, filter: &str) -> Vec<(u32, String, String)> {
        find(source, syntax, filter)
            .into_iter()
            .map(|r| (r.line, r.name, r.origin))
            .collect()
    }

    fn row(line: u32, name: &str, origin: &str) -> (u32, String, String) {
        (line, name.to_owned(), origin.to_owned())
    }

    #[test]
    fn rust_pub_use_groups_aliases_and_visibility() {
        let source = "use std::fmt;\npub use state::{CheckState, STATE_FILE};\npub(crate) use limits::{\n    MAX_ITEMS as LIMIT,\n    inner::{STATE_MODE},\n};\n// pub use old::STATE_OLD;\npub use crate::a::STATE_B;\nuse private::STATE_C;\n";
        assert_eq!(
            found(source, Syntax::Rust, "STATE"),
            [
                row(2, "STATE_FILE", "state::STATE_FILE"),
                row(5, "STATE_MODE", "limits::inner::STATE_MODE"),
                row(8, "STATE_B", "crate::a::STATE_B"),
            ]
        );
        assert_eq!(
            found(source, Syntax::Rust, "MAX"),
            [row(4, "LIMIT", "limits::MAX_ITEMS")]
        );
        assert_eq!(
            found(source, Syntax::Rust, "CheckState"),
            [row(2, "CheckState", "state::CheckState")]
        );
    }

    #[test]
    fn js_export_lists_with_and_without_source() {
        let source = "import { X } from './x'\nexport { MAX, other as MAX_ALIAS } from './limits'\nexport {\n  type MaxType,\n  MAX_LOCAL,\n}\nexport type { MaxOnly } from \"./t\";\nexport const MAX_DEF = 1\n// export { MAX_COMMENT }\n";
        assert_eq!(
            found(source, Syntax::Js, "Max"),
            [
                row(4, "MaxType", "MaxType"),
                row(7, "MaxOnly", "MaxOnly from \"./t\""),
            ]
        );
        assert_eq!(
            found(source, Syntax::Js, "MAX"),
            [
                row(2, "MAX", "MAX from './limits'"),
                row(2, "MAX_ALIAS", "other from './limits'"),
                row(5, "MAX_LOCAL", "MAX_LOCAL"),
            ]
        );
    }

    #[test]
    fn php_use_const_single_and_grouped() {
        let source = "<?php\nuse App\\Models\\User;\nuse const App\\Config\\MAX_ITEMS;\nuse const \\App\\{MAX_A, Other\\MAX_B as MAX_C};\n";
        assert_eq!(
            found(source, Syntax::Php, "MAX"),
            [
                row(3, "MAX_ITEMS", "App\\Config\\MAX_ITEMS"),
                row(4, "MAX_A", "App\\MAX_A"),
                row(4, "MAX_C", "App\\Other\\MAX_B"),
            ]
        );
    }
}
