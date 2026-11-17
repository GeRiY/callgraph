//! Közös szöveg-segédek: a nyelvek és nézetek korábban mindegyik a saját másolatát tartotta a vágásra, a
//! szóköz-összevonásra és a sor-számolásra. Itt egy helyen vannak; a hívóhelyek megfigyelhető kimenete nem változik
//! (a vágás „…”-tal vagy anélkül külön függvény).

/// A szóközök (sortörés, tab, több szóköz) egy szóközre vonva, a széleken levágva.
pub fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Az első `max` karakter (karakterhatáron), jelölés nélkül.
pub fn take_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Legfeljebb `max` karakter; ha vágni kellett, a végén `…` (így `max + 1` karakter is lehet).
pub fn clip_ellipsis(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut cut = take_chars(text, max);
        cut.push('…');
        cut
    } else {
        text.to_owned()
    }
}

/// Egy sorra vonva ([`collapse_ws`]), majd legfeljebb `max` karakterre vágva `…`-tal ([`clip_ellipsis`]).
pub fn flat_clip(text: &str, max: usize) -> String {
    clip_ellipsis(&collapse_ws(text), max)
}

/// Azonosító-bájt (`[A-Za-z0-9_]`).
pub fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// A szöveg 1-alapú sora a bájt-helyen.
pub fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()
        .iter()
        .take(offset)
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_ws_joins_words_with_one_space() {
        assert_eq!(collapse_ws("  a \n\t b   c "), "a b c");
        assert_eq!(collapse_ws(""), "");
    }

    #[test]
    fn clip_ellipsis_cuts_on_a_char_boundary() {
        assert_eq!(clip_ellipsis("héllo wörld", 5), "héllo…");
        assert_eq!(clip_ellipsis("héllo", 5), "héllo");
        assert_eq!(take_chars("héllo", 2), "hé");
    }

    #[test]
    fn flat_clip_collapses_before_cutting() {
        assert_eq!(flat_clip("a\n  b\n  c", 3), "a b…");
        assert_eq!(flat_clip("a  b", 10), "a b");
    }

    #[test]
    fn line_of_counts_from_one() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 4), 3);
        assert!(is_ident_byte(b'_') && !is_ident_byte(b'-'));
    }
}
