//! `strings`: string- és bájtstring-literálok a törzsekben, a `const` / `static` kezdőértékekben (a makró-
//! argumentumokban is), az átlátszatlan makrók tokenjeiben és a függvények / típusok attribútumaiban
//! (`#[get("/users/{id}")]`).

use proc_macro2::Span;
use syn::visit::Visit;
use syn::{LitByteStr, LitStr, Macro};

use super::attrs::{attr_strings, token_strings};
use super::body::{BodyVisit, Location, walk_bodies};
use super::macros::visit_body;
use super::{FileVisitor, Scopes};
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::{ParsedFile, shorten};
use crate::model::{FileGraphBuilder, StringLit};

/// A `strings` mezőt tölti (üres / csak whitespace értéket nem; ≤300 karakter).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct StringsVisitor;

impl FileVisitor for StringsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        _ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut collector = Collector {
            parsed,
            location: None,
            rows: Vec::new(),
        };
        for c in scopes.containers.iter().filter(|c| c.kind != "module") {
            collector.enter(top(&c.name));
            for (span, value) in attr_strings(c.attrs) {
                collector.push(span, value);
            }
        }
        for f in &scopes.fns {
            collector.enter(Location {
                container: f.container.clone(),
                caller: f.name.clone(),
                inline: Vec::new(),
                bindings: Default::default(),
                types: Default::default(),
            });
            for (span, value) in attr_strings(f.attrs) {
                collector.push(span, value);
            }
        }
        walk_bodies(scopes, &mut collector);
        collector.rows.sort_by_key(|s| s.line);
        for row in collector.rows {
            builder.string(row);
        }
    }
}

fn top(container: &str) -> Location {
    Location {
        container: container.to_owned(),
        caller: "__top__".to_owned(),
        inline: Vec::new(),
        bindings: Default::default(),
        types: Default::default(),
    }
}

struct Collector<'p, 'a> {
    parsed: &'p ParsedFile<'a>,
    location: Option<Location>,
    rows: Vec<StringLit>,
}

impl Collector<'_, '_> {
    fn push(&mut self, span: Span, value: String) {
        let Some(location) = &self.location else {
            return;
        };
        if value.trim().is_empty() {
            return;
        }
        let (line, end) = (self.parsed.line(span), self.parsed.end_line(span));
        self.rows.push(StringLit {
            container: Some(location.container.clone()),
            caller: Some(location.caller.clone()),
            line,
            end_line: (end > line).then_some(end),
            value: shorten(&value, 300),
        });
    }
}

impl BodyVisit for Collector<'_, '_> {
    fn enter(&mut self, location: Location) {
        self.location = Some(location);
    }
}

impl<'ast> Visit<'ast> for Collector<'_, '_> {
    fn visit_lit_str(&mut self, node: &'ast LitStr) {
        self.push(node.span(), node.value());
    }

    /// `b"…"` (pl. `const DATA: &[u8] = b"…"`): a bájtok UTF-8-ként (a hibás bájt `�`).
    fn visit_lit_byte_str(&mut self, node: &'ast LitByteStr) {
        self.push(
            node.span(),
            String::from_utf8_lossy(&node.value()).into_owned(),
        );
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        if !visit_body(self, node) {
            let mut found = Vec::new();
            token_strings(node.tokens.clone(), &mut found);
            for (span, value) in found {
                self.push(span, value);
            }
        }
    }
}
