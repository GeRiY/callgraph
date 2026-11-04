//! PHP-elemzés: az AST-t szempontonként bejáró visitorok és segédeik.

mod context;
pub mod docblock;
mod name_resolver;
mod type_tracker;
pub mod visitor;

pub use context::AnalysisContext;
pub use name_resolver::NameResolver;
pub use type_tracker::{NullTypeTracker, TypeTracker};
