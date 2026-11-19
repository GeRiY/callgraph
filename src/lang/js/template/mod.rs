//! Vue template-elemzés: tűrő parser + bejárás a `caller:'template'` hívásokhoz, sztringekhez, elágazásokhoz.

mod attrs;
mod emit;
mod entities;
mod expr;
mod facts;
mod node;
mod parser;
mod script;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::path::Path;

use super::calls::Scope;
use super::lines::LineIndex;
use super::sfc::Sfc;
use crate::model::{Constant, FileGraph};
use emit::Emitter;
pub use facts::{ComponentUse, TemplateFacts};
use script::ExprAnalyzer;

/// The component-contract facts of a `.vue` template (component tags with what they pass, `<slot>` outlets,
/// `$attrs` use); `None` without a plain-HTML template.
pub fn template_facts(text: &str, sfc: &Sfc) -> Option<TemplateFacts> {
    let block = sfc.template()?;
    let html = block.lang.as_deref().is_none_or(|l| l == "html");
    if !html || text[block.start..block.end].trim().is_empty() {
        return None;
    }
    let lines = LineIndex::new(text);
    let nodes = parser::parse(text, block.start, block.end, &lines);
    Some(facts::collect(&nodes))
}

/// A `.vue` template elemzése; az eredményt a kész fájlgráfhoz fűzi (a script-passzok után hívandó).
pub fn analyze_template(graph: &mut FileGraph, text: &str, sfc: &Sfc, scope: Scope) {
    let Some(block) = sfc.template() else { return };
    let html = block.lang.as_deref().is_none_or(|l| l == "html");
    if !html || text[block.start..block.end].trim().is_empty() {
        return;
    }
    let lines = LineIndex::new(text);
    let nodes = parser::parse(text, block.start, block.end, &lines);
    let container = graph
        .containers
        .first()
        .map_or(String::new(), |c| c.name.clone());
    let emit_prefix = super::emit_prefix(Path::new(&graph.file));
    let methods: HashSet<&str> = graph
        .methods
        .iter()
        .filter(|m| m.kind.as_deref() != Some("template"))
        .map(|m| m.name.as_str())
        .collect();
    let ast = ExprAnalyzer::new(
        Path::new(&graph.file).to_path_buf(),
        &container,
        scope,
        methods.iter().map(|m| m.to_string()),
    );
    let mut emitter = Emitter {
        container: &container,
        emit_prefix: &emit_prefix,
        methods: &methods,
        imports: &graph.imports,
        ast,
        locals: Vec::new(),
        out: Default::default(),
    };
    emitter.walk(&nodes);
    let out = emitter.out;
    // the `<slot>` outlets are the component's slots (`slots (N): head, default` in the outline)
    let mut seen = HashSet::new();
    let slots: Vec<Constant> = facts::collect(&nodes)
        .slots
        .into_iter()
        .filter(|(name, _)| seen.insert(name.clone()))
        .map(|(name, line)| Constant {
            container: Some(container.clone()),
            kind: "slot".into(),
            name: format!("slot:{name}"),
            line,
            value: None,
            visibility: None,
            caller: None,
            doc: None,
            js: true,
        })
        .collect();
    graph.calls.extend(out.calls);
    graph.strings.extend(out.strings);
    graph.control_flow.extend(out.control_flow);
    graph.constants.extend(slots);
}
