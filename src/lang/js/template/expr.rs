//! Direktíva-kifejezés hívásai: a régi motor `exprCalls` regexe (lookbehind kézzel).

use regex::Regex;
use std::sync::OnceLock;

/// Egy regex-találat: a hívott név és az első literál argumentum.
pub struct ExprCall {
    pub name: String,
    /// A teljes idézett argumentum (`'x'`), ha van.
    pub quoted: Option<String>,
    /// Az idézőjelek közti szöveg (üres → hiányzónak számít, mint a JS-ben).
    pub literal: Option<String>,
}

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"([A-Za-z_$][A-Za-z0-9_$]*(?:\.[A-Za-z_$][A-Za-z0-9_$]*)*)\s*\(\s*((['"`])([^'"`]*)['"`])?"#,
        )
        .expect("érvényes regex")
    })
}

fn word_or_dot(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'.')
}

/// `(?<![\w$.])ident(.ident)*\s*\(\s*(['"`]…['"`])?` összes találata.
pub fn expr_calls(exp: &str) -> Vec<ExprCall> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(m) = pattern().captures_at(exp, from) {
        let whole = m.get(0).expect("teljes találat");
        let start = whole.start();
        if start > 0 && word_or_dot(exp.as_bytes()[start - 1]) {
            from = start + exp[start..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        out.push(ExprCall {
            name: m[1].to_string(),
            quoted: m.get(2).map(|q| q.as_str().to_string()),
            literal: m
                .get(4)
                .map(|l| l.as_str().to_string())
                .filter(|l| !l.is_empty()),
        });
        from = whole.end().max(start + 1);
    }
    out
}

/// A régi `pascal()`: `(^|-)(\w)` → nagybetű.
pub fn pascal(tag: &str) -> String {
    let chars: Vec<char> = tag.chars().collect();
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if i == 0 && word(c) {
            out.push(c.to_ascii_uppercase());
            i += 1;
        } else if c == '-' && chars.get(i + 1).is_some_and(|n| word(*n)) {
            out.push(chars[i + 1].to_ascii_uppercase());
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_calls_with_lookbehind() {
        let c = expr_calls("foo.bar('x') + a.b (1) + x.y.z()");
        let names: Vec<_> = c.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["foo.bar", "a.b", "x.y.z"]);
        assert_eq!(c[0].literal.as_deref(), Some("x"));
        assert_eq!(c[0].quoted.as_deref(), Some("'x'"));
    }

    #[test]
    fn pascal_cases() {
        assert_eq!(pascal("my-card"), "MyCard");
        assert_eq!(pascal("div"), "Div");
    }
}
