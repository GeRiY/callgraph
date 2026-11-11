//! A Python-szintaxisfa bejárása (Visitor): importok, osztályok, függvények, hívások, stringek
//! → `FileGraphBuilder`. Tree-sitter-típus csak itt és a `parser` alatt jelenik meg.

mod body;
mod decls;
mod members;
mod scan;

use std::collections::HashSet;

use tree_sitter::Node;

use super::categorize::{Categorizer, FileSymbols};
use super::known::FRAMEWORKS;
use super::parser::ParsedPython;
use crate::model::FileGraphBuilder;

/// A bejárás aktuális helye: a container és a hívó metódus.
#[derive(Debug, Clone)]
pub(super) struct Scope {
    pub container: Option<String>,
    pub caller: String,
}

/// Egy deklarációs szint: a container neve és hogy osztály-törzs-e (modul-szint: `false`).
#[derive(Debug, Clone)]
pub(super) struct Ctx {
    pub container: String,
    pub in_class: bool,
}

/// A Python-fájl bejárója: egy parse-olt fáról a builderbe gyűjt.
pub(crate) struct PythonVisitor<'p, 's> {
    parsed: &'p ParsedPython<'s>,
    categorizer: &'p Categorizer,
    /// A fájl neve (`a.py`): a modul-container neve.
    file_name: String,
    symbols: FileSymbols,
    /// Már felvett (container, név) párok: a `@x.setter` / túlterhelés egyedi nevet kap (`x@L12`).
    taken: HashSet<(String, String)>,
}

impl<'p, 's> PythonVisitor<'p, 's> {
    pub(crate) fn new(
        parsed: &'p ParsedPython<'s>,
        categorizer: &'p Categorizer,
        file_name: &str,
    ) -> Self {
        Self {
            parsed,
            categorizer,
            file_name: file_name.to_string(),
            symbols: FileSymbols::default(),
            taken: HashSet::new(),
        }
    }

    /// A teljes bejárás: szimbólum- és import-gyűjtés, majd a deklarációk és a törzsek.
    pub(crate) fn run(mut self, b: &mut FileGraphBuilder) {
        let root = self.parsed.root();
        let imports = scan::scan(self.parsed, root, &mut self.symbols);
        let mut framework = "python";
        for (alias, full) in &imports {
            b.import(alias.clone(), full.clone());
            let top = full.split('.').next().unwrap_or(full);
            if let Some((_, name)) = FRAMEWORKS.iter().find(|(module, _)| *module == top)
                && framework == "python"
            {
                framework = name;
            }
        }
        b.framework(Some(framework.to_string()));
        self.module(root, b);
        b.sort_methods_by_line();
    }

    fn text(&self, node: Node<'_>) -> &'s str {
        self.parsed.text(node)
    }
}

/// Legfeljebb `max` karakter (a levágott szöveg végén `…`).
pub(super) fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max).collect();
    out.push('…');
    out
}

/// Láthatóság: a vezető `_` (nem dunder) privát.
pub(super) fn visibility(name: &str) -> &'static str {
    let dunder = name.starts_with("__") && name.ends_with("__") && name.len() > 4;
    if name.starts_with('_') && !dunder {
        "private"
    } else {
        "public"
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dynamic;
