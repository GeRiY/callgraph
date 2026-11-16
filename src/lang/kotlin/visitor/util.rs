//! Szintaxisfa-segédek: azonosítók, tokenek, annotációk, deklarált / következtetett típusok. Szabad függvények,
//! hogy a `scan` (a bejárás előtti szimbólum-gyűjtés) és a bejáró is használhassa.

use tree_sitter::Node;

use super::super::known::{FRAMEWORK_ANNOTATIONS, is_in};
use super::super::parser::{ParsedKotlin, children, named_children};

/// Azonosító-jellegű node-fajták (a grammar-változatok szerint).
pub(super) const IDENTS: &[&str] = &["identifier", "simple_identifier", "type_identifier"];

/// Komment node-fajták.
pub(super) const COMMENTS: &[&str] = &[
    "line_comment",
    "block_comment",
    "multiline_comment",
    "comment",
];

/// Generikus „gyári” függvények, amelyeknek a típus-argumentuma a példány típusa (`viewModel<T>()`).
const GENERIC_FACTORIES: &[&str] = &[
    "viewModel",
    "hiltViewModel",
    "koinViewModel",
    "viewModels",
    "activityViewModels",
    "hiltNavGraphViewModel",
    "inject",
    "get",
    "getValue",
    "remember",
];

/// A DI / ViewModel-befecskendező gyári függvények: a példány típusa a típus-argumentumból vagy a deklarált típusból
/// jön (`hiltViewModel()`, `by viewModels()`, `by inject()`).
const INJECTORS: &[&str] = &[
    "viewModel",
    "hiltViewModel",
    "koinViewModel",
    "getViewModel",
    "viewModels",
    "activityViewModels",
    "navGraphViewModels",
    "hiltNavGraphViewModel",
    "inject",
    "koinInject",
];

/// Befecskendező gyári függvény-e a név.
pub(super) fn is_injector(name: &str) -> bool {
    INJECTORS.contains(&name)
}

pub(super) fn is_comment(node: Node<'_>) -> bool {
    COMMENTS.contains(&node.kind())
}

/// Az első közvetlen azonosító-gyerek.
pub(super) fn first_ident(node: Node<'_>) -> Option<Node<'_>> {
    named_children(node)
        .into_iter()
        .find(|c| IDENTS.contains(&c.kind()))
}

/// A deklaráció neve: a `name` mező, különben az első közvetlen azonosító.
pub(super) fn name_node(node: Node<'_>) -> Option<Node<'_>> {
    node.child_by_field_name("name")
        .or_else(|| first_ident(node))
}

/// Az első közvetlen gyerek az adott fajtával.
pub(super) fn child_of<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    named_children(node).into_iter().find(|c| c.kind() == kind)
}

/// Van-e a node-nak közvetlen (akár névtelen token) gyereke az adott fajtával.
pub(super) fn has_token(node: Node<'_>, kind: &str) -> bool {
    children(node).iter().any(|c| c.kind() == kind)
}

/// A node `tok` tokenje utáni első nevesített gyerek (nem komment).
pub(super) fn after_token<'t>(node: Node<'t>, tok: &str) -> Option<Node<'t>> {
    let all = children(node);
    let pos = all.iter().position(|c| c.kind() == tok)?;
    all[pos + 1..]
        .iter()
        .copied()
        .find(|c| c.is_named() && !is_comment(*c))
}

/// Az annotáció-node-ok (a `modifiers` alatt vagy közvetlenül).
pub(super) fn annotation_nodes<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    for child in named_children(node) {
        match child.kind() {
            "modifiers" => out.extend(
                named_children(child)
                    .into_iter()
                    .filter(|c| c.kind() == "annotation"),
            ),
            "annotation" => out.push(child),
            _ => {}
        }
    }
    out
}

/// Egy annotáció neve: `@GET("x")` → `GET`; `@field:Inject` → `Inject`; `@dagger.Provides` → `Provides`.
pub(super) fn annotation_name(text: &str) -> String {
    let t = text.trim().trim_start_matches('@');
    let t = t.split_once(':').map_or(t, |(_, rest)| rest);
    let t = t.split(['(', ' ', '\n']).next().unwrap_or(t);
    t.rsplit('.').next().unwrap_or(t).trim().to_string()
}

/// Az annotáció zárójeles argumentumai (`@GET("users")` → `"users"`).
pub(super) fn annotation_args(text: &str) -> Option<String> {
    let start = text.find('(')?;
    let end = text.rfind(')')?;
    (end > start + 1).then(|| text[start + 1..end].trim().to_string())
}

/// A node annotációinak nevei.
pub(super) fn annotation_names(parsed: &ParsedKotlin<'_>, node: Node<'_>) -> Vec<String> {
    annotation_nodes(node)
        .into_iter()
        .map(|a| annotation_name(parsed.text(a)))
        .collect()
}

/// Ismert keretrendszer-annotáció-e.
pub(super) fn is_framework_annotation(name: &str) -> bool {
    is_in(FRAMEWORK_ANNOTATIONS, name)
}

/// Egy típus-szöveg alaptípusa: `StateFlow<List<X>>?` → `StateFlow`; csak nagybetűs kezdetű név (egyébként `None`).
pub(super) fn base_type(text: &str) -> Option<String> {
    let t = text.trim().trim_end_matches('?').trim();
    let t = t.split('<').next().unwrap_or(t).trim();
    let t = t.rsplit('.').next().unwrap_or(t).trim();
    (t.chars().next().is_some_and(char::is_uppercase)
        && t.chars().all(|c| c.is_alphanumeric() || c == '_'))
    .then(|| t.to_string())
}

/// Egy `name: Type` alakú deklaráció (paraméter, property, konstruktor-paraméter) neve és típus-szövege.
pub(super) fn name_and_type(
    parsed: &ParsedKotlin<'_>,
    node: Node<'_>,
) -> (Option<String>, Option<String>) {
    let holder = child_of(node, "variable_declaration").unwrap_or(node);
    let name = first_ident(holder).map(|n| parsed.text(n).to_string());
    let ty = after_token(holder, ":").map(|t| parsed.text(t).trim().to_string());
    (name, ty)
}

/// A `=` utáni kifejezés (alapérték / inicializáló).
pub(super) fn value_of<'t>(node: Node<'t>) -> Option<Node<'t>> {
    after_token(node, "=")
}

/// A hívás első típus-argumentuma (`viewModel<HomeViewModel>()` → `HomeViewModel`).
fn first_type_argument(parsed: &ParsedKotlin<'_>, node: Node<'_>, depth: usize) -> Option<String> {
    if node.kind() == "type_arguments" {
        let first = named_children(node).into_iter().find(|c| !is_comment(*c))?;
        return base_type(parsed.text(first));
    }
    if depth == 0 {
        return None;
    }
    named_children(node)
        .into_iter()
        .filter(|c| {
            !matches!(
                c.kind(),
                "lambda_literal" | "annotated_lambda" | "value_arguments"
            )
        })
        .find_map(|c| first_type_argument(parsed, c, depth - 1))
}

/// A hívás első típus-argumentuma (`hiltViewModel<HomeViewModel>()` → `HomeViewModel`).
pub(super) fn type_argument(parsed: &ParsedKotlin<'_>, node: Node<'_>) -> Option<String> {
    first_type_argument(parsed, node, 3)
}

/// A hívás értékét fogadó deklaráció típusa: `viewModel: X = hiltViewModel()` (paraméter-alapérték),
/// `val vm: X = viewModel()`, `val vm: X by viewModels()`; csak közvetlen inicializálónál / delegáltnál.
pub(super) fn declared_type_around(parsed: &ParsedKotlin<'_>, call: Node<'_>) -> Option<String> {
    let holder = match call.parent() {
        Some(p) if p.kind() == "property_delegate" => p.parent()?,
        Some(p) if p.kind() == "property_declaration" => p,
        _ => {
            let eq = call.prev_sibling().filter(|n| n.kind() == "=")?;
            eq.prev_sibling()?
        }
    };
    if !matches!(
        holder.kind(),
        "property_declaration" | "parameter" | "class_parameter"
    ) {
        return None;
    }
    name_and_type(parsed, holder)
        .1
        .as_deref()
        .and_then(base_type)
}

/// Egyszerű típus-következtetés az inicializálóból: literálok, `Foo(..)`, `viewModel<T>()`, `mutableStateOf(..)`.
pub(super) fn infer_type(parsed: &ParsedKotlin<'_>, value: Node<'_>) -> Option<String> {
    let text = parsed.text(value).trim();
    if text.starts_with('"') {
        return Some("String".into());
    }
    if text == "true" || text == "false" {
        return Some("Boolean".into());
    }
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return Some("Int".into());
    }
    if value.kind() != "call_expression" {
        return None;
    }
    let callee = named_children(value)
        .into_iter()
        .find(|c| !is_comment(*c))?;
    if !IDENTS.contains(&callee.kind()) {
        return None;
    }
    let name = parsed.text(callee);
    if name.chars().next().is_some_and(char::is_uppercase) {
        return Some(name.to_string());
    }
    if GENERIC_FACTORIES.contains(&name)
        && let Some(t) = first_type_argument(parsed, value, 3)
    {
        return Some(t);
    }
    match name {
        "mutableStateOf" => Some("MutableState".into()),
        "listOf" => Some("List".into()),
        "mutableListOf" => Some("MutableList".into()),
        "arrayListOf" => Some("ArrayList".into()),
        "mapOf" => Some("Map".into()),
        "mutableMapOf" => Some("MutableMap".into()),
        "setOf" => Some("Set".into()),
        _ => None,
    }
}

/// A `val` / `var` kulcsszó (a node közvetlen tokenje vagy a `binding_pattern_kind` alatt).
pub(super) fn binding_kind(parsed: &ParsedKotlin<'_>, node: Node<'_>) -> Option<&'static str> {
    for c in children(node) {
        let text = match c.kind() {
            "val" | "var" => c.kind(),
            "binding_pattern_kind" => parsed.text(c).trim(),
            _ => continue,
        };
        match text {
            "val" => return Some("val"),
            "var" => return Some("var"),
            _ => {}
        }
    }
    None
}
