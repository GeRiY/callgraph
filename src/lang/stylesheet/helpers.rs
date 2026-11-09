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

pub(crate) fn line(source: &str, offset: usize) -> u32 {
    (source.as_bytes()[..offset.min(source.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1) as u32
}

pub(crate) fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'-' || b == b'_'
}

pub(crate) fn is_ident(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}
