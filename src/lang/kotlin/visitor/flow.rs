//! Vezérlési szerkezetek: `if` / `else if` / `else` lánc, `when` (ágak).

use tree_sitter::Node;

use super::super::parser::{children, end_line, line, named_children};
use super::util::{after_token, has_token, is_comment};
use super::{KotlinVisitor, Scope, clip};
use crate::model::{Branch, ControlFlow, FileGraphBuilder};

/// Az `else` token utáni ág: ha egy másik `if`, az a lánc következő tagja.
fn else_if<'t>(parts: &[Node<'t>], else_pos: usize) -> Option<Node<'t>> {
    let next = parts[else_pos + 1..]
        .iter()
        .copied()
        .find(|c| c.is_named() && !is_comment(*c))?;
    if next.kind() == "if_expression" {
        return Some(next);
    }
    let inner: Vec<Node<'t>> = named_children(next)
        .into_iter()
        .filter(|c| !is_comment(*c))
        .collect();
    (next.kind() == "control_structure_body"
        && inner.len() == 1
        && inner[0].kind() == "if_expression")
        .then(|| inner[0])
}

/// Az `if` a másik `if` `else` ága-e (akkor a külső lánc része).
fn is_else_if(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let host = if parent.kind() == "control_structure_body" {
        match parent.parent() {
            Some(p) => (parent, p),
            None => return false,
        }
    } else {
        (node, parent)
    };
    host.1.kind() == "if_expression" && host.0.prev_sibling().is_some_and(|s| s.kind() == "else")
}

impl KotlinVisitor<'_, '_> {
    /// Egy `if`-lánc (a belső `else if` a külső if ágaként, nem külön elágazásként).
    pub(super) fn if_flow(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        if is_else_if(node) {
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
                end_line: branch_end.max(line(current)),
                fall_through: None,
            });
            let Some(i) = else_pos else { break };
            match else_if(&parts, i) {
                Some(next) => {
                    label = format!("elseif ({})", clip(&self.condition(next), 60));
                    current = next;
                }
                None => {
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

    /// `when (x) { a -> … else -> … }` — ágonként a feltételek.
    pub(super) fn when_flow(&mut self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let cond = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "when_subject")
            .map(|s| {
                self.text(s)
                    .trim()
                    .trim_matches(['(', ')'])
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        let branches = named_children(node)
            .into_iter()
            .filter(|c| c.kind() == "when_entry")
            .map(|entry| {
                let parts = children(entry);
                let arrow = parts.iter().position(|c| c.kind() == "->");
                let conditions: Vec<&str> = parts[..arrow.unwrap_or(parts.len())]
                    .iter()
                    .filter(|c| c.is_named() && !is_comment(**c))
                    .map(|c| self.text(*c))
                    .collect();
                let label = if has_token(entry, "else") || conditions.is_empty() {
                    "default".to_string()
                } else {
                    clip(&format!("case {}", conditions.join(", ")), 60)
                };
                Branch {
                    label,
                    start_line: line(entry),
                    end_line: end_line(entry),
                    fall_through: None,
                }
            })
            .collect();
        self.flow(node, "when", cond, branches, scope, b);
    }

    /// Az `if (feltétel)` feltétele: a `(` utáni első kifejezés.
    fn condition(&self, node: Node<'_>) -> String {
        node.child_by_field_name("condition")
            .or_else(|| after_token(node, "("))
            .map(|c| self.text(c).to_string())
            .unwrap_or_default()
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
