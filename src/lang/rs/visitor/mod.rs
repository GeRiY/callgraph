//! Visitorok: a szintaxisfa bejárása szempontonként (importok, névtábla, szerkezet, hívások, vezérlés,
//! stringek, kommentek). Syn-típus csak itt és a `parser` alatt jelenhet meg.

mod attrs;
mod body;
mod calls;
mod comments;
mod control_flow;
mod imports;
mod macros;
mod scopes;
mod strings;
mod structure;
mod symbols;

pub(crate) use calls::CallsVisitor;
pub(crate) use comments::CommentsVisitor;
pub(crate) use control_flow::ControlFlowVisitor;
pub(crate) use imports::ImportsVisitor;
pub(crate) use scopes::Scopes;
pub(crate) use strings::StringsVisitor;
pub(crate) use structure::StructureVisitor;
pub(crate) use symbols::SymbolsVisitor;

use super::context::RsContext;
use super::parser::ParsedFile;
use crate::model::FileGraphBuilder;

/// Egy szempont szerinti bejárás: a közös `Scopes`-ból és a szintaxisfából a builderbe gyűjt.
pub(crate) trait FileVisitor: Send + Sync {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        scopes: &Scopes<'_>,
        ctx: &mut RsContext,
        builder: &mut FileGraphBuilder,
    );
}

/// Az alapértelmezett sorrend: előbb a névtábla (importok, saját elemek), utána a kimenet mezői.
pub(crate) fn default_visitors() -> Vec<Box<dyn FileVisitor>> {
    vec![
        Box::new(ImportsVisitor),
        Box::new(SymbolsVisitor),
        Box::new(StructureVisitor),
        Box::new(CallsVisitor::with_defaults()),
        Box::new(ControlFlowVisitor),
        Box::new(StringsVisitor),
        Box::new(CommentsVisitor),
    ]
}

/// A visitorok futtatása egy parse-olt fájlon (a `Scopes` egyszer készül el).
pub(crate) fn run(
    visitors: &[Box<dyn FileVisitor>],
    parsed: &ParsedFile<'_>,
    ctx: &mut RsContext,
    builder: &mut FileGraphBuilder,
) {
    let scopes = scopes::collect(parsed, ctx.resolver.module());
    for visitor in visitors {
        visitor.visit(parsed, &scopes, ctx, builder);
    }
}
