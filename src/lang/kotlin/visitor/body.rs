//! Törzs-bejárás: egy függvény / computed property / inicializáló minden node-ja — hívás, metódus-hivatkozás,
//! elágazás, string-literál (a lambdák és a beágyazott függvények a külső hívóhoz tartoznak).

use tree_sitter::Node;

use super::super::parser::named_children;
use super::util::{base_type, has_token, infer_type, is_comment, name_and_type, value_of};
use super::{KotlinVisitor, Scope};
use crate::model::FileGraphBuilder;

impl KotlinVisitor<'_, '_> {
    /// A node és a leszármazottai bejárása a `scope` hívóval.
    pub(super) fn walk_body(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        match node.kind() {
            "call_expression" => self.call(node, scope, b),
            "callable_reference" => self.reference(node, scope, b),
            // `viewModel::refresh` / `this::go` / `Item::name`: `::` jelű navigáció, ha nem hívás célpontja
            "navigation_expression" if has_token(node, "::") && !is_callee(node) => {
                self.reference(node, scope, b)
            }
            "if_expression" => self.if_flow(node, scope, b),
            "when_expression" => self.when_flow(node, scope, b),
            "string_literal" | "multiline_string_literal" | "raw_string_literal" => {
                self.string_literal(node, scope, b)
            }
            "property_declaration" => self.local_variable(node),
            _ if is_comment(node) => return,
            _ => {}
        }
        for child in named_children(node) {
            self.walk_body(child, scope, b);
        }
    }

    /// Helyi `val x = Foo()` / `val x: Foo`: a név és a típus a fogadó-feloldáshoz.
    fn local_variable(&mut self, node: Node<'_>) {
        let (Some(name), declared) = name_and_type(self.parsed, node) else {
            return;
        };
        let ty = declared
            .as_deref()
            .and_then(base_type)
            .or_else(|| value_of(node).and_then(|v| infer_type(self.parsed, v)));
        match ty {
            Some(ty) => {
                self.locals.insert(name, ty);
            }
            None => {
                self.locals.remove(&name);
            }
        }
    }
}

/// A node egy `call_expression` hívott kifejezése-e (az első nevesített gyerek).
fn is_callee(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.kind() == "call_expression")
        && node.prev_named_sibling().is_none()
}
