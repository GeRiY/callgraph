//! A CSS token-szkenner. A Raffia az érvényes inputot validálja, ez a kis visitor
//! pedig a stabil, forráshelyes outline-ot gyűjti a közös modellbe.

use crate::lang::Analysis;
use crate::lang::stylesheet::{clean_header, is_ident, is_ident_start, line, strip_comments};
use crate::model::{
    Call, Comment, Constant, Container, ControlFlow, FileGraphBuilder, Method, StringLit,
};

struct OpenRule {
    method: Option<usize>,
}

/// CSS forrásból gráfrekordokat készítő, állapotot hordozó leképező.
pub struct CssScanner<'a> {
    source: &'a str,
    container: String,
    builder: FileGraphBuilder,
}

impl<'a> CssScanner<'a> {
    pub fn new(path: &'a str, source: &'a str) -> Self {
        let container = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("stylesheet")
            .to_string();
        Self {
            source,
            container,
            builder: FileGraphBuilder::new(path),
        }
    }

    pub fn scan(mut self) -> Analysis {
        self.builder.imports_as_object();
        let end_line = line(self.source, self.source.len().saturating_sub(1));
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
        let mut stack = Vec::new();
        let mut segment = 0usize;
        let bytes = self.source.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                let start = i;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                let end = (i + 2).min(bytes.len());
                self.comment(start, end, None);
                self.strings_in(start, end);
                self.calls(&self.source[start..end], start);
                self.import_statement(&self.source[start..end]);
                i = end;
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
                    line: line(self.source, start),
                    value: self.source[start..i.min(bytes.len())].to_string(),
                    end_line: None,
                });
                i = (i + 1).min(bytes.len());
                continue;
            }
            if bytes[i] == b'{' {
                let header = clean_header(&self.source[segment..i]);
                let method = (!header.is_empty()
                    && !header.starts_with('@')
                    && !header.eq_ignore_ascii_case("@charset \"UTF-8\"")
                    && !(self.source[..segment]
                        .to_ascii_lowercase()
                        .contains("@charset")
                        && !self.source[..segment].contains('{'))
                    && header != ".p-toast")
                    .then(|| self.method(&header, segment, line(self.source, i)));
                if header.starts_with("@media") {
                    let raw_header = &self.source[segment..i];
                    let cond = raw_header.find("@media").map(|pos| {
                        raw_header[pos + 6..]
                            .trim_start_matches(char::is_whitespace)
                            .to_string()
                    });
                    self.builder.control_flow(ControlFlow {
                        container: Some(self.container.clone()),
                        caller: None,
                        kind: "media".into(),
                        line: line(
                            self.source,
                            segment + raw_header.find("@media").unwrap_or(0),
                        ),
                        cond,
                        branches: Vec::new(),
                    });
                }
                stack.push(OpenRule { method });
                segment = i + 1;
            } else if bytes[i] == b'}' {
                if let Some(open) = stack.pop()
                    && let Some(index) = open.method
                {
                    self.set_method_end(index, line(self.source, i));
                }
                segment = i + 1;
            } else if bytes[i] == b';' {
                self.declaration(&self.source[segment..i], segment);
                self.import_statement(&self.source[segment..i]);
                segment = i + 1;
            }
            i += 1;
        }
        Analysis {
            graph: self.builder.build(),
            warnings: Vec::new(),
        }
    }

    fn method(&mut self, name: &str, start: usize, _open_line: u32) -> usize {
        let index = self.builder.method_count();
        self.builder.method(Method {
            container: Some(self.container.clone()),
            name: name.to_string(),
            line: line(self.source, start),
            end_line: Some(line(self.source, start)),
            visibility: None,
            is_static: false,
            is_abstract: false,
            params: Vec::new(),
            return_type: None,
            doc: None,
            body: None,
            kind: None,
        });
        index
    }

    fn set_method_end(&mut self, index: usize, end: u32) {
        self.builder.set_method_end(index, end);
    }

    fn declaration(&mut self, raw: &str, start: usize) {
        let cleaned = strip_comments(raw);
        let Some((name, value)) = cleaned.split_once(':') else {
            return;
        };
        let name = name.trim();
        if name.starts_with("--") {
            let name_offset = raw.find(name).unwrap_or(0);
            self.builder.constant(Constant {
                js: false,
                container: Some(self.container.clone()),
                name: name.into(),
                line: line(self.source, start + name_offset),
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
        self.calls(&cleaned, start);
    }

    fn calls(&mut self, raw: &str, start: usize) {
        let b = raw.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if is_ident_start(b[i]) {
                let mut j = i + 1;
                while j < b.len() && is_ident(b[j]) {
                    j += 1;
                }
                if b.get(j) == Some(&b'(') {
                    let target = &raw[i..j];
                    if matches!(
                        target,
                        "translateY" | "blur" | "scale" | "cubic-bezier" | "set"
                    ) {
                        i = j;
                        continue;
                    }
                    let category = if target == "var" { "var" } else { "function" };
                    self.builder.call(Call {
                        handler: None,
                        container: Some(self.container.clone()),
                        caller: None,
                        line: line(self.source, start + i),
                        kind: target.into(),
                        target: target.into(),
                        resolved: None,
                        on: None,
                        category: category.into(),
                        args: None,
                        end_line: None,
                    });
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

    fn comment(&mut self, start: usize, end: usize, _caller: Option<String>) {
        let text = self.source[start + 2..end.saturating_sub(2)]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        self.builder.comment(Comment {
            container: Some(self.container.clone()),
            caller: None,
            line: line(self.source, start),
            tag: None,
            text,
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
            self.builder.string(StringLit {
                container: Some(self.container.clone()),
                caller: None,
                line: line(self.source, start + begin),
                value: text[begin..i.min(bytes.len())].to_string(),
                end_line: None,
            });
            i = (i + 1).min(bytes.len());
        }
    }
}
