//! Forrás-szöveg szkennelés a keretrendszer-nézetekhez: megjegyzés-tisztító, zárójel-párosító, argumentum-vágó és a
//! hívás-lánc határa — nyelvjárással (`Php`: `#`, `//`, `/* */`; `Js`: backtick, `//`, `/* */`). A szűkített gráfban
//! nincs hívás-argumentum, ezért a Laravel- / Inertia-nézetek a forrást olvassák újra; a hét modul korábban saját
//! másolatot tartott erre (PHP-szabályokkal vágott JS-forrással: a `#` megjegyzést nyitott, a backtick nem
//! idézőjel volt). Regex-literált nem ismer fel (a `/` után `/` vagy `*` nélkül nincs szerepe).

/// A szkennelt forrás nyelvjárása.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// PHP: `'` / `"` string, `//`, `#` (de nem `#[`) és `/* */` megjegyzés.
    Php,
    /// JS / TS: `'` / `"` / backtick string, `//` és `/* */` megjegyzés (a `#` privát mező, nem megjegyzés).
    Js,
}

impl Dialect {
    fn is_quote(self, b: u8) -> bool {
        b == b'\'' || b == b'"' || (self == Self::Js && b == b'`')
    }

    /// Sor-megjegyzést nyit-e a `#` (a `next` a rá következő bájt).
    fn hash_comment(self, next: Option<u8>) -> bool {
        self == Self::Php && next != Some(b'[')
    }
}

/// Egy karakter helyére szóköz (a sortörés marad): a bájt-helyek így nem tolódnak el.
fn blank(out: &mut String, ch: char) {
    if ch == '\n' {
        out.push('\n');
    } else {
        out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
    }
}

/// A megjegyzések szóközre cserélve — a bájt-helyek és a sortörések megmaradnak, és a stringeken belüli `//` (url)
/// érintetlen.
pub fn strip_comments(text: &str, dialect: Dialect) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        let next = chars.peek().copied();
        let next_byte = next.and_then(|c| u8::try_from(c).ok());
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                out.push(ch);
            }
            '`' if dialect == Dialect::Js => {
                quote = Some(ch);
                out.push(ch);
            }
            '/' if next == Some('/') => blank_line(&mut chars, &mut out, ch),
            '#' if dialect.hash_comment(next_byte) => blank_line(&mut chars, &mut out, ch),
            '/' if next == Some('*') => {
                chars.next();
                out.push_str("  ");
                let mut star = false;
                for c in chars.by_ref() {
                    blank(&mut out, c);
                    if star && c == '/' {
                        break;
                    }
                    star = c == '*';
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Egy sor-megjegyzés a sor végéig szóközre cserélve (a sortörés megmarad).
fn blank_line(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String, first: char) {
    blank(out, first);
    while let Some(&c) = chars.peek() {
        if c == '\n' {
            break;
        }
        blank(out, c);
        chars.next();
    }
}

/// A `open` helyi nyitó zárójel párja a `to` bájt-határig (string- és beágyazás-tudóan; a szöveg megjegyzés-mentes);
/// lezáratlan: `None`.
pub fn matching(text: &str, open: usize, to: usize, dialect: Dialect) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, &b) in text.as_bytes().iter().enumerate().take(to).skip(open) {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            _ if dialect.is_quote(b) => quote = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// A `from..to` szöveg argumentumai a legfelső szintű vesszők mentén (levágva, az üresek nélkül): bájt-tartományok.
pub fn split_args(text: &str, from: usize, to: usize, dialect: Dialect) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut start = from;
    for (i, &b) in text.as_bytes().iter().enumerate().take(to).skip(from) {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            _ if dialect.is_quote(b) => quote = Some(b),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                spans.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    spans.push((start, to));
    spans
        .into_iter()
        .filter_map(|(a, b)| {
            let raw = text.get(a..b)?;
            let trimmed = raw.trim();
            let lead = raw.len() - raw.trim_start().len();
            (!trimmed.is_empty()).then_some((a + lead, a + lead + trimmed.len()))
        })
        .collect()
}

/// A szöveg darabjai a legfelső szintű (stringeken és zárójeleken kívüli) `sep` mentén.
pub fn split_top<'a>(text: &'a str, sep: &str, dialect: Dialect) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut start = 0;
    for (i, ch) in text.char_indices() {
        let byte = u8::try_from(ch).ok();
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if byte == Some(q) {
                quote = None;
            }
            continue;
        }
        match ch {
            _ if byte.is_some_and(|b| dialect.is_quote(b)) => quote = byte,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && i >= start && text[i..].starts_with(sep) => {
                parts.push(&text[start..i]);
                start = i + sep.len();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// A `(` utáni szövegből az argumentumok a záró `)`-ig (stringeken és zárójeleken belül nem vág). A megjegyzések
/// kimaradnak az argumentumokból — a bennük álló idézőjel nem nyit stringet. `max_scan`: a keresés bájt-korlátja
/// (egy lezáratlan hívás se járja be a fájl végéig). Lezáratlan: `None`.
pub fn call_args(text: &str, dialect: Dialect, max_scan: usize) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if i > max_scan {
            return None;
        }
        if let Some(q) = quote {
            current.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        let next = chars.peek().map(|&(_, c)| c);
        let next_byte = next.and_then(|c| u8::try_from(c).ok());
        match ch {
            '/' if next == Some('/') => skip_line(&mut chars),
            '#' if dialect.hash_comment(next_byte) => skip_line(&mut chars),
            '/' if next == Some('*') => {
                chars.next();
                let mut star = false;
                for (_, c) in chars.by_ref() {
                    if star && c == '/' {
                        break;
                    }
                    star = c == '*';
                }
            }
            '\'' | '"' => {
                quote = Some(ch);
                current.push(ch);
            }
            '`' if dialect == Dialect::Js => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' if depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            ')' => {
                let last = current.trim();
                if !last.is_empty() {
                    args.push(last.to_owned());
                }
                return Some(args);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    None
}

/// Egy sor-megjegyzés a sor végéig (a sorvég megmarad elválasztónak).
fn skip_line(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    while let Some(&(_, c)) = chars.peek() {
        if c == '\n' {
            break;
        }
        chars.next();
    }
}

/// A `from`-tól kezdődő hívás-lánc vége (bájt-hely): az első legfelső szintű `;`, vagy az a sortörés, amely után nem
/// láncolt tag (`.x`, `?.x`, `->x`, `::x`) következik; zárójeleken, stringeken és megjegyzéseken belül nem áll meg.
/// A többsoros láncolt hívás (`form⏎.transform(..)⏎.post(..)`) hívása a lánc kezdősorára mutat, a hívott név viszont
/// a későbbi sorokban áll — ebben a tartományban kell keresni, nem csak az első sorban. `max_scan`: bájt-korlát.
pub fn chain_end(text: &str, from: usize, dialect: Dialect, max_scan: usize) -> usize {
    let bytes = text.as_bytes();
    let mut limit = text.len().min(from.saturating_add(max_scan));
    while !text.is_char_boundary(limit) {
        limit -= 1;
    }
    // egy blokk nyitó / záró kapcsos zárójele nem hívás-lánc kezdete
    if matches!(
        text[from..].trim_start().as_bytes().first(),
        Some(b'{' | b'}')
    ) {
        return from;
    }
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut i = from;
    while i < limit {
        let b = bytes[i];
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        let next = bytes.get(i + 1).copied();
        match b {
            _ if dialect.is_quote(b) => quote = Some(b),
            b'/' if next == Some(b'/') => i = line_end(bytes, i) - 1,
            b'#' if dialect.hash_comment(next) => i = line_end(bytes, i) - 1,
            b'/' if next == Some(b'*') => {
                i = text[i + 2..]
                    .find("*/")
                    .map_or(limit, |found| i + 2 + found + 1);
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => match depth.checked_sub(1) {
                Some(d) => depth = d,
                None => return i,
            },
            b';' if depth == 0 => return i,
            b'\n' if depth == 0 && !continues_chain(&text[i..]) => return i,
            _ => {}
        }
        i += 1;
    }
    limit
}

/// A sor vége (a `\n` helye, különben a szöveg hossza).
fn line_end(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |p| from + p)
        .max(from + 1)
}

/// A (sortöréssel kezdődő) maradék a következő nem-üres sorban láncolt taggal folytatódik-e.
fn continues_chain(rest: &str) -> bool {
    let next = rest.trim_start();
    ["->", "?->", "?.", "::"]
        .iter()
        .any(|p| next.starts_with(p))
        || (next.starts_with('.') && !next.starts_with(".."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments_keeps_offsets_and_strings() {
        let src = "a // xx\nb /* yy\nzz */ c 'http://u' # nn\n";
        let clean = strip_comments(src, Dialect::Php);
        assert_eq!(clean.len(), src.len());
        assert!(clean.contains("'http://u'"), "{clean}");
        for gone in ["xx", "yy", "zz", "nn"] {
            assert!(!clean.contains(gone), "{clean}");
        }
        assert_eq!(clean.matches('\n').count(), src.matches('\n').count());
    }

    #[test]
    fn hash_is_a_comment_only_in_php_and_attributes_survive() {
        let src = "#[Attr]\n$a = 1; # note\n";
        let php = strip_comments(src, Dialect::Php);
        assert!(php.starts_with("#[Attr]") && !php.contains("note"), "{php}");
        let js = strip_comments("x = `#fff`; # y\n", Dialect::Js);
        assert!(js.contains("`#fff`") && js.contains("# y"), "{js}");
    }

    #[test]
    fn matching_pairs_brackets_and_skips_strings() {
        let t = "f(a, [')'], {b: 1})";
        assert_eq!(matching(t, 1, t.len(), Dialect::Php), Some(t.len() - 1));
        assert_eq!(matching("f(a", 1, 3, Dialect::Php), None);
        let js = "f(`)`, 2)";
        assert_eq!(matching(js, 1, js.len(), Dialect::Js), Some(js.len() - 1));
        assert_eq!(matching(js, 1, js.len(), Dialect::Php), Some(3));
    }

    #[test]
    fn split_args_cuts_on_top_level_commas() {
        let t = "(a, f(b, c),  'x,y' , )";
        let spans = split_args(t, 1, t.len() - 1, Dialect::Php);
        let parts: Vec<&str> = spans.iter().map(|&(a, b)| &t[a..b]).collect();
        assert_eq!(parts, ["a", "f(b, c)", "'x,y'"]);
        assert_eq!(
            split_top("a => [1, 2], b", ",", Dialect::Php),
            ["a => [1, 2]", " b"]
        );
    }

    #[test]
    fn call_args_skip_comments_by_dialect() {
        let php = "'a', // it's\n # no\n 'b')";
        assert_eq!(
            call_args(php, Dialect::Php, 1000),
            Some(vec!["'a'".to_owned(), "'b'".to_owned()])
        );
        let js = "{ color: `#fff`, n: 1 }, 'x')";
        assert_eq!(
            call_args(js, Dialect::Js, 1000),
            Some(vec!["{ color: `#fff`, n: 1 }".to_owned(), "'x'".to_owned()])
        );
        assert_eq!(call_args("'a', 'b'", Dialect::Php, 1000), None);
        assert_eq!(call_args("'a')", Dialect::Php, 1), None);
    }

    #[test]
    fn chain_end_follows_a_multiline_chain() {
        let t = "form\n  .transform(() => p)\n  .post('/x', {\n    a: 1,\n  });\nnext()";
        let end = chain_end(t, 0, Dialect::Js, 10_000);
        assert_eq!(&t[end..], ";\nnext()");
        let two = "a()\nb()";
        assert_eq!(chain_end(two, 0, Dialect::Js, 10_000), 3);
        let block = "    {\n    f();\n    }\n";
        assert_eq!(chain_end(block, 0, Dialect::Php, 10_000), 0);
        let php = "$x\n  ->a()\n  ?->b();\n";
        assert_eq!(&php[chain_end(php, 0, Dialect::Php, 10_000)..], ";\n");
    }
}
