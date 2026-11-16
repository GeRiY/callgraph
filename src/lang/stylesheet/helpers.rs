/// A fejléc egy sorra, egyszeres szóközökkel; a `/* … */` megjegyzések nélkül.
pub(crate) fn clean_header(raw: &str) -> String {
    strip_comments(raw)
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A `/* … */` megjegyzések szóközökre cserélve (a bájthossz marad); a string-literálon belüli `/*`
/// (`@source '../**/*.blade.php'`) nem megjegyzés, és a nem ASCII karakterek épen maradnak.
pub(crate) fn strip_comments(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    // az eddig még át nem másolt rész eleje
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                result.push_str(&raw[copied..i]);
                let end = raw[i + 2..]
                    .find("*/")
                    .map_or(raw.len(), |end| i + 2 + end + 2);
                result.extend(std::iter::repeat_n(' ', end - i));
                i = end;
                copied = end;
            }
            _ => i += 1,
        }
    }
    result.push_str(&raw[copied..]);
    result
}

/// Sorvégi `//` megjegyzés (SCSS) kezdődik-e a `bytes[i]`-nél: nem URL (`http://`, `url(//cdn)`), vagyis az előző
/// bájt nem `:` / `(` / `/`.
pub(crate) fn is_line_comment(bytes: &[u8], i: usize) -> bool {
    bytes.get(i) == Some(&b'/')
        && bytes.get(i + 1) == Some(&b'/')
        && (i == 0 || !matches!(bytes[i - 1], b':' | b'(' | b'/'))
}

/// A `//` sorvégi megjegyzések szóközökre cserélve (a bájthossz marad); a string-literálon és a `/* … */`
/// megjegyzésen belüli `//` nem számít.
pub(crate) fn strip_line_comments(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut result = String::with_capacity(raw.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = raw[i + 2..]
                    .find("*/")
                    .map_or(raw.len(), |end| i + 2 + end + 2);
            }
            b'/' if is_line_comment(bytes, i) => {
                let end = raw[i..].find('\n').map_or(raw.len(), |end| i + end);
                result.push_str(&raw[copied..i]);
                result.extend(std::iter::repeat_n(' ', end - i));
                copied = end;
                i = end;
            }
            _ => i += 1,
        }
    }
    result.push_str(&raw[copied..]);
    result
}

pub(crate) fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'-' || b == b'_'
}

pub(crate) fn is_ident(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_comments_are_blanked_but_urls_and_strings_stay() {
        let raw = "a: url(http://x/y); // note 'q\nb: \"//keep\"; /* // in block */ c";
        let out = strip_line_comments(raw);
        assert_eq!(out.len(), raw.len());
        assert!(out.contains("http://x/y"), "{out}");
        assert!(out.contains("\"//keep\""), "{out}");
        assert!(out.contains("// in block"), "{out}");
        assert!(!out.contains("note"), "{out}");
    }
}
