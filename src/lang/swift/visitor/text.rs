//! Szöveges részek: módosítók és attribútumok, láthatóság, docblock (`///`, `/** */`),
//! string-literálok és a (nem docblock) kommentek.

use tree_sitter::Node;

use super::super::parser::{line, named_children};
use super::{Scope, SwiftVisitor, clip};
use crate::model::{Comment, FileGraphBuilder, StringLit};

const VISIBILITY: &[&str] = &["private", "fileprivate", "internal", "public", "open"];

impl SwiftVisitor<'_, '_> {
    fn modifiers_node<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        named_children(node)
            .into_iter()
            .find(|c| c.kind() == "modifiers")
    }

    /// A módosító-szavak (`private`, `static`, `final`, `private(set)`…), attribútumok nélkül.
    pub(super) fn modifier_words(&self, node: Node<'_>) -> Vec<String> {
        self.modifiers_node(node)
            .map(|m| {
                named_children(m)
                    .into_iter()
                    .filter(|c| c.kind() != "attribute")
                    .map(|c| self.text(c).to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Az attribútumok forrása (`@State`, `@Environment(\.dismiss)`, `@ViewBuilder`…).
    pub(super) fn attributes(&self, node: Node<'_>) -> Vec<String> {
        self.modifiers_node(node)
            .map(|m| {
                named_children(m)
                    .into_iter()
                    .filter(|c| c.kind() == "attribute")
                    .map(|c| clip(self.text(c), 60))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A láthatóság (alapból `internal`; `private(set)` változatlanul).
    pub(super) fn visibility(&self, node: Node<'_>) -> String {
        self.modifier_words(node)
            .into_iter()
            .find(|w| VISIBILITY.iter().any(|v| w.starts_with(v)))
            .unwrap_or_else(|| "internal".to_string())
    }

    /// A deklaráció előtti, hézag nélkül összefüggő `///` sorok vagy `/** */` blokk (≤300 kar.).
    pub(super) fn doc_of(&self, node: Node<'_>) -> Option<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut expected = line(node);
        let mut prev = node.prev_named_sibling();
        while let Some(p) = prev {
            let text = self.text(p);
            let end = super::super::parser::end_line(p);
            if end + 1 != expected {
                break;
            }
            if p.kind() == "comment" && text.starts_with("///") {
                lines.push(text.trim_start_matches('/').trim().to_string());
            } else if p.kind() == "multiline_comment" && text.starts_with("/**") && lines.is_empty()
            {
                let inner = text.trim_start_matches("/**").trim_end_matches("*/");
                let joined: Vec<&str> = inner
                    .lines()
                    .map(|l| l.trim().trim_start_matches('*').trim())
                    .filter(|l| !l.is_empty())
                    .collect();
                lines.push(joined.join("\n"));
                break;
            } else {
                break;
            }
            expected = line(p);
            prev = p.prev_named_sibling();
        }
        if lines.is_empty() {
            return None;
        }
        lines.reverse();
        let doc = lines.join("\n");
        Some(doc.chars().take(300).collect())
    }

    /// String-literál: a szöveg-részek, az interpolációk `\(…)`-ként (üres / csak szóköz kimarad).
    pub(super) fn string_literal(&self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let mut value = String::new();
        for part in named_children(node) {
            match part.kind() {
                "interpolated_expression" => value.push_str("\\(…)"),
                _ => value.push_str(self.text(part)),
            }
        }
        if value.trim().is_empty() {
            return;
        }
        let end = super::super::parser::end_line(node);
        b.string(StringLit {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: line(node),
            end_line: (end > line(node)).then_some(end),
            value: value.chars().take(300).collect(),
        });
    }

    /// A nem docblock kommentek: `TODO|FIXME|XXX|HACK` címke vagy ≥15 karakter; a legszűkebb metódushoz.
    pub(super) fn comments(&self, node: Node<'_>, b: &mut FileGraphBuilder) {
        for child in named_children(node) {
            match child.kind() {
                "comment" | "multiline_comment" => self.comment(child, b),
                _ => self.comments(child, b),
            }
        }
    }

    fn comment(&self, node: Node<'_>, b: &mut FileGraphBuilder) {
        let raw = self.text(node);
        if raw.starts_with("///") || raw.starts_with("/**") {
            return;
        }
        let text = raw
            .trim_start_matches("//")
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim();
        let tag = ["TODO", "FIXME", "XXX", "HACK"]
            .into_iter()
            .find(|t| text.starts_with(t));
        if tag.is_none() && text.chars().count() < 15 {
            return;
        }
        let ln = line(node);
        let method = self
            .methods
            .iter()
            .filter(|m| m.line < ln && ln <= m.end_line)
            .min_by_key(|m| m.end_line - m.line);
        let (container, caller) = match method {
            Some(m) => (m.container.clone(), m.name.clone()),
            None => (
                self.containers
                    .iter()
                    .filter(|(_, s, e)| *s <= ln && ln <= *e)
                    .min_by_key(|(_, s, e)| e - s)
                    .map(|(n, _, _)| n.clone()),
                "__top__".to_string(),
            ),
        };
        b.comment(Comment {
            container,
            caller: Some(caller),
            line: ln,
            tag: tag.map(str::to_string),
            text: text.chars().take(200).collect(),
        });
    }
}
