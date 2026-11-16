//! HTML tag-szkenner a generátor `.html` / `.htm` céljaihoz: a nyitó / záró tagek párosítása (a script / style /
//! textarea / title nyers szövegét átugorva), az elem-keresés (`#id`, `.class`, `tag`), az írás előtti ellenőrzés
//! (tag-egyensúly, inline CSS: raffia, inline JS: oxc) és a duplikált `id` figyelmeztetés. Hibatűrő, nem teljes
//! HTML-parser: az elhagyható záró tagű elemek (`li`, `p`, `td` …) lezáratlanul is rendben vannak.

use raffia::ast::Stylesheet;
use raffia::{Parser, Syntax};

use super::check::{Problem, first_js_error};
use super::template::is_void;

/// Azok az elemek, amelyeknek a záró tagje HTML-ben elhagyható.
const OPTIONAL_END: [&str; 17] = [
    "html", "head", "body", "li", "p", "dt", "dd", "tr", "td", "th", "thead", "tbody", "tfoot",
    "option", "optgroup", "colgroup", "caption",
];

/// A tartalmát nyers szövegként kezelő elemek (a belsejében a `<` nem tag).
const RAW_TEXT: [&str; 4] = ["script", "style", "textarea", "title"];

/// Egy elem a szövegben (bájt-eltolások).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Kisbetűs tag-név.
    pub name: String,
    /// A nyitó tag `[kezdet, vég)` tartománya.
    pub open: (usize, usize),
    /// A záró tag tartománya; `None` void / önzáró / implicit módon lezárt elemnél.
    pub close: Option<(usize, usize)>,
}

fn unclosed(name: &str) -> String {
    crate::tf!("unclosed `<{}`", "lezáratlan `<{}`", name)
}

/// A `from` utáni első, idézőjeleken kívüli `>` indexe.
fn tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote = 0u8;
    for (k, &b) in bytes.iter().enumerate().skip(from) {
        match (quote, b) {
            (0, b'"' | b'\'') => quote = b,
            (q, c) if q != 0 && c == q => quote = 0,
            (0, b'>') => return Some(k),
            _ => {}
        }
    }
    None
}

/// A szöveg elemei nyitási sorrendben; az első hiba: (bájt-eltolás, üzenet).
pub fn scan(text: &str) -> Result<Vec<Span>, (usize, String)> {
    let bytes = text.as_bytes();
    let lower = text.to_ascii_lowercase();
    let mut spans: Vec<Span> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            i = rest.find("-->").map_or(bytes.len(), |e| i + e + 3);
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            i = rest.find('>').map_or(bytes.len(), |e| i + e + 1);
            continue;
        }
        let closing = bytes.get(i + 1) == Some(&b'/');
        let name_start = i + 1 + usize::from(closing);
        if !bytes.get(name_start).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        let name_end = text[name_start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))
            .map_or(bytes.len(), |e| name_start + e);
        let name = lower[name_start..name_end].to_owned();
        let Some(end) = tag_end(bytes, name_end) else {
            return Err((i, unclosed(&name)));
        };
        if closing {
            let Some(p) = stack.iter().rposition(|&k| spans[k].name == name) else {
                return Err((
                    i,
                    crate::tf!(
                        "`</{}>` has no opening tag",
                        "a `</{}>`-nek nincs nyitó tagje",
                        name
                    ),
                ));
            };
            if let Some(&k) = stack[p + 1..]
                .iter()
                .find(|&&k| !OPTIONAL_END.contains(&spans[k].name.as_str()))
            {
                return Err((
                    i,
                    crate::tf!(
                        "`</{}>` found while `<{}>` is still open",
                        "a `</{}>` előtt a `<{}>` még nyitva van",
                        name,
                        spans[k].name
                    ),
                ));
            }
            spans[stack[p]].close = Some((i, end + 1));
            stack.truncate(p);
            i = end + 1;
            continue;
        }
        let self_closed = bytes[end - 1] == b'/';
        spans.push(Span {
            name: name.clone(),
            open: (i, end + 1),
            close: None,
        });
        i = end + 1;
        if self_closed || is_void(&name) {
            continue;
        }
        stack.push(spans.len() - 1);
        if RAW_TEXT.contains(&name.as_str()) {
            i = lower[i..]
                .find(&format!("</{name}"))
                .map_or(bytes.len(), |e| i + e);
        }
    }
    match stack
        .iter()
        .rev()
        .find(|&&k| !OPTIONAL_END.contains(&spans[k].name.as_str()))
    {
        Some(&k) => Err((
            spans[k].open.0,
            crate::tf!(
                "`<{}>` is never closed",
                "a `<{}>` nincs lezárva",
                spans[k].name
            ),
        )),
        None => Ok(spans),
    }
}

/// A `off` bájt-eltolás 0-alapú sora.
pub fn line_of(text: &str, off: usize) -> usize {
    text.as_bytes()[..off.min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// A `off` bájt-eltolás oszlopa (bájt) a során belül.
pub fn col_of(text: &str, off: usize) -> usize {
    let off = off.min(text.len());
    off - text[..off].rfind('\n').map_or(0, |p| p + 1)
}

/// A nyitó tag attribútumai (`<a href="x" hidden>` → `[("href", Some("x")), ("hidden", None)]`).
pub fn open_attrs(tag: &str) -> Vec<(String, Option<String>)> {
    let chars: Vec<char> = tag.chars().collect();
    let mut out = Vec::new();
    let mut i = 1;
    while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], '>' | '/') {
        i += 1;
    }
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '/') {
            i += 1;
        }
        if i >= chars.len() || chars[i] == '>' {
            break;
        }
        let from = i;
        while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], '=' | '>' | '/') {
            i += 1;
        }
        let name: String = chars[from..i].iter().collect();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if chars.get(i) == Some(&'=') {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            let value: String = match chars.get(i) {
                Some(&q) if q == '"' || q == '\'' => {
                    let start = i + 1;
                    i = start;
                    while i < chars.len() && chars[i] != q {
                        i += 1;
                    }
                    let v = chars[start..i.min(chars.len())].iter().collect();
                    i += 1;
                    v
                }
                _ => {
                    let start = i;
                    while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '>' {
                        i += 1;
                    }
                    chars[start..i].iter().collect()
                }
            };
            out.push((name, Some(value)));
        } else if name.is_empty() {
            i += 1;
        } else {
            out.push((name, None));
        }
    }
    out
}

/// Egy egyszerű elem-választó: `tag`, `#id`, `.class`, vagy ezek együtt (`div#main.card`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

/// A választó értelmezése; `None`, ha üres vagy hibás.
pub fn parse_selector(text: &str) -> Option<Selector> {
    let text = text.trim();
    let tag_len = text.find(['#', '.']).unwrap_or(text.len());
    let tag = &text[..tag_len];
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
    {
        return None;
    }
    let mut sel = Selector {
        tag: (!tag.is_empty()).then(|| tag.to_ascii_lowercase()),
        id: None,
        classes: Vec::new(),
    };
    let mut rest = &text[tag_len..];
    while let Some(marker) = rest.chars().next() {
        let body = &rest[1..];
        let len = body.find(['#', '.']).unwrap_or(body.len());
        let piece = &body[..len];
        if piece.is_empty() || piece.chars().any(char::is_whitespace) {
            return None;
        }
        if marker == '#' {
            sel.id = Some(piece.to_owned());
        } else {
            sel.classes.push(piece.to_owned());
        }
        rest = &body[len..];
    }
    (sel.tag.is_some() || sel.id.is_some() || !sel.classes.is_empty()).then_some(sel)
}

/// Az első elem (nyitási sorrendben), amelyik megfelel a választónak.
pub fn find(text: &str, spans: &[Span], sel: &Selector) -> Option<usize> {
    spans.iter().position(|s| {
        if sel.tag.as_ref().is_some_and(|t| *t != s.name) {
            return false;
        }
        if sel.id.is_none() && sel.classes.is_empty() {
            return true;
        }
        let attrs = open_attrs(&text[s.open.0..s.open.1]);
        let value = |name: &str| {
            attrs
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .and_then(|(_, v)| v.clone())
        };
        if let Some(id) = &sel.id
            && value("id").as_deref() != Some(id.as_str())
        {
            return false;
        }
        let class = value("class").unwrap_or_default();
        sel.classes
            .iter()
            .all(|c| class.split_whitespace().any(|x| x == c))
    })
}

/// Az első CSS-hiba: (0-alapú sor, üzenet).
fn css_error(src: &str) -> Option<(usize, String)> {
    Parser::new(src, Syntax::Css)
        .parse::<Stylesheet>()
        .err()
        .map(|e| (line_of(src, e.span.start), format!("{:?}", e.kind)))
}

/// Egy kész HTML szöveg ellenőrzése: tag-egyensúly, az inline `<style>` (CSS-parser) és `<script>` (oxc, JS).
pub fn check_html(text: &str) -> Result<(), Problem> {
    let spans = scan(text).map_err(|(off, message)| Problem {
        section: "html".to_owned(),
        line: line_of(text, off) + 1,
        message,
    })?;
    for s in &spans {
        let Some(close) = s.close else { continue };
        if s.name != "style" && s.name != "script" {
            continue;
        }
        let content = &text[s.open.1..close.0];
        if content.trim().is_empty() {
            continue;
        }
        let base = line_of(text, s.open.1);
        let found = if s.name == "style" {
            css_error(content)
        } else {
            let attrs = open_attrs(&text[s.open.0..s.open.1]);
            let kind = attrs
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("type"))
                .and_then(|(_, v)| v.as_deref())
                .map(str::to_ascii_lowercase);
            let path = match kind.as_deref() {
                None | Some("" | "text/javascript" | "application/javascript") => Some("x.js"),
                Some("module") => Some("x.mjs"),
                _ => None,
            };
            path.and_then(|p| first_js_error(p, None, content))
        };
        if let Some((line, message)) = found {
            return Err(Problem {
                section: s.name.clone(),
                line: base + line + 1,
                message,
            });
        }
    }
    Ok(())
}

/// A `text`-ben szereplő `id` értékek a sorszámukkal (0-alapú sor).
fn ids(text: &str) -> Vec<(String, usize)> {
    let Ok(spans) = scan(text) else {
        return Vec::new();
    };
    spans
        .iter()
        .filter_map(|s| {
            open_attrs(&text[s.open.0..s.open.1])
                .into_iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("id"))
                .and_then(|(_, v)| v)
                .filter(|v| !v.is_empty())
                .map(|v| (v, line_of(text, s.open.0)))
        })
        .collect()
}

/// Figyelmeztetések: az `új` blokk `id`-i közül azok, amelyek a teljes szövegben többször szerepelnek.
pub fn duplicate_ids(text: &str, fresh: &str) -> Vec<String> {
    let all = ids(text);
    let mut warned: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (id, _) in ids(fresh) {
        if warned.contains(&id) {
            continue;
        }
        let lines: Vec<String> = all
            .iter()
            .filter(|(i, _)| *i == id)
            .map(|(_, l)| format!("L{}", l + 1))
            .collect();
        if lines.len() > 1 {
            out.push(crate::tf!(
                "warning: duplicate id `{}` ({})",
                "figyelem: duplikált id: `{}` ({})",
                id,
                lines.join(", ")
            ));
        }
        warned.push(id);
    }
    out
}
