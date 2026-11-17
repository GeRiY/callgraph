//! A TS / JS szótár-modul (`export default { … }`, `const en: Schema = { … }`) levelei kulcs-útvonalakkal, mint a
//! JSON-nál (`i18n_scan::php_leaves` párja). Hibatűrő: ami nem értelmezhető (számított kulcs, spread, függvény),
//! kimarad vagy érték nélküli levél.

use std::sync::LazyLock;

use regex::Regex;

use super::i18n_scan::Leaf;
use crate::core::LineIndex;
use crate::core::srcscan::{Dialect, strip_comments};

fn default_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\bexport\s+default\s+").expect("érvényes regex"));
    &RE
}

fn const_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(\bexport\s+)?\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=;{]+?)?=\s*(?:[A-Za-z_$][\w$.]*\s*\(\s*)?\{",
        )
        .expect("érvényes regex")
    });
    &RE
}

/// Azonosító- vagy pontozott-név-bájt (`en`, `Foo.bar`).
fn is_path_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'.')
}

/// Kulcs-bájt a nem idézőjeles objektum-kulcsban (`a-b` is, ha JS szerint nem is érvényes azonosító).
fn is_key_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'-')
}

/// A szótár objektum-literáljának nyitó `{` helye a (megjegyzés-mentes) szövegben.
fn dictionary_start(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    if let Some(found) = default_re().find(text) {
        let mut at = found.end();
        // `export default defineLocale({ … })` / `export default {` / `export default en;`
        loop {
            while b.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            match b.get(at) {
                Some(b'{') => return Some(at),
                Some(c) if c.is_ascii_alphabetic() || *c == b'_' || *c == b'$' => {
                    let begin = at;
                    while b.get(at).copied().is_some_and(is_path_byte) {
                        at += 1;
                    }
                    let name = &text[begin..at];
                    let mut after = at;
                    while b.get(after).is_some_and(u8::is_ascii_whitespace) {
                        after += 1;
                    }
                    if b.get(after) == Some(&b'(') {
                        at = after + 1;
                        continue;
                    }
                    return named_object(text, name);
                }
                _ => return None,
            }
        }
    }
    let mut first = None;
    for caps in const_re().captures_iter(text) {
        let Some(whole) = caps.get(0) else { continue };
        let at = whole.end() - 1;
        if caps.get(1).is_some() {
            return Some(at);
        }
        first.get_or_insert(at);
    }
    first
}

/// A `const <name> … = {` literál nyitó `{` helye.
fn named_object(text: &str, name: &str) -> Option<usize> {
    const_re()
        .captures_iter(text)
        .find(|caps| caps.get(2).is_some_and(|m| m.as_str() == name))
        .and_then(|caps| caps.get(0))
        .map(|whole| whole.end() - 1)
}

/// A szótár-modul levelei (`bed.locked`, sor, érték); üres, ha nincs felismerhető szótár.
pub(super) fn script_leaves(text: &str) -> Vec<Leaf> {
    let clean = strip_comments(text, Dialect::Js);
    let Some(start) = dictionary_start(&clean) else {
        return Vec::new();
    };
    let mut parser = Object {
        b: clean.as_bytes(),
        pos: start + 1,
        lines: LineIndex::new(&clean),
        out: Vec::new(),
    };
    parser.items("");
    parser.out
}

struct Object<'a> {
    b: &'a [u8],
    pos: usize,
    lines: LineIndex,
    out: Vec<Leaf>,
}

impl Object<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    /// Idézőjeles (`'`, `"`, `` ` ``) string és hogy egyszerű-e (a sablon-literál `${` után az érték nem ismert).
    fn string(&mut self) -> Option<(String, bool)> {
        let quote = self.peek().filter(|c| matches!(c, b'\'' | b'"' | b'`'))?;
        self.pos += 1;
        let mut out = Vec::new();
        let mut plain = true;
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == b'\\' {
                if let Some(next) = self.peek() {
                    self.pos += 1;
                    out.push(match next {
                        b'n' => b'\n',
                        b't' => b'\t',
                        other => other,
                    });
                }
            } else if c == quote {
                return Some((String::from_utf8_lossy(&out).into_owned(), plain));
            } else {
                if quote == b'`' && c == b'$' && self.peek() == Some(b'{') {
                    plain = false;
                }
                out.push(c);
            }
        }
        None
    }

    /// Egy kifejezés átugrása a legfelső szintű `,` vagy záró zárójelig (a zárójel marad).
    fn skip_expr(&mut self) {
        let mut depth = 0i32;
        while let Some(c) = self.peek() {
            match c {
                b'\'' | b'"' | b'`' => {
                    let _ = self.string();
                    continue;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                b',' if depth == 0 => return,
                _ => {}
            }
            self.pos += 1;
        }
    }

    /// A kulcs a `:` előtt: ismert név, vagy `None` (számított / ismeretlen alak).
    fn key(&mut self) -> Option<String> {
        match self.peek()? {
            b'\'' | b'"' | b'`' => self.string().map(|(text, _)| text),
            b'[' => {
                self.skip_expr();
                None
            }
            c if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$') => {
                let begin = self.pos;
                while self.peek().is_some_and(is_key_byte) {
                    self.pos += 1;
                }
                Some(String::from_utf8_lossy(&self.b[begin..self.pos]).into_owned())
            }
            _ => {
                self.skip_expr();
                None
            }
        }
    }

    /// Az objektum elemei a záró `}`-ig.
    fn items(&mut self, prefix: &str) {
        loop {
            self.ws();
            match self.peek() {
                None => return,
                Some(b'}') => {
                    self.pos += 1;
                    return;
                }
                Some(b',') => {
                    self.pos += 1;
                    continue;
                }
                _ => {}
            }
            let line = self.lines.line_of(self.pos);
            if self.b[self.pos..].starts_with(b"...") {
                self.skip_expr();
                continue;
            }
            let name = self.key();
            self.ws();
            if self.peek() != Some(b':') {
                // rövid alak / metódus: nincs érték, amit olvasni lehet
                self.skip_expr();
                continue;
            }
            self.pos += 1;
            self.ws();
            let path = name.map(|n| {
                if prefix.is_empty() {
                    n
                } else {
                    format!("{prefix}.{n}")
                }
            });
            self.value(path, line);
        }
    }

    fn value(&mut self, path: Option<String>, line: u32) {
        match self.peek() {
            Some(b'{') => {
                self.pos += 1;
                match path {
                    Some(path) => self.items(&path),
                    None => self.skip_object(),
                }
            }
            Some(b'\'' | b'"' | b'`') => {
                let text = self.string();
                self.ws();
                let simple = matches!(self.peek(), None | Some(b',' | b'}'));
                if !simple {
                    self.skip_expr();
                }
                if let Some(path) = path {
                    let value = text
                        .filter(|(_, plain)| *plain && simple)
                        .map(|(text, _)| text);
                    self.out.push((path, line, value));
                }
            }
            _ => {
                self.skip_expr();
                if let Some(path) = path {
                    self.out.push((path, line, None));
                }
            }
        }
    }

    /// Az ismeretlen kulcsú objektum átugrása (a nyitó `{` után áll), a záró `}`-ig.
    fn skip_object(&mut self) {
        let mut depth = 1i32;
        while let Some(c) = self.peek() {
            match c {
                b'\'' | b'"' | b'`' => {
                    let _ = self.string();
                    continue;
                }
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.pos += 1;
                        return;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::script_leaves;

    fn keys(text: &str) -> Vec<String> {
        script_leaves(text).into_iter().map(|l| l.0).collect()
    }

    #[test]
    fn export_default_object_with_nesting_and_comments() {
        let text =
            "// head\nexport default {\n  a: { b: 'x', 'c-d': \"y\" }, // tail\n  e: `z`,\n};\n";
        assert_eq!(keys(text), ["a.b", "a.c-d", "e"]);
        let leaves = script_leaves(text);
        assert_eq!(leaves[0].1, 3);
        assert_eq!(leaves[0].2.as_deref(), Some("x"));
    }

    #[test]
    fn typed_const_and_default_reference() {
        let text = "const en: Schema = { a: { b: 'x' } };\nexport default en;\n";
        assert_eq!(keys(text), ["a.b"]);
        assert_eq!(keys("export const de = { k: 'v' } as const;"), ["k"]);
    }

    #[test]
    fn dynamic_parts_are_skipped_or_valueless() {
        let text = "export default {\n ...base,\n [k]: 'q',\n f: (n) => `${n}`,\n g: 'a' + b,\n h: `x ${y}`,\n ok: 'v',\n};";
        let leaves = script_leaves(text);
        let names: Vec<&str> = leaves.iter().map(|l| l.0.as_str()).collect();
        assert_eq!(names, ["f", "g", "h", "ok"]);
        assert!(leaves[..3].iter().all(|l| l.2.is_none()));
        assert_eq!(leaves[3].2.as_deref(), Some("v"));
    }

    #[test]
    fn no_dictionary_yields_nothing() {
        assert!(script_leaves("export function t(k) { return k; }").is_empty());
    }
}
