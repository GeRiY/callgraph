//! Törzs-bejárás: egy metódus / computed property / inicializáló minden node-ja — hívás, elágazás,
//! string-literál (a trailing closure-ök és a beágyazott függvények a külső hívóhoz tartoznak).

use tree_sitter::Node;

use super::super::parser::named_children;
use super::{Scope, SwiftVisitor};
use crate::model::FileGraphBuilder;

impl SwiftVisitor<'_, '_> {
    /// A node és a leszármazottai bejárása a `scope` hívóval.
    pub(super) fn walk_body(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        match node.kind() {
            "call_expression" => self.call(node, scope, b),
            "if_statement" => self.if_flow(node, scope, b),
            "switch_statement" => self.switch_flow(node, scope, b),
            "guard_statement" => self.guard_flow(node, scope, b),
            "line_string_literal" | "multi_line_string_literal" | "raw_string_literal" => {
                self.string_literal(node, scope, b)
            }
            "comment" | "multiline_comment" => return,
            _ => {}
        }
        for child in named_children(node) {
            self.walk_body(child, scope, b);
        }
    }
}
