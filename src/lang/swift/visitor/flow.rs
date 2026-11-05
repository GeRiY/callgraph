//! Vezérlési szerkezetek: `if` / `else if` / `else` lánc, `switch` (case-ágak), `guard … else`.

use tree_sitter::Node;

use super::super::parser::{children, end_line, line, named_children};
use super::{Scope, SwiftVisitor, clip};
use crate::model::{Branch, ControlFlow, FileGraphBuilder};

impl SwiftVisitor<'_, '_> {
    /// Egy `if`-lánc (a belső `else if` a külső if ágaként, nem külön elágazásként).
    pub(super) fn if_flow(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        if node.parent().is_some_and(|p| p.kind() == "if_statement") {
            return;
        }
        let cond = self.condition(node);
        let mut branches = Vec::new();
        let mut current = node;
        let mut label = format!("if ({})", clip(&cond, 60));
        loop {
            let parts = children(current);
            let else_pos = parts.iter().position(|c| c.kind() == "else");
            let branch_end = match else_pos {
                Some(i) => line(parts[i]),
                None => end_line(current),
            };
            branches.push(Branch {
                label: label.clone(),
                start_line: line(current),
                end_line: branch_end,
                fall_through: None,
            });
            let Some(i) = else_pos else { break };
            match parts[i + 1..].iter().find(|c| c.is_named()) {
                Some(next) if next.kind() == "if_statement" => {
                    label = format!("elseif ({})", clip(&self.condition(*next), 60));
                    current = *next;
                }
                _ => {
                    branches.push(Branch {
                        label: "else".to_string(),
                        start_line: line(parts[i]),
                        end_line: end_line(current),
                        fall_through: None,
                    });
                    break;
                }
            }
        }
        self.flow(node, "if", cond, branches, scope, b);
    }

    /// `switch x { case …: … default: … }` — ágonként a case-minták.
    pub(super) fn switch_flow(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let cond = node
            .child_by_field_name("expr")
            .map(|e| self.text(e).to_string())
            .unwrap_or_default();
        let branches = named_children(node)
            .into_iter()
            .filter(|c| c.kind() == "switch_entry")
            .map(|entry| {
                let patterns: Vec<&str> = named_children(entry)
                    .into_iter()
                    .filter(|c| c.kind() == "switch_pattern")
                    .map(|p| self.text(p))
                    .collect();
                let label = if patterns.is_empty() {
                    "default".to_string()
                } else {
                    clip(&format!("case {}", patterns.join(", ")), 60)
                };
                let empty = !named_children(entry)
                    .iter()
                    .any(|c| c.kind() == "statements");
                Branch {
                    label,
                    start_line: line(entry),
                    end_line: end_line(entry),
                    fall_through: empty.then_some(true),
                }
            })
            .collect();
        self.flow(node, "switch", cond, branches, scope, b);
    }

    /// `guard feltétel else { … }` — egy `else` ág.
    pub(super) fn guard_flow(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let cond = self.condition(node);
        let parts = children(node);
        let start = parts
            .iter()
            .find(|c| c.kind() == "else")
            .map(|e| line(*e))
            .unwrap_or_else(|| line(node));
        let branches = vec![Branch {
            label: "else".to_string(),
            start_line: start,
            end_line: end_line(node),
            fall_through: None,
        }];
        self.flow(node, "guard", cond, branches, scope, b);
    }

    /// A feltétel(ek) forrása, vesszővel (`if let a = x, a > 1`).
    fn condition(&self, node: Node<'_>) -> String {
        let mut cursor = node.walk();
        let parts: Vec<&str> = node
            .children_by_field_name("condition", &mut cursor)
            .map(|c| self.text(c))
            .collect();
        parts.join(", ")
    }

    fn flow(
        &self,
        node: Node<'_>,
        kind: &str,
        cond: String,
        branches: Vec<Branch>,
        scope: &Scope,
        b: &mut FileGraphBuilder,
    ) {
        b.control_flow(ControlFlow {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            kind: kind.to_string(),
            line: line(node),
            cond: (!cond.is_empty()).then(|| clip(&cond, 100)),
            branches,
        });
    }
}
