//! `calls`: út-hívások (`Foo::new()`, `f()`), metódushívások (`x.bar()`) és makróhívások (`println!`),
//! a makró-argumentumokban lévő hívásokkal együtt; a kategóriát a `Categorizer` adja.

use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, Macro, Path};

use super::body::{BodyVisit, Location, walk_bodies};
use super::macros::visit_body;
use super::{FileVisitor, Scopes};
use crate::lang::rs::categorize::{CallInput, Callee, Categorizer};
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::ParsedFile;
use crate::lang::rs::resolve::Resolver;
use crate::model::{Call, FileGraphBuilder};

/// A `calls` mezőt tölti (a kategorizáló a konstruktoron át jön).
pub(crate) struct CallsVisitor {
    categorizer: Categorizer,
}

impl CallsVisitor {
    pub(crate) fn new(categorizer: Categorizer) -> Self {
        Self { categorizer }
    }

    pub(crate) fn with_defaults() -> Self {
        Self::new(Categorizer::with_defaults())
    }
}

impl FileVisitor for CallsVisitor {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    ) {
        let mut collector = Collector {
            parsed,
            resolver: &ctx.resolver,
            categorizer: &self.categorizer,
            location: None,
            calls: Vec::new(),
        };
        walk_bodies(scopes, &mut collector);
        for call in collector.calls {
            builder.call(call);
        }
    }
}

struct Collector<'p, 'a> {
    parsed: &'p ParsedFile<'a>,
    resolver: &'p Resolver,
    categorizer: &'p Categorizer,
    location: Option<Location>,
    calls: Vec<Call>,
}

fn segments(path: &Path) -> Vec<String> {
    path.segments.iter().map(|s| s.ident.to_string()).collect()
}

impl Collector<'_, '_> {
    fn record(&mut self, callee: Callee, line: u32, on: Option<String>) {
        let Some(location) = &self.location else {
            return;
        };
        let bound_locally = matches!(&callee, Callee::Path(segs)
            if segs.len() == 1 && location.bindings.contains(&segs[0]));
        let verdict = self.categorizer.categorize(&CallInput {
            callee: &callee,
            container: &location.container,
            inline: &location.inline,
            bound_locally,
            resolver: self.resolver,
        });
        self.calls.push(Call {
            container: Some(location.container.clone()),
            caller: Some(location.caller.clone()),
            line,
            end_line: None,
            kind: callee.kind().to_owned(),
            target: callee.target(),
            resolved: verdict.resolved,
            on,
            category: verdict.category.to_owned(),
            args: None,
            handler: None,
        });
    }
}

impl BodyVisit for Collector<'_, '_> {
    fn enter(&mut self, location: Location) {
        self.location = Some(location);
    }
}

impl<'ast> Visit<'ast> for Collector<'_, '_> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if let Expr::Path(p) = &*node.func {
            let line = self.parsed.line(p.span());
            self.record(Callee::Path(segments(&p.path)), line, None);
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let on_self = matches!(&*node.receiver, Expr::Path(p) if p.path.is_ident("self"));
        let callee = Callee::Method {
            name: node.method.to_string(),
            on_self,
        };
        let on = self.parsed.short(&*node.receiver, 40);
        let line = self.parsed.line(node.method.span());
        self.record(callee, line, Some(on));
        visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        let line = self.parsed.line(node.path.span());
        self.record(Callee::Macro(segments(&node.path)), line, None);
        visit_body(self, node);
    }
}
