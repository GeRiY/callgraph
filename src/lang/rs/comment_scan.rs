//! Kommentek kigyűjtése a Rust-forrásból (a `syn` a sima kommenteket eldobja): egy kis lexer, ami
//! átugorja a string-, raw-string- és karakter-literálokat, és kezeli a beágyazott blokk-kommenteket.

/// Egy (nem doc-) komment: kezdősor és a jelölők nélküli, egy sorba vont szöveg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawComment {
    pub line: u32,
    pub text: String,
}

/// Az összes sima komment (a `///`, `//!`, `/** */`, `/*! */` doc-kommentek nélkül), forrás-sorrendben.
pub(crate) fn scan(source: &str) -> Vec<RawComment> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                let end = source[i..].find('\n').map_or(bytes.len(), |n| i + n);
                let body = &source[i + 2..end];
                let doc =
                    (body.starts_with('/') && !body.starts_with("//")) || body.starts_with('!');
                if !doc {
                    out.push(RawComment {
                        line,
                        text: body.trim().to_owned(),
                    });
                }
                i = end;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let end = block_end(bytes, i);
                let body = source
                    .get(i + 2..end.saturating_sub(2).max(i + 2))
                    .unwrap_or("");
                let doc = (body.starts_with('*') && !body.starts_with("**") && body != "*")
                    || body.starts_with('!');
                if !doc {
                    out.push(RawComment {
                        line,
                        text: clean_block(body),
                    });
                }
                line += count_newlines(&bytes[i..end]);
                i = end;
            }
            b'"' => {
                let end = string_end(bytes, i + 1);
                line += count_newlines(&bytes[i..end]);
                i = end;
            }
            b'r' if is_raw_string_start(bytes, i) => {
                let end = raw_string_end(bytes, i + 1);
                line += count_newlines(&bytes[i..end]);
                i = end;
            }
            b'\'' => i = char_literal_end(bytes, i),
            _ => i += 1,
        }
    }
    out
}

fn count_newlines(bytes: &[u8]) -> u32 {
    bytes.iter().filter(|&&b| b == b'\n').count() as u32
}

/// A `/*` utáni (beágyazást követő) záró `*/` utáni index.
fn block_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < bytes.len() {
        match (bytes[i], bytes[i + 1]) {
            (b'/', b'*') => {
                depth += 1;
                i += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += 1,
        }
    }
    bytes.len()
}

/// A `"…"` literál záró idézőjele utáni index (`\` escape-ekkel).
fn string_end(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// `r"`, `r#"`, `br"`, `cr#"` — az `r` előtt nem állhat azonosító-karakter (a `b`/`c` előtag kivételével).
fn is_raw_string_start(bytes: &[u8], i: usize) -> bool {
    let before = i.checked_sub(1).map(|p| bytes[p]);
    let prefix_ok = match before {
        None => true,
        Some(b'b' | b'c') => i < 2 || !is_ident(bytes[i - 2]),
        Some(b) => !is_ident(b),
    };
    let mut j = i + 1;
    while bytes.get(j) == Some(&b'#') {
        j += 1;
    }
    prefix_ok && bytes.get(j) == Some(&b'"')
}

fn raw_string_end(bytes: &[u8], mut i: usize) -> usize {
    let mut hashes = 0;
    while bytes.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'"'
            && bytes[i + 1..]
                .iter()
                .take(hashes)
                .filter(|&&b| b == b'#')
                .count()
                == hashes
        {
            return i + 1 + hashes;
        }
        i += 1;
    }
    bytes.len()
}

/// Karakter-literál (`'a'`, `'\n'`, `'é'`) vagy élettartam (`'a`): az utána következő index.
fn char_literal_end(bytes: &[u8], i: usize) -> usize {
    if bytes.get(i + 1) == Some(&b'\\') {
        let mut j = i + 2;
        while j < bytes.len() && bytes[j] != b'\'' && bytes[j] != b'\n' {
            j += 1;
        }
        return (j + 1).min(bytes.len());
    }
    // Egy (akár többbájtos) karakter után záró aposztróf → literál; különben élettartam.
    let width = bytes.get(i + 1).map_or(1, |&b| {
        if b < 0x80 {
            1
        } else if b >= 0xF0 {
            4
        } else if b >= 0xE0 {
            3
        } else {
            2
        }
    });
    if bytes.get(i + 1 + width) == Some(&b'\'') && bytes.get(i + 1) != Some(&b'\n') {
        i + 2 + width
    } else {
        i + 1
    }
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Blokk-komment szövege: soronként a `*` előtag nélkül, egy sorba vonva.
fn clean_block(body: &str) -> String {
    body.lines()
        .map(|l| {
            let l = l.trim();
            l.strip_prefix('*').map_or(l, str::trim_start)
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str) -> Vec<(u32, String)> {
        scan(src).into_iter().map(|c| (c.line, c.text)).collect()
    }

    #[test]
    fn line_and_block_comments_without_docs() {
        let src =
            "/// doc\n//! inner\n// plain one\nfn a() {} /* block\n * two */\n//// four slashes\n";
        assert_eq!(
            texts(src),
            [
                (3, "plain one".to_owned()),
                (4, "block two".to_owned()),
                (6, "// four slashes".to_owned())
            ]
        );
    }

    #[test]
    fn strings_chars_and_lifetimes_are_skipped() {
        let src = "let s = \"// no\";\nlet r = r#\"/* no */\"#;\nlet c = '\"'; // yes\nfn f<'a>(x: &'a str) {} // also\n";
        assert_eq!(texts(src), [(3, "yes".to_owned()), (4, "also".to_owned())]);
    }

    #[test]
    fn nested_block_comment_and_line_count() {
        let src = "/* a /* b */ c */\n\n// after\n";
        assert_eq!(texts(src)[1], (3, "after".to_owned()));
    }
}
