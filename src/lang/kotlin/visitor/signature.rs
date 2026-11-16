//! Szignatúra-részek: paraméterek, visszatérési típus, receiver (extension), helyi változó-térkép.

use tree_sitter::Node;

use super::super::parser::{children, named_children};
use super::util::{base_type, is_comment, name_and_type, name_node};
use super::{KotlinVisitor, clip};

/// Típus-jellegű node-fajták (a receiver és a visszatérési típus felismeréséhez).
const TYPE_KINDS: &[&str] = &[
    "user_type",
    "nullable_type",
    "function_type",
    "parenthesized_type",
    "type_reference",
    "not_nullable_type",
    "receiver_type",
];

impl KotlinVisitor<'_, '_> {
    /// A paraméterek forrása (≤60 kar.), alapértékkel: `repo: Repo = RepoImpl()`.
    pub(super) fn params(&self, node: Node<'_>) -> Vec<String> {
        let mut out = Vec::new();
        for list in named_children(node).into_iter().filter(|c| {
            matches!(
                c.kind(),
                "function_value_parameters" | "primary_constructor"
            )
        }) {
            self.collect_params(list, 3, &mut out);
        }
        out
    }

    fn collect_params(&self, node: Node<'_>, depth: usize, out: &mut Vec<String>) {
        let all = children(node);
        for (i, child) in all.iter().enumerate() {
            match child.kind() {
                "parameter" | "class_parameter" => {
                    let mut text = self.text(*child).to_string();
                    if all.get(i + 1).is_some_and(|n| n.kind() == "=")
                        && let Some(value) = all.get(i + 2)
                    {
                        text = format!("{text} = {}", self.text(*value));
                    }
                    out.push(clip(&text, 60));
                }
                "modifiers" => {}
                _ if depth > 0 && child.is_named() => self.collect_params(*child, depth - 1, out),
                _ => {}
            }
        }
    }

    /// A paraméterek alapérték-kifejezései (`= hiltViewModel()`).
    pub(super) fn param_defaults<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        let mut out = Vec::new();
        for list in named_children(node)
            .into_iter()
            .filter(|c| c.kind() == "function_value_parameters")
        {
            collect_defaults(list, 3, &mut out);
        }
        out
    }

    /// A paraméterek nevei és típusai a helyi változó-térképhez (csak a nagybetűs alaptípusúak).
    pub(super) fn param_types(&self, node: Node<'_>) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for list in named_children(node)
            .into_iter()
            .filter(|c| c.kind() == "function_value_parameters")
        {
            let mut stack = vec![list];
            while let Some(n) = stack.pop() {
                for c in named_children(n) {
                    if c.kind() == "parameter" {
                        if let (Some(name), Some(ty)) = name_and_type(self.parsed, c)
                            && let Some(base) = base_type(&ty)
                        {
                            out.push((name, base));
                        }
                    } else if c.kind() != "modifiers" {
                        stack.push(c);
                    }
                }
            }
        }
        out
    }

    /// A `:` utáni típus a paraméter-lista után (`fun f(): Flow<X>`).
    pub(super) fn return_type(&self, node: Node<'_>) -> Option<String> {
        let all = children(node);
        let params = all
            .iter()
            .position(|c| c.kind() == "function_value_parameters")?;
        let mut iter = all[params + 1..].iter();
        while let Some(c) = iter.next() {
            match c.kind() {
                ":" => {
                    let t = iter.find(|c| c.is_named() && !is_comment(**c))?;
                    return Some(clip(self.text(*t), 60));
                }
                "=" | "function_body" | "block" => return None,
                _ => {}
            }
        }
        None
    }

    /// Az extension-függvény fogadó-típusa (`fun String.slug()` → `String`): a név előtti típus-node.
    pub(super) fn receiver_type(&self, node: Node<'_>) -> Option<String> {
        let name = name_node(node)?;
        children(node)
            .into_iter()
            .take_while(|c| c.id() != name.id())
            .find(|c| TYPE_KINDS.contains(&c.kind()))
            .map(|t| clip(self.text(t), 40))
    }
}

/// A `parameter` után `=` és az alapérték-kifejezés következik (testvér-node-ok).
fn collect_defaults<'t>(node: Node<'t>, depth: usize, out: &mut Vec<Node<'t>>) {
    let all = children(node);
    for (i, child) in all.iter().enumerate() {
        match child.kind() {
            "parameter" => {
                if all.get(i + 1).is_some_and(|n| n.kind() == "=")
                    && let Some(value) = all.get(i + 2)
                {
                    out.push(*value);
                }
            }
            "modifiers" => {}
            _ if depth > 0 && child.is_named() => collect_defaults(*child, depth - 1, out),
            _ => {}
        }
    }
}
