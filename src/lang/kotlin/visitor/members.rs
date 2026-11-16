//! Típus- és fájl-szintű tagok: `fun` / `constructor` / `init` / computed property → metódus, `val` / `var` →
//! property (delegálttal és `private set`-tel), `const val` / enum-érték → konstans.

use tree_sitter::Node;

use super::super::parser::{children, end_line, line, named_children};
use super::util::{
    annotation_names, base_type, binding_kind, child_of, infer_type, is_comment, name_and_type,
    name_node, value_of,
};
use super::{KotlinVisitor, MethodSpan, Scope, clip};
use crate::model::{Constant, FileGraphBuilder, Method, Property};

impl KotlinVisitor<'_, '_> {
    /// Egy deklaráció-tag felvétele; `false`, ha a node nem deklaráció (fájl-szintű utasítás).
    pub(super) fn member(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        interface: bool,
        b: &mut FileGraphBuilder,
    ) -> bool {
        match node.kind() {
            "function_declaration" => self.function(node, container, interface, b),
            "secondary_constructor" => self.special(node, container, "constructor", "init", b),
            "anonymous_initializer" => self.special(node, container, "init", "init", b),
            "property_declaration" => self.property(node, container, interface, b),
            "type_alias" | "getter" | "setter" => {}
            _ => return false,
        }
        true
    }

    fn function(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        interface: bool,
        b: &mut FileGraphBuilder,
    ) {
        let base = name_node(node)
            .map(|n| self.text(n).to_string())
            .unwrap_or_default();
        let annotations = annotation_names(self.parsed, node);
        let composable = annotations.iter().any(|a| a == "Composable");
        let words = self.modifier_words(node);
        let receiver = self.receiver_type(node);
        let top = container.is_some() && container == self.module.as_deref();
        let kind = if composable {
            "composable"
        } else if receiver.is_some() {
            "extension"
        } else if words.iter().any(|w| w == "suspend") {
            "suspend"
        } else if top {
            "function"
        } else {
            "method"
        };
        let mut params = self.params(node);
        if let Some(r) = receiver {
            params.insert(0, format!("receiver: {r}"));
        }
        let body = child_of(node, "function_body").or_else(|| child_of(node, "block"));
        let abstract_ = interface && body.is_none();
        let locals = self.param_types(node);
        let return_type = self.return_type(node);
        self.add_member_method(
            node,
            container,
            &base,
            kind,
            params,
            return_type,
            abstract_,
            composable,
            locals,
            body,
            b,
        );
    }

    /// `constructor(..)` / `init { }`: névvel ellátott metódus a törzs hívásaival.
    fn special(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        name: &str,
        kind: &str,
        b: &mut FileGraphBuilder,
    ) {
        let body = child_of(node, "block")
            .or_else(|| child_of(node, "function_body"))
            .or_else(|| child_of(node, "statements"));
        let locals = self.param_types(node);
        let mut params = Vec::new();
        for list in children(node) {
            if matches!(list.kind(), "function_value_parameters" | "parameters") {
                params.extend(
                    named_children(list)
                        .into_iter()
                        .filter(|c| !is_comment(*c))
                        .map(|c| clip(self.text(c), 60)),
                );
            }
        }
        self.add_member_method(
            node, container, name, kind, params, None, false, false, locals, body, b,
        );
    }

    fn property(
        &mut self,
        node: Node<'_>,
        container: Option<&str>,
        interface: bool,
        b: &mut FileGraphBuilder,
    ) {
        let (Some(name), declared) = name_and_type(self.parsed, node) else {
            // destrukturáló deklaráció: nincs tagja
            return;
        };
        let words = self.modifier_words(node);
        let is_const = words.iter().any(|w| w == "const");
        let is_val = binding_kind(self.parsed, node) == Some("val");
        let value = value_of(node);
        let delegate = child_of(node, "property_delegate");
        let getter = child_of(node, "getter");
        let composable_getter = annotation_names(self.parsed, node)
            .iter()
            .any(|a| a == "Composable");
        if let Some(getter) = getter
            && value.is_none()
            && delegate.is_none()
        {
            let body = child_of(getter, "function_body")
                .or_else(|| child_of(getter, "block"))
                .or_else(|| value_of(getter));
            self.add_member_method(
                node,
                container,
                &name,
                "computed",
                Vec::new(),
                declared,
                interface,
                composable_getter,
                Vec::new(),
                body,
                b,
            );
            return;
        }
        let top = container.is_some() && container == self.module.as_deref();
        if is_const && is_val {
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
            if let Some(value) = value {
                self.walk_property_value(value, container, b);
            }
            return;
        }
        let delegate_text = delegate.map(|d| {
            let t = self.text(d).trim_start_matches("by").trim();
            let head = t.split(['(', '{', '<']).next().unwrap_or(t).trim();
            format!("by {head}")
        });
        let inferred = declared
            .as_deref()
            .and_then(base_type)
            .or_else(|| value.and_then(|v| infer_type(self.parsed, v)))
            .or_else(|| {
                delegate
                    .and_then(|d| named_children(d).into_iter().next_back())
                    .and_then(|e| infer_type(self.parsed, e))
            });
        let shown_type = declared.map(|d| clip(&d, 60)).or(inferred);
        let mut parts = self.annotations(node);
        parts.extend(
            words
                .iter()
                .filter(|w| matches!(w.as_str(), "lateinit"))
                .cloned(),
        );
        parts.extend(delegate_text);
        parts.extend(shown_type);
        if let Some(setter) = child_of(node, "setter")
            && self.modifier_words(setter).iter().any(|w| w == "private")
        {
            parts.push("(private set)".to_string());
        }
        b.property(Property {
            container: container.map(str::to_string),
            name,
            line: line(node),
            type_hint: (!parts.is_empty()).then(|| parts.join(" ")),
            visibility: self.visibility(node),
            is_static: top,
            readonly: is_val,
            doc: self.doc_of(node),
            default: None,
            js: false,
        });
        self.annotation_calls(node, container, "__top__", b);
        if let Some(value) = value {
            self.walk_property_value(value, container, b);
        }
        if let Some(delegate) = delegate {
            self.walk_property_value(delegate, container, b);
        }
    }

    /// A property inicializálója / delegáltja: az osztály-szintű hívások (`__top__`).
    fn walk_property_value(
        &mut self,
        value: Node<'_>,
        container: Option<&str>,
        b: &mut FileGraphBuilder,
    ) {
        let scope = Scope {
            container: container.map(str::to_string),
            caller: "__top__".to_string(),
            in_compose: false,
        };
        self.locals.clear();
        self.walk_body(value, &scope, b);
    }

    /// `A, B(1)` → egy-egy `enum_case` konstans (a törzsük hívásaival).
    pub(super) fn enum_entry(&mut self, node: Node<'_>, container: &str, b: &mut FileGraphBuilder) {
        let Some(name) = name_node(node) else {
            return;
        };
        b.constant(Constant {
            container: Some(container.to_string()),
            kind: "enum_case".to_string(),
            name: self.text(name).to_string(),
            line: line(name),
            value: child_of(node, "value_arguments").map(|v| clip(self.text(v), 80)),
            visibility: None,
            caller: None,
            doc: self.doc_of(node),
            js: false,
        });
        for child in named_children(node) {
            if matches!(child.kind(), "value_arguments" | "class_body") {
                self.walk_property_value(child, Some(container), b);
            }
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
        in_compose: bool,
        locals: Vec<(String, String)>,
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
            is_static: false,
            is_abstract: is_abstract || words.iter().any(|w| w == "abstract"),
            kind: Some(kind.to_string()),
            params,
            return_type,
            doc_types: None,
            doc: self.doc_of(node),
            body: None,
        });
        self.add_method_span(&container, &name, line(node), end_line(node));
        self.annotation_calls(node, container.as_deref(), &name, b);
        if let Some(body) = body {
            self.locals.clear();
            self.locals.extend(locals);
            let scope = Scope {
                container,
                caller: name,
                in_compose,
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
