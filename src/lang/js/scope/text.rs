//! Szöveg-segédek a JS-motor (Babel + JS string) szemantikájához: UTF-16 hosszak, `srcOf`, trim.

/// A JS `String.length`: UTF-16 kódegységek száma.
pub(crate) fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// A JS `s.slice(0, max)`: legfeljebb `max` UTF-16 egység (karakterhatáron vág).
pub(crate) fn truncate_utf16(s: &str, max: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > max {
            return &s[..i];
        }
    }
    s
}

/// A JS `String.prototype.trim` szóköz-készlete (U+FEFF igen, U+0085 nem).
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// A `srcOf(code, node, max)`: whitespace összevonva, `max` fölött `max-1` egység + `…`.
pub(crate) fn src_of(src: &str, start: u32, end: u32, max: usize) -> String {
    let raw = src.get(start as usize..end as usize).unwrap_or("");
    let s = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if utf16_len(&s) > max {
        format!("{}…", truncate_utf16(&s, max - 1))
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_by_utf16_units() {
        assert_eq!(truncate_utf16("aé😀b", 3), "aé");
        assert_eq!(utf16_len("a😀"), 3);
    }

    #[test]
    fn src_of_collapses_and_shortens() {
        assert_eq!(src_of("a  \n b", 0, 6, 100), "a b");
        assert_eq!(src_of("abcdefgh", 0, 8, 5), "abcd…");
    }
}
