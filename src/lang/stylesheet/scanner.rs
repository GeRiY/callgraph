//! A stíluslap-szkenner: egy bájt-szegmens alapú motor a CSS-hez és az SCSS-hez. A Raffia az érvényes inputot
//! validálja (a frontendekben), ez a szkenner pedig a stabil, forráshelyes outline-t gyűjti a közös modellbe.
//! A nyelvi eltéréseket a `Dialect` hordozza: az SCSS `//` megjegyzést, `#{}` interpolációt, `$változót`,
//! `@mixin` / `@function` / `@include` / `@extend` / vezérlő direktívákat tud a CSS-en felül.

use std::borrow::Cow;
use std::collections::HashSet;

use super::helpers::{
    clean_header, is_ident, is_ident_start, is_line_comment, strip_comments, strip_line_comments,
};
use super::rules::{header_offset, is_statement_at_rule, rule_kind};
use super::scss_parts as parts;
use crate::core::LineIndex;
use crate::lang::Analysis;
use crate::model::{
    Call, Comment, Constant, Container, ControlFlow, FileGraphBuilder, Method, StringLit, Write,
};

/// A stíluslap nyelve: a szkenner vezérlését és a kinyert rekordok körét ez dönti el.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    Css,
    Scss,
}

struct OpenRule {
    method: Option<usize>,
    /// A szabály outline-neve (a benne lévő hívások `caller`-e); üres fejlécnél / vezérlő direktívánál `None`.
    name: Option<String>,
}

/// A legbelső nyitott, névvel rendelkező szabály neve.
fn innermost(stack: &[OpenRule]) -> Option<String> {
    stack.iter().rev().find_map(|open| open.name.clone())
}

/// Stíluslap forrásból gráfrekordokat készítő, állapotot hordozó leképező.
pub(crate) struct StylesheetScanner<'a> {
    source: &'a str,
    container: String,
    builder: FileGraphBuilder,
    dialect: Dialect,
    lines: LineIndex,
    /// SCSS: a már deklarált `$változók` (hatókör = a legbelső névvel rendelkező szabály, név).
    declared: HashSet<(Option<String>, String)>,
}

impl<'a> StylesheetScanner<'a> {
    pub fn new(dialect: Dialect, path: &'a str, source: &'a str) -> Self {
        let container = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("stylesheet")
            .to_string();
        Self {
            source,
            container,
            builder: FileGraphBuilder::new(path),
            dialect,
            lines: LineIndex::new(source),
            declared: HashSet::new(),
        }
    }

    fn scss(&self) -> bool {
        self.dialect == Dialect::Scss
    }

    fn line(&self, offset: usize) -> u32 {
        self.lines.line_of(offset)
    }

    pub fn scan(mut self) -> Analysis {
        self.builder.imports_as_object();
        let source = self.source;
        let end_line = self.line(source.len().saturating_sub(1));
        self.builder.container(Container {
            kind: "stylesheet".into(),
            name: self.container.clone(),
            line: 1,
            end_line,
            is_abstract: None,
            is_final: None,
            extends: None,
            implements: None,
            traits: None,
            doc: None,
        });
        let mut stack: Vec<OpenRule> = Vec::new();
        let mut segment = 0usize;
        let bytes = source.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                let start = i;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                let end = (i + 2).min(bytes.len());
                self.comment(start, end);
                self.strings_in(start, end);
                if !self.scss() {
                    self.css_calls(&source[start..end], start, None);
                    self.import_statement(&source[start..end]);
                }
                i = end;
                continue;
            }
            if self.scss() && is_line_comment(bytes, i) {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                self.line_comment(start, i);
                continue;
            }
            if self.scss() && bytes[i] == b'#' && bytes.get(i + 1) == Some(&b'{') {
                // `#{…}` interpoláció: nem blokk, a szegmens része marad (`&--#{$name}`)
                let mut depth = 0usize;
                let mut j = i + 1;
                while j < bytes.len() {
                    match bytes[j] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                i = (j + 1).min(bytes.len());
                continue;
            }
            if bytes[i] == b'"' || bytes[i] == b'\'' {
                let quote = bytes[i];
                let start = i + 1;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
                self.builder.string(StringLit {
                    container: Some(self.container.clone()),
                    caller: None,
                    line: self.line(start),
                    value: source[start..i.min(bytes.len())].to_string(),
                    end_line: None,
                });
                i = (i + 1).min(bytes.len());
                continue;
            }
            if bytes[i] == b'{' {
                let open = self.open_block(&source[segment..i], segment, &stack);
                stack.push(open);
                segment = i + 1;
            } else if bytes[i] == b'}' {
                if self.scss() {
                    // a záró `}` előtti, `;` nélküli utolsó utasítás (`.a { @include x }`)
                    self.scss_statement(segment, i, &stack);
                }
                if let Some(open) = stack.pop()
                    && let Some(index) = open.method
                {
                    let end = self.line(i);
                    self.builder.set_method_end(index, end);
                }
                segment = i + 1;
            } else if bytes[i] == b';' {
                if self.scss() {
                    self.scss_statement(segment, i, &stack);
                } else {
                    self.statement_at_rule(segment, i);
                    let caller = innermost(&stack);
                    self.declaration(&source[segment..i], segment, caller);
                    self.import_statement(&source[segment..i]);
                }
                segment = i + 1;
            }
            i += 1;
        }
        Analysis {
            graph: self.builder.build(),
            warnings: Vec::new(),
        }
    }

    /// A szegmens a megjegyzés-előkészítés után: SCSS-ben a `//` sorvégi megjegyzések szóközökre cserélve.
    fn prepare<'t>(&self, raw: &'t str) -> Cow<'t, str> {
        if self.scss() {
            Cow::Owned(strip_line_comments(raw))
        } else {
            Cow::Borrowed(raw)
        }
    }

    /// A `{` előtti fejléc: a szabály és a blokkos at-szabály (`@media`, `@theme`, `@mixin` …) az outline része; a
    /// sora a fejléc első karaktere (az előző szabály vége és a megjegyzések nem számítanak).
    fn open_block(&mut self, raw: &str, segment: usize, stack: &[OpenRule]) -> OpenRule {
        let text = self.prepare(raw);
        let header = clean_header(&text);
        let start = segment + header_offset(&text);
        if self.scss() {
            return self.scss_open(&text, header, segment, start, stack);
        }
        let method = (!header.is_empty())
            .then(|| self.method(&header, rule_kind(&header), Vec::new(), start, start));
        if header.starts_with("@media") {
            self.flow("media", &text, segment);
        }
        OpenRule {
            method,
            name: (!header.is_empty()).then_some(header),
        }
    }

    /// Egy szabály / at-szabály az outline-ban; `start`: a fejléc eleje, `end`: az utasítás `;`-je (a blokkos szabály
    /// végét a záró `}` állítja be).
    fn method(
        &mut self,
        name: &str,
        kind: &str,
        params: Vec<String>,
        start: usize,
        end: usize,
    ) -> usize {
        let index = self.builder.method_count();
        let (line, end_line) = (self.line(start), self.line(end));
        self.builder.method(Method {
            container: Some(self.container.clone()),
            name: name.to_string(),
            line,
            end_line: Some(end_line),
            visibility: None,
            is_static: false,
            is_abstract: false,
            params,
            return_type: None,
            doc_types: None,
            doc: None,
            body: None,
            kind: Some(kind.into()),
        });
        index
    }

    /// Vezérlési szerkezet (`@media`, SCSS: `@if` / `@each` …) a fejléc szövegéből: a feltétel a kulcsszó utáni rész.
    fn flow(&mut self, kind: &str, text: &str, segment: usize) {
        let key = format!("@{kind}");
        let Some(pos) = text.find(&key) else { return };
        let cond = text[pos + key.len()..]
            .trim_start_matches(char::is_whitespace)
            .to_string();
        let line = self.line(segment + pos);
        self.builder.control_flow(ControlFlow {
            container: Some(self.container.clone()),
            caller: None,
            kind: kind.into(),
            line,
            cond: Some(cond),
            branches: Vec::new(),
        });
    }

    // ---- CSS ----

    /// A `;`-re végződő at-szabály (`@import`, `@tailwind`, `@plugin`, `@source` …) az outline-ba, a soraival; a
    /// szabályon belüli (`@apply`) kimarad.
    fn statement_at_rule(&mut self, segment: usize, end: usize) {
        let raw = &self.source[segment..end];
        let header = clean_header(raw);
        if is_statement_at_rule(&header) {
            let start = segment + header_offset(raw);
            self.method(&header, "at-rule", Vec::new(), start, end);
        }
    }

    fn declaration(&mut self, raw: &str, start: usize, caller: Option<String>) {
        let cleaned = strip_comments(raw);
        let Some((name, value)) = cleaned.split_once(':') else {
            return;
        };
        let name = name.trim();
        if name.starts_with("--") {
            let name_offset = raw.find(name).unwrap_or(0);
            let line = self.line(start + name_offset);
            self.builder.constant(Constant {
                js: false,
                container: Some(self.container.clone()),
                name: name.into(),
                line,
                value: Some(
                    value
                        .trim()
                        .split('\n')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .into(),
                ),
                kind: "custom-property".into(),
                visibility: None,
                caller: None,
                doc: None,
            });
        }
        self.css_calls(&cleaned, start, caller);
    }

    fn css_calls(&mut self, raw: &str, start: usize, caller: Option<String>) {
        let b = raw.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if is_ident_start(b[i]) {
                let mut j = i + 1;
                while j < b.len() && is_ident(b[j]) {
                    j += 1;
                }
                if b.get(j) == Some(&b'(') {
                    let function = &raw[i..j];
                    // a `var(--x, fallback)` olvasás célja a custom property neve (a függvénynév nem információ)
                    let property = if function == "var" {
                        raw[j + 1..]
                            .split([',', ')'])
                            .next()
                            .map(str::trim)
                            .filter(|name| name.starts_with("--"))
                    } else {
                        None
                    };
                    // `var(--x)` alakban: a `--grep var` és a property neve is megtalálja
                    let target =
                        property.map_or_else(|| function.to_owned(), |p| format!("var({p})"));
                    let category = if function == "var" { "var" } else { "function" };
                    let line = self.line(start + i);
                    self.builder.call(
                        Call::new(function, target, category)
                            .at(line)
                            .within(Some(self.container.clone()), caller.clone()),
                    );
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
    }

    fn import_statement(&mut self, raw: &str) {
        let cleaned = strip_comments(raw);
        let s = cleaned.trim();
        if !s.starts_with("@import") {
            if let Some(pos) = raw.find("@import")
                && let Some(alias) = raw[pos + 7..].split_whitespace().next()
            {
                self.builder.import(alias.into(), alias.into());
            }
            return;
        }
        let value = s[7..]
            .trim()
            .trim_start_matches("url(")
            .trim_end_matches(')');
        let value = value.trim().trim_matches(['"', '\'']);
        if !value.is_empty() {
            self.builder.import(value.into(), value.into());
        }
    }

    // ---- SCSS ----

    /// SCSS `{` fejléc: mixin / függvény (paraméterekkel), vezérlő direktíva (`@if` … nem metódus, csak
    /// vezérlési szerkezet), `@include` blokk (nem metódus), `@media` / más at-szabály és szelektor (metódus).
    fn scss_open(
        &mut self,
        text: &str,
        header: String,
        segment: usize,
        start: usize,
        stack: &[OpenRule],
    ) -> OpenRule {
        let unnamed = OpenRule {
            method: None,
            name: None,
        };
        if header.is_empty() {
            return unnamed;
        }
        let caller = innermost(stack);
        let cleaned = strip_comments(text);
        if let Some((kind, name, params)) = parts::declaration(&header) {
            let method = self.method(&name, kind, params, start, start);
            return OpenRule {
                method: Some(method),
                name: Some(name),
            };
        }
        if !header.starts_with('@') {
            // szelektor; a beágyazott tulajdonság-blokk (`font: {`) nem szabály
            self.scss_interpolations(&cleaned, segment, caller.as_deref());
            if header.ends_with(':') {
                return unnamed;
            }
            let method = self.method(&header, rule_kind(&header), Vec::new(), start, start);
            return OpenRule {
                method: Some(method),
                name: Some(header),
            };
        }
        let keyword: String = header[1..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        self.scss_calls(&cleaned, segment, caller.as_deref());
        match keyword.as_str() {
            "if" | "each" | "for" | "while" => {
                self.flow(&keyword, text, segment);
                unnamed
            }
            "else" | "include" => unnamed,
            _ => {
                if matches!(keyword.as_str(), "media" | "supports") {
                    self.flow(&keyword, text, segment);
                }
                let method = self.method(&header, rule_kind(&header), Vec::new(), start, start);
                OpenRule {
                    method: Some(method),
                    name: Some(header),
                }
            }
        }
    }

    /// SCSS utasítás (`;`-ig vagy a záró `}`-ig): import, `$változó`, hívások.
    fn scss_statement(&mut self, segment: usize, end: usize, stack: &[OpenRule]) {
        let source = self.source;
        let text = self.prepare(&source[segment..end]);
        let cleaned = strip_comments(&text);
        let s = cleaned.trim();
        if s.is_empty() {
            return;
        }
        let lead = cleaned.len() - cleaned.trim_start().len();
        let caller = innermost(stack);
        if let Some((alias, spec)) = parts::import(s) {
            self.builder.import(alias, spec);
        }
        if let Some((name, value)) = parts::variable(s) {
            self.variable(name, value, segment + lead, caller.as_deref());
        }
        self.scss_calls(&cleaned, segment, caller.as_deref());
    }

    /// `$név: érték`: az első értékadás konstans; ugyanabban a hatókörben az újraértékadás (`$n: $n + 1`) írás.
    fn variable(&mut self, name: String, value: String, offset: usize, caller: Option<&str>) {
        let line = self.line(offset);
        let scope = caller.map(str::to_owned);
        if !self.declared.insert((scope, name.clone())) {
            self.builder.write(Write {
                container: Some(self.container.clone()),
                caller: caller.unwrap_or("__top__").to_owned(),
                line,
                kind: "assign".into(),
                on: name,
                property: String::new(),
            });
            return;
        }
        self.builder.constant(Constant {
            js: false,
            container: Some(self.container.clone()),
            caller: None,
            kind: "scss-variable".into(),
            name,
            line,
            value: Some(value),
            visibility: None,
            doc: None,
        });
    }

    /// `@include` / `@extend` / függvényhívások és interpolációk a (megjegyzés nélküli) szövegben; `seg`: a szöveg
    /// bájt-helye a forrásban.
    fn scss_calls(&mut self, s: &str, seg: usize, caller: Option<&str>) {
        let container = self.container.clone();
        if let Some(a) = s.find("@include ")
            && let Some((target, args)) = parts::include_parts(&s[a + 9..])
        {
            let line = self.line(seg + a);
            self.builder.call(parts::call(
                &container,
                caller,
                line,
                "include",
                target,
                "include",
                args.clone(),
            ));
            self.builder.call(parts::call(
                &container,
                caller,
                line,
                "func_call",
                target.rsplit('.').next().unwrap_or(target),
                "function",
                args,
            ));
        }
        if let Some(a) = s.find("@extend ") {
            let target = s[a + 8..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches(';');
            if !target.is_empty() {
                let line = self.line(seg + a);
                self.builder.call(parts::call(
                    &container,
                    caller,
                    line,
                    "extend",
                    target,
                    "extend",
                    Vec::new(),
                ));
            }
        }
        let included = s
            .find("@include ")
            .and_then(|a| parts::include_parts(&s[a + 9..]))
            .map(|(target, _)| target.rsplit('.').next().unwrap_or(target));
        let head = s.trim_start();
        for (offset, name, args) in parts::functions_at(s) {
            if included == Some(name) || head.starts_with(&format!("@{name}")) {
                continue;
            }
            let line = self.line(seg + offset);
            self.builder.call(parts::call(
                &container,
                caller,
                line,
                "func_call",
                name,
                "function",
                args,
            ));
        }
        self.scss_interpolations(s, seg, caller);
    }

    /// A `#{…}` interpolációk (a kifejezés a cél).
    fn scss_interpolations(&mut self, s: &str, seg: usize, caller: Option<&str>) {
        let mut from = 0;
        while let Some(a) = s[from..].find("#{") {
            let a = from + a + 2;
            let Some(b) = s[a..].find('}') else { break };
            let line = self.line(seg + a);
            self.builder.call(
                Call::new("interpolation", &s[a..a + b], "interpolation")
                    .at(line)
                    .within(Some(self.container.clone()), caller.map(str::to_string)),
            );
            from = a + b + 1;
        }
    }

    // ---- közös ----

    fn comment(&mut self, start: usize, end: usize) {
        let text = self.source[start + 2..end.saturating_sub(2)]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let line = self.line(start);
        self.builder.comment(Comment {
            container: Some(self.container.clone()),
            caller: None,
            line,
            tag: None,
            text,
        });
    }

    /// SCSS `// …` sorvégi megjegyzés (a `//` után a sor végéig).
    fn line_comment(&mut self, start: usize, end: usize) {
        let line = self.line(start);
        self.builder.comment(Comment {
            container: Some(self.container.clone()),
            caller: None,
            line,
            tag: None,
            text: self.source[start + 2..end].trim().into(),
        });
    }

    fn strings_in(&mut self, start: usize, end: usize) {
        let text = &self.source[start..end.min(self.source.len())];
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'"' && bytes[i] != b'\'' {
                i += 1;
                continue;
            }
            let quote = bytes[i];
            let begin = i + 1;
            i = begin;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let line = self.line(start + begin);
            self.builder.string(StringLit {
                container: Some(self.container.clone()),
                caller: None,
                line,
                value: text[begin..i.min(bytes.len())].to_string(),
                end_line: None,
            });
            i = (i + 1).min(bytes.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> String {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/samples/scss");
        std::fs::read_to_string(dir.join(name)).expect("SCSS golden sample")
    }

    fn scss(name: &str, source: &str) -> serde_json::Value {
        let scanner = StylesheetScanner::new(Dialect::Scss, name, source);
        serde_json::to_value(scanner.scan().graph).unwrap()
    }

    fn constants(name: &str, source: &str) -> Vec<(String, String)> {
        scss(name, source)["constants"]
            .as_array()
            .map(|list| {
                list.iter()
                    .map(|c| {
                        let text = |k: &str| c[k].as_str().unwrap_or_default().to_owned();
                        (text("name"), text("value"))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn method_names(graph: &serde_json::Value) -> Vec<String> {
        graph["methods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["name"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    #[test]
    fn mixin_and_function_parameters_are_not_constants() {
        // regresszió: a `@mixin button($color: t.$brand, $radius: 4px) {` sora `$color` konstans lett
        assert!(constants("_mixins.scss", &sample("_mixins.scss")).is_empty());
        assert!(constants("_helpers.scss", &sample("_helpers.scss")).is_empty());
        assert!(constants("theme.scss", &sample("theme.scss")).is_empty());
        let graph = scss("_mixins.scss", &sample("_mixins.scss"));
        let button = graph["methods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["name"] == "button")
            .expect("button mixin");
        assert_eq!(
            button["params"],
            serde_json::json!(["$color: t.$brand", "$radius: 4px"])
        );

        let multi = "@mixin card(\n  $pad: 4px,\n  $bg: url(http://x/y.png)\n) {\n  padding: $pad;\n}\n$after: 1px;\n";
        assert_eq!(
            constants("multi.scss", multi),
            vec![("$after".to_owned(), "1px".to_owned())]
        );
        let card = &scss("multi.scss", multi)["methods"][0];
        assert_eq!(card["name"], "card");
        assert_eq!(
            card["params"],
            serde_json::json!(["$pad: 4px", "$bg: url(http://x/y.png)"])
        );
    }

    #[test]
    fn variable_declarations_are_still_constants() {
        assert_eq!(
            constants("_tokens.scss", &sample("_tokens.scss")),
            vec![
                ("$brand".to_owned(), "#2f6fed".to_owned()),
                (
                    "$spacing".to_owned(),
                    "(sm: 4px, md: 8px, lg: 16px)".to_owned()
                ),
                (
                    "$breakpoints".to_owned(),
                    "(tablet: 768px, desktop: 1200px)".to_owned()
                ),
            ]
        );
        let src = ".a {\n  $local: 2px; // note (x\n  margin: $local;\n}\n$bg: url(http://h/a.png);\n$top: 3;\n";
        let names: Vec<String> = constants("x.scss", src).into_iter().map(|c| c.0).collect();
        assert_eq!(names, ["$local", "$bg", "$top"]);
    }

    /// Regresszió: ékezetes szöveg a függvényhívás előtt (`// Fejléc háttér`, `content: 'Kérés'`) nem pánikol.
    #[test]
    fn accented_text_does_not_panic() {
        let source = "// Fejléc háttér árnyéka\n.fejléc {\n  box-shadow: 0 1px rgba(0, 0, 0, .2);\n  &:hover { color: lighten($kék, 5%); }\n}\n";
        let _ = scss("_fejlec.scss", source);
    }

    #[test]
    fn interpolated_selectors_keep_their_whole_name() {
        let graph = scss(
            "layout.scss",
            ".grid {\n  &--#{$name} { color: red; }\n  .span-#{$i} { width: 1px; }\n}\n",
        );
        assert_eq!(
            method_names(&graph),
            [".grid", "&--#{$name}", ".span-#{$i}"]
        );
    }

    #[test]
    fn reassigned_variables_are_writes_not_constants() {
        let graph = scss("v.scss", "$n: 1;\n$n: $n + 1;\n.a { $n: 5; }\n");
        let names: Vec<_> = constants("v.scss", "$n: 1;\n$n: $n + 1;\n.a { $n: 5; }\n")
            .into_iter()
            .map(|c| c.0)
            .collect();
        // a második `$n` ugyanabban a hatókörben írás; a `.a`-beli új (helyi) deklaráció
        assert_eq!(names, ["$n", "$n"]);
        let writes = graph["writes"].as_array().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            (writes[0]["on"].as_str(), writes[0]["line"].as_u64()),
            (Some("$n"), Some(2))
        );
    }

    #[test]
    fn media_and_control_directives_follow_the_css_rules() {
        let graph = scss(
            "m.scss",
            "@media (min-width: 1px) {\n  .a { @if $x == 1 { b: c; } @else { d: e; } }\n}\n",
        );
        assert_eq!(method_names(&graph), ["@media (min-width: 1px)", ".a"]);
        let kinds: Vec<_> = graph["controlFlow"]
            .as_array()
            .map(|list| {
                list.iter()
                    .map(|f| f["type"].as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(kinds, ["media", "if"]);
    }

    #[test]
    fn all_golden_samples_finish() {
        use std::sync::mpsc;
        use std::time::Duration;
        for name in [
            "_helpers.scss",
            "_mixins.scss",
            "_placeholders.scss",
            "_tokens.scss",
            "forms.scss",
            "index.scss",
            "layout.scss",
            "theme.scss",
        ] {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .map(|d| d.join("golden/samples/scss"))
                .find(|d| d.is_dir())
                .expect("golden/samples/scss");
            let source = std::fs::read_to_string(dir.join(name)).expect("SCSS golden sample");
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let scanner = StylesheetScanner::new(Dialect::Scss, name, &source);
                let _ = sender.send(scanner.scan());
            });
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or_else(|_| panic!("scanner stalled on {name}"));
        }
    }
}
