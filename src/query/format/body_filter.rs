//! A `--body` tömör nézete: a metódus sorait egy egyszerű, nyelvenkénti lexer osztályozza (kód / megjegyzés /
//! üres sor). Csak a teljes egészében megjegyzés vagy üres sor esik ki; a kódsor a sor végi megjegyzésével együtt
//! marad, és a string / heredoc / template literal belsejében kezdődő sor is mindig marad (bizonytalanul is).
//! A sorszámok a forrásbeliek: a kihagyott sorokat a számozás hézaga jelzi.

/// A megjegyzés- és string-szintaxis családja (a `--body`-t támogató nyelvek).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Syntax {
    Php,
    Js,
    Rust,
    Swift,
}

impl Syntax {
    /// A fájl kiterjesztéséből; ismeretlennél `None` (ilyenkor szűrés nélkül, a régi módon).
    pub(super) fn for_path(path: &str) -> Option<Self> {
        let ext = std::path::Path::new(path)
            .extension()?
            .to_str()?
            .to_ascii_lowercase();
        match ext.as_str() {
            "php" | "phtml" | "inc" => Some(Self::Php),
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" | "vue" => Some(Self::Js),
            "rs" => Some(Self::Rust),
            "swift" => Some(Self::Swift),
            _ => None,
        }
    }

    /// Rustban és Swiftben a `/* */` egymásba ágyazható.
    fn nested_blocks(self) -> bool {
        matches!(self, Self::Rust | Self::Swift)
    }

    /// Attribútum / dekorátor kezdősora (`#[…]`, `@Foo`).
    fn is_attribute_start(self, trimmed: &str) -> bool {
        match self {
            Self::Rust | Self::Php => trimmed.starts_with("#["),
            Self::Js | Self::Swift => trimmed.starts_with('@'),
        }
    }

    /// Egysoros megjegyzés kezdete (`//…`, PHP-ben `#…`, de nem `#[`).
    fn is_line_comment(self, trimmed: &str) -> bool {
        trimmed.starts_with("//")
            || (self == Self::Php && trimmed.starts_with('#') && !trimmed.starts_with("#["))
    }
}

/// Egy forrássor fajtája a tömör nézethez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineKind {
    /// Kód (vagy string-tartalom): marad.
    Code,
    /// Csak megjegyzés: kiesik.
    Comment,
    /// Üres sor (nem stringben): kiesik.
    Blank,
}

/// A lexer állapota a sorhatáron át.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Code,
    /// `/* … */`, a mélységgel (Rust / Swift: egymásba ágyazva).
    Block(u32),
    /// `'…'` / `"…"`; `multiline`: a sor végén nyitva maradhat (PHP, Rust).
    Quoted {
        quote: u8,
        multiline: bool,
    },
    /// Rust `r#"…"#`, Swift `"""…"""` / `#"…"#`: a záró `"` (vagy `"""`) után `hashes` db `#`.
    Delimited {
        triple: bool,
        hashes: usize,
        escapes: bool,
        multiline: bool,
    },
    /// PHP heredoc / nowdoc a záró azonosítóval.
    Heredoc(String),
    /// JS template literal (`` `…` ``).
    Template,
}

/// Soronként hívott lexer: a `classify` a sort osztályozza, és továbbviszi az állapotot.
pub(super) struct LineScanner {
    syntax: Syntax,
    state: State,
    /// JS: a nyitott `${` szintek külső kapcsos-mélysége.
    templates: Vec<u32>,
    braces: u32,
}

impl LineScanner {
    pub(super) fn new(syntax: Syntax) -> Self {
        Self {
            syntax,
            state: State::Code,
            templates: Vec::new(),
            braces: 0,
        }
    }

    pub(super) fn classify(&mut self, line: &str) -> LineKind {
        let in_text = !matches!(self.state, State::Code | State::Block(_));
        if let State::Heredoc(id) = &self.state {
            let rest = line
                .trim_start()
                .strip_prefix(id.as_str())
                .filter(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'));
            if let Some(rest) = rest {
                let offset = line.len() - rest.len();
                self.state = State::Code;
                self.scan(line, offset);
                self.end_of_line(line);
            }
            return LineKind::Code;
        }
        let has_code = self.scan(line, 0);
        self.end_of_line(line);
        if in_text || has_code {
            LineKind::Code
        } else if line.trim().is_empty() {
            LineKind::Blank
        } else {
            LineKind::Comment
        }
    }

    /// A sor bejárása `start`-tól; igaz, ha megjegyzésen kívüli, nem üres karakter (kód vagy string) volt benne.
    fn scan(&mut self, line: &str, start: usize) -> bool {
        let bytes = line.as_bytes();
        let mut i = start;
        let mut has_code = false;
        // az utolsó kód-bájt ezen a soron (a JS regex-literál felismeréséhez)
        let mut prev = 0u8;
        while i < bytes.len() {
            // a `Heredoc` itt nem fordul elő tovább (a sor maradékát átlépi), a klón olcsó
            i = match self.state.clone() {
                State::Code => self.code(line, i, &mut has_code, &mut prev),
                State::Block(depth) => self.block(bytes, i, depth),
                State::Quoted { quote, .. } => self.quoted(bytes, i, quote),
                State::Delimited {
                    triple,
                    hashes,
                    escapes,
                    ..
                } => self.delimited(bytes, i, triple, hashes, escapes),
                State::Template => self.template(bytes, i),
                // a `<<<ID` sor maradéka: a heredoc a következő sorban kezdődik
                State::Heredoc(_) => bytes.len(),
            };
        }
        has_code
    }

    fn end_of_line(&mut self, line: &str) {
        match self.state {
            State::Quoted {
                multiline: false, ..
            } if !line.ends_with('\\') => self.state = State::Code,
            State::Delimited {
                multiline: false, ..
            } => self.state = State::Code,
            _ => {}
        }
    }

    fn code(&mut self, line: &str, i: usize, has_code: &mut bool, prev: &mut u8) -> usize {
        let bytes = line.as_bytes();
        let c = bytes[i];
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        if c.is_ascii_whitespace() {
            return i + 1;
        }
        if c == b'/' && next == b'/' {
            return bytes.len();
        }
        if c == b'/' && next == b'*' {
            self.state = State::Block(1);
            return i + 2;
        }
        if c == b'#' && self.syntax == Syntax::Php && next != b'[' {
            return bytes.len();
        }
        *has_code = true;
        let js = self.syntax == Syntax::Js;
        let after = match c {
            b'"' | b'\'' => self.open_quote(line, i),
            b'`' if js => {
                self.state = State::Template;
                i + 1
            }
            b'r' | b'b' if self.syntax == Syntax::Rust && !ident_before(bytes, i) => {
                match rust_raw_start(bytes, i) {
                    Some((hashes, after)) => {
                        self.state = State::Delimited {
                            triple: false,
                            hashes,
                            escapes: false,
                            multiline: true,
                        };
                        after
                    }
                    None => i + 1,
                }
            }
            b'#' if self.syntax == Syntax::Swift => self.swift_raw_start(bytes, i),
            b'<' if self.syntax == Syntax::Php && bytes[i..].starts_with(b"<<<") => {
                self.heredoc_start(bytes, i)
            }
            b'/' if js && regex_allowed(*prev) => skip_regex(bytes, i),
            b'{' if js && !self.templates.is_empty() => {
                self.braces += 1;
                i + 1
            }
            b'}' if js && !self.templates.is_empty() => {
                if self.braces == 0 {
                    self.braces = self.templates.pop().unwrap_or(0);
                    self.state = State::Template;
                } else {
                    self.braces -= 1;
                }
                i + 1
            }
            _ => i + 1,
        };
        *prev = c;
        after
    }

    fn open_quote(&mut self, line: &str, i: usize) -> usize {
        let bytes = line.as_bytes();
        let quote = bytes[i];
        match (self.syntax, quote) {
            (Syntax::Rust, b'\'') => rust_char_end(line, i),
            // Swiftben a `'` nem string-határoló
            (Syntax::Swift, b'\'') => i + 1,
            (Syntax::Swift, _) if bytes[i..].starts_with(b"\"\"\"") => {
                self.state = State::Delimited {
                    triple: true,
                    hashes: 0,
                    escapes: true,
                    multiline: true,
                };
                i + 3
            }
            (syntax, _) => {
                self.state = State::Quoted {
                    quote,
                    multiline: matches!(syntax, Syntax::Php | Syntax::Rust),
                };
                i + 1
            }
        }
    }

    /// Swift `#"…"#` / `#"""…"""#`; más `#` (`#if`, `#selector`) sima kód.
    fn swift_raw_start(&mut self, bytes: &[u8], i: usize) -> usize {
        let hashes = bytes[i..].iter().take_while(|&&b| b == b'#').count();
        let at = i + hashes;
        if bytes.get(at) != Some(&b'"') {
            return i + hashes;
        }
        let triple = bytes[at..].starts_with(b"\"\"\"");
        self.state = State::Delimited {
            triple,
            hashes,
            escapes: false,
            multiline: triple,
        };
        at + if triple { 3 } else { 1 }
    }

    /// PHP `<<<ID`, `<<<"ID"`, `<<<'ID'`: a heredoc a következő sortól a záró `ID`-ig tart.
    fn heredoc_start(&mut self, bytes: &[u8], i: usize) -> usize {
        let mut at = i + 3;
        while bytes.get(at).is_some_and(|b| *b == b' ' || *b == b'\t') {
            at += 1;
        }
        if bytes.get(at).is_some_and(|b| *b == b'"' || *b == b'\'') {
            at += 1;
        }
        let id: String = bytes[at.min(bytes.len())..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
            .map(|b| char::from(*b))
            .collect();
        if id.is_empty() {
            return i + 3;
        }
        self.state = State::Heredoc(id);
        bytes.len()
    }

    fn block(&mut self, bytes: &[u8], i: usize, depth: u32) -> usize {
        if bytes[i..].starts_with(b"*/") {
            self.state = if depth <= 1 {
                State::Code
            } else {
                State::Block(depth - 1)
            };
            return i + 2;
        }
        if self.syntax.nested_blocks() && bytes[i..].starts_with(b"/*") {
            self.state = State::Block(depth + 1);
            return i + 2;
        }
        i + 1
    }

    fn quoted(&mut self, bytes: &[u8], i: usize, quote: u8) -> usize {
        match bytes[i] {
            b'\\' => i + 2,
            c if c == quote => {
                self.state = State::Code;
                i + 1
            }
            _ => i + 1,
        }
    }

    fn delimited(
        &mut self,
        bytes: &[u8],
        i: usize,
        triple: bool,
        hashes: usize,
        escapes: bool,
    ) -> usize {
        if escapes && bytes[i] == b'\\' {
            return i + 2;
        }
        let quotes = if triple { 3 } else { 1 };
        let rest = &bytes[i..];
        let closes = rest.len() >= quotes + hashes
            && rest[..quotes].iter().all(|b| *b == b'"')
            && rest[quotes..quotes + hashes].iter().all(|b| *b == b'#');
        if closes {
            self.state = State::Code;
            return i + quotes + hashes;
        }
        i + 1
    }

    fn template(&mut self, bytes: &[u8], i: usize) -> usize {
        match bytes[i] {
            b'\\' => i + 2,
            b'`' => {
                self.state = State::Code;
                i + 1
            }
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                self.templates.push(self.braces);
                self.braces = 0;
                self.state = State::Code;
                i + 2
            }
            _ => i + 1,
        }
    }
}

fn ident_before(bytes: &[u8], i: usize) -> bool {
    i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')
}

/// Rust `r"…"`, `r#"…"#`, `br"…"`: a `#`-ek száma és a nyitó `"` utáni index.
fn rust_raw_start(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut at = i;
    if bytes[at] == b'b' {
        at += 1;
    }
    if bytes.get(at) != Some(&b'r') {
        return None;
    }
    at += 1;
    let hashes = bytes[at..].iter().take_while(|&&b| b == b'#').count();
    at += hashes;
    (bytes.get(at) == Some(&b'"')).then_some((hashes, at + 1))
}

/// Rust `'x'` / `'\n'` karakter-literál vége; élettartamnál (`'a`) csak a `'`-t lépi át.
fn rust_char_end(line: &str, i: usize) -> usize {
    let bytes = line.as_bytes();
    if bytes.get(i + 1) == Some(&b'\\') {
        return bytes
            .get(i + 3..)
            .and_then(|rest| rest.iter().position(|b| *b == b'\''))
            .map_or(i + 1, |p| i + 3 + p + 1);
    }
    match line.get(i + 1..).and_then(|rest| rest.chars().next()) {
        Some(ch) if bytes.get(i + 1 + ch.len_utf8()) == Some(&b'\'') => i + 2 + ch.len_utf8(),
        _ => i + 1,
    }
}

/// JS: a `/` regex-literált nyit-e (az előző kód-bájt alapján; sor elején is).
fn regex_allowed(prev: u8) -> bool {
    prev == 0 || b"(,=:[!&|?{};+-*%<>~^".contains(&prev)
}

/// A regex-literál utáni index; ha a sorban nincs záró `/`, osztásnak veszi.
fn skip_regex(bytes: &[u8], i: usize) -> usize {
    let mut at = i + 1;
    let mut class = false;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 1,
            b'[' => class = true,
            b']' => class = false,
            b'/' if !class => return at + 1,
            _ => {}
        }
        at += 1;
    }
    i + 1
}

/// A metódus előtti „vezető” sáv első sora: a közvetlenül fölötte álló doc / megjegyzés / attribútum sorok
/// (docblocknál a köztes üres sorokkal). Ha nincs ilyen, a metódus sora.
pub(super) fn lead_start(lines: &[String], method_line: u32, syntax: Syntax, has_doc: bool) -> u32 {
    let text = |number: u32| {
        lines
            .get(number.saturating_sub(1) as usize)
            .map_or("", |line| line.trim())
    };
    let mut first = method_line;
    let mut cursor = method_line.saturating_sub(1);
    if has_doc {
        while cursor > 0 && text(cursor).is_empty() {
            cursor -= 1;
        }
    }
    while cursor > 0 {
        let trimmed = text(cursor);
        let start = if trimmed.is_empty() {
            None
        } else if syntax.is_line_comment(trimmed) || syntax.is_attribute_start(trimmed) {
            Some(cursor)
        } else if trimmed.ends_with("*/") {
            block_comment_start(&text, cursor)
        } else if trimmed.ends_with(']') || trimmed.ends_with(')') {
            attribute_block_start(&text, cursor, syntax)
        } else {
            None
        };
        let Some(start) = start else { break };
        first = start;
        cursor = start - 1;
    }
    first
}

/// A `*/`-re végződő sorhoz tartozó, sor elején álló `/*` sora.
fn block_comment_start<'a>(text: &impl Fn(u32) -> &'a str, end: u32) -> Option<u32> {
    let mut cursor = end;
    while cursor > 0 {
        let line = text(cursor);
        if let Some(at) = line.find("/*") {
            return (at == 0).then_some(cursor);
        }
        cursor -= 1;
    }
    None
}

/// Többsoros attribútum (`#[tool(\n…\n)]`, `@Component({\n…\n})`) kezdősora, legfeljebb 30 sorral feljebb.
fn attribute_block_start<'a>(
    text: &impl Fn(u32) -> &'a str,
    end: u32,
    syntax: Syntax,
) -> Option<u32> {
    let floor = end.saturating_sub(30).max(1);
    let mut cursor = end.checked_sub(1)?;
    while cursor >= floor {
        let line = text(cursor);
        if line.is_empty() {
            return None;
        }
        if syntax.is_attribute_start(line) {
            return balanced(text, cursor, end).then_some(cursor);
        }
        cursor -= 1;
    }
    None
}

/// A `start..=end` sorok zárójelei a végén zárulnak, közben végig nyitva vannak (egyetlen többsoros kifejezés).
fn balanced<'a>(text: &impl Fn(u32) -> &'a str, start: u32, end: u32) -> bool {
    let mut depth = 0i32;
    for number in start..=end {
        for byte in text(number).bytes() {
            match byte {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ => {}
            }
        }
        if number < end && depth <= 0 {
            return false;
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(syntax: Syntax, source: &str) -> Vec<LineKind> {
        let mut scanner = LineScanner::new(syntax);
        source.lines().map(|line| scanner.classify(line)).collect()
    }

    /// A megtartott sorok (kód / string-tartalom) szövege.
    fn kept(syntax: Syntax, source: &str) -> Vec<&str> {
        let mut scanner = LineScanner::new(syntax);
        source
            .lines()
            .filter(|line| scanner.classify(line) == LineKind::Code)
            .collect()
    }

    #[test]
    fn php_drops_comments_keeps_attributes_strings_and_heredoc() {
        let source = "    #[Route('/x')]\n    public function run()\n    {\n        // note\n        # hash note\n\n        /* block\n         * more\n         */\n        $a = 1; // trailing\n        $s = 'first\n// not a comment\n';\n        $h = <<<EOT\n# inside heredoc\n\n/* also text */\nEOT;\n        return $a; # end\n    }";
        assert_eq!(
            kept(Syntax::Php, source),
            [
                "    #[Route('/x')]",
                "    public function run()",
                "    {",
                "        $a = 1; // trailing",
                "        $s = 'first",
                "// not a comment",
                "';",
                "        $h = <<<EOT",
                "# inside heredoc",
                "",
                "/* also text */",
                "EOT;",
                "        return $a; # end",
                "    }",
            ]
        );
    }

    #[test]
    fn php_nowdoc_with_indented_closer() {
        let source = "$x = <<<'SQL'\n    -- select\n    // keep\n    SQL;\n// drop";
        assert_eq!(
            kinds(Syntax::Php, source),
            [
                LineKind::Code,
                LineKind::Code,
                LineKind::Code,
                LineKind::Code,
                LineKind::Comment
            ]
        );
    }

    #[test]
    fn js_template_literal_and_regex_are_not_comments() {
        let source = "function f() {\n  // drop\n  const t = `line one\n// kept: inside template\n${ a /* inline */ }\n/* kept too */`;\n  const re = /\\/\\*[\"']/g;\n  /* drop */\n  const u = 'http://x'; // trailing\n  return `${`nested`}` // ok\n}";
        assert_eq!(
            kept(Syntax::Js, source),
            [
                "function f() {",
                "  const t = `line one",
                "// kept: inside template",
                "${ a /* inline */ }",
                "/* kept too */`;",
                "  const re = /\\/\\*[\"']/g;",
                "  const u = 'http://x'; // trailing",
                "  return `${`nested`}` // ok",
                "}",
            ]
        );
    }

    #[test]
    fn js_single_quoted_string_does_not_leak_into_the_next_line() {
        let source = "  const s = 'unterminated\n  // drop";
        assert_eq!(
            kinds(Syntax::Js, source),
            [LineKind::Code, LineKind::Comment]
        );
    }

    #[test]
    fn rust_keeps_attributes_and_multiline_strings_drops_doc_comments() {
        let source = "    /// Doc.\n    #[inline]\n    fn f<'a>(x: &'a str) -> char {\n        //! inner doc\n        let q = '\"';\n        let s = \"first\n// kept: string\n\";\n        let r = r#\"raw \"quoted\"\n/* kept */\n\"#;\n        /* outer /* nested */ still comment\n        */\n        let c = '\\'';\n        'x' // trailing\n    }";
        assert_eq!(
            kept(Syntax::Rust, source),
            [
                "    #[inline]",
                "    fn f<'a>(x: &'a str) -> char {",
                "        let q = '\"';",
                "        let s = \"first",
                "// kept: string",
                "\";",
                "        let r = r#\"raw \"quoted\"",
                "/* kept */",
                "\"#;",
                "        let c = '\\'';",
                "        'x' // trailing",
                "    }",
            ]
        );
    }

    #[test]
    fn swift_multiline_and_raw_strings_are_kept() {
        let source = "    func f() -> String {\n        // drop\n        let s = \"\"\"\n        // kept\n\n        \"\"\"\n        let r = #\"a \"b\" // c\"#\n        /* drop /* nested */ */\n        return s + r // trailing\n    }";
        assert_eq!(
            kept(Syntax::Swift, source),
            [
                "    func f() -> String {",
                "        let s = \"\"\"",
                "        // kept",
                "",
                "        \"\"\"",
                "        let r = #\"a \"b\" // c\"#",
                "        return s + r // trailing",
                "    }",
            ]
        );
    }

    fn lines(source: &str) -> Vec<String> {
        source.lines().map(str::to_owned).collect()
    }

    #[test]
    fn lead_covers_docblock_doc_comments_and_attributes() {
        let php = lines(
            "<?php\nclass A {\n    /**\n     * Doc.\n     */\n\n    public function run() {}\n}",
        );
        assert_eq!(lead_start(&php, 7, Syntax::Php, true), 3);
        let rust =
            lines("}\n\n/// Doc.\n#[tool(\n    description = \"x\"\n)]\n/// More.\nfn f() {}");
        assert_eq!(lead_start(&rust, 8, Syntax::Rust, true), 3);
        let plain = lines("}\n\nfn f() {}");
        assert_eq!(lead_start(&plain, 3, Syntax::Rust, false), 3);
        // a fölötte álló kódsor (`foo(); /* x */`) nem része
        let js = lines("  foo() /* x */\n  bar() {}");
        assert_eq!(lead_start(&js, 2, Syntax::Js, false), 2);
    }
}
