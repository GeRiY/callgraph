//! A PHP-forrás szerkezeti modellje az átnevezéshez: szegmensekre bontás (HTML / kód / megjegyzés / docblock /
//! sztring-tartalom), megtisztított másolatok (azonos bájt-hosszal, így az eltolások közösek), névtér, `use`
//! importok, típus-deklarációk (ős, interfészek, törzs-tartomány) és a névfeloldás (rövid név → teljes név a
//! fájl névterével és importjaival, mint a `lang::php::analysis::name_resolver`).

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::generate::fs::{FileSystem, walk_php};
use crate::generate::project::relative;

/// A szegmens fajtája.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `<?php` előtti / `?>` utáni szöveg.
    Html,
    /// Kód (a sztring idézőjelei is).
    Code,
    /// `//`, `#`, `/* */` megjegyzés.
    Comment,
    /// `/** */` docblock.
    Doc,
    /// Sztring-literál tartalma (az idézőjelek nélkül).
    Str,
}

/// Egy szegmens bájt-tartománya.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seg {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

fn push(segs: &mut Vec<Seg>, start: usize, end: usize, kind: Kind) {
    if end > start {
        segs.push(Seg { start, end, kind });
    }
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// A következő PHP nyitó tag helye és hossza.
fn find_open(src: &str, from: usize) -> Option<(usize, usize)> {
    let mut at = from;
    while let Some(found) = src.get(at..)?.find("<?") {
        let pos = at + found;
        let rest = &src[pos..];
        if rest.starts_with("<?php") {
            return Some((pos, 5));
        }
        if rest.starts_with("<?=") {
            return Some((pos, 3));
        }
        at = pos + 2;
    }
    None
}

/// A szöveg szegmensei (fedik a teljes szöveget, sorrendben).
pub(crate) fn lex(src: &str) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut i = 0;
    while i < src.len() {
        match find_open(src, i) {
            None => {
                push(&mut segs, i, src.len(), Kind::Html);
                break;
            }
            Some((pos, len)) => {
                push(&mut segs, i, pos, Kind::Html);
                push(&mut segs, pos, pos + len, Kind::Code);
                i = lex_php(src, pos + len, &mut segs);
            }
        }
    }
    segs
}

/// Egy idézőjeles sztring tartalmának vége (a záró idézőjel helye, vagy a szöveg vége).
fn string_end(b: &[u8], from: usize, quote: u8) -> usize {
    let mut j = from;
    while j < b.len() && b[j] != quote {
        if b[j] == b'\\' {
            j += 1;
        }
        j += 1;
    }
    j.min(b.len())
}

/// A heredoc / nowdoc: `(a tartalom kezdete, a záró azonosító sorának kezdete)`, ha az `i`-n `<<<` áll.
fn heredoc(src: &str, i: usize) -> Option<(usize, usize)> {
    let b = src.as_bytes();
    if !src.get(i..)?.starts_with("<<<") {
        return None;
    }
    let mut j = i + 3;
    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    let quoted = j < b.len() && (b[j] == b'\'' || b[j] == b'"');
    if quoted {
        j += 1;
    }
    let label_start = j;
    while j < b.len() && is_ident(b[j]) {
        j += 1;
    }
    if j == label_start {
        return None;
    }
    let label = &src[label_start..j];
    if quoted {
        j += 1;
    }
    let body = src.get(j..)?.find('\n')? + j + 1;
    let mut line = body;
    while line <= src.len() {
        let rest = &src[line..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if let Some(after) = trimmed.strip_prefix(label)
            && !after.as_bytes().first().is_some_and(|c| is_ident(*c))
        {
            return Some((body, line));
        }
        match rest.find('\n') {
            Some(n) => line += n + 1,
            None => break,
        }
    }
    None
}

/// A PHP-kód szegmentálása a `from`-tól; a visszaadott érték a `?>` utáni (vagy a szöveg végi) hely.
fn lex_php(src: &str, from: usize, segs: &mut Vec<Seg>) -> usize {
    let b = src.as_bytes();
    let len = b.len();
    let mut i = from;
    let mut code = from;
    while i < len {
        match b[i] {
            q @ (b'\'' | b'"' | b'`') => {
                push(segs, code, i + 1, Kind::Code);
                let end = string_end(b, i + 1, q);
                push(segs, i + 1, end, Kind::Str);
                code = end;
                i = end + 1;
            }
            _ if is_line_comment(b, i) => {
                push(segs, code, i, Kind::Code);
                let end = line_comment_end(src, i);
                push(segs, i, end, Kind::Comment);
                code = end;
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                push(segs, code, i, Kind::Code);
                let end = src[i + 2..].find("*/").map_or(len, |p| i + 2 + p + 2);
                let kind = if src[i..].starts_with("/**") && end > i + 4 {
                    Kind::Doc
                } else {
                    Kind::Comment
                };
                push(segs, i, end, kind);
                code = end;
                i = end;
            }
            b'?' if b.get(i + 1) == Some(&b'>') => {
                push(segs, code, i + 2, Kind::Code);
                return i + 2;
            }
            b'<' if b.get(i + 1) == Some(&b'<') => {
                if let Some((body, close)) = heredoc(src, i) {
                    push(segs, code, body, Kind::Code);
                    push(segs, body, close, Kind::Str);
                    code = close;
                    i = close;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    push(segs, code, len, Kind::Code);
    len
}

/// `//` vagy `#` (de nem a `#[` attribútum) áll-e az `i`-n.
fn is_line_comment(b: &[u8], i: usize) -> bool {
    (b[i] == b'/' && b.get(i + 1) == Some(&b'/')) || (b[i] == b'#' && b.get(i + 1) != Some(&b'['))
}

fn line_comment_end(src: &str, from: usize) -> usize {
    let rest = &src[from..];
    let newline = rest.find('\n').map_or(src.len(), |p| from + p);
    let close = rest.find("?>").map_or(src.len(), |p| from + p);
    newline.min(close)
}

/// A szöveg azonos hosszú másolata: a `blank` szegmens-fajták bájtjai szóközök (a sortörések maradnak).
pub(crate) fn mask(src: &str, segs: &[Seg], blank: impl Fn(Kind) -> bool) -> String {
    let mut out = String::with_capacity(src.len());
    for seg in segs {
        let part = &src[seg.start..seg.end];
        if blank(seg.kind) {
            for ch in part.chars() {
                if ch == '\n' {
                    out.push('\n');
                } else {
                    for _ in 0..ch.len_utf8() {
                        out.push(' ');
                    }
                }
            }
        } else {
            out.push_str(part);
        }
    }
    out
}

/// Egy `use` import.
#[derive(Debug, Clone)]
pub(crate) struct Import {
    pub alias: String,
    pub fqcn: String,
    /// A teljes név szövegének tartománya az utasításban (az elején `\` nélkül).
    pub name_span: (usize, usize),
    pub group: bool,
    pub explicit_alias: bool,
}

/// Egy típus-deklaráció.
#[derive(Debug, Clone)]
pub(crate) struct Decl {
    pub kind: String,
    pub name: String,
    pub fqcn: String,
    pub name_span: (usize, usize),
    /// Az ős és az interfészek teljes nevei.
    pub parents: Vec<String>,
    /// Az első `extends` teljes neve.
    pub extends: Option<String>,
    /// A törzs `{` és a hozzá tartozó `}` helye.
    pub open: usize,
    pub close: usize,
}

/// Egy beolvasott PHP-fájl.
pub(crate) struct PhpFile {
    pub rel: String,
    pub text: String,
    pub segs: Vec<Seg>,
    /// Csak a kód (megjegyzés, HTML és sztring-tartalom szóközzel kitöltve).
    pub code: String,
    /// Kód és sztringek (a megjegyzés és a HTML szóközzel kitöltve).
    pub code_strings: String,
    pub ns: String,
    pub ns_span: Option<(usize, usize)>,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
    /// A `namespace` / `use` / `declare` utasítások tartománya (ezekben nem keresünk neveket).
    pub header_spans: Vec<(usize, usize)>,
    /// A `use` beszúrásának helye: a fejléc utolsó utasítását követő sor kezdete.
    pub insert_at: usize,
    line_starts: Vec<usize>,
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("érvényes regex")
}

static NAMESPACE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\bnamespace\s+([A-Za-z_][A-Za-z0-9_\\]*)\s*[;{]"));
static DECL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?:^|[\s;{}])(?:(?:final|abstract|readonly)\s+)*(class|interface|trait|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
});
static USE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\buse\s+(function\s+|const\s+)?(\\?[A-Za-z_][A-Za-z0-9_\\]*)(?:\s+as\s+([A-Za-z_][A-Za-z0-9_]*))?\s*;",
    )
});
static USE_GROUP: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\buse\s+(function\s+|const\s+)?(\\?[A-Za-z_][A-Za-z0-9_\\]*)\\\{([^}]*)\}\s*;")
});
static DECLARE: LazyLock<Regex> = LazyLock::new(|| re(r"\bdeclare\s*\([^)]*\)\s*;"));
static NAME: LazyLock<Regex> =
    LazyLock::new(|| re(r"\\?[A-Za-z_][A-Za-z0-9_]*(?:\\[A-Za-z_][A-Za-z0-9_]*)*"));

/// Egy név-token (osztály-név a kódban).
#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// A `text` név-tokenei a `base` eltolással; a tagnevek (`->x`, `::x`), a változók (`$x`) és a hívott függvények
/// (`x(`, kivéve `new x(`) kimaradnak.
pub(crate) fn name_tokens(text: &str, base: usize) -> Vec<Token> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for m in NAME.find_iter(text) {
        let start = m.start();
        if start > 0 && (is_ident(b[start - 1]) || b[start - 1] == b'$') {
            continue;
        }
        let before = text[..start].trim_end();
        if before.ends_with("->") || before.ends_with("::") || before.ends_with("?->") {
            continue;
        }
        let after = text[m.end()..].trim_start();
        if after.starts_with('(') && !before.ends_with("new") {
            continue;
        }
        out.push(Token {
            start: base + start,
            end: base + m.end(),
            text: m.as_str().to_owned(),
        });
    }
    out
}

impl PhpFile {
    pub(crate) fn parse(rel: &str, text: String) -> Self {
        let segs = lex(&text);
        let code = mask(&text, &segs, |k| k != Kind::Code);
        let code_strings = mask(&text, &segs, |k| {
            matches!(k, Kind::Html | Kind::Comment | Kind::Doc)
        });
        let mut line_starts = vec![0usize];
        line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        let mut file = Self {
            rel: rel.to_owned(),
            text,
            segs,
            code,
            code_strings,
            ns: String::new(),
            ns_span: None,
            imports: Vec::new(),
            decls: Vec::new(),
            header_spans: Vec::new(),
            insert_at: 0,
            line_starts,
        };
        file.parse_namespace();
        file.parse_imports();
        file.parse_decls();
        file.parse_insert_point();
        file
    }

    fn parse_namespace(&mut self) {
        if let Some(c) = NAMESPACE.captures(&self.code) {
            let whole = c.get(0).map(|m| (m.start(), m.end())).unwrap_or((0, 0));
            if let Some(name) = c.get(1) {
                self.ns = name.as_str().to_owned();
                self.ns_span = Some((name.start(), name.end()));
                self.header_spans.push(whole);
            }
        }
    }

    fn parse_decls(&mut self) {
        let code = self.code.clone();
        for c in DECL.captures_iter(&code) {
            let (Some(kind), Some(name)) = (c.get(1), c.get(2)) else {
                continue;
            };
            let before = code[..kind.start()].trim_end();
            if before.ends_with("new") || before.ends_with("::") || before.ends_with("->") {
                continue;
            }
            let Some(open) = code[name.end()..].find('{').map(|p| name.end() + p) else {
                continue;
            };
            let header = &code[name.end()..open];
            let Some(close) = matching_brace(&code, open) else {
                continue;
            };
            let (extends, parents) = self.parse_header(header);
            let fqcn = if self.ns.is_empty() {
                name.as_str().to_owned()
            } else {
                format!("{}\\{}", self.ns, name.as_str())
            };
            self.decls.push(Decl {
                kind: kind.as_str().to_owned(),
                name: name.as_str().to_owned(),
                fqcn,
                name_span: (name.start(), name.end()),
                parents,
                extends,
                open,
                close,
            });
        }
    }

    /// `extends` / `implements` a fejlécből: `(az első ős, minden ős és interfész)` teljes névvel. Az importok
    /// ekkorra már betöltöttek.
    fn parse_header(&self, header: &str) -> (Option<String>, Vec<String>) {
        let header = header.split(':').next().unwrap_or(header);
        let (left, right) = match header.split_once("implements") {
            Some((l, r)) => (l, r),
            None => (header, ""),
        };
        let extends_part = left.split_once("extends").map_or("", |(_, r)| r);
        let mut parents = Vec::new();
        let mut first = None;
        for token in NAME.find_iter(extends_part) {
            let full = self.resolve(token.as_str());
            if first.is_none() {
                first = Some(full.clone());
            }
            parents.push(full);
        }
        for token in NAME.find_iter(right) {
            parents.push(self.resolve(token.as_str()));
        }
        (first, parents)
    }

    fn parse_imports(&mut self) {
        // csak a legfelső szintű `use`-ok: az első deklaráció előttiek (a trait-`use` és a closure-`use` nem)
        let limit = DECL
            .captures(&self.code)
            .and_then(|c| c.get(1))
            .map_or(self.code.len(), |m| m.start());
        let code = self.code[..limit].to_owned();
        let strings = self.code_strings[..limit].to_owned();
        let mut imports = Vec::new();
        let mut spans = Vec::new();
        for c in USE_GROUP.captures_iter(&code) {
            if c.get(1).is_some() {
                continue;
            }
            let (Some(whole), Some(prefix), Some(inner)) = (c.get(0), c.get(2), c.get(3)) else {
                continue;
            };
            spans.push((whole.start(), whole.end()));
            let prefix = prefix.as_str().trim_start_matches('\\');
            let mut offset = inner.start();
            for item in inner.as_str().split(',') {
                let lead = item.len() - item.trim_start().len();
                let trimmed = item.trim();
                let start = offset + lead;
                offset += item.len() + 1;
                if trimmed.is_empty()
                    || trimmed.starts_with("function ")
                    || trimmed.starts_with("const ")
                {
                    continue;
                }
                let (name, alias) = match trimmed.split_once(" as ") {
                    Some((n, a)) => (n.trim(), Some(a.trim())),
                    None => (trimmed, None),
                };
                let last = name.rsplit('\\').next().unwrap_or(name);
                imports.push(Import {
                    alias: alias.unwrap_or(last).to_owned(),
                    fqcn: format!("{prefix}\\{name}"),
                    name_span: (start, start + name.len()),
                    group: true,
                    explicit_alias: alias.is_some(),
                });
            }
        }
        for c in USE.captures_iter(&strings) {
            if c.get(1).is_some() {
                continue;
            }
            let (Some(whole), Some(name)) = (c.get(0), c.get(2)) else {
                continue;
            };
            if spans
                .iter()
                .any(|(s, e)| whole.start() >= *s && whole.end() <= *e)
            {
                continue;
            }
            spans.push((whole.start(), whole.end()));
            let full = name.as_str().trim_start_matches('\\');
            let skipped = name.as_str().len() - full.len();
            let last = full.rsplit('\\').next().unwrap_or(full);
            imports.push(Import {
                alias: c.get(3).map_or(last, |a| a.as_str()).to_owned(),
                fqcn: full.to_owned(),
                name_span: (name.start() + skipped, name.end()),
                group: false,
                explicit_alias: c.get(3).is_some(),
            });
        }
        self.imports = imports;
        self.header_spans.extend(spans);
    }

    fn parse_insert_point(&mut self) {
        let mut end = 0usize;
        for (_, e) in &self.header_spans {
            end = end.max(*e);
        }
        for m in DECLARE.find_iter(&self.code) {
            self.header_spans.push((m.start(), m.end()));
            end = end.max(m.end());
        }
        if end == 0 {
            // `<?php` sora után
            end = self.text.find('\n').unwrap_or(self.text.len());
        }
        self.insert_at = self.text[end..]
            .find('\n')
            .map_or(self.text.len(), |p| end + p + 1);
    }

    /// A név teljes alakja (a vezető `\` nélkül): `\X` → X; `A\B` / `A` az importok, különben a névtér szerint.
    pub(crate) fn resolve(&self, token: &str) -> String {
        if let Some(rest) = token.strip_prefix('\\') {
            return rest.to_owned();
        }
        let (first, rest) = match token.split_once('\\') {
            Some((f, r)) => (f, Some(r)),
            None => (token, None),
        };
        if first.eq_ignore_ascii_case("namespace")
            && let Some(rest) = rest
        {
            return self.in_namespace(rest);
        }
        if let Some(import) = self
            .imports
            .iter()
            .find(|i| i.alias.eq_ignore_ascii_case(first))
        {
            return match rest {
                Some(rest) => format!("{}\\{rest}", import.fqcn),
                None => import.fqcn.clone(),
            };
        }
        self.in_namespace(token)
    }

    fn in_namespace(&self, name: &str) -> String {
        if self.ns.is_empty() {
            name.to_owned()
        } else {
            format!("{}\\{name}", self.ns)
        }
    }

    /// A bájt-hely 1-alapú sora.
    pub(crate) fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&s| s <= offset)
    }

    /// A sor (1-alapú) kezdő bájt-helye.
    pub(crate) fn line_start(&self, line: usize) -> usize {
        self.line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(self.text.len())
    }

    /// A sor szövege, levágva (a `⚠` sorokhoz).
    pub(crate) fn line_text(&self, offset: usize) -> String {
        let start = self.line_starts[self.line_of(offset) - 1];
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |p| start + p);
        self.text[start..end].trim().chars().take(80).collect()
    }

    /// A bájt-helyet tartalmazó legbelső deklaráció.
    pub(crate) fn enclosing(&self, offset: usize) -> Option<&Decl> {
        self.decls
            .iter()
            .filter(|d| d.open <= offset && offset <= d.close)
            .min_by_key(|d| d.close - d.open)
    }

    /// Az új sor-elválasztó (a fájl szerint).
    pub(crate) fn eol(&self) -> &'static str {
        if self.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }

    pub(crate) fn in_header(&self, offset: usize) -> bool {
        self.header_spans
            .iter()
            .any(|(s, e)| offset >= *s && offset < *e)
    }
}

/// A `{` párja (a megtisztított kódban).
pub(crate) fn matching_brace(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in code.bytes().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
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

/// A `(` párja (a megtisztított kódban, ahol a sztringek tartalma üres).
pub(crate) fn matching_paren(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in code.bytes().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
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

/// A projekt összes (nem Blade) PHP-fájlja beolvasva és elemezve; a nem olvasható fájl kimarad.
pub(crate) fn load_all(fs: &dyn FileSystem, root: &Path) -> Vec<PhpFile> {
    let mut files = Vec::new();
    for path in walk_php(fs, root.to_path_buf()) {
        let Some(rel) = relative(root, &path) else {
            continue;
        };
        if rel.ends_with(".blade.php") {
            continue;
        }
        if let Ok(text) = fs.read(&path) {
            files.push(PhpFile::parse(&rel, text));
        }
    }
    files
}

/// Az osztály-hivatkozás feloldása: pontos teljes név (kis-nagybetű nélkül), vagy egyedi rövid név.
pub(crate) fn find_decl<'a>(
    files: &'a [PhpFile],
    name: &str,
) -> Result<(&'a PhpFile, &'a Decl), String> {
    let wanted = name.trim_start_matches('\\');
    let mut found: Vec<(&PhpFile, &Decl)> = Vec::new();
    for file in files {
        for decl in &file.decls {
            let hit = if wanted.contains('\\') {
                decl.fqcn.eq_ignore_ascii_case(wanted)
            } else {
                decl.name.eq_ignore_ascii_case(wanted)
            };
            if hit {
                found.push((file, decl));
            }
        }
    }
    match found.as_slice() {
        [] => Err(crate::tf!(
            "class {} not found in the project",
            "nincs ilyen osztály a projektben: {}",
            wanted
        )),
        [one] => Ok(*one),
        many => {
            let names: Vec<String> = many.iter().take(5).map(|(_, d)| d.fqcn.clone()).collect();
            Err(crate::tf!(
                "{} is ambiguous, give the full name: {}",
                "{} nem egyértelmű, add meg a teljes nevet: {}",
                wanted,
                names.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_separates_comments_strings_and_html() {
        let src = "<html><?php\n// c 'x'\n$a = 'it''s'; /** d */ $b = \"q\\\"z\"; ?>tail";
        let segs = lex(src);
        let code = mask(src, &segs, |k| k != Kind::Code);
        assert_eq!(code.len(), src.len());
        assert!(!code.contains("tail"));
        assert!(!code.contains("// c"));
        assert!(code.contains("$a = '"));
        let kept = mask(src, &segs, |k| {
            matches!(k, Kind::Html | Kind::Comment | Kind::Doc)
        });
        assert!(kept.contains("'it'"));
    }

    #[test]
    fn parses_namespace_imports_and_declarations() {
        let src = "<?php\nnamespace App\\Models;\n\nuse App\\Base\\Entity as E;\nuse Foo\\{A, B as C};\n\nclass Fee extends E implements \\Countable\n{\n    use Foo;\n}\n";
        let file = PhpFile::parse("app/Models/Fee.php", src.to_owned());
        assert_eq!(file.ns, "App\\Models");
        assert_eq!(file.decls.len(), 1);
        assert_eq!(file.decls[0].fqcn, "App\\Models\\Fee");
        assert_eq!(file.decls[0].extends.as_deref(), Some("App\\Base\\Entity"));
        assert!(file.decls[0].parents.contains(&"Countable".to_owned()));
        assert_eq!(file.resolve("C"), "Foo\\B");
        assert_eq!(file.resolve("Other"), "App\\Models\\Other");
        assert_eq!(file.resolve("\\X\\Y"), "X\\Y");
    }
}
