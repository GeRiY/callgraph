//! Egyszerű, hibatűrő HTML-tag-szkenner (nem teljes HTML-parser): a `<title>`, a `<h1>`–`<h6>` címsorok, a tagek
//! közötti látható szöveg, a gyakori szöveges attribútumok (`title`, `alt`, `placeholder`, `aria-label`, `value`) és
//! a linkek (`<a href>`, `<img src>`). A kommentet (`<!-- -->`), a `<!DOCTYPE>` / `<?…?>` deklarációt, valamint a
//! `<script>` és `<style>` tartalmát átugorja. A nem zárt tag, idézőjel, komment vagy címsor a fájl végéig tart —
//! sosem pánikol: minden szeletelés ASCII-határoló bájton (vagy a szöveg végén) történik, így UTF-8-határon.

use crate::core::markup::{Tag, find, find_ci, parse_tag, skip_to_gt, tag_name};
use crate::core::text::collapse_ws;

/// Egy címsor: szint (`0` = `<title>`, `1`–`6` = `<hN>`), a nyitó tag és a lezárás bájt-pozíciója, a szövege.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Heading {
    pub level: u8,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Egy látható szöveg (tagek közötti szöveg vagy szöveges attribútum értéke): a bájt-tartománya és az értéke
/// (entitások feloldva, a szóközök egyre vonva).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Text {
    pub start: usize,
    pub end: usize,
    pub value: String,
}

/// Egy hivatkozás: `<a href>` (`image: false`) vagy `<img src>` (`image: true`), a tag kezdő pozíciójával.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    pub image: bool,
    pub start: usize,
    pub target: String,
}

/// Egy belépő-hivatkozás: `<script src>` vagy `<link rel="stylesheet" href>`, a tag kezdő pozíciójával.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Import {
    pub start: usize,
    pub target: String,
}

/// A szkennelés eredménye, dokumentum-sorrendben.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Scan {
    pub title: Option<Heading>,
    pub headings: Vec<Heading>,
    pub texts: Vec<Text>,
    pub links: Vec<Link>,
    pub imports: Vec<Import>,
}

/// A látható szöveget hordozó attribútumok (a `--strings` ezekben is keres).
const TEXT_ATTRS: [&str; 5] = ["title", "alt", "placeholder", "aria-label", "value"];

/// A szkenner állapota.
struct Scanner<'s> {
    src: &'s str,
    bytes: &'s [u8],
    out: Scan,
    /// A nyitott (még le nem zárt) `<hN>` címsor.
    open: Option<Heading>,
}

/// A HTML-szöveg szkennelése.
pub(crate) fn scan(src: &str) -> Scan {
    let mut scanner = Scanner {
        src,
        bytes: src.as_bytes(),
        out: Scan::default(),
        open: None,
    };
    scanner.run();
    scanner.out
}

impl<'s> Scanner<'s> {
    fn run(&mut self) {
        let len = self.bytes.len();
        let mut i = 0;
        let mut text_start = 0;
        while i < len {
            if self.bytes[i] != b'<' {
                i += 1;
                continue;
            }
            let next = self.bytes.get(i + 1).copied();
            let after = if self.bytes[i..].starts_with(b"<!--") {
                // komment: a `-->`-ig (nem zárt kommentnél a fájl végéig)
                find(self.bytes, i + 4, b"-->").map_or(len, |at| at + 3)
            } else if matches!(next, Some(b'!' | b'?')) {
                // `<!DOCTYPE …>`, `<![CDATA[…]]>`, `<?xml …?>`: a következő `>`-ig
                skip_to_gt(self.bytes, i + 2)
            } else if next == Some(b'/')
                && self.bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic)
            {
                self.flush(text_start, i);
                let (name, _) = tag_name(self.bytes, i + 2);
                if heading_level(&name).is_some() {
                    self.close_heading(i);
                }
                text_start = skip_to_gt(self.bytes, i + 2);
                i = text_start;
                continue;
            } else if next.is_some_and(|b| b.is_ascii_alphabetic()) {
                self.flush(text_start, i);
                let tag = self.open_tag(i);
                text_start = self.after_tag(i, &tag);
                i = text_start;
                continue;
            } else {
                // magányos `<` (pl. `a < b`): a szöveg része
                i += 1;
                continue;
            };
            // komment / deklaráció: ami előtte volt, szöveg; maga nem
            self.flush(text_start, i);
            text_start = after;
            i = after;
        }
        self.flush(text_start, len);
        self.close_heading(len);
    }

    /// A `[from, to)` szöveg-szakasz: a nyitott címsor szövegéhez fűzve, és (ha nem üres) látható szövegként.
    fn flush(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        let segment = &self.src[from..to];
        let decoded = decode(segment);
        if let Some(open) = self.open.as_mut() {
            open.text.push_str(&decoded);
        }
        self.push_text(from, segment, &decoded);
    }

    /// Egy szöveg felvétele (a `raw` a forrásbeli alak a pozíciókhoz, a `decoded` az érték); üres szöveg kimarad.
    fn push_text(&mut self, from: usize, raw: &str, decoded: &str) {
        let value = collapse_ws(decoded);
        if value.is_empty() {
            return;
        }
        let start = from + (raw.len() - raw.trim_start().len());
        let end = from + raw.trim_end().len();
        self.out.texts.push(Text {
            start,
            end: end.max(start),
            value,
        });
    }

    /// A nyitott címsor lezárása az `at` pozíción (az üres címsor kimarad).
    fn close_heading(&mut self, at: usize) {
        if let Some(mut heading) = self.open.take() {
            heading.text = collapse_ws(&heading.text);
            heading.end = at.max(heading.start);
            if !heading.text.is_empty() {
                self.out.headings.push(heading);
            }
        }
    }

    /// Egy nyitó tag beolvasása (`start`: a `<` helye) és az attribútumai feldolgozása.
    fn open_tag(&mut self, start: usize) -> Tag<'s> {
        let tag = parse_tag(self.src, start);
        if let Some(level) = heading_level(&tag.name) {
            // a nem zárt előző címsor itt véget ér
            self.close_heading(start);
            self.open = Some(Heading {
                level,
                start,
                end: start,
                text: String::new(),
            });
        }
        for attr in &tag.attrs {
            if TEXT_ATTRS.contains(&attr.name.as_str()) {
                let decoded = decode(attr.value);
                self.push_text(attr.start, attr.value, &decoded);
            }
            let link = match (tag.name.as_str(), attr.name.as_str()) {
                ("a", "href") => Some(false),
                ("img", "src") => Some(true),
                _ => None,
            };
            if let Some(image) = link {
                let target = decode(attr.value.trim());
                if !target.is_empty() {
                    self.out.links.push(Link {
                        image,
                        start,
                        target,
                    });
                }
            }
        }
        self.collect_import(start, &tag);
        tag
    }

    /// `<script src>` / `<link rel="stylesheet" href>`: belépő-hivatkozás (külső URL-t a hívó szűri).
    fn collect_import(&mut self, start: usize, tag: &Tag<'s>) {
        let attr = |name: &str| tag.attrs.iter().find(|a| a.name == name).map(|a| a.value);
        let value = match tag.name.as_str() {
            "script" => attr("src"),
            "link"
                if attr("rel").is_some_and(|rel| {
                    rel.split_ascii_whitespace()
                        .any(|word| word.eq_ignore_ascii_case("stylesheet"))
                }) =>
            {
                attr("href")
            }
            _ => None,
        };
        let target = value.map(|v| decode(v.trim())).unwrap_or_default();
        if !target.is_empty() {
            self.out.imports.push(Import { start, target });
        }
    }

    /// A tag utáni pozíció: a `<script>` / `<style>` tartalma átugorva, a `<title>` szövege a címként felvéve.
    fn after_tag(&mut self, start: usize, tag: &Tag<'s>) -> usize {
        let raw_text = matches!(tag.name.as_str(), "script" | "style" | "title");
        if !raw_text || self.src[start..tag.end].ends_with("/>") {
            return tag.end;
        }
        let closing = format!("</{}", tag.name);
        let close = find_ci(self.bytes, tag.end, closing.as_bytes());
        let content_end = close.unwrap_or(self.bytes.len());
        if tag.name == "title" {
            let raw = &self.src[tag.end..content_end];
            let decoded = decode(raw);
            let text = collapse_ws(&decoded);
            if self.out.title.is_none() && !text.is_empty() {
                self.out.title = Some(Heading {
                    level: 0,
                    start,
                    end: content_end,
                    text,
                });
            }
            self.push_text(tag.end, raw, &decoded);
        }
        close.map_or(self.bytes.len(), |at| skip_to_gt(self.bytes, at + 2))
    }
}

/// A `<hN>` címsor szintje (`h1` … `h6`); más tagnál `None`.
fn heading_level(name: &str) -> Option<u8> {
    let digit = name.strip_prefix('h')?;
    match digit.as_bytes() {
        [d @ b'1'..=b'6'] => Some(d - b'0'),
        _ => None,
    }
}

/// A gyakori HTML-entitások feloldása (`&amp;`, `&lt;`, `&nbsp;`, `&#39;`, `&#x2014;` …); az ismeretlen marad.
pub(crate) fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let entity = tail
            .find(';')
            .filter(|&n| (1..=10).contains(&n))
            .and_then(|n| entity_char(&tail[..n]).map(|c| (c, n)));
        match entity {
            Some((c, n)) => {
                out.push(c);
                rest = &tail[n + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Egy entitás (a `&` és a `;` közötti rész) karaktere.
fn entity_char(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse::<u32>().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "copy" => '©',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(scan: &Scan) -> Vec<&str> {
        scan.texts.iter().map(|t| t.value.as_str()).collect()
    }

    #[test]
    fn collects_title_headings_texts_attributes_and_links() {
        let src = "<!DOCTYPE html>\n<html><head><title>Design &amp; plan</title></head>\n<body>\n<h1 class=\"x\">Intro <em>part</em></h1>\n<p>Hello&nbsp;world</p>\n<img src=\"a.png\" alt='Diagram'>\n<input placeholder=Search value=\"go\" disabled>\n<a href=\"#top\" aria-label=\"Back\">up</a>\n</body></html>\n";
        let scan = scan(src);
        let title = scan.title.as_ref().unwrap();
        assert_eq!((title.level, title.text.as_str()), (0, "Design & plan"));
        assert_eq!(scan.headings.len(), 1);
        assert_eq!(
            (scan.headings[0].level, scan.headings[0].text.as_str()),
            (1, "Intro part")
        );
        let texts = values(&scan);
        for want in [
            "Design & plan",
            "Intro",
            "part",
            "Hello world",
            "Diagram",
            "Search",
            "go",
            "Back",
            "up",
        ] {
            assert!(texts.contains(&want), "{want}: {texts:?}");
        }
        // a nem szöveges attribútum (class, src, href) nem szöveg
        assert!(
            !texts.iter().any(|t| t.contains("a.png") || *t == "x"),
            "{texts:?}"
        );
        let links: Vec<(bool, &str)> = scan
            .links
            .iter()
            .map(|l| (l.image, l.target.as_str()))
            .collect();
        assert_eq!(links, [(true, "a.png"), (false, "#top")]);
    }

    #[test]
    fn collects_script_and_stylesheet_imports() {
        let src = "<link rel=\"stylesheet\" href=\"/a.css\"><link rel=\"icon\" href=\"x.png\">\n<script type=\"module\" src=\"/src/main.ts\"></script><script>var x = 1;</script><link REL=\"preload stylesheet\" href=b.css>";
        let scan = scan(src);
        let found: Vec<&str> = scan.imports.iter().map(|i| i.target.as_str()).collect();
        assert_eq!(found, ["/a.css", "/src/main.ts", "b.css"]);
    }

    #[test]
    fn skips_comments_scripts_and_styles() {
        let src = "<p>keep</p><!-- hidden comment --><script>var secret = '<h1>no</h1>';</script>\n<STYLE>.x{content:'styled'}</STYLE><p>after</p>";
        let scan = scan(src);
        assert_eq!(values(&scan), ["keep", "after"]);
        assert!(scan.headings.is_empty(), "{:?}", scan.headings);
    }

    #[test]
    fn broken_html_never_panics() {
        for src in [
            "<",
            "<h1",
            "<h2>open heading\nno close",
            "<p title=\"unterminated",
            "<!-- never closed",
            "<script>never closed",
            "<title>never closed",
            "<a href=>x</a><img src>",
            "a < b && c > d",
            "<p =x>árvíztűrő &#x1F600; &bogus; &#xZZ; &</p>",
            "</",
            "</h1",
            "<h3/>",
            "<!",
        ] {
            let _ = scan(src);
        }
        let scan = scan("<h2>open heading\nno close");
        assert_eq!(scan.headings[0].text, "open heading no close");
        assert_eq!(scan.headings[0].end, 25);
    }

    #[test]
    fn an_unclosed_heading_ends_at_the_next_heading() {
        let scan = scan("<h1>One\n<h2>Two</h2>");
        let found: Vec<(u8, &str)> = scan
            .headings
            .iter()
            .map(|h| (h.level, h.text.as_str()))
            .collect();
        assert_eq!(found, [(1, "One"), (2, "Two")]);
    }

    #[test]
    fn decodes_common_entities_and_keeps_unknown_ones() {
        assert_eq!(
            decode("a &lt;b&gt; &quot;c&quot; &#39;d&#x27;"),
            "a <b> \"c\" 'd'"
        );
        assert_eq!(decode("&bogus; & &#xZZ;"), "&bogus; & &#xZZ;");
        assert_eq!(decode("árvíz &mdash; tűrő"), "árvíz — tűrő");
    }
}
