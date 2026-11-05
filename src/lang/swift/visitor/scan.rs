//! Előzetes szimbólum-gyűjtés: a fájl típusai (nézet-e), protokolljai és függvénynevei,
//! valamint hogy kell-e fájl-szintű container.

use tree_sitter::Node;

use super::super::categorize::FileSymbols;
use super::super::parser::{ParsedSwift, named_children};

/// A szimbólumok és hogy van-e fájl-szintű (típuson kívüli) függvény / változó / utasítás.
pub(super) fn scan(parsed: &ParsedSwift<'_>, root: Node<'_>) -> (FileSymbols, bool) {
    let mut symbols = FileSymbols::default();
    collect(parsed, root, &mut symbols);
    let needs_module = named_children(root).iter().any(|n| {
        !matches!(
            n.kind(),
            "import_declaration"
                | "class_declaration"
                | "protocol_declaration"
                | "comment"
                | "multiline_comment"
                | "typealias_declaration"
                | "operator_declaration"
                | "precedence_group_declaration"
        )
    });
    (symbols, needs_module)
}

/// Importálja-e a fájl a SwiftUI-t.
pub(super) fn imports_swiftui(parsed: &ParsedSwift<'_>, root: Node<'_>) -> bool {
    named_children(root)
        .iter()
        .filter(|n| n.kind() == "import_declaration")
        .any(|n| {
            named_children(*n)
                .iter()
                .any(|c| c.kind() == "identifier" && parsed.text(*c) == "SwiftUI")
        })
}

/// Egy típus-deklaráció örökölt / megfelelt típusainak nevei (`View`, `ObservableObject`…).
pub(super) fn inherited(parsed: &ParsedSwift<'_>, decl: Node<'_>) -> Vec<String> {
    named_children(decl)
        .iter()
        .filter(|c| c.kind() == "inheritance_specifier")
        .map(|c| parsed.text(*c).trim().to_string())
        .collect()
}

fn collect(parsed: &ParsedSwift<'_>, node: Node<'_>, symbols: &mut FileSymbols) {
    for child in named_children(node) {
        match child.kind() {
            "class_declaration" | "protocol_declaration" => {
                let is_extension = child
                    .child_by_field_name("declaration_kind")
                    .is_some_and(|k| k.kind() == "extension");
                if let Some(name) = child.child_by_field_name("name") {
                    let name = parsed.text(name).to_string();
                    if child.kind() == "protocol_declaration" {
                        symbols.protocols.insert(name.clone());
                    }
                    let view = inherited(parsed, child).iter().any(|i| i == "View");
                    if !is_extension {
                        symbols.types.insert(name, view);
                    } else if view {
                        symbols.types.insert(name, true);
                    }
                }
                if let Some(body) = child.child_by_field_name("body") {
                    collect(parsed, body, symbols);
                }
            }
            "function_declaration" | "protocol_function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    symbols.functions.insert(parsed.text(name).to_string());
                }
            }
            _ => {}
        }
    }
}
