//! Szignatúra-részek: paraméterek, visszatérési típus, típus-következtetés, nézet-típus felismerése.

use tree_sitter::Node;

use super::super::parser::{children, named_children};
use super::{SwiftVisitor, clip};

impl SwiftVisitor<'_, '_> {
    /// A paraméterek forrása (≤60 kar.), alapértékkel: `store: TodoStore = UserDefaultsStore()`.
    pub(super) fn params(&self, node: Node<'_>) -> Vec<String> {
        let all = children(node);
        let mut out = Vec::new();
        for (i, child) in all.iter().enumerate() {
            if child.kind() != "parameter" {
                continue;
            }
            let mut text = self.text(*child).to_string();
            if all.get(i + 1).is_some_and(|n| n.kind() == "=")
                && let Some(value) = all.get(i + 2)
            {
                text = format!("{text} = {}", self.text(*value));
            }
            out.push(clip(&text, 60));
        }
        out
    }

    /// A `->` utáni típus forrása.
    pub(super) fn return_type(&self, node: Node<'_>) -> Option<String> {
        let all = children(node);
        let arrow = all.iter().position(|c| c.kind() == "->")?;
        all[arrow + 1..]
            .iter()
            .find(|c| c.is_named())
            .map(|t| clip(self.text(*t), 60))
    }

    /// Egyszerű típus-következtetés az inicializálóból (`TodoViewModel()`, literálok).
    pub(super) fn infer_type(&self, value: Node<'_>) -> Option<String> {
        match value.kind() {
            "line_string_literal" | "multi_line_string_literal" => Some("String".into()),
            "integer_literal" => Some("Int".into()),
            "real_literal" => Some("Double".into()),
            "boolean_literal" => Some("Bool".into()),
            "call_expression" => {
                let callee = named_children(value).into_iter().next()?;
                let name = self.text(callee);
                (callee.kind() == "simple_identifier"
                    && name.chars().next().is_some_and(char::is_uppercase))
                .then(|| name.to_string())
            }
            _ => None,
        }
    }
}

/// `some View` / `View` / `AnyView` visszatérési típus: a törzs nézetet épít.
pub(super) fn is_view_type(t: &str) -> bool {
    let t = t.trim();
    t == "some View" || t == "View" || t == "AnyView" || t.ends_with(" View")
}
