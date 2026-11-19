//! Hibatűrő Blade-szkenner: a sablon szövegéből a direktívák (`@extends('x')`), az echo-kifejezések (`{{ }}`,
//! `{!! !!}`), a `@php` blokkok és a komponens-címkék (`<x-alert>`, `<livewire:counter>`) forrásbeli sorrendben.
//! A komment (`{{-- --}}`), az escape-elt (`@{{ }}`, `@@x`) és a `@verbatim` blokk kimarad. Nem teljes parser:
//! ami nem ismerhető fel, azt egyszerűen nem jelzi — soha nem hiba.

/// Egy komponens-címke attribútuma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Attr {
    /// Az attribútum neve (a `:` előtaggal együtt, ha kötött).
    pub name: String,
    /// Az érték idézőjelek nélkül (üres, ha nincs érték).
    pub value: String,
    /// Az érték első bájtjának helye a szövegben.
    pub at: usize,
}

impl Attr {
    /// Kötött (`:message="$x"`) attribútum-e — az értéke PHP-kifejezés (a `::alpine` nem az).
    pub(super) fn bound(&self) -> bool {
        self.name.starts_with(':') && !self.name.starts_with("::")
    }
}

/// A szkennelt elemek.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Item {
    /// `@név` vagy `@név(args)`; `args` a zárójel belseje, `args_at` az első bájtjának helye.
    Directive {
        name: String,
        args: Option<String>,
        start: usize,
        args_at: usize,
    },
    /// `{{ kifejezés }}` / `{!! kifejezés !!}`.
    Echo { expr: String, start: usize },
    /// `@php … @endphp` (vagy `<?php … ?>`) blokk törzse.
    PhpBlock { code: String, start: usize },
    /// `<x-név …>`, `</x-név>`, `<livewire:név …>`; a név a címke neve (`x-forms.input`, `x-slot:title`).
    Tag {
        name: String,
        start: usize,
        close: bool,
        self_closing: bool,
        attrs: Vec<Attr>,
    },
}

impl Item {
    /// A forrásbeli kezdőhely (a rendezéshez).
    pub(super) fn start(&self) -> usize {
        match self {
            Self::Directive { start, .. }
            | Self::Echo { start, .. }
            | Self::PhpBlock { start, .. }
            | Self::Tag { start, .. } => *start,
        }
    }
}

/// Argumentumot váró (zárójeles) direktívák; a többi (`@endif`, `@csrf`) argumentum nélküli.
const WITH_ARGS: &[&str] = &[
    "extends",
    "section",
    "yield",
    "push",
    "pushonce",
    "pushif",
    "prepend",
    "stack",
    "include",
    "includeif",
    "includewhen",
    "includeunless",
    "includefirst",
    "each",
    "component",
    "componentfirst",
    "livewire",
    "slot",
    "props",
    "aware",
    "if",
    "elseif",
    "unless",
    "isset",
    "empty",
    "auth",
    "guest",
    "can",
    "cannot",
    "canany",
    "elsecan",
    "elsecannot",
    "env",
    "hassection",
    "sectionmissing",
    "switch",
    "case",
    "foreach",
    "forelse",
    "for",
    "while",
    "php",
    "lang",
    "choice",
    "method",
    "vite",
    "class",
    "style",
    "json",
    "js",
    "checked",
    "selected",
    "disabled",
    "readonly",
    "required",
    "dump",
    "dd",
    "inject",
    "once",
    "error",
    "production",
    "session",
    "use",
];

/// A szöveg szkennelése: az elemek a forrásbeli kezdőhely szerint rendezve.
pub(super) fn scan(text: &str) -> Vec<Item> {
    let b = text.as_bytes();
    let mut items = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => i = brace(text, i, &mut items),
            b'@' => i = at_sign(text, i, &mut items),
            b'<' => i = angle(text, i, &mut items),
            _ => i += 1,
        }
    }
    items.sort_by_key(Item::start);
    items
}

/// `{{ }}`, `{!! !!}`, `{{-- --}}` a `i` helyen; az új pozíció.
fn brace(text: &str, i: usize, items: &mut Vec<Item>) -> usize {
    let rest = &text[i..];
    if let Some(comment) = rest.strip_prefix("{{--") {
        return comment.find("--}}").map_or(text.len(), |p| i + 4 + p + 4);
    }
    let (open, close) = if rest.starts_with("{!!") {
        (3, "!!}")
    } else if rest.starts_with("{{") {
        (2, "}}")
    } else {
        return i + 1;
    };
    match read_until(text, i + open, close) {
        Some((expr, at, end)) => {
            items.push(Item::Echo { expr, start: at });
            end
        }
        None => i + open,
    }
}

/// A `start`-tól az első `close` (idézőjelen kívül): a levágott szöveg, az első nem szóköz bájtja, a záró utáni hely.
fn read_until(text: &str, start: usize, close: &str) -> Option<(String, usize, usize)> {
    let b = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = start;
    let mut found = None;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == b'\'' || c == b'"' {
            quote = Some(c);
        } else if b[i..].starts_with(close.as_bytes()) {
            found = Some(i);
            break;
        }
        i += 1;
    }
    // idézőjel-egyensúly nélkül (pl. egy aposztróf a kifejezésben): a sima keresés
    let end = found.or_else(|| text[start..].find(close).map(|p| start + p))?;
    let raw = &text[start..end];
    let lead = raw.len() - raw.trim_start().len();
    Some((raw.trim().to_owned(), start + lead, end + close.len()))
}

/// `@` a `i` helyen; az új pozíció.
fn at_sign(text: &str, i: usize, items: &mut Vec<Item>) -> usize {
    let b = text.as_bytes();
    // `user@example.com` nem direktíva
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return i + 1;
    }
    if b.get(i + 1) == Some(&b'@') {
        return i + 2;
    }
    if text[i + 1..].starts_with("{{") {
        return text[i + 3..]
            .find("}}")
            .map_or(text.len(), |p| i + 3 + p + 2);
    }
    let name_start = i + 1;
    let name_end = name_start
        + b[name_start..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == b'_')
            .count();
    if name_end == name_start {
        return i + 1;
    }
    let name = &text[name_start..name_end];
    let lower = name.to_ascii_lowercase();
    if lower == "verbatim" {
        return text[name_end..]
            .find("@endverbatim")
            .map_or(text.len(), |p| name_end + p + "@endverbatim".len());
    }
    let paren = paren_start(text, name_end);
    if lower == "php" && paren.is_none() {
        return match text[name_end..].find("@endphp") {
            Some(p) => {
                items.push(Item::PhpBlock {
                    code: text[name_end..name_end + p].to_owned(),
                    start: name_end,
                });
                name_end + p + "@endphp".len()
            }
            None => {
                items.push(Item::PhpBlock {
                    code: text[name_end..].to_owned(),
                    start: name_end,
                });
                text.len()
            }
        };
    }
    if WITH_ARGS.contains(&lower.as_str())
        && let Some(open) = paren
        && let Some((args, end)) = read_parens(text, open)
    {
        items.push(Item::Directive {
            name: name.to_owned(),
            args: Some(args),
            start: i,
            args_at: open + 1,
        });
        return end;
    }
    items.push(Item::Directive {
        name: name.to_owned(),
        args: None,
        start: i,
        args_at: name_end,
    });
    name_end
}

/// A direktíva neve utáni `(` helye (szóköz / tabulátor engedett).
fn paren_start(text: &str, from: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = from;
    while matches!(b.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    (b.get(i) == Some(&b'(')).then_some(i)
}

/// A `(`-től a párjáig (idézőjel-tudóan): a belseje és a záró utáni hely.
fn read_parens(text: &str, open: usize) -> Option<(String, usize)> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut i = open;
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else {
            match c {
                b'\'' | b'"' => quote = Some(c),
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((text[open + 1..i].to_owned(), i + 1));
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// `<` a `i` helyen (komponens-címke, `<?php`); az új pozíció.
fn angle(text: &str, i: usize, items: &mut Vec<Item>) -> usize {
    let rest = &text[i..];
    if rest.starts_with("<?php") {
        let body = i + "<?php".len();
        let end = text[body..].find("?>").map_or(text.len(), |p| body + p);
        items.push(Item::PhpBlock {
            code: text[body..end].to_owned(),
            start: body,
        });
        return (end + 2).min(text.len());
    }
    let (close, tail_at) = if rest.starts_with("</") {
        (true, i + 2)
    } else {
        (false, i + 1)
    };
    let tail = &text[tail_at..];
    if !(tail.starts_with("x-") || tail.starts_with("livewire:")) {
        return i + 1;
    }
    let name_len = tail
        .bytes()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':'))
        .count();
    let name = tail[..name_len].trim_end_matches(['.', ':']).to_owned();
    let name_end = tail_at + name.len();
    let (attrs, self_closing) = if close {
        (Vec::new(), false)
    } else {
        read_attrs(text, name_end)
    };
    items.push(Item::Tag {
        name,
        start: i,
        close,
        self_closing,
        attrs,
    });
    // az attribútumok közti echo-k / direktívák a fő ciklusban is megjelennek
    name_end
}

/// A címke attribútumai a név után a `>`-ig (idézőjel-tudóan, `{{ }}` átugorva), és hogy `/>`-re végződik-e.
fn read_attrs(text: &str, from: usize) -> (Vec<Attr>, bool) {
    let b = text.as_bytes();
    let mut attrs = Vec::new();
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b'>' => return (attrs, false),
            b'/' if b.get(i + 1) == Some(&b'>') => return (attrs, true),
            c if c.is_ascii_whitespace() || c == b'/' => i += 1,
            b'{' if text[i..].starts_with("{{") || text[i..].starts_with("{!!") => {
                let close = if text[i..].starts_with("{{") {
                    "}}"
                } else {
                    "!!}"
                };
                i = text[i..]
                    .find(close)
                    .map_or(b.len(), |p| i + p + close.len());
            }
            _ => {
                let name_len = text[i..]
                    .bytes()
                    .take_while(|c| !c.is_ascii_whitespace() && !matches!(c, b'=' | b'>' | b'/'))
                    .count()
                    .max(1);
                let mut name_end = i + name_len;
                while !text.is_char_boundary(name_end) {
                    name_end += 1;
                }
                let name = text[i..name_end].to_owned();
                i = name_end;
                if b.get(i) != Some(&b'=') {
                    attrs.push(Attr {
                        name,
                        value: String::new(),
                        at: i,
                    });
                    continue;
                }
                i += 1;
                match b.get(i) {
                    Some(&q) if q == b'"' || q == b'\'' => {
                        let value_at = i + 1;
                        let end = text[value_at..]
                            .find(q as char)
                            .map_or(b.len(), |p| value_at + p);
                        attrs.push(Attr {
                            name,
                            value: text[value_at..end].to_owned(),
                            at: value_at,
                        });
                        i = (end + 1).min(b.len());
                    }
                    _ => {
                        let len = text[i..]
                            .bytes()
                            .take_while(|c| !c.is_ascii_whitespace() && *c != b'>')
                            .count();
                        attrs.push(Attr {
                            name,
                            value: text[i..i + len].to_owned(),
                            at: i,
                        });
                        i += len;
                    }
                }
            }
        }
    }
    (attrs, false)
}

/// A felső szintű (zárójelen / idézőjelen kívüli) `sep`-ek mentén szétvágott darabok: a levágott szöveg és az
/// eredeti `text`-beli kezdőhelye (az első nem szóköz bájt).
pub(super) fn split_top(text: &str, sep: u8) -> Vec<(String, usize)> {
    let b = text.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut start = 0;
    let mut i = 0;
    let flush = |from: usize, to: usize, parts: &mut Vec<(String, usize)>| {
        let raw = &text[from..to];
        let lead = raw.len() - raw.trim_start().len();
        if !raw.trim().is_empty() {
            parts.push((raw.trim().to_owned(), from + lead));
        }
    };
    while i < b.len() {
        let c = b[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == b'\'' || c == b'"' {
            quote = Some(c);
        } else if matches!(c, b'(' | b'[' | b'{') {
            depth += 1;
        } else if matches!(c, b')' | b']' | b'}') {
            depth -= 1;
        } else if c == sep && depth == 0 {
            flush(start, i, &mut parts);
            start = i + 1;
        }
        i += 1;
    }
    flush(start, text.len(), &mut parts);
    parts
}

/// Sima string-literál értéke (`'x'` / `"x"`); más: `None`.
pub(super) fn literal(arg: &str) -> Option<String> {
    let t = arg.trim();
    let quote = t.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let inner = t.strip_prefix(quote)?.strip_suffix(quote)?;
    let plain = !inner.is_empty()
        && !inner.contains(quote)
        && !inner.contains('\\')
        && !(quote == '"' && inner.contains('$'));
    plain.then(|| inner.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        scan(text)
            .iter()
            .map(|item| match item {
                Item::Directive { name, args, .. } => match args {
                    Some(args) => format!("@{name}({args})"),
                    None => format!("@{name}"),
                },
                Item::Echo { expr, .. } => format!("{{{expr}}}"),
                Item::PhpBlock { code, .. } => format!("php[{}]", code.trim()),
                Item::Tag {
                    name,
                    close,
                    self_closing,
                    ..
                } => format!(
                    "<{}{name}{}>",
                    if *close { "/" } else { "" },
                    if *self_closing { "/" } else { "" }
                ),
            })
            .collect()
    }

    #[test]
    fn finds_directives_echoes_and_components_in_order() {
        let text = "@extends('layouts.app')\n@section('content')\n<x-alert type=\"error\" :msg=\"$e->text()\"/>\n{{ $user->name }} {!! $html !!}\n@endsection\n";
        assert_eq!(
            names(text),
            [
                "@extends('layouts.app')",
                "@section('content')",
                "<x-alert/>",
                "{$user->name}",
                "{$html}",
                "@endsection"
            ]
        );
    }

    #[test]
    fn skips_comments_escapes_verbatim_and_emails() {
        let text = "{{-- @include('no') --}}\n@{{ $js }}\n@@include('no')\nme@example.com\n@verbatim\n{{ $vue }}\n@endverbatim\n{{ $yes }}\n";
        assert_eq!(names(text), ["{$yes}"]);
    }

    #[test]
    fn php_blocks_and_inline_php_are_separate() {
        let text = "@php\n  $a = strlen($x);\n@endphp\n@php($b = 2)\n";
        assert_eq!(names(text), ["php[$a = strlen($x);]", "@php($b = 2)"]);
    }

    #[test]
    fn attributes_are_read_with_quotes_and_echoes() {
        let text =
            "<x-forms.input name=\"email\" :value=\"old('email')\" {{ $attributes }} required>";
        let items = scan(text);
        let Item::Tag { name, attrs, .. } = &items[0] else {
            panic!("tag expected: {items:?}");
        };
        assert_eq!(name, "x-forms.input");
        let bound: Vec<&str> = attrs
            .iter()
            .filter(|a| a.bound())
            .map(|a| a.value.as_str())
            .collect();
        assert_eq!(bound, ["old('email')"]);
        assert!(attrs.iter().any(|a| a.name == "required"));
    }

    #[test]
    fn top_level_split_respects_nesting_and_quotes() {
        let parts = split_top("'a', [1, 2], f('x,y'), $z", b',');
        let texts: Vec<&str> = parts.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(texts, ["'a'", "[1, 2]", "f('x,y')", "$z"]);
        assert_eq!(literal("'ok'").as_deref(), Some("ok"));
        assert_eq!(literal("$x"), None);
    }

    #[test]
    fn unterminated_input_does_not_panic() {
        for text in [
            "{{ $a",
            "@if(",
            "<x-alert :a=\"",
            "@php\nx",
            "{!! ",
            "@section('a",
        ] {
            let _ = scan(text);
        }
    }
}
