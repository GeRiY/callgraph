//! A Go-szintaxisfa bejárása (Visitor): importok, típusok, függvények, metódusok, hívások, stringek
//! → `FileGraphBuilder`. Tree-sitter-típus csak itt és a `parser` alatt jelenik meg.

mod body;
mod decls;
mod members;
mod scan;

use std::collections::HashSet;

use tree_sitter::Node;

use super::categorize::{Categorizer, FileSymbols};
use super::known::FRAMEWORKS;
use super::parser::ParsedGo;
use crate::model::FileGraphBuilder;

/// A bejárás aktuális helye: a container, a hívó függvény / metódus és a metódus receiver-neve.
#[derive(Debug, Clone)]
pub(super) struct Scope {
    pub container: Option<String>,
    pub caller: String,
    /// A receiver változó neve (`r` a `func (r *Repo) Find()`-ban); függvénynél és `_`/névtelen receivernél `None`.
    pub receiver: Option<String>,
}

/// A Go-fájl bejárója: egy parse-olt fáról a builderbe gyűjt.
pub(crate) struct GoVisitor<'p, 's> {
    parsed: &'p ParsedGo<'s>,
    categorizer: &'p Categorizer,
    /// A fájl neve (`a.go`): a modul-container neve.
    file_name: String,
    symbols: FileSymbols,
    /// Már felvett (container, név) párok: a többszörös `init` egyedi nevet kap (`init@L12`).
    taken: HashSet<(String, String)>,
}

impl<'p, 's> GoVisitor<'p, 's> {
    pub(crate) fn new(
        parsed: &'p ParsedGo<'s>,
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
        let scanned = scan::scan(self.parsed, root, &mut self.symbols);
        let mut framework = "go";
        for (alias, full) in &scanned.imports {
            b.import(alias.clone(), full.clone());
            if framework == "go"
                && let Some((_, name)) = FRAMEWORKS
                    .iter()
                    .find(|(prefix, _)| full.starts_with(prefix))
            {
                framework = name;
            }
        }
        b.framework(Some(framework.to_string()));
        b.namespace_if_unset(scanned.package);
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

/// Láthatóság: a nagybetűvel kezdődő név exportált (`public`), a többi `private`.
pub(super) fn visibility(name: &str) -> &'static str {
    if name.chars().next().is_some_and(char::is_uppercase) {
        "public"
    } else {
        "private"
    }
}

#[cfg(test)]
mod tests;
