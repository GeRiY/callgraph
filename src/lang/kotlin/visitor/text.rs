//! Szöveges részek: módosítók és annotációk, láthatóság, KDoc, string-literálok, a (nem KDoc) kommentek.

use tree_sitter::Node;

use super::super::parser::{end_line, line, named_children};
use super::util::{
    annotation_args, annotation_name, annotation_nodes, child_of, is_comment,
    is_framework_annotation,
};
use super::{KotlinVisitor, Scope, clip};
use crate::model::{Call, Comment, FileGraphBuilder, StringLit};

const VISIBILITY: &[&str] = &["private", "protected", "internal", "public"];

impl KotlinVisitor<'_, '_> {
    /// A módosító-szavak (`private`, `suspend`, `data`, `override`…), annotációk nélkül.
    pub(super) fn modifier_words(&self, node: Node<'_>) -> Vec<String> {
        child_of(node, "modifiers")
            .map(|m| {
                named_children(m)
                    .into_iter()
                    .filter(|c| c.kind() != "annotation")
                    .flat_map(|c| {
                        self.text(c)
                            .split_whitespace()
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Az annotációk forrása (`@Inject`, `@GET("users")`…), ≤60 karakterrel.
    pub(super) fn annotations(&self, node: Node<'_>) -> Vec<String> {
        annotation_nodes(node)
            .into_iter()
            .map(|a| clip(self.text(a), 60))
            .collect()
    }

    /// A láthatóság (alapból `public`).
    pub(super) fn visibility(&self, node: Node<'_>) -> String {
        self.modifier_words(node)
            .into_iter()
            .find(|w| VISIBILITY.contains(&w.as_str()))
            .unwrap_or_else(|| "public".to_string())
    }

    /// A keretrendszer-annotációk `framework_use` hívás-élként (`@GET`, `@Inject`, `@Provides`, `@Dao`…).
    pub(super) fn annotation_calls(
        &self,
        node: Node<'_>,
        container: Option<&str>,
        caller: &str,
        b: &mut FileGraphBuilder,
    ) {
        for a in annotation_nodes(node) {
            let text = self.text(a);
            let name = annotation_name(text);
            if !is_framework_annotation(&name) {
                continue;
            }
            let end = end_line(a);
            b.call(Call {
                container: container.map(str::to_string),
                caller: Some(caller.to_string()),
                line: line(a),
                end_line: (end > line(a)).then_some(end),
                kind: "annotation".to_string(),
                target: format!("@{name}"),
                resolved: None,
                on: None,
                category: "framework_use".to_string(),
                args: annotation_args(text).map(|args| vec![clip(&args, 60)]),
                handler: None,
                from: None,
            });
        }
    }

    /// A deklaráció előtti, hézag nélkül szomszédos KDoc (`/** … */`, ≤300 kar.).
    pub(super) fn doc_of(&self, node: Node<'_>) -> Option<String> {
        let prev = node.prev_named_sibling()?;
        if !is_comment(prev) || end_line(prev) + 1 != line(node) {
            return None;
        }
        let text = self.text(prev);
        if !text.starts_with("/**") {
            return None;
        }
        let inner = text.trim_start_matches("/**").trim_end_matches("*/");
        let joined: Vec<&str> = inner
            .lines()
            .map(|l| l.trim().trim_start_matches('*').trim())
            .filter(|l| !l.is_empty())
            .collect();
        let doc = joined.join("\n");
        (!doc.is_empty()).then(|| doc.chars().take(300).collect())
    }

    /// String-literál: a szöveg-részek, az interpolációk `${…}`-ként (üres / csak szóköz kimarad).
    pub(super) fn string_literal(&self, node: Node<'_>, scope: &Scope, b: &mut FileGraphBuilder) {
        let value = flatten_string(self.text(node));
        if value.trim().is_empty() {
            return;
        }
        let end = end_line(node);
        b.string(StringLit {
            container: scope.container.clone(),
            caller: Some(scope.caller.clone()),
            line: line(node),
            end_line: (end > line(node)).then_some(end),
            value: value.chars().take(300).collect(),
        });
    }

    /// A nem KDoc kommentek: `TODO|FIXME|XXX|HACK` címke vagy ≥15 karakter; a legszűkebb metódushoz.
    pub(super) fn comments(&self, node: Node<'_>, b: &mut FileGraphBuilder) {
        for child in named_children(node) {
            if is_comment(child) {
                self.comment(child, b);
            } else {
                self.comments(child, b);
            }
        }
    }

    fn comment(&self, node: Node<'_>, b: &mut FileGraphBuilder) {
        let raw = self.text(node);
        if raw.starts_with("/**") {
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

/// A string-literál tartalma idézőjelek nélkül; a `$x` / `${…}` interpoláció `${…}`.
pub(super) fn flatten_string(raw: &str) -> String {
    let inner = raw
        .strip_prefix("\"\"\"")
        .and_then(|r| r.strip_suffix("\"\"\""))
        .or_else(|| raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')))
        .unwrap_or(raw);
    let chars: Vec<char> = inner.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            out.push(c);
            out.push(chars[i + 1]);
            i += 2;
        } else if c == '$' && chars.get(i + 1) == Some(&'{') {
            let mut depth = 0usize;
            while i < chars.len() {
                match chars[i] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            i += 1;
            out.push_str("${…}");
        } else if c == '$'
            && chars
                .get(i + 1)
                .is_some_and(|n| n.is_alphabetic() || *n == '_')
        {
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push_str("${…}");
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}
