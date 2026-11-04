//! HTML-entitás dekódolás (a gyakori nevesített + numerikus alakok).

const NAMED: &[(&str, &str)] = &[
    ("amp", "&"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("apos", "'"),
    ("nbsp", "\u{a0}"),
    ("copy", "\u{a9}"),
    ("reg", "\u{ae}"),
    ("hellip", "\u{2026}"),
    ("mdash", "\u{2014}"),
    ("ndash", "\u{2013}"),
    ("laquo", "\u{ab}"),
    ("raquo", "\u{bb}"),
    ("times", "\u{d7}"),
    ("middot", "\u{b7}"),
    ("bull", "\u{2022}"),
    ("euro", "\u{20ac}"),
    ("rarr", "\u{2192}"),
    ("larr", "\u{2190}"),
    ("trade", "\u{2122}"),
    ("deg", "\u{b0}"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("ldquo", "\u{201c}"),
    ("rdquo", "\u{201d}"),
];

/// `&name;` / `&#NN;` / `&#xHH;` feloldása; ismeretlen alak változatlan marad.
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        match entity_at(rest) {
            Some((text, len)) => {
                out.push_str(&text);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity_at(s: &str) -> Option<(String, usize)> {
    let semi = s.find(';').filter(|&i| i <= 12)?;
    let body = &s[1..semi];
    let text = if let Some(num) = body.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        char::from_u32(code)?.to_string()
    } else {
        NAMED.iter().find(|(n, _)| *n == body)?.1.to_string()
    };
    Some((text, semi + 1))
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_known_and_keeps_unknown() {
        assert_eq!(decode("a&amp;b &#65;&#x42; &foo; &"), "a&b AB &foo; &");
    }
}
