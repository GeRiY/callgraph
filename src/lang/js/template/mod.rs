//! Vue template-elemzés: tűrő parser + bejárás a `caller:'template'` hívásokhoz, sztringekhez, elágazásokhoz.

mod attrs;
mod emit;
mod entities;
mod expr;
mod node;
mod parser;

use std::collections::HashSet;

use super::lines::LineIndex;
use super::sfc::Sfc;
use crate::model::FileGraph;
use emit::Emitter;

/// A `.vue` template elemzése; az eredményt a kész fájlgráfhoz fűzi (a script-passzok után hívandó).
pub fn analyze_template(graph: &mut FileGraph, text: &str, sfc: &Sfc) {
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
    let methods: HashSet<&str> = graph
        .methods
        .iter()
        .filter(|m| m.kind.as_deref() != Some("template"))
        .map(|m| m.name.as_str())
        .collect();
    let mut emitter = Emitter {
        container: &container,
        methods: &methods,
        imports: &graph.imports,
        out: Default::default(),
    };
    emitter.walk(&nodes);
    let out = emitter.out;
    graph.calls.extend(out.calls);
    graph.strings.extend(out.strings);
    graph.control_flow.extend(out.control_flow);
}
