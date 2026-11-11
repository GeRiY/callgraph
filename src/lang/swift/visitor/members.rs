//! Típus- és fájl-szintű tagok: `func` / `init` / `deinit` / `subscript` / computed property → metódus,
//! `var` / `let` → property (property wrapperrel a típus előtt), `static let` / enum case → konstans.

use tree_sitter::Node;

use super::super::parser::{children, end_line, line, named_children};
use super::signature::is_view_type;
use super::{MethodSpan, Scope, SwiftVisitor, clip};
use crate::model::{Constant, FileGraphBuilder, Method, Property};

impl SwiftVisitor<'_, '_> {
    /// Egy deklaráció-tag felvétele; `false`, ha a node nem deklaráció (fájl-szintű utasítás).
    pub(super) fn member(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        protocol: bool,
        b: &mut FileGraphBuilder,
    ) -> bool {
        match node.kind() {
            "function_declaration"
            | "protocol_function_declaration"
            | "init_declaration"
            | "deinit_declaration"
            | "subscript_declaration" => self.function(node, container, protocol, b),
            "property_declaration" | "protocol_property_declaration" => {
                self.property(node, container, protocol, b)
            }
            "typealias_declaration"
            | "associatedtype_declaration"
            | "operator_declaration"
            | "precedence_group_declaration" => {}
            _ => return false,
        }
        true
    }

    fn function(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        protocol: bool,
        b: &mut FileGraphBuilder,
    ) {
        let (base, kind) = match node.kind() {
            "init_declaration" => ("init".to_string(), "init"),
            "deinit_declaration" => ("deinit".to_string(), "deinit"),
            "subscript_declaration" => ("subscript".to_string(), "subscript"),
            _ => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| self.text(n).to_string())
                    .unwrap_or_default();
                let top = container.is_some() && container == self.module.as_deref();
                (name, if top { "function" } else { "method" })
            }
        };
        let return_type = self.return_type(node);
        let attributes = self.attributes(node);
        let in_view = return_type.as_deref().is_some_and(is_view_type)
            || attributes.iter().any(|a| a == "@ViewBuilder");
        let body = node.child_by_field_name("body");
        let abstract_ = protocol;
        self.add_member_method(
            node,
            container,
            &base,
            kind,
            self.params(node),
            return_type,
            abstract_,
            in_view,
            body,
            b,
        );
    }

    fn property(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        protocol: bool,
        b: &mut FileGraphBuilder,
    ) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node).trim_start_matches("var ").trim();
        let name = name.trim_start_matches("let ").trim().to_string();
        let words = self.modifier_words(node);
        let is_static = words.iter().any(|w| w == "static" || w == "class");
        let is_let = children(node)
            .iter()
            .chain(children(name_node).iter())
            .any(|c| c.kind() == "value_binding_pattern" && self.text(*c) == "let")
            || (node.kind() == "protocol_property_declaration" && !self.text(node).contains("set"));
        let declared = named_children(node)
            .into_iter()
            .find(|c| c.kind() == "type_annotation")
            .map(|t| self.text(t).trim_start_matches(':').trim().to_string());
        let value = node.child_by_field_name("value");
        if let Some(computed) = node.child_by_field_name("computed_value") {
            let in_view = declared.as_deref().is_some_and(is_view_type)
                || self.attributes(node).iter().any(|a| a == "@ViewBuilder");
            let kind = "computed";
            self.add_member_method(
                node,
                container,
                &name,
                kind,
                Vec::new(),
                declared,
                protocol,
                in_view,
                Some(computed),
                b,
            );
            return;
        }
        let top = container.is_some() && container == self.module.as_deref();
        if is_let && (is_static || top) && !protocol {
            b.constant(Constant {
                container: container.map(str::to_string),
                kind: "const".to_string(),
                name,
                line: line(node),
                value: value.map(|v| clip(self.text(v), 80)),
                visibility: Some(self.visibility(node)),
                caller: None,
                doc: self.doc_of(node),
                js: false,
            });
        } else {
            let inferred = declared.or_else(|| value.and_then(|v| self.infer_type(v)));
            let mut parts = self.attributes(node);
            parts.extend(inferred);
            b.property(Property {
                container: container.map(str::to_string),
                name,
                line: line(node),
                type_hint: (!parts.is_empty()).then(|| parts.join(" ")),
                visibility: self.visibility(node),
                is_static,
                readonly: is_let,
                doc: self.doc_of(node),
                js: false,
            });
        }
        if let Some(value) = value {
            let scope = Scope {
                container: container.map(str::to_string),
                caller: "__top__".to_string(),
                in_view: false,
            };
            self.walk_body(value, &scope, b);
        }
    }

    /// `case a = 1, b` → egy-egy `enum_case` konstans (nyers érték vagy asszociált értékek).
    pub(super) fn enum_cases(&mut self, node: Node<'_>, container: &str, b: &mut FileGraphBuilder) {
        let doc = self.doc_of(node);
        let mut cursor = node.walk();
        let mut current: Option<(String, u32)> = None;
        let mut cases: Vec<(String, u32, Option<String>)> = Vec::new();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match cursor.field_name() {
                    Some("name") => {
                        if let Some((n, l)) = current.take() {
                            cases.push((n, l, None));
                        }
                        current = Some((self.text(child).to_string(), line(child)));
                    }
                    Some("raw_value") | Some("data_contents") => {
                        if let Some((n, l)) = current.take() {
                            cases.push((n, l, Some(clip(self.text(child), 80))));
                        }
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        cases.extend(current.map(|(n, l)| (n, l, None)));
        for (name, line, value) in cases {
            b.constant(Constant {
                container: Some(container.to_string()),
                kind: "enum_case".to_string(),
                name,
                line,
                value,
                visibility: None,
                caller: None,
                doc: doc.clone(),
                js: false,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_member_method(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        base: &str,
        kind: &str,
        params: Vec<String>,
        return_type: Option<String>,
        is_abstract: bool,
        in_view: bool,
        body: Option<Node<'_>>,
        b: &mut FileGraphBuilder,
    ) {
        let container = container.map(str::to_string);
        let taken = self
            .methods
            .iter()
            .any(|m| m.container == container && m.name == base);
        let name = if taken {
            format!("{base}@L{}", line(node))
        } else {
            base.to_string()
        };
        let words = self.modifier_words(node);
        b.method(Method {
            container: container.clone(),
            name: name.clone(),
            line: line(node),
            end_line: Some(end_line(node)),
            visibility: Some(self.visibility(node)),
            is_static: words.iter().any(|w| w == "static" || w == "class"),
            is_abstract,
            kind: Some(kind.to_string()),
            params,
            return_type,
            doc_types: None,
            doc: self.doc_of(node),
            body: None,
        });
        self.add_method_span(&container, &name, line(node), end_line(node));
        if let Some(body) = body {
            let scope = Scope {
                container,
                caller: name,
                in_view,
            };
            self.walk_body(body, &scope, b);
        }
    }

    pub(super) fn add_method_span(
        &mut self,
        container: &Option<String>,
        name: &str,
        l: u32,
        e: u32,
    ) {
        self.methods.push(MethodSpan {
            container: container.clone(),
            name: name.to_string(),
            line: l,
            end_line: e,
        });
    }
}
