//! A Kotlin-szintaxisfa bejárása (Visitor): deklarációk, hívások, vezérlés, stringek, kommentek
//! → `FileGraphBuilder`. Tree-sitter-típus csak itt és a `parser` alatt jelenik meg.

mod body;
mod calls;
mod decls;
mod flow;
mod links;
mod members;
mod nav;
mod scan;
mod signature;
mod text;
mod util;

use std::collections::HashMap;

use tree_sitter::Node;

use super::categorize::{Categorizer, FileSymbols};
use super::parser::ParsedKotlin;
use crate::model::FileGraphBuilder;

/// A bejárás aktuális helye: a container és a hívó metódus.
#[derive(Debug, Clone)]
pub(super) struct Scope {
    pub container: Option<String>,
    pub caller: String,
    /// `@Composable` függvény törzse (a benne lévő lambdák is).
    pub in_compose: bool,
}

/// Egy felvett metódus sortartománya (a kommentek hozzárendeléséhez).
#[derive(Debug, Clone)]
pub(super) struct MethodSpan {
    pub container: Option<String>,
    pub name: String,
    pub line: u32,
    pub end_line: u32,
}

/// A Kotlin-fájl bejárója: egy parse-olt fáról a builderbe gyűjt.
pub(crate) struct KotlinVisitor<'p, 's> {
    parsed: &'p ParsedKotlin<'s>,
    categorizer: &'p Categorizer,
    file_name: String,
    /// `*.gradle.kts` build-szkript (a framework `gradle` jelölést kap).
    gradle: bool,
    symbols: FileSymbols,
    /// A fájl-szintű függvények / változók containere (`<fájlnév>`), ha kell.
    module: Option<String>,
    methods: Vec<MethodSpan>,
    containers: Vec<(String, u32, u32)>,
    /// A jelenlegi függvény helyi változói és paraméterei: név → típus.
    locals: HashMap<String, String>,
}

impl<'p, 's> KotlinVisitor<'p, 's> {
    pub(crate) fn new(
        parsed: &'p ParsedKotlin<'s>,
        categorizer: &'p Categorizer,
        file_name: &str,
        gradle: bool,
    ) -> Self {
        Self {
            parsed,
            categorizer,
            file_name: file_name.to_string(),
            gradle,
            symbols: FileSymbols::default(),
            module: None,
            methods: Vec::new(),
            containers: Vec::new(),
            locals: HashMap::new(),
        }
    }

    /// A teljes bejárás: előzetes szimbólum-gyűjtés, deklarációk és törzsek, végül a kommentek.
    pub(crate) fn run(mut self, b: &mut FileGraphBuilder) {
        let root = self.parsed.root();
        let scanned = scan::scan(self.parsed, root);
        self.symbols = scanned.symbols;
        let framework = if self.gradle {
            "gradle"
        } else {
            scan::framework_of(&scanned.imports)
        };
        b.framework(Some(framework.to_string()));
        if scanned.needs_module {
            self.module_container(root, b);
        }
        self.visit_top_level(root, b);
        self.comments(root, b);
        b.sort_methods_by_line();
    }

    fn text(&self, node: Node<'_>) -> &'s str {
        self.parsed.text(node)
    }

    /// A jelenlegi osztály típus-térképe (property / konstruktor-paraméter neve → típusa).
    fn class_members(&self, container: Option<&str>) -> Option<&HashMap<String, String>> {
        let simple = container?.rsplit('.').next()?;
        self.symbols.members.get(simple)
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

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_links;
