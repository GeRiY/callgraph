//! JS / TS / Vue frontend: oxc-parser, Vue SFC felbontás és a közös fájlgráf váza.

mod analysis;
mod calls;
mod comments;
mod constants;
mod container_doc;
mod control_flow;
mod frontend;
mod imports;
mod lines;
mod meta;
mod methods;
mod parse;
mod props;
mod resolve;
mod scope;
mod sfc;
mod strings;
mod template;

pub use analysis::{JsAnalysis, JsPass, PassContext};
pub use frontend::JsFrontend;
pub use imports::{ImportBinding, ImportsPass, categorize, import_bindings};
pub use lines::LineIndex;
pub use methods::{Docs, MethodsPass, container_doc, template_method};
pub use parse::{ParsedProgram, parse_program, source_type_for};
pub use resolve::{ResolveConfig, Resolved, Resolver};
pub use sfc::{Sfc, SfcBlock, SfcBlockKind};
pub use template::analyze_template;
