//! Közös jelölőnyelv-primitívek (HTML / Vue-sablon / Blade-címkék): a tűrő nyitó-tag szkenner (név, attribútumok
//! bájt-eltolással), a `void` elemek listája, a komment / deklaráció és a nyers-szöveg elem határa, a szóköz-osztály.
//! A `lang/html` a nyitó-tag szkennert, a Vue-sablon parser (`lang/js/template`) a többit használja; a hibatűrés
//! egységes: a nem zárt tag, komment vagy idézőjel a szöveg végéig tart, és sosem pánikol (minden szeletelés
//! ASCII-határoló bájton történik).

/// A `void` HTML-elemek (nincs záró tagjuk).
pub const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Void elem-e a (pontosan így írt, kisbetűs) tag-név.
pub fn is_void(name: &str) -> bool {
    VOID.contains(&name)
}

/// HTML szóköz-bájt (szóköz, tab, sortörés, kocsi-vissza, lapdobás).
pub fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

/// A `<` helyén álló komment (`<!-- -->`) vagy deklaráció (`<!DOCTYPE …>`, `<?xml …?>`) utáni pozíció, legfeljebb
/// `end`-ig; nem zárt: `end`.
pub fn declaration_end(src: &str, at: usize, end: usize) -> usize {
    let rest = &src[at..end];
    let stop = if let Some(body) = rest.strip_prefix("<!--") {
        body.find("-->").map(|r| r + 7)
    } else {
        rest.find('>').map(|r| r + 1)
    };
    at + stop.unwrap_or(rest.len())
}

/// A nyers-szöveg elem (`script`, `style`, `textarea`, `title`) tartalmának vége: a `from`-tól az első `</name`
/// (kisbetűs `name`), amelyet szóköz, `/` vagy `>` követ, legfeljebb `end`-ig; nincs ilyen: `end`.
pub fn raw_text_end(src: &str, from: usize, end: usize, name: &str) -> usize {
    let hay = src[from..end].to_ascii_lowercase();
    let pat = format!("</{name}");
    hay.match_indices(&pat)
        .map(|(r, _)| r)
        .find(|r| {
            hay.as_bytes()
                .get(r + pat.len())
                .is_none_or(|c| is_ws(*c) || *c == b'/' || *c == b'>')
        })
        .map_or(end, |r| from + r)
}

/// Egy attribútum: kisbetűs név, nyers érték és az érték kezdő pozíciója.
pub struct Attr<'s> {
    pub name: String,
    pub value: &'s str,
    pub start: usize,
}

/// Egy nyitó tag: kisbetűs név, attribútumok, és a tag utáni pozíció.
pub struct Tag<'s> {
    pub name: String,
    pub attrs: Vec<Attr<'s>>,
    pub end: usize,
}

/// A tag neve kisbetűvel a `from` pozíciótól, és a név utáni pozíció.
pub fn tag_name(bytes: &[u8], from: usize) -> (String, usize) {
    let mut end = from;
    while end < bytes.len() && is_name_byte(bytes[end]) {
        end += 1;
    }
    let name = String::from_utf8_lossy(&bytes[from..end]).to_ascii_lowercase();
    (name, end)
}

/// A tag nevének bájtja-e (betű, számjegy, `-`, `:`, `_`).
pub fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b':' | b'_')
}

/// Az attribútum-név végét jelző bájt-e (szóköz, `=`, `>`, `/`).
pub fn ends_attr_name(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'=' | b'>' | b'/')
}

/// Egy nyitó tag (`start`: a `<` helye): név, attribútumok (idézett, idézőjel nélküli és érték nélküli), és a záró
/// `>` utáni pozíció (nem zárt tagnél / idézőjelnél a szöveg vége).
pub fn parse_tag(src: &str, start: usize) -> Tag<'_> {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let (name, mut p) = tag_name(bytes, start + 1);
    let mut attrs = Vec::new();
    loop {
        while p < len && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        if p >= len {
            break;
        }
        match bytes[p] {
            b'>' => {
                p += 1;
                break;
            }
            b'/' => {
                p += 1;
                continue;
            }
            _ => {}
        }
        let name_start = p;
        while p < len && !ends_attr_name(bytes[p]) {
            p += 1;
        }
        if p == name_start {
            // váratlan bájt (pl. magányos `=`): átlépve, hogy mindig haladjon
            p += 1;
            continue;
        }
        let attr_name = src[name_start..p].to_ascii_lowercase();
        let mut q = p;
        while q < len && bytes[q].is_ascii_whitespace() {
            q += 1;
        }
        if q >= len || bytes[q] != b'=' {
            // érték nélküli attribútum (`disabled`)
            continue;
        }
        q += 1;
        while q < len && bytes[q].is_ascii_whitespace() {
            q += 1;
        }
        let (value_start, value_end, next) = match bytes.get(q) {
            Some(&quote @ (b'"' | b'\'')) => {
                let from = q + 1;
                match bytes[from..].iter().position(|&b| b == quote) {
                    Some(n) => (from, from + n, from + n + 1),
                    None => (from, len, len),
                }
            }
            _ => {
                let mut e = q;
                while e < len && !bytes[e].is_ascii_whitespace() && bytes[e] != b'>' {
                    e += 1;
                }
                (q, e, e)
            }
        };
        attrs.push(Attr {
            name: attr_name,
            value: &src[value_start..value_end],
            start: value_start,
        });
        p = next;
    }
    Tag {
        name,
        attrs,
        end: p,
    }
}

/// A `from` pozíciótól az első `>` utáni pozíció (ha nincs, a szöveg vége).
pub fn skip_to_gt(bytes: &[u8], from: usize) -> usize {
    let from = from.min(bytes.len());
    bytes[from..]
        .iter()
        .position(|&b| b == b'>')
        .map_or(bytes.len(), |n| from + n + 1)
}

/// A `needle` első előfordulása a `from` pozíciótól.
pub fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let from = from.min(bytes.len());
    bytes[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|n| from + n)
}

/// A (ASCII) `needle` első előfordulása a `from` pozíciótól, kis-/nagybetűtől függetlenül.
pub fn find_ci(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    let from = from.min(bytes.len());
    bytes[from..]
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
        .map(|n| from + n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_tag_has_names_attributes_and_offsets() {
        let src = "<A HREF='x y' disabled data=v>t";
        let tag = parse_tag(src, 0);
        assert_eq!(tag.name, "a");
        let attrs: Vec<(&str, &str)> = tag
            .attrs
            .iter()
            .map(|a| (a.name.as_str(), a.value))
            .collect();
        assert_eq!(attrs, [("href", "x y"), ("data", "v")]);
        assert_eq!(&src[tag.attrs[0].start..][..3], "x y");
        assert_eq!(&src[tag.end..], "t");
    }

    #[test]
    fn unterminated_tag_runs_to_the_end() {
        let tag = parse_tag("<a href=\"x", 0);
        assert_eq!(tag.attrs[0].value, "x");
        assert_eq!(tag.end, 10);
    }

    #[test]
    fn declarations_and_comments_end_where_they_close() {
        let src = "<!-- a > b --><p>";
        assert_eq!(declaration_end(src, 0, src.len()), 14);
        let doc = "<!DOCTYPE html><p>";
        assert_eq!(declaration_end(doc, 0, doc.len()), 15);
        assert_eq!(declaration_end("<!-- open", 0, 9), 9);
    }

    #[test]
    fn raw_text_ends_at_the_matching_close_tag() {
        let src = "var a = '</scripty>'; </SCRIPT >";
        assert_eq!(raw_text_end(src, 0, src.len(), "script"), 22);
        assert_eq!(raw_text_end("x", 0, 1, "style"), 1);
        assert!(is_void("br") && !is_void("div"));
        assert_eq!(skip_to_gt(b"ab>c", 0), 3);
        assert_eq!(find_ci(b"xx</Title", 0, b"</title"), Some(2));
    }
}
