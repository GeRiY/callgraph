//! Előzetes szimbólum-gyűjtés: modul-függvények, osztályok, metódusnevek és az importok
//! (a gráfba kerülő `(alias, teljes út)` párok is).

use tree_sitter::Node;

use super::super::categorize::FileSymbols;
use super::super::parser::{ParsedPython, named_children};

/// A szimbólumok a `symbols`-ba; visszaadja a fájl importjait `(alias, teljes út)` párokként.
pub(super) fn scan(
    parsed: &ParsedPython<'_>,
    root: Node<'_>,
    symbols: &mut FileSymbols,
) -> Vec<(String, String)> {
    let mut imports = Vec::new();
    walk(parsed, root, false, false, symbols, &mut imports);
    imports
}

fn walk(
    parsed: &ParsedPython<'_>,
    node: Node<'_>,
    in_class: bool,
    in_func: bool,
    symbols: &mut FileSymbols,
    imports: &mut Vec<(String, String)>,
) {
    for child in named_children(node) {
        let name = child
            .child_by_field_name("name")
            .map(|n| parsed.text(n).to_string());
        match child.kind() {
            "import_statement" => import(parsed, child, symbols, imports),
            "import_from_statement" => import_from(parsed, child, symbols, imports),
            "class_definition" => {
                symbols.classes.extend(name);
                walk(parsed, child, true, in_func, symbols, imports);
            }
            "function_definition" => {
                if !in_func {
                    match (in_class, name) {
                        (true, Some(n)) => symbols.methods.insert(n),
                        (false, Some(n)) => symbols.functions.insert(n),
                        _ => false,
                    };
                }
                walk(parsed, child, false, true, symbols, imports);
            }
            _ => walk(parsed, child, in_class, in_func, symbols, imports),
        }
    }
}

/// `import a.b`, `import a.b as c, d`: a gráfba a pontozott név (vagy alias → út), a kötött név a gyökér.
fn import(
    parsed: &ParsedPython<'_>,
    node: Node<'_>,
    symbols: &mut FileSymbols,
    imports: &mut Vec<(String, String)>,
) {
    let mut cursor = node.walk();
    for item in node.children_by_field_name("name", &mut cursor) {
        if item.kind() == "aliased_import" {
            let (Some(path), Some(alias)) = (
                item.child_by_field_name("name"),
                item.child_by_field_name("alias"),
            ) else {
                continue;
            };
            let (path, alias) = (parsed.text(path).to_string(), parsed.text(alias).to_string());
            symbols.imports.insert(alias.clone(), path.clone());
            imports.push((alias, path));
        } else {
            let path = parsed.text(item).to_string();
            let bound = path.split('.').next().unwrap_or(&path).to_string();
            symbols.imports.insert(bound.clone(), bound);
            imports.push((path.clone(), path));
        }
    }
}

/// `from a import b as c, d`, `from . import x`, `from .m import *`.
fn import_from(
    parsed: &ParsedPython<'_>,
    node: Node<'_>,
    symbols: &mut FileSymbols,
    imports: &mut Vec<(String, String)>,
) {
    let Some(module) = node.child_by_field_name("module_name") else {
        return;
    };
    let module = parsed.text(module).to_string();
    let join = |name: &str| {
        if module.ends_with('.') {
            format!("{module}{name}")
        } else {
            format!("{module}.{name}")
        }
    };
    if named_children(node)
        .iter()
        .any(|c| c.kind() == "wildcard_import")
    {
        imports.push(("*".to_string(), join("*")));
        return;
    }
    let mut cursor = node.walk();
    for item in node.children_by_field_name("name", &mut cursor) {
        let (name, alias) = if item.kind() == "aliased_import" {
            let (Some(n), Some(a)) = (
                item.child_by_field_name("name"),
                item.child_by_field_name("alias"),
            ) else {
                continue;
            };
            (parsed.text(n).to_string(), parsed.text(a).to_string())
        } else {
            let n = parsed.text(item).to_string();
            (n.clone(), n)
        };
        let full = join(&name);
        symbols.imports.insert(alias.clone(), full.clone());
        imports.push((alias, full));
    }
}
