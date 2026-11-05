//! A Swift-szintaxisfa bejárása (Visitor): deklarációk, hívások, vezérlés, stringek, kommentek
//! → `FileGraphBuilder`. Tree-sitter-típus csak itt és a `parser` alatt jelenik meg.

mod body;
mod calls;
mod decls;
mod flow;
mod members;
mod scan;
mod signature;
mod text;

use tree_sitter::Node;

use super::categorize::{Categorizer, FileSymbols};
use super::parser::ParsedSwift;
use crate::model::FileGraphBuilder;

/// A bejárás aktuális helye: a container és a hívó metódus.
#[derive(Debug, Clone)]
pub(super) struct Scope {
    pub container: Option<String>,
    pub caller: String,
    /// SwiftUI-nézetet építő törzs (`var body: some View`, `@ViewBuilder`).
    pub in_view: bool,
}

/// Egy felvett metódus sortartománya (a kommentek hozzárendeléséhez).
#[derive(Debug, Clone)]
pub(super) struct MethodSpan {
    pub container: Option<String>,
    pub name: String,
    pub line: u32,
    pub end_line: u32,
}

/// A Swift-fájl bejárója: egy parse-olt fáról a builderbe gyűjt.
pub(crate) struct SwiftVisitor<'p, 's> {
    parsed: &'p ParsedSwift<'s>,
    categorizer: &'p Categorizer,
    file_name: String,
    symbols: FileSymbols,
    /// A fájl-szintű függvények / változók containere (`<fájlnév>.swift`), ha kell.
    module: Option<String>,
    methods: Vec<MethodSpan>,
    containers: Vec<(String, u32, u32)>,
}

impl<'p, 's> SwiftVisitor<'p, 's> {
    pub(crate) fn new(
        parsed: &'p ParsedSwift<'s>,
        categorizer: &'p Categorizer,
        file_name: &str,
    ) -> Self {
        Self {
            parsed,
            categorizer,
            file_name: file_name.to_string(),
            symbols: FileSymbols::default(),
            module: None,
            methods: Vec::new(),
            containers: Vec::new(),
        }
    }

    /// A teljes bejárás: előzetes szimbólum-gyűjtés, deklarációk és törzsek, végül a kommentek.
    pub(crate) fn run(mut self, b: &mut FileGraphBuilder) {
        let root = self.parsed.root();
        let (symbols, needs_module) = scan::scan(self.parsed, root);
        self.symbols = symbols;
        let swiftui = scan::imports_swiftui(self.parsed, root);
        b.framework(Some(if swiftui { "swiftui" } else { "swift" }.to_string()));
        if needs_module {
            self.module_container(root, b);
        }
        self.visit_top_level(root, b);
        self.comments(root, b);
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

#[cfg(test)]
mod tests;
