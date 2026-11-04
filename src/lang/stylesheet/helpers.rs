pub(crate) fn clean_header(raw: &str) -> String {
    strip_comments(raw)
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn strip_comments(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                result.push(' ');
                i += 1;
            }
            if i + 1 < bytes.len() {
                result.push(' ');
                result.push(' ');
                i += 2;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
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
