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
pub mod react;
mod resolve;
mod scope;
mod sfc;
mod strings;
mod template;
mod vite_alias;

pub use analysis::{JsAnalysis, JsPass, PassContext};
pub use frontend::JsFrontend;
pub use imports::{ImportBinding, ImportsPass, categorize, import_bindings};
pub use lines::LineIndex;
pub use methods::{Docs, MethodsPass, container_doc, template_method};
pub use parse::{ParsedProgram, parse_program, source_type_for};
pub use resolve::{ResolveConfig, Resolved, Resolver};
pub use sfc::{Sfc, SfcBlock, SfcBlockKind};
pub use template::{ComponentUse, TemplateFacts, analyze_template, template_facts};

/// The key prefix of a component's emits: `Card.vue::emit:` for a Vue file, so `emit('save')` in `Card.vue` and the
/// parent's `@save` on `<Card>` meet in one `Card.vue::emit:save` key. Outside a `.vue` file (a composable that
/// receives `emit`) the component is unknown and the key stays the global `vue::emit:`.
pub fn emit_prefix(path: &std::path::Path) -> String {
    match path.file_name().map(|n| n.to_string_lossy()) {
        Some(name) if name.ends_with(".vue") => format!("{name}::emit:"),
        _ => "vue::emit:".to_string(),
    }
}
