//! Előzetes szimbólum-gyűjtés: a fájl típusai (nézet-e), protokolljai és függvénynevei,
//! valamint hogy kell-e fájl-szintű container.

use tree_sitter::Node;

use super::super::categorize::FileSymbols;
use super::super::parser::{ParsedSwift, named_children};

/// A szimbólumok és hogy van-e fájl-szintű (típuson kívüli) függvény / változó / utasítás.
pub(super) fn scan(parsed: &ParsedSwift<'_>, root: Node<'_>) -> (FileSymbols, bool) {
    let mut symbols = FileSymbols::default();
    collect(parsed, root, None, &mut symbols);
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

fn collect(
    parsed: &ParsedSwift<'_>,
    node: Node<'_>,
    owner: Option<&str>,
    symbols: &mut FileSymbols,
) {
    for child in named_children(node) {
        match child.kind() {
            "class_declaration" | "protocol_declaration" => {
                let is_extension = child
                    .child_by_field_name("declaration_kind")
                    .is_some_and(|k| k.kind() == "extension");
                let mut own: Option<String> = None;
                if let Some(name) = child.child_by_field_name("name") {
                    let name = parsed.text(name).to_string();
                    own = Some(type_key(&name).to_string());
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
                    collect(parsed, body, own.as_deref(), symbols);
                }
            }
            "function_declaration" | "protocol_function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    let name = parsed.text(name).to_string();
                    if let Some(owner) = owner {
                        symbols
                            .methods
                            .entry(owner.to_string())
                            .or_default()
                            .insert(name.clone());
                    }
                    symbols.functions.insert(name);
                }
            }
            "property_declaration" | "protocol_property_declaration" => {
                if let (Some(owner), Some((name, ty))) = (owner, property_type(parsed, child)) {
                    symbols
                        .props
                        .entry(owner.to_string())
                        .or_default()
                        .insert(name, ty);
                }
            }
            _ => {}
        }
    }
}

/// A típusnév kulcsa a property-térképhez: `Külső.Belső` → `Belső`, `Foo@L12` (extension) → `Foo`.
pub(super) fn type_key(name: &str) -> &str {
    let name = name.split('@').next().unwrap_or(name);
    name.rsplit('.').next().unwrap_or(name)
}

/// Egy típus-szöveg (`: ChatViewModel?`, `some Store`, `Foo<Bar>`) alap-típusneve; kisbetűs, gyűjtemény-,
/// függvény- vagy tuple-típusnál `None`.
pub(super) fn base_type(text: &str) -> Option<String> {
    let t = text.trim().trim_start_matches(':').trim();
    let t = t
        .trim_start_matches("some ")
        .trim_start_matches("any ")
        .trim();
    let t = t.trim_end_matches(['?', '!']);
    let t = t.split('<').next().unwrap_or(t).trim();
    let valid = t.chars().next().is_some_and(char::is_uppercase)
        && t.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    valid.then(|| t.to_string())
}

/// Egy tömb-típus (`: [Item]`) elemtípusa; más alakra (`Array<Item>`, szótár, tuple) `None`.
pub(super) fn element_type(text: &str) -> Option<String> {
    let t = text.trim().trim_start_matches(':').trim();
    let inner = t.strip_prefix('[')?.strip_suffix(']')?;
    base_type(inner)
}

/// A `Típus(...)` hívás típusa (nagybetűs puszta azonosító hívása), a property inicializálójához.
pub(super) fn constructed_type(parsed: &ParsedSwift<'_>, value: Node<'_>) -> Option<String> {
    if value.kind() != "call_expression" {
        return None;
    }
    let callee = named_children(value).into_iter().next()?;
    (callee.kind() == "simple_identifier")
        .then(|| parsed.text(callee))
        .and_then(base_type)
}

/// Egy property neve és típusa (üres, ha nem állapítható meg): deklarált típus, `Típus()` inicializáló vagy
/// `@Environment(Típus.self)`.
pub(super) fn property_type(parsed: &ParsedSwift<'_>, node: Node<'_>) -> Option<(String, String)> {
    let name = parsed.text(node.child_by_field_name("name")?);
    let name = name
        .trim_start_matches("var ")
        .trim_start_matches("let ")
        .trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let declared = named_children(node)
        .into_iter()
        .find(|c| c.kind() == "type_annotation")
        .and_then(|t| base_type(parsed.text(t)));
    let ty = declared
        .or_else(|| {
            node.child_by_field_name("value")
                .and_then(|v| constructed_type(parsed, v))
        })
        .or_else(|| environment_type(parsed, node))
        .unwrap_or_default();
    Some((name.to_string(), ty))
}

/// `@Environment(Store.self)` attribútum típusa.
fn environment_type(parsed: &ParsedSwift<'_>, node: Node<'_>) -> Option<String> {
    let modifiers = named_children(node)
        .into_iter()
        .find(|c| c.kind() == "modifiers")?;
    named_children(modifiers)
        .into_iter()
        .filter(|c| c.kind() == "attribute")
        .find_map(|a| {
            let text = parsed.text(a).trim();
            let inner = text.strip_prefix("@Environment(")?.strip_suffix(')')?;
            if inner.trim() == "\\.modelContext" {
                // a SwiftData-kontextus kulcs-útvonala: a property típusa `ModelContext`
                return Some("ModelContext".to_string());
            }
            base_type(inner.strip_suffix(".self")?)
        })
}
