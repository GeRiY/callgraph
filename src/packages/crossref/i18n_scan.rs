//! Az i18n-kapcsolat szövegolvasói: a PHP nyelvi tömb (`return [...]`) levelei és a fordítási hívások
//! (`$t('a.b')`, `__('x')`, `trans(...)`, `@lang(...)`, `v-t`, `keypath`) a forrásfájlokból. Hibatűrő: ami nem
//! olvasható, kimarad.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::core::LineIndex;

/// Egy nyelvi levél: kulcs-út, sor, a string-érték (más típusnál `None`).
pub(super) type Leaf = (String, u32, Option<String>);

/// Egy statikus kulcs-használat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Usage {
    pub(crate) key: String,
    pub(crate) file: String,
    pub(crate) line: u32,
}

/// A forrásfájlok használat-gyűjtése (a `scan` eredménye).
#[derive(Debug, Default)]
pub(crate) struct Found {
    /// Literál kulccsal hívott helyek.
    pub(crate) usages: Vec<Usage>,
    /// Dinamikus kulcs előtagja (`x.` a `` `x.${y}` `` / `'x.' + y` hívásból).
    pub(crate) prefixes: Vec<String>,
    /// Nem literál első argumentumú hívások száma (nem ellenőrizhetők).
    pub(crate) variable_args: usize,
    /// A forrásokban idézőjelben előforduló pontozott nevek (a kulcs más úton is hivatkozható).
    pub(crate) literals: HashSet<String>,
}

fn call_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(@lang|\$tc|\$te|\$t|trans_choice|trans|__|Lang::(?:get|choice|has)|\btc|\bt)\s*\(\s*(['"`])?"#,
        )
        .expect("érvényes regex")
    });
    &RE
}

fn directive_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"\bv-t(?:\.\w+)?\s*=\s*"\s*'([^'"]+)'\s*"|\bkeypath\s*=\s*"\s*'?([^"'\s]+)'?\s*""#,
        )
        .expect("érvényes regex")
    });
    &RE
}

fn literal_re() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"['"`]([A-Za-z_][\w-]*(?:\.[\w-]+)+)['"`]"#).expect("érvényes regex")
    });
    &RE
}

fn return_re() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\breturn\s*(?:\[|array\s*\()").expect("érvényes regex"));
    &RE
}

/// Az `ext` JS-szerű (a `t` / `$t` hívások itt értelmezettek).
fn is_js(ext: &str) -> bool {
    matches!(
        ext,
        "vue" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "html" | "htm"
    )
}

/// A hívó név engedélyezett-e a fájltípusban (a PHP-ban nincs `t()` / `$t()`).
fn allowed(function: &str, ext: &str) -> bool {
    match function {
        "t" | "tc" | "$t" | "$tc" | "$te" => is_js(ext),
        "@lang" | "Lang::get" | "Lang::choice" | "Lang::has" | "trans_choice" => {
            matches!(ext, "php" | "html" | "htm")
        }
        _ => true,
    }
}

/// A `.` előtti szöveg i18n-objektumra utal-e (`i18n.t(`, `this.$t(`).
fn i18n_receiver(before_dot: &str) -> bool {
    ["i18n", "$i18n", "this", "i18next", "vm", "app", "global"]
        .iter()
        .any(|word| before_dot.ends_with(word))
}

/// A hívás előtti karakter alapján a találat valódi hívás-e (nem `->t(`, `x.t(`, `format(`).
fn call_position(text: &str, start: usize, function: &str) -> bool {
    let prev = text[..start].chars().next_back();
    let bare = function
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_');
    match prev {
        None => true,
        Some('>' | ':') => false,
        Some('\\') => function.starts_with("Lang::"),
        Some('.') => {
            matches!(function, "t" | "tc" | "$t" | "$tc" | "$te")
                && i18n_receiver(&text[..start - 1])
        }
        Some(c) if bare => !(c.is_alphanumeric() || c == '_' || c == '$'),
        Some(_) => true,
    }
}

/// A fordítási hívások és kulcs-hivatkozások a forrásszövegből.
pub(super) fn scan(rel: &str, ext: &str, text: &str, found: &mut Found) {
    let mut lines: Option<LineIndex> = None;
    let mut line_of = |offset: usize| -> u32 {
        lines
            .get_or_insert_with(|| LineIndex::new(text))
            .line_of(offset)
    };
    for caps in call_re().captures_iter(text) {
        let (Some(whole), Some(function)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        let function = function.as_str();
        if !allowed(function, ext) || !call_position(text, whole.start(), function) {
            continue;
        }
        let Some(quote) = caps.get(2) else {
            found.variable_args += 1;
            continue;
        };
        let quote_char = quote.as_str().chars().next().unwrap_or('\'');
        let Some((body, after)) = read_literal(text, quote.end(), quote_char) else {
            found.variable_args += 1;
            continue;
        };
        let next = text[after..].trim_start().chars().next();
        // interpoláció / összefűzés: csak az előtag ismert
        let cut = interpolation_cut(body, quote_char, ext);
        let concatenated = matches!(next, Some('+')) || (ext == "php" && matches!(next, Some('.')));
        if cut.is_some() || concatenated {
            let prefix = &body[..cut.unwrap_or(body.len())];
            if !prefix.is_empty() {
                found.prefixes.push(prefix.to_owned());
            }
            continue;
        }
        if body.is_empty() || body.contains('\n') || body.contains(':') {
            continue;
        }
        // a vue-i18n-szerű `t` / `$t` kulcsai szóköz nélküliek (a `__('Welcome back')` mondat-kulcs más)
        if matches!(function, "t" | "tc" | "$t" | "$tc" | "$te")
            && body.contains(char::is_whitespace)
        {
            continue;
        }
        found.usages.push(Usage {
            key: body.to_owned(),
            file: rel.to_owned(),
            line: line_of(whole.start()),
        });
    }
    for caps in directive_re().captures_iter(text) {
        let (Some(whole), Some(key)) = (caps.get(0), caps.get(1).or_else(|| caps.get(2))) else {
            continue;
        };
        if key.as_str().contains(':') || key.as_str().contains("{{") {
            continue;
        }
        found.usages.push(Usage {
            key: key.as_str().to_owned(),
            file: rel.to_owned(),
            line: line_of(whole.start()),
        });
    }
    for caps in literal_re().captures_iter(text) {
        if let Some(name) = caps.get(1)
            && name.as_str().len() <= 120
        {
            found.literals.insert(name.as_str().to_owned());
        }
    }
}

/// A literál törzse a nyitó idézőjel után (`start`) a záróig, és a záró utáni bájt-pozíció. Nem backtick literálban
/// az újsor „nem literál” (`None`).
fn read_literal(text: &str, start: usize, quote: char) -> Option<(&str, usize)> {
    let rest = &text[start..];
    let mut escaped = false;
    for (offset, c) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            c if c == quote => return Some((&rest[..offset], start + offset + c.len_utf8())),
            '\n' if quote != '`' => return None,
            _ => {}
        }
    }
    None
}

/// Az interpoláció helye a literál törzsében: JS sablon-literál `${`, PHP `"…$x…"` / `{$x}`.
fn interpolation_cut(body: &str, quote: char, ext: &str) -> Option<usize> {
    if quote == '\'' {
        return None;
    }
    let mut cut = body.find("${");
    if quote == '"' && ext == "php" {
        cut = [cut, body.find('$')].into_iter().flatten().min();
    }
    cut
}

/// A `return [ ... ];` tömb levelei (`prefix.kulcs` utakkal); a nem string értékű levél `None` értékű. Ami nem
/// értelmezhető, kimarad.
pub(super) fn php_leaves(text: &str, prefix: &str) -> Vec<Leaf> {
    let Some(start) = return_re().find(text) else {
        return Vec::new();
    };
    let mut parser = PhpArray {
        b: text.as_bytes(),
        pos: start.end(),
        lines: LineIndex::new(text),
        out: Vec::new(),
    };
    parser.items(prefix);
    parser.out
}

struct PhpArray<'a> {
    b: &'a [u8],
    pos: usize,
    lines: LineIndex,
    out: Vec<Leaf>,
}

impl PhpArray<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn starts(&self, prefix: &[u8]) -> bool {
        self.b[self.pos.min(self.b.len())..].starts_with(prefix)
    }

    /// Szóközök és kommentek átugrása.
    fn ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => self.pos += 1,
                Some(b'#') if !self.starts(b"#[") => self.skip_line(),
                Some(b'/') if self.starts(b"//") => self.skip_line(),
                Some(b'/') if self.starts(b"/*") => {
                    self.pos += 2;
                    while self.pos < self.b.len() && !self.starts(b"*/") {
                        self.pos += 1;
                    }
                    self.pos = (self.pos + 2).min(self.b.len());
                }
                _ => return,
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            if c == b'\n' {
                return;
            }
            self.pos += 1;
        }
    }

    /// Idézőjeles string (a `\'`, `\"`, `\\` feloldva).
    fn string(&mut self) -> Option<String> {
        let quote = self.peek().filter(|c| matches!(c, b'\'' | b'"'))?;
        self.pos += 1;
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == b'\\' {
                if let Some(next) = self.peek() {
                    self.pos += 1;
                    if next != quote && next != b'\\' {
                        out.push(b'\\');
                    }
                    out.push(next);
                }
            } else if c == quote {
                return Some(String::from_utf8_lossy(&out).into_owned());
            } else {
                out.push(c);
            }
        }
        None
    }

    /// Egy kifejezés átugrása a legfelső szintű `,`, `=>` vagy záró zárójelig.
    fn skip_expr(&mut self) {
        let mut depth = 0i32;
        while let Some(c) = self.peek() {
            match c {
                b'\'' | b'"' => {
                    let _ = self.string();
                    continue;
                }
                b'(' | b'[' => depth += 1,
                b')' | b']' => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                b',' if depth == 0 => return,
                b'=' if depth == 0 && self.starts(b"=>") => return,
                _ => {}
            }
            self.pos += 1;
        }
    }

    fn arrow(&mut self) -> bool {
        self.ws();
        if self.starts(b"=>") {
            self.pos += 2;
            true
        } else {
            false
        }
    }

    /// A tömb elemei a záró zárójelig.
    fn items(&mut self, prefix: &str) {
        let mut index = 0usize;
        loop {
            self.ws();
            match self.peek() {
                None => return,
                Some(b']' | b')') => {
                    self.pos += 1;
                    return;
                }
                Some(b',') => {
                    self.pos += 1;
                    continue;
                }
                _ => {}
            }
            let at = self.pos;
            let line = self.lines.line_of(at);
            let name = if matches!(self.peek(), Some(b'\'' | b'"')) {
                let Some(text) = self.string() else { return };
                if self.arrow() {
                    text
                } else {
                    // pozicionális string-érték
                    let name = index.to_string();
                    index += 1;
                    self.finish_value(format!("{prefix}.{name}"), line, Some(text));
                    continue;
                }
            } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                let begin = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
                let number = String::from_utf8_lossy(&self.b[begin..self.pos]).into_owned();
                if self.arrow() {
                    number
                } else {
                    self.skip_expr();
                    index += 1;
                    continue;
                }
            } else {
                self.skip_expr();
                if self.arrow() {
                    // konstans / hívás kulcs: nem ismert név
                    format!("?{at}")
                } else {
                    index += 1;
                    continue;
                }
            };
            self.ws();
            self.value(format!("{prefix}.{name}"), line);
        }
    }

    /// Az érték a `=>` után: beágyazott tömb, string vagy más kifejezés.
    fn value(&mut self, path: String, line: u32) {
        match self.peek() {
            Some(b'[') => {
                self.pos += 1;
                self.items(&path);
            }
            Some(b'a' | b'A') if self.starts_array_call() => {
                self.items(&path);
            }
            Some(b'\'' | b'"') => {
                let text = self.string();
                self.finish_value(path, line, text);
            }
            _ => {
                self.skip_expr();
                if !path.contains(".?") {
                    self.out.push((path, line, None));
                }
            }
        }
    }

    /// `array(` — ha igen, a pozíció a nyitó zárójel után áll.
    fn starts_array_call(&mut self) -> bool {
        let rest = &self.b[self.pos..];
        if rest.len() < 5 || !rest[..5].eq_ignore_ascii_case(b"array") {
            return false;
        }
        let mut at = self.pos + 5;
        while self.b.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if self.b.get(at) == Some(&b'(') {
            self.pos = at + 1;
            true
        } else {
            false
        }
    }

    /// A string-érték rögzítése; ha utána még kifejezés jön (`'a' . $x`), az érték nem ismert.
    fn finish_value(&mut self, path: String, line: u32, text: Option<String>) {
        self.ws();
        let simple = matches!(self.peek(), None | Some(b',' | b']' | b')'));
        if !simple {
            self.skip_expr();
        }
        if path.contains(".?") {
            return;
        }
        self.out.push((path, line, text.filter(|_| simple)));
    }
}
