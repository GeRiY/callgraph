//! A fájl névtáblája a `Scopes`-ból: saját típusok, szabad függvények és `self`-es metódusok
//! (a hívás-kategorizálás `internal_helper` döntéseihez).

use super::scopes::FnRole;
use super::{FileVisitor, Scopes};
use crate::lang::rs::context::RsContext;
use crate::lang::rs::parser::ParsedFile;
use crate::model::FileGraphBuilder;

/// A `RsContext::resolver` saját elemeit tölti; a kimenethez nem ír.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct SymbolsVisitor;

impl FileVisitor for SymbolsVisitor {
    fn visit(
        &self,
        _parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        ctx: &mut RsContext,
        _builder: &mut FileGraphBuilder,
    ) {
        for c in &scopes.containers {
            if matches!(c.kind, "struct" | "enum" | "union" | "trait") {
                ctx.resolver.add_type(c.name.clone());
            }
        }
        for f in &scopes.fns {
            let name = f.sig.ident.to_string();
            match f.role {
                FnRole::Free => ctx.resolver.add_free_fn(&name, &f.container),
                _ if f.has_receiver() => ctx.resolver.add_method(&name, &f.container),
                _ => {}
            }
        }
    }
}
