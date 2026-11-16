//! Előzetes szimbólum-gyűjtés: a fájl típusai, függvény- és `@Composable` nevei, az osztályok property- és
//! konstruktor-paraméter típusai (a fogadó-feloldáshoz), az importok, és hogy kell-e fájl-szintű container.

use std::collections::HashMap;

use tree_sitter::Node;

use super::super::categorize::FileSymbols;
use super::super::parser::{ParsedKotlin, named_children};
use super::nav;
use super::util::{
    annotation_names, base_type, binding_kind, infer_type, is_comment, name_and_type, name_node,
    value_of,
};

/// A gyűjtés eredménye.
pub(super) struct Scanned {
    pub symbols: FileSymbols,
    /// Az importok teljes nevei (`androidx.compose.runtime.remember`, `kotlin.math.*`).
    pub imports: Vec<String>,
    /// Van-e fájl-szintű (típuson kívüli) függvény / változó / utasítás.
    pub needs_module: bool,
}

/// A szimbólumok, importok és a fájl-szintű container szükségessége.
pub(super) fn scan(parsed: &ParsedKotlin<'_>, root: Node<'_>) -> Scanned {
    let mut symbols = FileSymbols::default();
    let mut imports = Vec::new();
    collect(parsed, root, None, &mut symbols);
    collect_imports(parsed, root, &mut imports);
    let needs_module = named_children(root).iter().any(|n| {
        !is_comment(*n)
            && !matches!(
                n.kind(),
                "package_header"
                    | "import"
                    | "import_header"
                    | "import_list"
                    | "class_declaration"
                    | "object_declaration"
                    | "type_alias"
                    | "file_annotation"
                    | "annotation"
                    | "shebang_line"
            )
    });
    Scanned {
        symbols,
        imports,
        needs_module,
    }
}

/// A framework neve az importokból: Compose → `compose`, egyéb Android → `android`, különben `kotlin`.
pub(super) fn framework_of(imports: &[String]) -> &'static str {
    if imports.iter().any(|i| i.starts_with("androidx.compose.")) {
        "compose"
    } else if imports
        .iter()
        .any(|i| i.starts_with("android.") || i.starts_with("androidx."))
    {
        "android"
    } else {
        "kotlin"
    }
}

/// Az `import a.b.C` / `import a.b.C as D` sorok teljes neve (alias nélkül), `*`-os is.
pub(super) fn import_path(text: &str) -> String {
    let body = text.trim().trim_start_matches("import").trim();
    let body = body.split(" as ").next().unwrap_or(body);
    body.trim().trim_end_matches(';').trim().to_string()
}

fn collect_imports(parsed: &ParsedKotlin<'_>, node: Node<'_>, out: &mut Vec<String>) {
    for child in named_children(node) {
        match child.kind() {
            "import" | "import_header" => out.push(import_path(parsed.text(child))),
            "import_list" => collect_imports(parsed, child, out),
            _ => {}
        }
    }
}

/// A node egy osztály-törzs közvetlen gyereke-e (a helyi változók nem osztály-tagok).
fn in_class_body(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        matches!(
            p.kind(),
            "class_body" | "enum_class_body" | "class_member_declarations"
        )
    })
}

fn collect(parsed: &ParsedKotlin<'_>, node: Node<'_>, class: Option<&str>, out: &mut FileSymbols) {
    for child in named_children(node) {
        match child.kind() {
            "class_declaration" | "object_declaration" | "companion_object" => {
                let name = name_node(child)
                    .map_or_else(|| "Companion".to_string(), |n| parsed.text(n).to_string());
                if child.kind() != "companion_object" {
                    out.types.insert(name.clone());
                }
                let mut members: HashMap<String, String> = HashMap::new();
                collect_ctor_params(parsed, child, &mut members);
                out.members.entry(name.clone()).or_default().extend(members);
                collect(parsed, child, Some(&name), out);
            }
            "function_declaration" => {
                if let Some(n) = name_node(child) {
                    let name = parsed.text(n).to_string();
                    if annotation_names(parsed, child)
                        .iter()
                        .any(|a| a == "Composable")
                    {
                        out.composables.insert(name.clone());
                    }
                    out.functions.insert(name);
                }
                collect(parsed, child, class, out);
            }
            "property_declaration" if class.is_some() && in_class_body(child) => {
                if let (Some(class), (Some(name), declared)) = (class, name_and_type(parsed, child))
                {
                    let ty = declared
                        .as_deref()
                        .and_then(base_type)
                        .or_else(|| value_of(child).and_then(|v| infer_type(parsed, v)))
                        .or_else(|| delegate_type(parsed, child));
                    if let Some(ty) = ty {
                        out.members
                            .entry(class.to_string())
                            .or_default()
                            .insert(name, ty);
                    }
                }
                collect(parsed, child, class, out);
            }
            "call_expression" => {
                if let Some((route, screens)) = nav::destination(parsed, child) {
                    for (node, name) in screens {
                        out.screen_nodes.insert(node.id(), route.clone());
                        out.routes.push((route.clone(), name));
                    }
                }
                collect(parsed, child, class, out);
            }
            "binary_expression" => {
                if let Some((route, screens)) = nav::generic_destination(parsed, child) {
                    for (node, name) in screens {
                        out.screen_nodes.insert(node.id(), route.clone());
                        out.routes.push((route.clone(), name));
                    }
                }
                collect(parsed, child, class, out);
            }
            _ => collect(parsed, child, class, out),
        }
    }
}

/// A `by viewModels<T>()` / `by inject<T>()` delegált property típusa.
fn delegate_type(parsed: &ParsedKotlin<'_>, node: Node<'_>) -> Option<String> {
    let delegate = named_children(node)
        .into_iter()
        .find(|c| c.kind() == "property_delegate")?;
    let expr = named_children(delegate).into_iter().next_back()?;
    infer_type(parsed, expr)
}

/// A primary constructor paramétereinek típusai (a `val` / `var`-osak property-k, a többi csak az `init`-ben él,
/// de a fogadó-feloldásnak mindkettő jó).
fn collect_ctor_params(
    parsed: &ParsedKotlin<'_>,
    class: Node<'_>,
    out: &mut HashMap<String, String>,
) {
    for param in ctor_params(class) {
        if let (Some(name), Some(ty)) = name_and_type(parsed, param)
            && let Some(base) = base_type(&ty)
        {
            out.insert(name, base);
        }
    }
}

/// A primary constructor `class_parameter` node-jai.
pub(super) fn ctor_params(class: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    for child in named_children(class) {
        if child.kind() == "primary_constructor" {
            gather_params(child, 3, &mut out);
        }
    }
    out
}

fn gather_params<'t>(node: Node<'t>, depth: usize, out: &mut Vec<Node<'t>>) {
    for child in named_children(node) {
        if child.kind() == "class_parameter" {
            out.push(child);
        } else if depth > 0 && child.kind() != "modifiers" {
            gather_params(child, depth - 1, out);
        }
    }
}

/// A `class_parameter` `val` / `var` kulcsszavas-e (property).
pub(super) fn is_property_param(parsed: &ParsedKotlin<'_>, param: Node<'_>) -> bool {
    binding_kind(parsed, param).is_some()
}
