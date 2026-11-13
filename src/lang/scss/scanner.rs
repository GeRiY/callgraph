use crate::lang::Analysis;
use crate::lang::scss::scan_helpers::{
    call, call_parts, constant, control_flow, declaration, functions, is_rule, paren_delta,
};
use crate::lang::stylesheet::{line, normalize};
use crate::model::{Call, Comment, Container, FileGraphBuilder, Method, StringLit};

/// A SCSS forrás egyszerű, stabil outline-leképezője.
pub struct ScssScanner<'a> {
    source: &'a str,
    container: String,
    builder: FileGraphBuilder,
}

impl<'a> ScssScanner<'a> {
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
        let end = line(self.source, self.source.len().saturating_sub(1));
        self.builder.container(Container {
            kind: "stylesheet".into(),
            name: self.container.clone(),
            line: 1,
            end_line: end,
            is_abstract: None,
            is_final: None,
            extends: None,
            implements: None,
            traits: None,
            doc: None,
        });
        let mut depth = 0usize;
        let mut methods: Vec<(usize, usize)> = Vec::new();
        // nyitott zárójel a sor elején: többsoros paraméterlista / argumentum / map folytatása — nem deklaráció
        let mut paren_depth = 0isize;
        for (n, raw) in self.source.lines().enumerate() {
            let ln = n as u32 + 1;
            let clean = raw.trim();
            if let Some(comment) = clean.strip_prefix("//") {
                self.builder.comment(Comment {
                    container: Some(self.container.clone()),
                    caller: None,
                    line: ln,
                    tag: None,
                    text: comment.trim().into(),
                });
                continue;
            }
            self.comments(raw, ln);
            self.strings(raw, ln);
            self.import(raw);
            if paren_depth == 0 {
                constant(&mut self.builder, &self.container, raw, ln);
            }
            paren_depth = (paren_depth + paren_delta(raw)).max(0);
            control_flow(&mut self.builder, &self.container, raw, ln);
            self.calls(raw, ln);
            if let Some(head) = raw
                .split('{')
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if let Some((kind, name, params)) = declaration(head) {
                    let idx = self.builder.method_count();
                    self.builder.method(Method {
                        container: Some(self.container.clone()),
                        name,
                        line: ln,
                        end_line: Some(ln),
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
                    methods.push((idx, depth));
                    let _ = kind;
                } else if is_rule(head) {
                    let idx = self.builder.method_count();
                    self.builder.method(Method {
                        container: Some(self.container.clone()),
                        name: normalize(head),
                        line: ln,
                        end_line: Some(ln),
                        visibility: None,
                        is_static: false,
                        is_abstract: false,
                        params: Vec::new(),
                        return_type: None,
                        doc_types: None,
                        doc: None,
                        body: None,
                        kind: Some("rule".into()),
                    });
                    methods.push((idx, depth));
                }
            }
            for c in raw.chars() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth = depth.saturating_sub(1);
                    if let Some(pos) = methods.iter().rposition(|(_, d)| *d == depth) {
                        let (idx, _) = methods.remove(pos);
                        self.builder.set_method_end(idx, ln);
                    }
                }
            }
        }
        Analysis {
            graph: self.builder.build(),
            warnings: Vec::new(),
        }
    }

    fn comments(&mut self, raw: &str, ln: u32) {
        if let Some(a) = raw.find("/*") {
            let b = raw[a + 2..].find("*/").unwrap_or(raw.len() - a - 2);
            self.builder.comment(Comment {
                container: Some(self.container.clone()),
                caller: None,
                line: ln,
                tag: None,
                text: raw[a + 2..a + 2 + b].trim().into(),
            });
        }
    }
    fn strings(&mut self, raw: &str, ln: u32) {
        let mut rest = raw;
        while let Some(a) = rest.find(['"', '\'']) {
            let q = rest.as_bytes()[a];
            let tail = &rest[a + 1..];
            let b = tail
                .as_bytes()
                .iter()
                .position(|x| *x == q)
                .unwrap_or(tail.len());
            self.builder.string(StringLit {
                container: Some(self.container.clone()),
                caller: None,
                line: ln,
                value: tail[..b].into(),
                end_line: None,
            });
            rest = &tail[b.saturating_add(1).min(tail.len())..];
            if rest.is_empty() {
                break;
            }
        }
    }
    fn import(&mut self, raw: &str) {
        let s = raw.trim();
        let Some(kind) = s
            .strip_prefix("@use ")
            .or_else(|| s.strip_prefix("@forward "))
            .or_else(|| s.strip_prefix("@import "))
        else {
            return;
        };
        let Some(q) = kind.find(['"', '\'']) else {
            return;
        };
        let tail = &kind[q + 1..];
        let end = tail.find(['"', '\'']).unwrap_or(tail.len());
        let spec = &tail[..end];
        let alias = kind[end + q + 1..]
            .split(" as ")
            .nth(1)
            .and_then(|x| x.split_whitespace().next())
            .unwrap_or_else(|| {
                spec.rsplit('/')
                    .next()
                    .unwrap_or(spec)
                    .trim_start_matches('_')
            })
            .trim_end_matches(';');
        self.builder.import(alias.into(), spec.into());
    }
    fn calls(&mut self, raw: &str, ln: u32) {
        let s = raw.trim();
        if let Some(a) = s.find("@include ")
            && let Some((target, args)) = call_parts(&s[a + 9..])
        {
            self.builder.call(call(
                &self.container,
                ln,
                "include",
                target,
                "include",
                args.clone(),
            ));
            self.builder.call(call(
                &self.container,
                ln,
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
            self.builder.call(call(
                &self.container,
                ln,
                "extend",
                target,
                "extend",
                Vec::new(),
            ));
        }
        for (name, args) in functions(s) {
            let included = s
                .find("@include ")
                .and_then(|a| call_parts(&s[a + 9..]))
                .is_some_and(|(target, _)| target.rsplit('.').next() == Some(name));
            if included || s.starts_with(&format!("@{name}")) {
                continue;
            }
            self.builder.call(call(
                &self.container,
                ln,
                "func_call",
                name,
                "function",
                args,
            ));
        }
        let mut from = 0;
        while let Some(a) = s[from..].find("#{") {
            let a = from + a + 2;
            if let Some(b) = s[a..].find('}') {
                self.builder.call(Call {
                    handler: None,
                    from: None,
                    container: Some(self.container.clone()),
                    caller: None,
                    line: ln,
                    end_line: None,
                    kind: "interpolation".into(),
                    target: s[a..a + b].into(),
                    resolved: None,
                    on: None,
                    category: "interpolation".into(),
                    args: None,
                });
                from = a + b + 1;
            } else {
                break;
            }
        }
    }
}
