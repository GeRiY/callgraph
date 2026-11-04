//! Visitor-réteg: az AST bejárása szempontonként; mindegyik a `FileGraphBuilder`-be gyűjt.

mod calls;
mod comments;
mod constants;
mod context;
mod if_else;
mod match_switch;
mod properties;
mod strings;
mod structure;
mod writes;

pub use calls::CallsVisitor;
pub use comments::CommentsVisitor;
pub use constants::ConstantsVisitor;
pub use if_else::IfElseVisitor;
pub use match_switch::MatchSwitchVisitor;
pub use properties::PropertiesVisitor;
pub use strings::StringsVisitor;
pub use structure::StructureVisitor;
pub use writes::WritesVisitor;

use super::AnalysisContext;
use crate::lang::php::parser::ParsedFile;
use crate::model::FileGraphBuilder;

/// Egy szempontú bejáró: a parse-olt fájlból tölti a builder megfelelő részét.
pub trait FileVisitor: Send + Sync {
    fn visit(
        &self,
        parsed: &ParsedFile<'_>,
        ctx: &mut AnalysisContext,
        builder: &mut FileGraphBuilder,
    );
}

/// Az alapértelmezett visitor-sor (a still-üres visitorok a helyükön vannak).
pub fn default_visitors() -> Vec<Box<dyn FileVisitor>> {
    vec![
        Box::new(StructureVisitor),
        Box::new(CallsVisitor),
        Box::new(IfElseVisitor),
        Box::new(MatchSwitchVisitor),
        Box::new(StringsVisitor),
        Box::new(CommentsVisitor),
        Box::new(ConstantsVisitor),
        Box::new(WritesVisitor),
        Box::new(PropertiesVisitor),
    ]
}
