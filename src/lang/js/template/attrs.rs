//! Nyitó tag és attribútumok tűrő szkennelése (statikus attribútum, `v-`/`:`/`@`/`#`/`.` direktíva).

use super::entities::decode;
use super::node::Prop;
use crate::lang::js::LineIndex;

/// A beolvasott nyitó tag.
pub struct OpenTag {
    pub tag: String,
    pub props: Vec<Prop>,
    /// A záró `>` indexe.
    pub gt: usize,
    pub self_closing: bool,
}

pub fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn ends_section(b: u8) -> bool {
    is_ws(b) || b == b'/' || b == b'>'
}

/// A `<tag ...>` beolvasása az `at` (`<`) helyről; `None`, ha a tag nem záródik a `limit`-ig.
pub fn parse_open_tag(
    src: &str,
    at: usize,
    limit: usize,
    lines: &LineIndex,
    in_pre: bool,
) -> Option<OpenTag> {
    let b = src.as_bytes();
    let mut i = at + 1;
    while i < limit && !ends_section(b[i]) {
        i += 1;
    }
    let tag = src[at + 1..i].to_string();
    let mut props = Vec::new();
    loop {
        while i < limit && is_ws(b[i]) {
            i += 1;
        }
        if i >= limit {
            return None;
        }
        match b[i] {
            b'>' => {
                return Some(OpenTag {
                    tag,
                    props,
                    gt: i,
                    self_closing: false,
                });
            }
            b'/' if i + 1 < limit && b[i + 1] == b'>' => {
                return Some(OpenTag {
                    tag,
                    props,
                    gt: i + 1,
                    self_closing: true,
                });
            }
            b'/' => i += 1,
            _ => {
                let (prop, next) = parse_attr(src, i, limit, lines, in_pre);
                props.push(prop);
                i = next;
            }
        }
    }
}

fn parse_attr(
    src: &str,
    start: usize,
    limit: usize,
    lines: &LineIndex,
    in_pre: bool,
) -> (Prop, usize) {
    let b = src.as_bytes();
    let line = lines.line_of(start);
    let first = b[start];
    let directive = !in_pre
        && ((first == b'v' && start + 1 < limit && b[start + 1] == b'-')
            || matches!(first, b'.' | b':' | b'@' | b'#'));
    let mut i = start;
    let name;
    let mut arg = None;
    if directive {
        if first == b'v' {
            i = scan_until(b, start, limit, |c| {
                matches!(c, b'=' | b'.' | b':') || ends_section(c)
            });
            name = src[start + 2..i].to_string();
        } else {
            name = match first {
                b'@' => "on",
                b'#' => "slot",
                _ => "bind",
            }
            .to_string();
        }
        let has_arg = if first == b'v' {
            b.get(i) == Some(&b':')
        } else {
            true
        };
        if has_arg && i < limit {
            let arg_start = if first == b'v' { i + 1 } else { start + 1 };
            let (a, next) = scan_arg(src, arg_start, limit);
            arg = Some(a);
            i = next;
        }
        while i < limit && b[i] == b'.' {
            i = scan_until(b, i + 1, limit, |c| {
                matches!(c, b'=' | b'.') || ends_section(c)
            });
        }
    } else {
        i = scan_until(b, start + 1, limit, |c| c == b'=' || ends_section(c));
        name = src[start..i].to_string();
    }
    let (value, next) = parse_value(src, i, limit);
    let prop = if directive {
        Prop::Dir {
            line,
            name,
            arg,
            exp: value,
        }
    } else {
        let value = match value {
            Some(v) if name == "class" => Some(collapse_ws(&v)),
            other => other,
        };
        Prop::Attr { line, value }
    };
    (prop, next.max(start + 1))
}

fn scan_until(b: &[u8], mut i: usize, limit: usize, stop: impl Fn(u8) -> bool) -> usize {
    while i < limit && !stop(b[i]) {
        i += 1;
    }
    i
}

/// Direktíva-argumentum: `[dinamikus]` vagy statikus (`.`/`=`/szakaszvég előtt).
fn scan_arg(src: &str, from: usize, limit: usize) -> (String, usize) {
    let b = src.as_bytes();
    if from < limit && b[from] == b'[' {
        let close = scan_until(b, from + 1, limit, |c| c == b']');
        let end = (close + 1).min(limit);
        return (src[from + 1..close].to_string(), end);
    }
    let end = scan_until(b, from, limit, |c| {
        matches!(c, b'=' | b'.') || ends_section(c)
    });
    (src[from..end].to_string(), end)
}

/// Az `=` utáni érték (idézett / idézetlen), entitás-dekódolva; a következő pozícióval.
fn parse_value(src: &str, from: usize, limit: usize) -> (Option<String>, usize) {
    let b = src.as_bytes();
    let mut i = from;
    while i < limit && is_ws(b[i]) {
        i += 1;
    }
    if i >= limit || b[i] != b'=' {
        return (None, from);
    }
    i += 1;
    while i < limit && is_ws(b[i]) {
        i += 1;
    }
    if i >= limit {
        return (Some(String::new()), i);
    }
    if b[i] == b'"' || b[i] == b'\'' {
        let q = b[i];
        let close = scan_until(b, i + 1, limit, |c| c == q);
        let value = decode(&src[i + 1..close]);
        return (Some(value), (close + 1).min(limit));
    }
    let end = scan_until(b, i, limit, |c| is_ws(c) || c == b'>');
    (Some(decode(&src[i..end])), end)
}

/// A `class` érték: whitespace-összevonás és vágás (mint a Vue parserben).
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
