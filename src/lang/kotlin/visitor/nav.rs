//! Compose Navigation: a `composable("útvonal") { Képernyő() }` célpontok felismerése és a `navigate("detail/$id")`
//! hívások párosítása az útvonalakkal (a literál-előtag alapján; típusbiztos `composable<Detail>` / `navigate(Detail(1))`
//! esetén a típusnév).

use tree_sitter::Node;

use super::super::parser::{ParsedKotlin, named_children};
use super::util::{IDENTS, is_comment, type_argument};

/// A navigációs gráf célpont-függvényei.
const DESTINATIONS: &[&str] = &["composable", "dialog", "bottomSheet"];

/// Záró lambdás hívásnál (`f(x) { }`) a külső `call_expression` első gyereke a belső hívás (név + argumentumok).
fn head(call: Node<'_>) -> Node<'_> {
    named_children(call)
        .into_iter()
        .find(|c| !is_comment(*c))
        .filter(|c| c.kind() == "call_expression")
        .unwrap_or(call)
}

/// A hívás zárójeles argumentum-node-jai.
pub(super) fn value_args(node: Node<'_>) -> Vec<Node<'_>> {
    let node = head(node);
    let mut holders = named_children(node);
    if let Some(pos) = holders.iter().position(|c| c.kind() == "call_suffix") {
        let inner = named_children(holders[pos]);
        holders.extend(inner);
    }
    holders
        .into_iter()
        .find(|c| c.kind() == "value_arguments")
        .map(|v| {
            named_children(v)
                .into_iter()
                .filter(|c| !is_comment(*c))
                .collect()
        })
        .unwrap_or_default()
}

/// Az argumentum értékének szövege: `route = "x"` → `"x"`.
fn arg_text<'s>(parsed: &ParsedKotlin<'s>, arg: Node<'_>) -> &'s str {
    let text = parsed.text(arg).trim();
    text.strip_prefix("route")
        .map(str::trim_start)
        .and_then(|rest| rest.strip_prefix('='))
        .map_or(text, str::trim)
}

/// Egyszerű (nem összefűzött) string-literál tartalma.
fn string_value(text: &str) -> Option<String> {
    let inner = text.trim().strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains('"')).then(|| inner.to_string())
}

/// A hívott puszta azonosító szövege (`composable`, `DetailScreen`).
fn callee_name<'s>(parsed: &ParsedKotlin<'s>, call: Node<'_>) -> Option<&'s str> {
    let first = named_children(head(call))
        .into_iter()
        .find(|c| !is_comment(*c))?;
    IDENTS.contains(&first.kind()).then(|| parsed.text(first))
}

/// A hívás törzs-lambdája (a zárójeles argumentumokon kívül).
fn trailing_lambda(node: Node<'_>) -> Option<Node<'_>> {
    for child in named_children(node) {
        match child.kind() {
            "lambda_literal" => return Some(child),
            "value_arguments" => {}
            _ => {
                if let Some(lambda) = trailing_lambda(child) {
                    return Some(lambda);
                }
            }
        }
    }
    None
}

/// A lambdában hívott, nagybetűs nevű függvények (képernyők); a megtalált hívás belsejébe nem lép.
fn collect_screens<'t>(
    parsed: &ParsedKotlin<'_>,
    node: Node<'t>,
    out: &mut Vec<(Node<'t>, String)>,
) {
    for child in named_children(node) {
        if child.kind() == "call_expression"
            && let Some(name) = callee_name(parsed, child)
            && name.chars().next().is_some_and(char::is_uppercase)
        {
            out.push((child, name.to_string()));
            continue;
        }
        collect_screens(parsed, child, out);
    }
}

/// `composable("detail/{id}") { DetailScreen() }`: az útvonal és a törzsben hívott képernyők (node, név).
pub(super) fn destination<'t>(
    parsed: &ParsedKotlin<'_>,
    call: Node<'t>,
) -> Option<(String, Vec<(Node<'t>, String)>)> {
    if !DESTINATIONS.contains(&callee_name(parsed, call)?) {
        return None;
    }
    let route = value_args(call)
        .first()
        .and_then(|arg| string_value(arg_text(parsed, *arg)))
        .or_else(|| type_argument(parsed, call))?;
    let mut screens = Vec::new();
    collect_screens(parsed, trailing_lambda(call)?, &mut screens);
    (!screens.is_empty()).then_some((route, screens))
}

/// Típusbiztos `composable<Detail> { DetailScreen() }`: a tree-sitter a zárójel nélküli generikus hívást
/// összehasonlításként olvassa (`(composable < Detail) > { … }`); ezt az alakot ismeri fel, mint a `destination`.
pub(super) fn generic_destination<'t>(
    parsed: &ParsedKotlin<'_>,
    node: Node<'t>,
) -> Option<(String, Vec<(Node<'t>, String)>)> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let parts: Vec<Node<'t>> = named_children(node)
        .into_iter()
        .filter(|c| !is_comment(*c))
        .collect();
    let [left, right] = parts.as_slice() else {
        return None;
    };
    if left.kind() != "binary_expression" || right.kind() != "lambda_literal" {
        return None;
    }
    let inner: Vec<Node<'t>> = named_children(*left);
    let [callee, ty] = inner.as_slice() else {
        return None;
    };
    if !IDENTS.contains(&callee.kind())
        || !IDENTS.contains(&ty.kind())
        || !DESTINATIONS.contains(&parsed.text(*callee))
    {
        return None;
    }
    let between_inner = parsed
        .text(*left)
        .trim_start()
        .strip_prefix(parsed.text(*callee))?
        .trim_start();
    let after_left = parsed
        .text(node)
        .trim_start()
        .strip_prefix(parsed.text(*left))?
        .trim_start();
    if !between_inner.starts_with('<') || !after_left.starts_with('>') {
        return None;
    }
    let mut screens = Vec::new();
    collect_screens(parsed, *right, &mut screens);
    (!screens.is_empty()).then(|| (parsed.text(*ty).to_string(), screens))
}

/// A `navigate(..)` célja: a string-literál (`"detail/$id"`) vagy a típusbiztos útvonal neve (`Detail(id)`, `Detail`).
pub(super) fn navigate_target(parsed: &ParsedKotlin<'_>, call: Node<'_>) -> Option<String> {
    let arg = *value_args(call).first()?;
    let text = arg_text(parsed, arg);
    if let Some(literal) = string_value(text) {
        return Some(literal);
    }
    let name = text.split('(').next().unwrap_or(text).trim();
    (name.chars().next().is_some_and(char::is_uppercase)
        && name.chars().all(|c| c.is_alphanumeric() || c == '_'))
    .then(|| name.to_string())
}

/// Az útvonal literál-előtagja a `{…}` / `$…` / `?…` előtt, záró `/` nélkül.
fn route_prefix(route: &str) -> &str {
    let end = route.find(['{', '$', '?']).unwrap_or(route.len());
    route[..end].trim_end_matches('/')
}

/// A `target` (navigate-argumentum) útvonalának párja: az egyező literál-előtagú, vagy a paraméteres útvonal
/// (`detail/{id}`) alatti (`detail/42`); több találatnál a leghosszabb előtag.
pub(super) fn match_route<'a>(
    routes: &'a [(String, String)],
    target: &str,
) -> Option<&'a (String, String)> {
    let wanted = route_prefix(target);
    if wanted.is_empty() {
        return None;
    }
    routes
        .iter()
        .filter(|(route, _)| {
            let prefix = route_prefix(route);
            !prefix.is_empty()
                && (prefix == wanted
                    || (route.contains('{') && wanted.starts_with(&format!("{prefix}/"))))
        })
        .max_by_key(|(route, _)| route_prefix(route).len())
}
